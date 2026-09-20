//! OpenSSH owns configuration precedence, includes, Match, and local defaults.
use super::*;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EffectiveConfig {
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub identity_files: Vec<String>,
    pub proxy_jump: String,
    pub proxy_command: String,
    pub remote_command: String,
    pub request_tty: String,
    /// Preserve repeated and future directives without interpreting their syntax.
    pub directives: BTreeMap<String, Vec<String>>,
}

impl EffectiveConfig {
    pub fn parse(output: &str) -> Result<Self> {
        let mut directives: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for line in output.lines() {
            let Some((key, value)) = line.trim().split_once(char::is_whitespace) else {
                continue;
            };
            directives
                .entry(key.to_ascii_lowercase())
                .or_default()
                .push(value.trim().to_owned());
        }
        let first = |key: &str| {
            directives
                .get(key)
                .and_then(|v| v.first())
                .cloned()
                .unwrap_or_default()
        };
        let optional = |key: &str| {
            let value = first(key);
            if value == "none" {
                String::new()
            } else {
                value
            }
        };
        let result = Self {
            hostname: first("hostname"),
            user: first("user"),
            port: first("port"),
            identity_files: directives.get("identityfile").cloned().unwrap_or_default(),
            proxy_jump: optional("proxyjump"),
            proxy_command: optional("proxycommand"),
            remote_command: optional("remotecommand"),
            request_tty: first("requesttty"),
            directives,
        };
        anyhow::ensure!(
            !result.hostname.is_empty()
                && !result.user.is_empty()
                && result.port.parse::<u16>().is_ok_and(|p| p > 0),
            "ssh -G returned incomplete or invalid configuration"
        );
        Ok(result)
    }

    /// A display/probe projection; the declaration remains untouched for editing.
    pub fn host(&self, declared: &Host) -> Host {
        Host {
            hostname: self.hostname.clone(),
            user: self.user.clone(),
            port: self.port.clone(),
            identity: self.identity_files.first().cloned().unwrap_or_default(),
            extra_identities: self.identity_files.iter().skip(1).cloned().collect(),
            proxy_jump: self.proxy_jump.clone(),
            proxy_command: self.proxy_command.clone(),
            remote_command: self.remote_command.clone(),
            request_tty: self.request_tty.clone(),
            ..declared.clone()
        }
    }
}

pub fn resolve(config: &Path, alias: &str) -> Result<EffectiveConfig> {
    // Reuse connection validation, but never run a connection for configuration resolution.
    super::ssh_arguments(config, alias, false, &[], None)?;
    let mut command = Command::new("ssh");
    command.arg("-G").arg("-F").arg(config).arg(alias);
    let (status, stdout, stderr) = capture(&mut command, Duration::from_secs(5))
        .context("resolve configuration with ssh -G")?;
    anyhow::ensure!(
        status.success(),
        "ssh -G exited {}: {}",
        super::exit_code(status),
        stderr.trim()
    );
    EffectiveConfig::parse(&stdout)
}

/// Owns the child as well as its private process group. Keep the leader unreaped
/// until group cleanup so the kernel cannot recycle the numeric group identifier.
#[cfg(unix)]
struct DiagnosticChild {
    child: std::process::Child,
    group_owned: bool,
}
#[cfg(unix)]
impl DiagnosticChild {
    fn exited(&mut self) -> std::io::Result<bool> {
        loop {
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            // SAFETY: valid siginfo buffer, inspecting only the child owned by this guard.
            let rc = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id(),
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if rc == 0 {
                return Ok(unsafe { info.assume_init().si_pid() } != 0);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if error.raw_os_error() == Some(libc::ECHILD) {
                // An outside reaper removed ownership: never signal this numeric ID.
                self.group_owned = false;
            }
            return Err(error);
        }
    }
    fn finish(&mut self) -> std::io::Result<std::process::ExitStatus> {
        // Recheck ownership on timeout, error and unwind as well as normal completion.
        let observation = self.exited();
        if self.group_owned {
            // SAFETY: waitid has not reaped our leader, and ECHILD disarms this path.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            self.group_owned = false;
        }
        // Even an unexpected observation error must reap the child. Never signal again
        // once wait can release its PID. Child::wait retries interrupted waits itself.
        let status = self.child.wait();
        observation?;
        status
    }
}
#[cfg(unix)]
impl Drop for DiagnosticChild {
    fn drop(&mut self) {
        if self.group_owned {
            let _ = self.finish();
        }
    }
}

/// File-backed output avoids pipe deadlocks, even if a proxy retains inherited descriptors.
/// A private process group stops remaining proxy/Match children on every completion path.
pub(super) fn capture(
    command: &mut Command,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, String, String)> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn()?;
    #[cfg(unix)]
    let mut child = DiagnosticChild {
        child,
        group_owned: true,
    };
    #[cfg(not(unix))]
    let mut child = child;
    let deadline = Instant::now() + timeout;
    let (status, timed_out) = loop {
        #[cfg(unix)]
        if child.exited()? {
            break (child.finish()?, false);
        }
        #[cfg(not(unix))]
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            break (child.finish()?, true);
            #[cfg(not(unix))]
            {
                let _ = child.kill();
                break (child.wait()?, true);
            }
        }
        thread::sleep(Duration::from_millis(20));
    };
    fn read(file: &mut std::fs::File) -> Result<String> {
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(256 * 1024).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    let stderr = read(&mut stderr)?;
    anyhow::ensure!(!timed_out, "SSH command timed out: {}", stderr.trim());
    Ok((status, read(&mut stdout)?, stderr))
}

