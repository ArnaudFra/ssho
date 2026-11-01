//! Native, bounded multi-host commands. The UI owns polling and history writes.
use crate::types::Host;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub const CAPTURE_LIMIT: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub alias: String,
    pub root: PathBuf,
    pub source: PathBuf,
}
impl Target {
    pub fn freeze(root: &Path, host: &Host) -> Result<Self> {
        super::ssh_arguments(root, &host.name, false, &[], None)?;
        anyhow::ensure!(!host.name.is_empty(), "empty SSH target");
        Ok(Self {
            alias: host.name.clone(),
            root: root.canonicalize().context("freeze SSH root")?,
            source: host
                .source_file
                .canonicalize()
                .context("freeze target source")?,
        })
    }
    fn validate(&self) -> Result<()> {
        let host = crate::config::find_host(&self.root, &self.alias)
            .context("batch target no longer exists")?;
        anyhow::ensure!(
            host.source_file.canonicalize()? == self.source,
            "batch target source changed; clear and mark it again"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub parallel: usize,
    pub timeout: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            parallel: 4,
            timeout: Duration::from_secs(30),
        }
    }
}
impl Options {
    pub fn validate(self) -> Result<()> {
        anyhow::ensure!(
            (1..=16).contains(&self.parallel),
            "parallel connections must be 1–16"
        );
        anyhow::ensure!(
            (Duration::from_secs(1)..=Duration::from_secs(3600)).contains(&self.timeout),
            "timeout must be 1–3600 seconds"
        );
        Ok(())
    }
}
pub fn arguments(target: &Target, remote: &str) -> Result<Vec<String>> {
    let mut args =
        super::ssh_arguments(&target.root, &target.alias, false, &[remote.into()], None)?;
    // Command-line values win over config, including inherited RemoteCommand and -f/-N equivalents.
    let options = [
        "BatchMode=yes",
        "RequestTTY=no",
        "RemoteCommand=none",
        "ControlMaster=no",
        "ControlPath=none",
        "ControlPersist=no",
        "ForkAfterAuthentication=no",
        "SessionType=default",
        "ClearAllForwardings=yes",
        "StdinNull=yes",
        "PermitLocalCommand=no",
    ];
    args.splice(
        0..0,
        options
            .iter()
            .flat_map(|o| ["-o".to_owned(), o.to_string()]),
    );
    Ok(args)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Pending,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}
impl State {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => crate::t!("Pending", "En attente"),
            Self::Running => crate::t!("Running", "En cours"),
            Self::Succeeded => crate::t!("Succeeded", "Réussi"),
            Self::Failed => crate::t!("Failed", "Échec"),
            Self::TimedOut => crate::t!("Timed out", "Délai dépassé"),
            Self::Cancelled => crate::t!("Cancelled", "Annulé"),
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Output {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}
impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
    fn append(&mut self, bytes: &[u8]) {
        let keep = bytes
            .len()
            .min(CAPTURE_LIMIT.saturating_sub(self.bytes.len()));
        self.bytes.extend_from_slice(&bytes[..keep]);
        self.truncated |= keep < bytes.len();
    }
}
#[derive(Clone, Debug)]
pub struct Record {
    pub target: Target,
    pub state: State,
    pub stdout: Output,
    pub stderr: Output,
    pub exit_code: Option<i32>,
    pub diagnostic: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug)]
