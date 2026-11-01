pub mod availability;
pub mod batch;
pub mod effective;
pub mod keys;
pub mod tunnels;
use crate::{
    storage::{AppConfig, AppPaths, History},
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
impl PingStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => crate::t!("? Unknown", "? Inconnu"),
            Self::Connecting => crate::t!("~ Connecting", "~ En cours"),
            Self::Online => crate::t!("+ Online", "+ En ligne"),
            Self::Offline => crate::t!("! Offline", "! Hors ligne"),
        }
    }
}
#[derive(Clone, Debug)]
pub struct PingResult {
    pub method: crate::storage::AvailabilityMethod,
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
    // Wrappers only own ordinary connections. Forwarding (including a shell)
    // retains native OpenSSH, like managed tunnels, resolution and diagnostics.
    let launcher = if forward.is_none() {
        AppConfig::load(&paths.root)?.ssh_command
    } else {
        vec!["ssh".into()]
    };
    if let Err(error) =
        History::load(&paths.root).and_then(|mut history| history.record(name, forward))
    {
        eprintln!("Warning: Could not record connection history: {error:#}");
    }
    if remote.is_empty() && forward.is_none() {
        eprintln!("Connecting to {name}...");
    }
    let result = Command::new(&launcher[0])
        .args(&launcher[1..])
        .args(arguments)
        .status()
        .with_context(|| {
            if forward.is_none() {
                format!("Error executing SSH command (ssh_command executable {:?}); check its path and arguments in config.json", launcher[0])
            } else {
                "Error executing SSH command; ensure OpenSSH is installed".into()
            }
        })
        .map(exit_code);
    if let Err(error) = History::load(&paths.root).and_then(|mut history| {
        history.complete(
            name,
            result.as_ref().ok().copied(),
            result.as_ref().err().map(|e| format!("{e:#}")),
        )
    }) {
        eprintln!("Warning: Could not record SSH process result: {error:#}");
    }
    result
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

fn ping_host(host: &Host, config: &Path) -> PingResult {
    let started = Instant::now();
    let check = if host.proxy_jump.is_empty() && host.proxy_command.is_empty() {
        direct_ping(host)
    } else {
        proxy_ping(host, config)
    };
    PingResult {
        method: crate::storage::AvailabilityMethod::Ssh,
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
                session.handshake().context("SSH handshake failed")?;
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
    let mut command = Command::new("ssh");
    command
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            &format!("UserKnownHostsFile={null_device}"),
            "-o",
            "ConnectTimeout=5",
            "-o",
            "RequestTTY=no",
            "-o",
            "RemoteCommand=none",
            "-F",
        ])
        .arg(config)
        .arg(&host.name)
        .arg("exit");
    let (status, _, stderr) = effective::capture(&mut command, Duration::from_secs(5))?;
    anyhow::ensure!(
        status.success(),
        "SSH proxy check exited {}: {}",
        exit_code(status),
        stderr.trim()
    );
    Ok(())
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

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn http_server_is_not_online_ssh() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        });
        let host = Host {
            name: "http".into(),
            hostname: "127.0.0.1".into(),
            port: port.to_string(),
            ..Host::default()
        };
        assert!(
            direct_ping(&host).is_err(),
            "an open HTTP port must not be Online SSH"
        );
        server.join().unwrap();
    }
}
