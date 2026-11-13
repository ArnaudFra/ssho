//! OpenSSH owns configuration precedence, includes, Match, and local defaults.
use super::*;
use crate::storage::AvailabilityMethod;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
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
    resolve_controlled(config, alias, OPERATION_TIMEOUT, Control::ssh(None))
}
fn resolve_controlled(
    config: &Path,
    alias: &str,
    timeout: Duration,
    control: Control<'_>,
) -> Result<EffectiveConfig> {
    // Reuse connection validation, but never run a connection for configuration resolution.
    super::ssh_arguments(config, alias, false, &[], None)?;
    let mut command = Command::new("ssh");
    command.arg("-G").arg("-F").arg(config).arg(alias);
    let (status, stdout, stderr) = capture_with(&mut command, timeout, control)
        .context("resolve configuration with ssh -G")?;
    anyhow::ensure!(
        status.success(),
        "ssh -G exited {}: {}",
        super::exit_code(status),
        excerpt(&stderr)
    );
    EffectiveConfig::parse(&stdout)
}

/// Limits of the native diagnostics, per operation and per session.
///
/// One operation: `OPERATION_TIMEOUT`, and `OUTPUT_LIMIT` per output stream.
/// One session: `WORKERS` operations at most run at once, hence twice as many
/// capture files. These files are anonymous: the system releases them as soon
/// as the operation ends, whatever its outcome.
pub(super) const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const OUTPUT_LIMIT: u64 = 256 * 1024;
pub(super) const WORKERS: usize = 8;
/// How often a running operation is checked for cancellation, deadline and size.
const POLL: Duration = Duration::from_millis(10);
/// Longest diagnostic text kept in an error message; the end is the useful part.
const EXCERPT: usize = 2000;
const TRUNCATED: &str = "\n[output truncated at 256 KiB]";

/// The useful end of a diagnostic text, with a visible mark when it was cut.
pub(super) fn excerpt(text: &str) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= EXCERPT {
        return text.to_owned();
    }
    let tail: String = text.chars().skip(count - EXCERPT).collect();
    format!("[beginning truncated] {tail}")
}

/// How one native operation is supervised.
#[derive(Clone, Copy)]
pub(super) struct Control<'a> {
    /// Checked at every poll: a refresh or the end of the session stops the
    /// operation and its whole process group at once.
    pub cancel: Option<&'a AtomicBool>,
    /// Kernel-enforced size limit on every file the child writes. Only for a
    /// tool this application fully owns (ping): an SSH proxy helper or a
    /// `Match exec` command may legitimately append to its own log file.
    pub hard_limit: bool,
    pub label: &'static str,
}
impl<'a> Control<'a> {
    pub(super) fn ssh(cancel: Option<&'a AtomicBool>) -> Self {
        Self {
            cancel,
            hard_limit: false,
            label: "SSH command",
        }
    }
    pub(super) fn owned_tool(cancel: &'a AtomicBool) -> Self {
        Self {
            cancel: Some(cancel),
            hard_limit: true,
            label: "native diagnostic",
        }
    }
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
    capture_with(command, timeout, Control::ssh(None))
}