pub enum Event {
    Started { index: usize, at: DateTime<Utc> },
    Finished { index: usize, at: DateTime<Utc> },
}
struct Running {
    index: usize,
    process: OwnedChild,
    deadline: Instant,
}
/// A manager owns a single batch; start refuses to replace pending or running work.
pub struct Manager {
    program: PathBuf,
    records: Vec<Record>,
    running: Vec<Running>,
    options: Options,
    command: String,
    events: Vec<Event>,
}
impl Default for Manager {
    fn default() -> Self {
        Self::with_program("ssh".into())
    }
}
impl Manager {
    /// Inject a native executable for isolated tests; no local shell is involved.
    pub fn with_program(program: PathBuf) -> Self {
        Self {
            program,
            records: vec![],
            running: vec![],
            options: Options::default(),
            command: String::new(),
            events: vec![],
        }
    }
    pub fn records(&self) -> &[Record] {
        &self.records
    }
    pub fn has_active(&self) -> bool {
        self.records
            .iter()
            .any(|r| matches!(r.state, State::Pending | State::Running))
    }
    pub fn start(&mut self, targets: Vec<Target>, command: String, options: Options) -> Result<()> {
        anyhow::ensure!(!self.has_active(), "a batch is already active");
        options.validate()?;
        anyhow::ensure!(
            !command.trim().is_empty() && !command.contains('\0'),
            "remote command is empty or contains NUL"
        );
        anyhow::ensure!(!targets.is_empty(), "mark at least one target first");
        let mut seen = BTreeSet::new();
        let mut records = vec![];
        for target in targets {
            arguments(&target, &command)?;
            if seen.insert((target.root.clone(), target.alias.clone())) {
                records.push(Record {
                    target,
                    state: State::Pending,
                    stdout: Output::default(),
                    stderr: Output::default(),
                    exit_code: None,
                    diagnostic: String::new(),
                    started_at: None,
                    completed_at: None,
                });
            }
        }
        self.records = records;
        self.options = options;
        self.command = command;
        self.events.clear();
        Ok(())
    }
    fn complete(&mut self, index: usize, state: State, diagnostic: String) {
        let at = Utc::now();
        let record = &mut self.records[index];
        record.state = state;
        record.diagnostic = diagnostic;
        record.completed_at = Some(at);
        self.events.push(Event::Finished { index, at });
    }
    pub fn poll(&mut self) -> Vec<Event> {
        let mut slot = 0;
        while slot < self.running.len() {
            let job = &mut self.running[slot];
            let record = &mut self.records[job.index];
            let read = job.process.drain(record);
            let observation = job.process.exited();
            let outcome = if let Err(error) = read {
                Some((State::Failed, format!("capture: {error}")))
            } else if let Err(error) = observation {
                Some((State::Failed, format!("observe: {error}")))
            } else if observation.unwrap_or(false) {
                Some((State::Succeeded, String::new()))
            } else if Instant::now() >= job.deadline {
                Some((
                    State::TimedOut,
                    "local SSH timeout; remote detached work may continue".into(),
                ))
            } else {
                None
            };
            if let Some((mut state, mut diagnostic)) = outcome {
                let mut job = self.running.swap_remove(slot);
                match job.process.finish() {
                    Ok(status) => {
                        self.records[job.index].exit_code = Some(super::exit_code(status))
                    }
                    Err(error) => {
                        state = State::Failed;
                        diagnostic = error.to_string();
                    }
                }
                if let Err(error) = job.process.drain(&mut self.records[job.index]) {
                    state = State::Failed;
                    diagnostic = error.to_string();
                }
                if state == State::Succeeded && self.records[job.index].exit_code != Some(0) {
                    state = State::Failed;
                }
                self.complete(job.index, state, diagnostic);
            } else {
                slot += 1;
            }
        }
        // Include rejected targets in the budget: stale selections must not monopolize the UI.
        let mut attempted = 0;
        while self.running.len() < self.options.parallel && attempted < 16 {
            let Some(index) = self.records.iter().position(|r| r.state == State::Pending) else {
                break;
            };
            attempted += 1;
            // A stale marked alias must not silently switch to another source after F5.
            if let Err(error) = self.records[index].target.validate() {
                self.complete(index, State::Failed, format!("{error:#}"));
                continue;
            }
            let at = Utc::now();
            self.records[index].started_at = Some(at);
            self.events.push(Event::Started { index, at });
            let result = arguments(&self.records[index].target, &self.command)
                .and_then(|args| OwnedChild::spawn(&self.program, &args));
            match result {
                Ok(process) => {
                    self.records[index].state = State::Running;
                    self.running.push(Running {
                        index,
                        process,
                        deadline: Instant::now() + self.options.timeout,
                    });
                }
                Err(error) => self.complete(
                    index,
                    State::Failed,
                    format!("launch native SSH: {error:#}"),
                ),
            }
        }
        std::mem::take(&mut self.events)
    }
    pub fn cancel(&mut self) {
        while let Some(mut job) = self.running.pop() {
            let mut diagnostic =
                "local SSH cancelled; remote detached work may continue".to_owned();
            match job.process.finish() {
                Ok(status) => self.records[job.index].exit_code = Some(super::exit_code(status)),
                Err(error) => diagnostic.push_str(&format!("; cleanup: {error}")),
            }
            if let Err(error) = job.process.drain(&mut self.records[job.index]) {
                diagnostic.push_str(&format!("; capture: {error}"));
            }
            self.complete(job.index, State::Cancelled, diagnostic);
        }
        for index in 0..self.records.len() {
            if self.records[index].state == State::Pending {
                self.complete(index, State::Cancelled, "cancelled before attempt".into());
            }
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Never reap the leader before the final group signal: its waitable PID pins the PGID.
struct OwnedChild {
    child: Child,
    group_owned: bool,
}
impl OwnedChild {
    fn spawn(program: &Path, args: &[String]) -> Result<Self> {
        #[cfg(not(unix))]
        anyhow::bail!("bounded batch process ownership requires Unix");
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::process::CommandExt};
            let child = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0)
                .spawn()?;
            let owned = Self {
                child,
                group_owned: true,
            };
            for fd in [
                owned.child.stdout.as_ref().unwrap().as_raw_fd(),
                owned.child.stderr.as_ref().unwrap().as_raw_fd(),
            ] {
                // SAFETY: live owned pipe FDs. O_NONBLOCK bounds every UI poll, even if a descendant retains a pipe.
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
                anyhow::ensure!(
                    flags >= 0
                        && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
                    "set nonblocking capture: {}",
                    std::io::Error::last_os_error()
                );
            }
            Ok(owned)
        }
    }
    fn drain(&mut self, record: &mut Record) -> std::io::Result<()> {
        fn read(reader: &mut impl Read, output: &mut Output) -> std::io::Result<()> {
            let mut buffer = [0; 8192];
            // Per-poll CPU bound as well as memory bound. Continue draining discarded bytes.
            for _ in 0..128 {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => output.append(&buffer[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }
        if let Some(pipe) = &mut self.child.stdout {
            read(pipe, &mut record.stdout)?;
        }
        if let Some(pipe) = &mut self.child.stderr {
            read(pipe, &mut record.stderr)?;
        }
        Ok(())
    }
    fn exited(&mut self) -> std::io::Result<bool> {
        #[cfg(unix)]
        loop {
            let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
            // SAFETY: valid buffer and the child exclusively owned by this guard.
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
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if e.raw_os_error() == Some(libc::ECHILD) {
                self.group_owned = false;
            }
            return Err(e);
        }
        #[cfg(not(unix))]
        Ok(self.child.try_wait()?.is_some())
    }
    fn finish(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let observed = self.exited();
        if self.group_owned {
            #[cfg(unix)]
            // SAFETY: unreaped owned leader pins this process group; ECHILD disarms it.
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            self.group_owned = false;
        }
        let status = self.child.wait();
        observed?;
        status
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.group_owned {
            let _ = self.finish();
        }
    }
}

#[cfg(all(test, unix))]
mod ownership_tests {
    use super::*;
    fn running_fixture() -> (tempfile::TempDir, Manager) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        std::fs::write(&root, "Host fixture\n HostName invalid.test\n").unwrap();
        let target = Target::freeze(
            &root,
            &Host {
                name: "fixture".into(),
                source_file: root.clone(),
                ..Host::default()
            },
        )
        .unwrap();
        let mut manager = Manager::with_program("true".into());
        manager
            .start(vec![target], "fixture".into(), Options::default())
            .unwrap();
        manager.poll();
        assert_eq!(manager.records()[0].state, State::Running);
        (temp, manager)
    }

    #[test]
    fn batch_cancel_preserves_cleanup_error_and_cancellation_explanation() {
        let (_temp, mut manager) = running_fixture();
        // Real ECHILD: an outside reaper has released the process before cancellation.
        manager.running[0].process.child.wait().unwrap();
        manager.cancel();
        let record = &manager.records()[0];
        let error = std::io::Error::from_raw_os_error(libc::ECHILD).to_string();
        assert_eq!(record.state, State::Cancelled);
        assert!(record.diagnostic.contains("local SSH cancelled"));
        assert!(
            record.diagnostic.contains(&format!("cleanup: {error}")),
            "{}",
            record.diagnostic
        );
        assert!(record.completed_at.is_some());
        assert!(matches!(
            manager.poll().as_slice(),
            [Event::Finished { index: 0, .. }]
        ));
    }

    #[test]
    fn batch_cancel_preserves_capture_error_and_cancellation_explanation() {
        use std::os::fd::OwnedFd;
        let (temp, mut manager) = running_fixture();
        // Reading a directory returns a real EISDIR, without closing/reusing a live pipe FD.
        let directory: OwnedFd = std::fs::File::open(temp.path()).unwrap().into();
        manager.running[0].process.child.stdout = Some(directory.into());
        manager.cancel();
        let record = &manager.records()[0];
        let error = std::io::Error::from_raw_os_error(libc::EISDIR).to_string();
        assert_eq!(record.state, State::Cancelled);
        assert!(record.diagnostic.contains("local SSH cancelled"));
        assert!(
            record.diagnostic.contains(&format!("capture: {error}")),
            "{}",
            record.diagnostic
        );
        assert!(record.exit_code.is_some());
    }

    #[test]
    fn batch_ownership_keeps_leader_waitable_and_disarms_echild() {
        use std::os::unix::process::CommandExt;
        let child = Command::new("sh")
            .args(["-c", "exit 37"])
            .process_group(0)
            .spawn()
            .unwrap();
        let mut owned = OwnedChild {
            child,
            group_owned: true,
        };
        let pid = owned.child.id();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !owned.exited().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
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
            0
        );
        assert_eq!(unsafe { info.assume_init().si_status() }, 37);
        assert_eq!(owned.finish().unwrap().code(), Some(37));
        assert!(!owned.group_owned);
        let child = Command::new("sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        let mut lost = OwnedChild {
            child,
            group_owned: true,
        };
        lost.child.wait().unwrap();
        assert_eq!(
            lost.exited().unwrap_err().raw_os_error(),
            Some(libc::ECHILD)
        );
        assert!(!lost.group_owned);
    }
}
