use crate::{
    storage::{AppPaths, History},
    types::{Forward, Host},
};
use anyhow::{Context, Result};
use std::{
    collections::VecDeque,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PingStatus {
    #[default]
    Unknown,
    Connecting,
    Online,
    Offline,
}
#[derive(Clone, Debug)]
pub struct PingResult {
    pub name: String,
    pub status: PingStatus,
    pub duration: Duration,
    pub error: Option<String>,
}
pub fn connect(
    paths: &AppPaths,
    config: &Path,
    name: &str,
    tty: bool,
    remote: &[String],
    forward: Option<&Forward>,
) -> Result<i32> {
    crate::config::find_host(config, name)?;
    let arguments = ssh_arguments(config, name, tty, remote, forward)?;
    paths.initialize()?;
    if let Err(error) =
        History::load(&paths.root).and_then(|mut history| history.record(name, forward))
    {
        eprintln!("Warning: Could not record connection history: {error:#}");
    }
    if remote.is_empty() && forward.is_none() {
        eprintln!("Connecting to {name}...");
    }
    let status = Command::new("ssh")
        .args(arguments)
        .status()
        .context("Error executing SSH command; ensure OpenSSH is installed")?;
    Ok(exit_code(status))
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

pub fn ssh_arguments(
    config: &Path,
    name: &str,
    tty: bool,
    remote: &[String],
    forward: Option<&Forward>,
) -> Result<Vec<String>> {
    anyhow::ensure!(
        !name.starts_with('-') && !name.chars().any(char::is_control),
        "invalid SSH host name"
    );
    let mut arguments = vec!["-F".into(), config.to_string_lossy().into_owned()];
    if tty {
        arguments.push("-t".into())
    }
    if let Some(forward) = forward {
        arguments.extend(forward.ssh_args()?)
    }
    arguments.push(name.into());
    arguments.extend_from_slice(remote);
    Ok(arguments)
}

pub fn check_hosts(hosts: Vec<Host>, config: PathBuf) -> Receiver<PingResult> {
    let (tx, rx) = mpsc::channel();
    let workers = hosts.len().min(8);
    let queue = Arc::new(Mutex::new(VecDeque::from(hosts)));
    for _ in 0..workers {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        let config = config.clone();
        thread::spawn(move || loop {
            let host = queue.lock().ok().and_then(|mut q| q.pop_front());
            let Some(host) = host else { break };
            if tx
                .send(PingResult {
                    name: host.name.clone(),
                    status: PingStatus::Connecting,
                    duration: Duration::ZERO,
                    error: None,
                })
                .is_err()
            {
                break;
            }
            let result = ping_host(&host, &config);
            if tx.send(result).is_err() {
                break;
            }
        });
    }
    drop(tx);
    rx
}

fn ping_host(host: &Host, config: &Path) -> PingResult {
    let started = Instant::now();
    let check = if host.proxy_jump.is_empty() && host.proxy_command.is_empty() {
        direct_ping(host)
    } else {
        proxy_ping(host, config)
    };
    PingResult {
        name: host.name.clone(),
        status: if check.is_ok() {
            PingStatus::Online
        } else {
            PingStatus::Offline
        },
        duration: started.elapsed(),
        error: check.err().map(|e| format!("{e:#}")),
    }
}

fn direct_ping(host: &Host) -> Result<()> {
    let hostname = if host.hostname.is_empty() {
        &host.name
    } else {
        &host.hostname
    };
    let port = if host.port.is_empty() {
        22
    } else {
        host.port.parse::<u16>().context("invalid SSH port")?
    };
    let addresses = (hostname.as_str(), port)
        .to_socket_addrs()
        .context("cannot resolve host")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_error = None;
    for address in addresses {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match TcpStream::connect_timeout(&address, remaining) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                let mut session = ssh2::Session::new()?;
                session.set_tcp_stream(stream);
                session.set_timeout(2000);
                // As in SSHM, authentication is unnecessary for the reachability check.
                if let Err(error) = session.handshake() {
                    let text = error.to_string().to_lowercase();
                    if text.contains("timeout")
                        || text.contains("timed out")
                        || text.contains("connection refused")
                        || text.contains("no route to host")
                        || text.contains("network is unreachable")
                    {
                        return Err(error.into());
                    }
                }
                return Ok(());
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("host has no reachable address")))
}

fn proxy_ping(host: &Host, config: &Path) -> Result<()> {
    anyhow::ensure!(!host.name.starts_with('-'), "invalid SSH host name");
    let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut child = Command::new("ssh")
        .args([
            "-q",
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            &format!("UserKnownHostsFile={null_device}"),
            "-o",
            "ConnectTimeout=5",
            "-F",
        ])
        .arg(config)
        .arg(&host.name)
        .arg("exit")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::ensure!(
                status.success(),
                "SSH connection check exited with {}",
                exit_code(status)
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("SSH connection check timed out")
        }
        thread::sleep(Duration::from_millis(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_process_arguments_preserve_each_remote_argument() {
        let remote = vec!["ls".into(), "-la".into(), "/a b".into()];
        assert_eq!(
            ssh_arguments(Path::new("/tmp/a b"), "prod", true, &remote, None).unwrap(),
            ["-F", "/tmp/a b", "-t", "prod", "ls", "-la", "/a b"]
        );
    }
    #[test]
    fn forwarding_argument_precedes_host_and_never_requires_shell() {
        let f = Forward {
            kind: "dynamic".into(),
            local_port: "1080".into(),
            bind_address: "127.0.0.1".into(),
            ..Forward::default()
        };
        assert_eq!(
            ssh_arguments(Path::new("config"), "prod", false, &[], Some(&f)).unwrap(),
            ["-F", "config", "-D", "127.0.0.1:1080", "prod"]
        );
    }
}