pub struct Inspection {
    pub name: String,
    pub effective: std::result::Result<EffectiveConfig, String>,
    pub ping: Option<PingResult>,
}

struct Job {
    host: Host,
    config: PathBuf,
    probe: bool,
    sender: mpsc::Sender<Inspection>,
}
#[derive(Default)]
struct Work {
    queue: VecDeque<Job>,
    stopped: bool,
}

/// One fixed pool per browser; refresh replaces queued work without spawning more workers.
/// Results use a new receiver each refresh, so stale generations cannot enter the UI cache.
pub struct Inspector {
    work: Arc<(Mutex<Work>, std::sync::Condvar)>,
}
impl Default for Inspector {
    fn default() -> Self {
        let work = Arc::new((Mutex::new(Work::default()), std::sync::Condvar::new()));
        for _ in 0..8 {
            let work = Arc::clone(&work);
            thread::spawn(move || loop {
                let job = {
                    let (lock, ready) = &*work;
                    let Ok(mut state) = lock.lock() else { return };
                    while state.queue.is_empty() && !state.stopped {
                        let Ok(next) = ready.wait(state) else { return };
                        state = next;
                    }
                    if state.stopped {
                        return;
                    }
                    state.queue.pop_front().expect("nonempty job queue")
                };
                let Job {
                    host,
                    config,
                    probe,
                    sender,
                } = job;
                let effective = resolve(&config, &host.name).map_err(|error| format!("{error:#}"));
                if sender
                    .send(Inspection {
                        name: host.name.clone(),
                        effective: effective.clone(),
                        ping: None,
                    })
                    .is_err()
                {
                    continue;
                }
                if probe {
                    let ping = match &effective {
                        Ok(value) => super::ping_host(&value.host(&host), &config),
                        Err(error) => PingResult {
                            name: host.name.clone(),
                            status: PingStatus::Unknown,
                            duration: Duration::ZERO,
                            error: Some(error.clone()),
                        },
                    };
                    let _ = sender.send(Inspection {
                        name: host.name,
                        effective,
                        ping: Some(ping),
                    });
                }
            });
        }
        Self { work }
    }
}
impl Inspector {
    pub fn inspect(&self, hosts: Vec<Host>, config: PathBuf, probe: bool) -> Receiver<Inspection> {
        let (sender, receiver) = mpsc::channel();
        let (lock, ready) = &*self.work;
        if let Ok(mut state) = lock.lock() {
            state.queue = hosts
                .into_iter()
                .map(|host| Job {
                    host,
                    config: config.clone(),
                    probe,
                    sender: sender.clone(),
                })
                .collect();
            ready.notify_all();
        }
        receiver
    }
}
impl Drop for Inspector {
    fn drop(&mut self) {
        let (lock, ready) = &*self.work;
        if let Ok(mut state) = lock.lock() {
            state.stopped = true;
            state.queue.clear();
            ready.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn diagnostic_child(script: &str) -> DiagnosticChild {
        use std::os::unix::process::CommandExt;
        let child = Command::new("sh")
            .args(["-c", script])
            .process_group(0)
            .spawn()
            .unwrap();
        DiagnosticChild {
            child,
            group_owned: true,
        }
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_observation_keeps_leader_waitable_until_group_cleanup() {
        let mut owned = diagnostic_child("exit 37");
        let pid = owned.child.id();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owned.exited().unwrap() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        // Independent kernel observation proves exited() did not release the PID.
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        assert_eq!(
            unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid,
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            },
            0,
            "leader must remain waitable while its private group can be signalled"
        );
        let info = unsafe { info.assume_init() };
        assert_eq!(unsafe { info.si_pid() }, pid as i32);
        assert_eq!(unsafe { info.si_status() }, 37);
        assert_eq!(owned.finish().unwrap().code(), Some(37));
        assert!(!owned.group_owned);
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_echild_disarms_group_and_unwind_reaps_owned_child() {
        let mut lost = diagnostic_child("exit 0");
        lost.child.wait().unwrap(); // Model an outside reaper releasing ownership.
        assert_eq!(
            lost.exited().unwrap_err().raw_os_error(),
            Some(libc::ECHILD)
        );
        assert!(
            !lost.group_owned,
            "ECHILD must disarm numeric group signalling"
        );
        drop(lost);
        let owned = diagnostic_child("sleep 30");
        let pid = owned.child.id();
        let unwind = std::panic::catch_unwind(move || {
            let _owned = owned;
            panic!("fixture unwind");
        });
        assert!(unwind.is_err());
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }

    #[test]
    fn parser_retains_repeated_directives_and_literal_command_spaces() {
        let value = EffectiveConfig::parse("hostname test\nuser deploy\nport 2222\nidentityfile /one key\nidentityfile /two\nproxycommand ssh -W %h:%p bastion\nfuture one\nfuture two\nignored\n").unwrap();
        assert_eq!(value.identity_files, ["/one key", "/two"]);
        assert_eq!(value.directives["future"], ["one", "two"]);
        assert_eq!(value.proxy_command, "ssh -W %h:%p bastion");
        assert!(EffectiveConfig::parse("hostname test").is_err());
    }

    #[test]
    fn inherited_proxy_failure_has_cause_and_background_resolution_without_probe() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config");
        std::fs::write(&path, "Host test\n HostName 127.0.0.1\nHost *\n User inherited\n Port 2209\n ProxyCommand sh -c 'echo proxy-fixture-failure >&2; exit 37'\n").unwrap();
        let host = Host {
            name: "test".into(),
            ..Host::default()
        };
        let inspector = Inspector::default();
        let receiver = inspector.inspect(vec![host.clone()], path.clone(), false);
        let update = receiver.recv_timeout(Duration::from_secs(6)).unwrap();
        assert!(update.ping.is_none());
        let effective = update.effective.unwrap();
        assert_eq!(effective.user, "inherited");
        assert_eq!(effective.port, "2209");
        assert_eq!(host.hostname, "");
        let update = inspector
            .inspect(vec![host], path, true)
            .into_iter()
            .find(|r| r.ping.is_some())
            .unwrap();
        let ping = update.ping.unwrap();
        assert_eq!(ping.status, PingStatus::Offline);
        assert!(ping.error.unwrap().contains("proxy-fixture-failure"));
    }

    // Linux /proc distinguishes a killed orphan waiting for init to reap it from a live child.
    #[cfg(target_os = "linux")]
    struct DescendantFixture(std::path::PathBuf);
    #[cfg(target_os = "linux")]
    impl DescendantFixture {
        fn pid(&self) -> i32 {
            std::fs::read_to_string(&self.0)
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        }
        fn assert_stopped(&self) {
            let pid = self.pid();
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                let status = std::fs::read_to_string(format!("/proc/{pid}/stat"));
                let live = status.is_ok_and(|stat| {
                    !matches!(
                        stat.rsplit_once(") ").unwrap().1.chars().next(),
                        Some('Z' | 'X')
                    )
                });
                if !live {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "diagnostic descendant {pid} survived normal SSH completion"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    #[cfg(target_os = "linux")]
    impl Drop for DescendantFixture {
        fn drop(&mut self) {
            if let Ok(contents) = std::fs::read_to_string(&self.0) {
                if let Ok(pid) = contents.trim().parse::<i32>() {
                    if pid > 1 {
                        // SAFETY: pid was written by this isolated test's background sleep.
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    fn capture_completed_descendant(exit: i32) {
        let temp = tempfile::tempdir().unwrap();
        let fixture = DescendantFixture(temp.path().join("pid"));
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"; exit \"$2\"", "fixture"])
            .arg(&fixture.0)
            .arg(exit.to_string());
        let (status, _, _) = capture(&mut command, Duration::from_secs(2)).unwrap();
        assert_eq!(status.code(), Some(exit));
        fixture.assert_stopped();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn capture_success_cleans_up_background_descendant() {
        capture_completed_descendant(0);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn capture_failure_cleans_up_background_descendant() {
        capture_completed_descendant(37);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_resolution_cleans_up_match_exec_background_descendant() {
        let temp = tempfile::tempdir().unwrap();
        let fixture = DescendantFixture(temp.path().join("pid"));
        let config = temp.path().join("config");
        // tempfile's generated path has no shell metacharacters; fixture is never user config.
        std::fs::write(&config, format!("Host fixture\n HostName 127.0.0.1\nMatch exec \"sh -c 'sleep 30 & echo $! > {}'\"\n User fixture-user\n", fixture.0.display())).unwrap();
        let effective = resolve(&config, "fixture").unwrap();
        assert_eq!(effective.user, "fixture-user");
        fixture.assert_stopped();
    }

    #[cfg(unix)]
    #[test]
    fn capture_drains_large_diagnostics_and_kills_timeout_process_group() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "head -c 300000 /dev/zero >&2; echo diagnostic-tail >&2; exit 37",
        ]);
        let (status, _, stderr) = capture(&mut command, Duration::from_secs(2)).unwrap();
        assert_eq!(status.code(), Some(37));
        assert!(stderr.len() <= 256 * 1024);
        let mut command = Command::new("sh");
        command.args(["-c", "echo timeout-cause >&2; sleep 30 & wait"]);
        let started = Instant::now();
        let error = capture(&mut command, Duration::from_millis(100)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(error.to_string().contains("timed out: timeout-cause"));
    }
}