/// ICMP entry point: cancellation and a hard per-stream file limit.
pub(super) fn capture_controlled(
    command: &mut Command,
    timeout: Duration,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<(std::process::ExitStatus, String, String)> {
    match cancel {
        Some(cancel) => capture_with(command, timeout, Control::owned_tool(cancel)),
        None => capture_with(command, timeout, Control::ssh(None)),
    }
}

/// Every native operation goes through here: owned process group, deadline,
/// cancellation, and an output that is watched while the operation runs.
pub(super) fn capture_with(
    command: &mut Command,
    timeout: Duration,
    control: Control<'_>,
) -> Result<(std::process::ExitStatus, String, String)> {
    use std::sync::atomic::Ordering;
    let Control {
        cancel,
        hard_limit,
        label,
    } = control;
    anyhow::ensure!(
        !cancel.is_some_and(|c| c.load(Ordering::Relaxed)),
        "{label} cancelled"
    );
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
        if hard_limit {
            // SAFETY: only async-signal-safe setrlimit is used between fork and exec.
            unsafe {
                command.pre_exec(|| {
                    let limit = libc::rlimit {
                        rlim_cur: OUTPUT_LIMIT as libc::rlim_t,
                        rlim_max: OUTPUT_LIMIT as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
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
    let mut cancelled = false;
    // A stream that keeps growing is stopped when it is noticed, not at the deadline.
    let oversized =
        |file: &std::fs::File| file.metadata().is_ok_and(|data| data.len() >= OUTPUT_LIMIT);
    let mut flooded = false;
    let (status, timed_out) = loop {
        #[cfg(unix)]
        if child.exited()? {
            break (child.finish()?, false);
        }
        #[cfg(not(unix))]
        if let Some(status) = child.try_wait()? {
            break (status, false);
        }
        cancelled = cancel.is_some_and(|c| c.load(Ordering::Relaxed));
        flooded = oversized(&stdout) || oversized(&stderr);
        if cancelled || flooded || Instant::now() >= deadline {
            #[cfg(unix)]
            break (child.finish()?, true);
            #[cfg(not(unix))]
            {
                let _ = child.kill();
                break (child.wait()?, true);
            }
        }
        thread::sleep(POLL);
    };
    /// At most `OUTPUT_LIMIT` bytes, with a visible mark when the stream held more.
    fn read(file: &mut std::fs::File) -> Result<String> {
        let truncated = file.metadata()?.len() >= OUTPUT_LIMIT;
        let keep = if truncated {
            OUTPUT_LIMIT - TRUNCATED.len() as u64
        } else {
            OUTPUT_LIMIT
        };
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(keep).read_to_end(&mut bytes)?;
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if truncated {
            text.push_str(TRUNCATED);
        }
        Ok(text)
    }
    anyhow::ensure!(!cancelled, "{label} cancelled");
    // Stopped while running, or a tool whose every byte is under a hard limit.
    let over = oversized(&stdout) || oversized(&stderr);
    anyhow::ensure!(
        !(flooded || hard_limit && over),
        "{label} output limit (256 KiB per stream) reached: the process was stopped"
    );
    let stderr = read(&mut stderr)?;
    anyhow::ensure!(!timed_out, "{label} timed out: {}", excerpt(&stderr));
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
    method: AvailabilityMethod,
    cancel: Arc<AtomicBool>,
    sender: mpsc::Sender<Inspection>,
}
#[derive(Default)]
struct Work {
    /// Jobs that may own a native child: waited for when the session ends.
    active_native: usize,
    cancel: Arc<AtomicBool>,
    queue: VecDeque<Job>,
    stopped: bool,
}

/// Registered under the queue lock before a job can leave it. Every return and
/// unwind releases the count after the owned native child has been cleaned up.
struct ActiveNative(Option<Arc<(Mutex<Work>, std::sync::Condvar)>>);
impl Drop for ActiveNative {
    fn drop(&mut self) {
        if let Some(work) = &self.0 {
            let (lock, ready) = &**work;
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            state.active_native -= 1;
            ready.notify_all();
        }
    }
}

/// One fixed pool per browser; refresh replaces queued work without spawning more workers.
/// Results use a new receiver each refresh, so stale generations cannot enter the UI cache.
pub struct Inspector {
    work: Arc<(Mutex<Work>, std::sync::Condvar)>,
}
impl Default for Inspector {
    fn default() -> Self {
        let work = Arc::new((Mutex::new(Work::default()), std::sync::Condvar::new()));
        for _ in 0..WORKERS {
            let work = Arc::clone(&work);
            thread::spawn(move || loop {
                let (job, mut active) = {
                    let (lock, ready) = &*work;
                    let Ok(mut state) = lock.lock() else { return };
                    while state.queue.is_empty() && !state.stopped {
                        let Ok(next) = ready.wait(state) else { return };
                        state = next;
                    }
                    if state.stopped {
                        return;
                    }
                    let job = state.queue.pop_front().expect("nonempty job queue");
                    // Registered under the queue lock, whatever the method: an
                    // SSH diagnostic owns ssh processes just as ICMP owns ping.
                    state.active_native += 1;
                    (job, ActiveNative(Some(Arc::clone(&work))))
                };
                let Job {
                    host,
                    config,
                    probe,
                    method,
                    cancel,
                    sender,
                } = job;
                if cancel.load(Ordering::Relaxed) {
                    continue;
                }
                let started = Instant::now();
                let deadline = started + OPERATION_TIMEOUT;
                // Both methods resolve under the same deadline and cancellation;
                // only a tool this application owns gets the hard file limit.
                let effective = resolve_controlled(
                    &config,
                    &host.name,
                    deadline.saturating_duration_since(Instant::now()),
                    if method == AvailabilityMethod::Icmp {
                        Control::owned_tool(&cancel)
                    } else {
                        Control::ssh(Some(&cancel))
                    },
                )
                .map_err(|error| format!("{error:#}"));
                if cancel.load(Ordering::Relaxed) {
                    continue;
                }
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
                        Ok(value) if method == AvailabilityMethod::Icmp => {
                            let mut ping = super::availability::probe(
                                &value.host(&host),
                                deadline.saturating_duration_since(Instant::now()),
                                &cancel,
                            );
                            ping.duration = started.elapsed();
                            ping
                        }
                        Ok(value) => {
                            let target = value.host(&host);
                            if !super::via_proxy(&target) {
                                // A direct handshake owns no process: the end of
                                // the session does not have to wait for it.
                                drop(std::mem::replace(&mut active, ActiveNative(None)));
                            }
                            super::ping_host(&target, &config, Some(&cancel))
                        }
                        Err(error) => PingResult {
                            method,
                            name: host.name.clone(),
                            status: PingStatus::Unknown,
                            duration: if method == AvailabilityMethod::Icmp {
                                started.elapsed()
                            } else {
                                Duration::ZERO
                            },
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
    /// Compatibility entry point: CLI and existing callers keep SSH diagnostics.
    pub fn inspect(&self, hosts: Vec<Host>, config: PathBuf, probe: bool) -> Receiver<Inspection> {
        self.inspect_with_method(hosts, config, probe, AvailabilityMethod::Ssh)
    }
    pub fn inspect_with_method(
        &self,
        hosts: Vec<Host>,
        config: PathBuf,
        probe: bool,
        method: AvailabilityMethod,
    ) -> Receiver<Inspection> {
        let (sender, receiver) = mpsc::channel();
        let (lock, ready) = &*self.work;
        if let Ok(mut state) = lock.lock() {
            state.cancel.store(true, Ordering::Relaxed);
            state.cancel = Arc::new(AtomicBool::new(false));
            state.queue = hosts
                .into_iter()
                .map(|host| Job {
                    host,
                    config: config.clone(),
                    probe,
                    method,
                    cancel: Arc::clone(&state.cancel),
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
            state.cancel.store(true, Ordering::Relaxed);
            state.stopped = true;
            state.queue.clear();
            ready.notify_all();
            // The terminal guard is restored before Inspector is dropped. Waiting
            // here prevents process exit from abandoning a worker that still owns
            // a native child (ping, ssh -G, proxy check). Cancellation is seen
            // within one poll, so this wait is short and never indefinite.
            while state.active_native > 0 {
                state = ready.wait(state).unwrap_or_else(|error| error.into_inner());
            }
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
            let deadline = Instant::now() + Duration::from_secs(3);
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
        // The command writes more than the limit and exits at once. Depending on
        // whether the watchdog looks before or after the exit, the output is cut
        // with its status kept, or the process is stopped for exceeding the limit.
        match capture(&mut command, Duration::from_secs(2)) {
            Ok((status, _, stderr)) => {
                assert_eq!(status.code(), Some(37));
                assert!(stderr.len() <= 256 * 1024);
                // The cut is visible instead of silent, and the exit status is kept.
                assert!(stderr.ends_with("[output truncated at 256 KiB]"), "tail");
            }
            Err(error) => assert!(
                error
                    .to_string()
                    .contains("output limit (256 KiB per stream) reached"),
                "{error:#}"
            ),
        }
        let mut command = Command::new("sh");
        command.args(["-c", "echo timeout-cause >&2; sleep 30 & wait"]);
        let started = Instant::now();
        let error = capture(&mut command, Duration::from_millis(100)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(error.to_string().contains("timed out: timeout-cause"));
    }
}

#[cfg(all(test, unix))]
mod bounded_diagnostic_tests {
    use super::*;

    #[test]
    fn runaway_output_is_stopped_when_noticed_not_at_the_deadline() {
        for stream in ["", " >&2"] {
            let mut command = Command::new("sh");
            command.args(["-c", &format!("while :; do echo flood{stream}; done")]);
            let started = Instant::now();
            let error = capture(&mut command, Duration::from_secs(30)).unwrap_err();
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "stopped long before the 30 s deadline: {:?}",
                started.elapsed()
            );
            let message = error.to_string();
            assert!(
                message
                    .contains("output limit (256 KiB per stream) reached: the process was stopped"),
                "{message}"
            );
        }
    }

    #[test]
    fn an_ssh_diagnostic_is_cancelled_at_once_and_says_so() {
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let waker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            stop.store(true, Ordering::Relaxed);
        });
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        let started = Instant::now();
        let error = capture_with(
            &mut command,
            Duration::from_secs(30),
            Control::ssh(Some(&cancel)),
        )
        .unwrap_err();
        waker.join().unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(error.to_string(), "SSH command cancelled");
        // A cancellation already requested starts nothing at all.
        let mut command = Command::new("sh");
        command.args(["-c", "exit 0"]);
        assert!(capture_with(
            &mut command,
            Duration::from_secs(1),
            Control::ssh(Some(&cancel))
        )
        .is_err());
    }

    #[test]
    fn an_ssh_helper_keeps_its_own_files_while_an_owned_tool_is_hard_limited() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("helper.log");
        let script = format!(
            "head -c 400000 /dev/zero > '{}' && echo done",
            log.display()
        );
        // A proxy helper or a Match exec command may write its own large file.
        let mut command = Command::new("sh");
        command.args(["-c", &script]);
        let cancel = AtomicBool::new(false);
        let (status, stdout, _) = capture_with(
            &mut command,
            Duration::from_secs(5),
            Control::ssh(Some(&cancel)),
        )
        .unwrap();
        assert!(status.success());
        assert_eq!(stdout.trim(), "done");
        assert_eq!(std::fs::metadata(&log).unwrap().len(), 400_000);
        // A tool this application fully owns cannot write past the limit anywhere.
        std::fs::remove_file(&log).unwrap();
        let mut command = Command::new("sh");
        command.args(["-c", &script]);
        let outcome = capture_with(
            &mut command,
            Duration::from_secs(5),
            Control::owned_tool(&cancel),
        );
        assert!(std::fs::metadata(&log).unwrap().len() <= OUTPUT_LIMIT);
        assert!(
            !outcome.is_ok_and(|(status, stdout, _)| status.success() && stdout.trim() == "done")
        );
    }

    #[test]
    fn a_long_diagnostic_keeps_its_useful_end_with_a_visible_cut() {
        assert_eq!(excerpt("  short cause \n"), "short cause");
        let long = format!("{}final cause", "x".repeat(10_000));
        let cut = excerpt(&long);
        assert!(cut.starts_with("[beginning truncated] "), "{cut}");
        assert!(cut.ends_with("final cause"));
        assert!(cut.chars().count() <= EXCERPT + 30);
        // Multi-byte text is cut on character boundaries.
        let accents = "é".repeat(5_000);
        assert!(excerpt(&accents).ends_with('é'));
    }

    #[test]
    fn session_limits_are_stated_and_the_pool_never_exceeds_them() {
        // One session: at most WORKERS operations, hence twice as many capture
        // files of at most OUTPUT_LIMIT each while they are being watched.
        assert_eq!(WORKERS, 8);
        assert_eq!(OUTPUT_LIMIT, 256 * 1024);
        assert_eq!(OPERATION_TIMEOUT, Duration::from_secs(5));
        assert!(POLL <= Duration::from_millis(20));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_end_of_the_session_stops_ssh_diagnostics_without_waiting_for_them() {
        let temp = tempfile::tempdir().unwrap();
        let pid = temp.path().join("pid");
        let config = temp.path().join("config");
        // `ssh -G` waits for this Match exec command: a diagnostic that would
        // last until its deadline if nothing stopped it.
        std::fs::write(
            &config,
            format!(
                "Host slow\n HostName 127.0.0.1\nMatch exec \"sh -c 'echo $$ > {}; sleep 30'\"\n User fixture\n",
                pid.display()
            ),
        )
        .unwrap();
        let inspector = Inspector::default();
        let receiver = inspector.inspect_with_method(
            vec![Host {
                name: "slow".into(),
                ..Host::default()
            }],
            config,
            true,
            AvailabilityMethod::Ssh,
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while !pid.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(pid.exists(), "the diagnostic started its helper");
        let started = Instant::now();
        drop(inspector);
        // Ended by cancellation, well before the 5 s deadline of the operation.
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        assert!(
            receiver.try_recv().is_err(),
            "a cancelled diagnostic reports nothing"
        );
        let helper: i32 = std::fs::read_to_string(&pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // The whole group was killed, but a grandchild may take an instant to
        // die after its leader was reaped: wait for it instead of assuming it.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{helper}/stat"));
            let live = stat.is_ok_and(|stat| {
                !matches!(
                    stat.rsplit_once(") ").unwrap().1.chars().next(),
                    Some('Z' | 'X')
                )
            });
            if !live {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the helper of the cancelled diagnostic was stopped"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod availability_generation_tests {
    use super::*;
    use crate::storage::AvailabilityMethod;
    #[test]
    fn availability_inspector_method_and_generation_replace_stale_receiver() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("config");
        std::fs::write(&config, "Host old new\n HostName\n User fixture\n").unwrap();
        let inspector = Inspector::default();
        let old = inspector.inspect_with_method(
            vec![Host {
                name: "old".into(),
                ..Host::default()
            }],
            config.clone(),
            false,
            AvailabilityMethod::Ssh,
        );
        let old_result = old.recv_timeout(Duration::from_secs(2)).unwrap();
        let receiver = inspector.inspect_with_method(
            vec![Host {
                name: "new".into(),
                ..Host::default()
            }],
            config,
            true,
            AvailabilityMethod::Icmp,
        );
        let results: Vec<_> = receiver.into_iter().collect();
        assert!(results.iter().all(|result| result.name == "new"));
        let ping = results.into_iter().find_map(|result| result.ping).unwrap();
        assert_eq!(ping.method, AvailabilityMethod::Icmp);
        assert_eq!(ping.status, PingStatus::Unknown);
        assert!(
            !ping.duration.is_zero(),
            "failed resolution is still timed diagnostic work"
        );
        assert!(ping.error.unwrap().contains("ssh -G"));
        assert_eq!(old_result.name, "old");
    }
}
