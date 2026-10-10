use super::*;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};
#[derive(Clone, Debug)]
pub struct Entry {
    pub private: PathBuf,
    pub public: Option<PublicKey>,
    pub usages: Vec<String>,
    pub diagnostic: String,
}
pub const INVENTORY_LIMIT: usize = 256;

fn resolve(value: &str, home: &Path) -> Option<PathBuf> {
    if value.contains('%')
        || value.contains('$')
        || value.starts_with('~') && !value.starts_with("~/")
    {
        return None;
    }
    let path = if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(value)
    };
    path.is_absolute().then_some(path)
}
/// Nonrecursive inventory, capped at 256 candidates and eight seconds total.
/// Usages are declared literal references, not inferred effective OpenSSH matches.
pub fn inventory(home: &Path, hosts: &[Host], keygen: &Path, cancelled: &AtomicBool) -> Vec<Entry> {
    inventory_with_candidates(home, hosts, &[], keygen, cancelled)
}
/// Session-generated paths share the same candidate and fingerprint budgets.
/// Most recent generated paths have priority so the next operation remains available.
pub fn inventory_with_candidates(
    home: &Path,
    hosts: &[Host],
    session: &[PathBuf],
    keygen: &Path,
    cancelled: &AtomicBool,
) -> Vec<Entry> {
    let mut candidates = BTreeMap::<PathBuf, Entry>::new();
    let mut add = |path: PathBuf, usage: Option<&str>, diagnostic: String| {
        if candidates.len() >= INVENTORY_LIMIT && !candidates.contains_key(&path) {
            return;
        }
        let entry = candidates.entry(path.clone()).or_insert(Entry {
            private: path,
            public: None,
            usages: vec![],
            diagnostic,
        });
        if let Some(name) = usage {
            if !entry.usages.iter().any(|s| s == name) {
                entry.usages.push(name.into());
            }
        }
    };
    for path in session.iter().rev().take(INVENTORY_LIMIT) {
        add(path.clone(), None, String::new());
    }
    // References take priority over optional directory discoveries.
    for host in hosts {
        for value in std::iter::once(&host.identity)
            .chain(&host.extra_identities)
            .filter(|s| !s.is_empty())
        {
            match resolve(value, home) {
                Some(path) => add(path, Some(&host.name), String::new()),
                None => add(
                    PathBuf::from(value),
                    Some(&host.name),
                    "unresolved identity (%/variable/relative path); no guessed match".into(),
                ),
            }
        }
    }
    if let Ok(files) = fs::read_dir(home.join(".ssh")) {
        for file in files.take(1024).flatten() {
            let path = file.path();
            if path.extension().is_some_and(|s| s == "pub") {
                if let Some(stem) = path.file_stem() {
                    add(path.with_file_name(stem), None, String::new());
                }
            } else if path
                .file_name()
                .is_some_and(|s| s.to_string_lossy().starts_with("id_"))
                && path.is_file()
            {
                add(path, None, String::new());
            }
        }
    }
    let started = Instant::now();
    let mut entries: Vec<_> = candidates.into_values().collect();
    for entry in &mut entries {
        if !entry.diagnostic.is_empty() {
            continue;
        }
        if cancelled.load(Ordering::Relaxed) || started.elapsed() >= Duration::from_secs(8) {
            entry.diagnostic =
                "inventory budget reached; refresh to retry (256 candidates / 8 seconds)".into();
            continue;
        }
        match review_public(&public_path(&entry.private), keygen) {
            Ok(public) => entry.public = Some(public),
            Err(error) => entry.diagnostic = format!("{error:#}"),
        }
    }
    if entries.len() == INVENTORY_LIMIT {
        if let Some(last) = entries.last_mut() {
            last.diagnostic.push_str(" • inventory capped at 256 keys");
        }
    }
    entries
}
type ScanJob = (Receiver<Vec<Entry>>, JoinHandle<()>, Arc<AtomicBool>);

/// Exactly one worker; repeated refresh requests while busy are ignored.
#[derive(Default)]
pub struct Scanner {
    running: Option<ScanJob>,
}
impl Scanner {
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }
    pub fn start(&mut self, home: PathBuf, hosts: Vec<Host>, session: Vec<PathBuf>) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker = thread::spawn(move || {
            let entries = inventory_with_candidates(
                &home,
                &hosts,
                &session,
                Path::new("ssh-keygen"),
                &worker_cancel,
            );
            let _ = tx.send(entries);
        });
        self.running = Some((rx, worker, cancel));
    }
    pub fn poll(&mut self) -> Option<Vec<Entry>> {
        let result = self.running.as_ref()?.0.try_recv();
        match result {
            Ok(entries) => {
                if let Some((_, worker, _)) = self.running.take() {
                    let _ = worker.join();
                }
                Some(entries)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some((_, worker, _)) = self.running.take() {
                    let _ = worker.join();
                }
                Some(vec![])
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}
impl Drop for Scanner {
    fn drop(&mut self) {
        if let Some((_, worker, cancel)) = self.running.take() {
            cancel.store(true, Ordering::Relaxed);
            let _ = worker.join();
        }
    }
}
