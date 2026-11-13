//! Owned background SSH processes. Active means process alive, never forwarding readiness.
use super::*;
use std::{
    collections::BTreeMap,
    io::Read,
    process::{Child, ChildStderr},
    sync::atomic::{AtomicBool, Ordering},
};

const DIAGNOSTIC_LIMIT: usize = 32 * 1024;
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub config: PathBuf,
    pub host: String,
    pub profile: Option<String>,
    pub forward: Forward,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Active,
    Completed,
    Failed,
    Stopped,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub id: u64,
    pub snapshot: Snapshot,
    pub state: State,
    pub exit_code: Option<i32>,
    pub diagnostic: String,
}
fn diagnostic(tail: &VecDeque<u8>) -> String {
    let mut text: String = String::from_utf8_lossy(&tail.iter().copied().collect::<Vec<_>>())
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut start = text.len().saturating_sub(DIAGNOSTIC_LIMIT);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text.drain(..start);
    text
}
// Drain independently of the UI: a foreground shell handoff can last indefinitely.
// The pipe is nonblocking, retained output is bounded, cancellation never joins a thread.
struct Capture {
    tail: Arc<Mutex<VecDeque<u8>>>,
    cancel: Arc<AtomicBool>,
    finished: Receiver<()>,
}
impl Capture {
    #[cfg_attr(not(unix), allow(dead_code))]
    fn start(mut stderr: ChildStderr) -> std::io::Result<Self> {
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, finished) = mpsc::channel();
        let output = Arc::clone(&tail);
        let stop = Arc::clone(&cancel);
        thread::Builder::new()
            .name("tunnel-stderr".into())
            .spawn(move || {
                loop {
                    let stopping = stop.load(Ordering::Acquire);
                    let mut eof = false;
                    for _ in 0..32 {
                        let mut buf = [0; 4096];
                        match stderr.read(&mut buf) {
                            Ok(0) => {
                                eof = true;
                                break;
                            }
                            Ok(n) => {
                                if let Ok(mut tail) = output.lock() {
                                    tail.extend(&buf[..n]);
                                    let excess = tail.len().saturating_sub(DIAGNOSTIC_LIMIT);
                                    tail.drain(..excess);
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => {
                                eof = true;
                                break;
                            }
                        }
                    }
                    if eof || stopping {
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                let _ = sender.send(());
            })?;
        Ok(Self {
            tail,
            cancel,
            finished,
        })
    }
    fn diagnostic(&self) -> String {
        self.tail
            .try_lock()
            .map(|tail| diagnostic(&tail))
            .unwrap_or_default()
    }
    fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
        // Descendants may retain pipe descriptors. Never block waiting for EOF or join.
        let _ = self.finished.recv_timeout(Duration::from_millis(100));
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
struct Owned {
    child: Child,
    capture: Capture,
}
impl Owned {
    fn terminate(&mut self) {
        #[cfg(unix)]
        // SAFETY: the live, unreaped child owns its private group. Called exactly once
        // before wait releases that PID; no persisted or external PID is accepted.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(not(unix))]
        let _ = self.child.kill();
    }
    #[cfg(unix)]
    fn exited(&self) -> std::io::Result<bool> {
        // Observe without reaping, keeping the group leader's PID reserved until cleanup.
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let rc = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init().si_pid() } != 0)
    }
    #[cfg(not(unix))]
    fn exited(&mut self) -> std::io::Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
}
pub struct Manager {
    #[cfg_attr(not(unix), allow(dead_code))]
    program: PathBuf,
    records: Vec<Record>,
    owned: BTreeMap<u64, Owned>,
    next: u64,
}
impl Default for Manager {
    fn default() -> Self {
        Self::with_program("ssh".into())
    }
}
impl Manager {
    pub fn with_program(program: PathBuf) -> Self {
        Self {
            program,
            records: vec![],
            owned: BTreeMap::new(),
            next: 1,
        }
    }
    pub fn records(&self) -> &[Record] {
        &self.records
    }
    pub fn has_active(&self) -> bool {
        !self.owned.is_empty()
    }
    pub fn start(&mut self, mut snapshot: Snapshot) -> Result<u64> {
        let id = self.next;
        self.next += 1;
        self.records.push(Record {
            id,
            snapshot: snapshot.clone(),
            state: State::Active,
            exit_code: None,
            diagnostic: String::new(),
        });
        let result: Result<Owned> = (|| {
            crate::config::find_host(&snapshot.config, &snapshot.host)?;
            snapshot.config = std::fs::canonicalize(&snapshot.config)?;
            let mut args = super::ssh_arguments(
                &snapshot.config,
                &snapshot.host,
                false,
                &[],
                Some(&snapshot.forward),
            )?;
            args.splice(
                0..0,
                [
                    "-N",
                    "-T",
                    "-o",
                    "ExitOnForwardFailure=yes",
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "RemoteCommand=none",
                    "-o",
                    "RequestTTY=no",
                    "-o",
                    "ForkAfterAuthentication=no",
                    "-o",
                    "ControlMaster=no",
                    "-o",
                    "ControlPath=none",
                ]
                .map(String::from),
            );
            self.spawn(&args)
        })();
        self.records.last_mut().unwrap().snapshot = snapshot;
        match result {
            Ok(owned) => {
                self.owned.insert(id, owned);
                Ok(id)
            }
            Err(e) => {
                let record = self.records.last_mut().unwrap();
                record.state = State::Failed;
                record.diagnostic = format!("{e:#}");
                Err(e)
            }
        }
    }
    #[cfg_attr(not(unix), allow(unused_variables))]
    fn spawn(&self, args: &[String]) -> Result<Owned> {
        #[cfg(not(unix))]
        anyhow::bail!("owned background tunnels currently require Unix");
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::process::CommandExt};
            let mut child = Command::new(&self.program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .process_group(0)
                .spawn()
                .context("launch tunnel SSH")?;
            let stderr = child.stderr.take().expect("piped stderr");
            let fd = stderr.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                let error = std::io::Error::last_os_error();
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                return Err(error.into());
            }
            match Capture::start(stderr) {
                Ok(capture) => Ok(Owned { child, capture }),
                Err(error) => {
                    unsafe {
                        libc::kill(-(child.id() as i32), libc::SIGKILL);
                    }
                    let _ = child.wait();
                    Err(error.into())
                }
            }
        }
    }
    pub fn poll(&mut self) {
        let mut finished = vec![];
        for (&id, owned) in &mut self.owned {
            self.records
                .iter_mut()
                .find(|r| r.id == id)
                .unwrap()
                .diagnostic = owned.capture.diagnostic();
            match owned.exited() {
                Ok(true) => finished.push((id, None)),
                Ok(false) => {}
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => finished.push((id, Some(e))),
            }
        }
        for (id, error) in finished {
            let _ = self.finish(id, false, error);
        }
    }
    pub fn stop(&mut self, id: u64) -> Result<()> {
        self.finish(id, true, None)
    }
    pub fn stop_all(&mut self) {
        let ids: Vec<_> = self.owned.keys().copied().collect();
        for id in ids {
            let _ = self.stop(id);
        }
    }
    fn finish(&mut self, id: u64, stopped: bool, error: Option<std::io::Error>) -> Result<()> {
        let Some(mut owned) = self.owned.remove(&id) else {
            return Ok(());
        };
        // An external SIGCHLD reaper could remove ownership; never signal a reused PID.
        if !error
            .as_ref()
            .is_some_and(|e| e.raw_os_error() == Some(libc::ECHILD))
        {
            owned.terminate();
        }
        let status = owned.child.wait();
        owned.capture.stop();
        let record = self.records.iter_mut().find(|r| r.id == id).unwrap();
        record.diagnostic = owned.capture.diagnostic();
        record.exit_code = status.as_ref().ok().map(|s| super::exit_code(*s));
        record.state = if stopped {
            State::Stopped
        } else if error.is_some() || status.as_ref().map_or(true, |s| !s.success()) {
            State::Failed
        } else {
            State::Completed
        };
        if let Some(error) = error {
            record.diagnostic.push_str(&format!("\n{error}"));
        }
        status.context("reap tunnel SSH")?;
        Ok(())
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        self.stop_all();
    }
}
