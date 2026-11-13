use super::parser::{absolute, quote, split_line};
use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use tempfile::NamedTempFile;

pub(super) static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
    pub(super) permissions: Option<fs::Permissions>,
    pub(super) access: PathBuf,
    access_state: Option<ConfigAccess>,
}
impl Snapshot {
    pub fn read(path: &Path) -> Result<Self> {
        let access = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let path = resolve_destination(&access)?;
        let access_state = match fs::metadata(&access) {
            Ok(metadata) => {
                anyhow::ensure!(
                    metadata.is_file(),
                    "SSH config {} is not a regular file",
                    access.display()
                );
                Some(ConfigAccess::read(&access)?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let snapshot = Self {
            bytes: access_state.as_ref().map(|_| fs::read(&path)).transpose()?,
            permissions: access_state.as_ref().map(|state| state.permissions.clone()),
            path,
            access,
            access_state,
        };
        snapshot.unchanged()?;
        Ok(snapshot)
    }
    pub fn access_description(&self) -> String {
        let metadata = match &self.access_state {
            #[cfg(unix)]
            Some(state) => {
                use std::os::unix::fs::PermissionsExt;
                format!(
                    "mode {:04o}, owner {}, group {}",
                    state.permissions.mode() & 0o7777,
                    state.owner,
                    state.group
                )
            }
            #[cfg(not(unix))]
            Some(state) => format!("permissions {:?}", state.permissions),
            None => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    format!(
                        "new file; mode {:04o}",
                        self.permissions
                            .as_ref()
                            .map_or(0o600, |p| p.mode() & 0o777)
                    )
                }
                #[cfg(not(unix))]
                {
                    "new file; recorded/private permissions".into()
                }
            }
        };
        format!(
            "{}\nDestination: {}\nExpected: {metadata}",
            self.access.display(),
            self.path.display()
        )
    }
    pub fn text(&self) -> Result<&str> {
        std::str::from_utf8(self.bytes.as_deref().unwrap_or_default())
            .context("SSH config is not UTF-8")
    }
    pub fn stage(&self, content: &[u8]) -> Result<NamedTempFile> {
        let parent = self
            .path
            .parent()
            .context("SSH config has no parent directory")?;
        if !parent.exists() {
            secure_mkdir(parent)?;
        }
        let mut file = NamedTempFile::new_in(parent)
            .with_context(|| format!("preparing {}", self.path.display()))?;
        if let Some(permissions) = &self.permissions {
            file.as_file().set_permissions(permissions.clone())?;
        } else {
            private_file(file.path())?;
        }
        file.write_all(content)?;
        file.as_file().sync_all()?;
        Ok(file)
    }
    pub fn unchanged(&self) -> Result<()> {
        let resolved = resolve_destination(&self.access).with_context(|| {
            format!(
                "SSH config access changed at {}; reload and retry",
                self.access.display()
            )
        })?;
        let actual = match fs::metadata(&self.access) {
            Ok(_) => Some(ConfigAccess::read(&self.access)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(
            resolved == self.path && actual == self.access_state,
            "SSH config access or permissions changed at {}; reload and retry",
            self.access.display()
        );

        let current = match fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if current != self.bytes {
            bail!(
                "SSH config {} changed while preparing the edit; reload and retry",
                self.path.display()
            );
        }
        Ok(())
    }
    pub fn commit(&self, staged: NamedTempFile) -> Result<()> {
        self.unchanged()?;
        if self.bytes.is_some() {
            staged
                .persist(&self.path)
                .map_err(|error| error.error)
                .with_context(|| format!("replacing {}", self.path.display()))?;
        } else {
            staged
                .persist_noclobber(&self.path)
                .map_err(|error| error.error)
                .with_context(|| format!("creating {}", self.path.display()))?;
        }
        Ok(())
    }
    pub fn backup(&self, app_dir: &Path) -> Result<()> {
        let Some(bytes) = &self.bytes else {
            return Ok(());
        };
        super::backups::record(self, app_dir)?;
        let backup_dir = app_dir.join("backups");
        guard_path(app_dir, &backup_dir)?;
        secure_mkdir(&backup_dir)?;
        guard_path(app_dir, &backup_dir)?;
        let name = self
            .path
            .file_name()
            .context("SSH config filename is missing")?
            .to_string_lossy();
        // Retain SSHM's familiar latest backup and a path-specific copy so files
        // with the same basename never erase one another's recovery data.
        let mut hasher = DefaultHasher::new();
        self.path.hash(&mut hasher);
        for filename in [
            format!("{name}.backup"),
            format!("{name}.{:016x}.backup", hasher.finish()),
        ] {
            let target = backup_dir.join(filename);
            guard_path(app_dir, &target)?;
            let mut file = NamedTempFile::new_in(&backup_dir)?;
            private_file(file.path())?;
            file.write_all(bytes)?;
            file.as_file().sync_all()?;
            file.persist(&target).map_err(|error| error.error)?;
        }
        Ok(())
    }
    pub fn staged_snapshot(&self, staged: &NamedTempFile, bytes: &[u8]) -> Result<Self> {
        let mut state = ConfigAccess::read(staged.path())?;
        state.resolved = self.path.clone();
        Ok(Self {
            path: self.path.clone(),
            access: self.access.clone(),
            bytes: Some(bytes.to_vec()),
            permissions: Some(state.permissions.clone()),
            access_state: Some(state),
        })
    }
    pub fn restore(&self, staged: Option<NamedTempFile>, expected: &Snapshot) -> Result<()> {
        expected.unchanged().context(
            "cannot roll back destination because it changed externally; recovery copies retained",
        )?;
        // Never overwrite an external edit made after the target was published.
        if resolve_destination(&self.access)? != self.path
            || fs::read(&self.path)? != expected.bytes.as_deref().unwrap_or_default()
        {
            bail!("cannot roll back {} because it changed externally; the host remains in its source and backups", self.path.display());
        }
        match staged {
            Some(file) => {
                file.persist(&self.path).map_err(|error| error.error)?;
            }
            None => fs::remove_file(&self.path)?,
        }
        Ok(())
    }
}

#[cfg(unix)]
fn native_include_paths(pattern: &Path) -> Result<Vec<PathBuf>> {
    use std::{
        ffi::{CStr, CString},
        os::unix::ffi::OsStrExt,
    };
    let pattern = CString::new(pattern.as_os_str().as_bytes())?;
    // SAFETY: a zeroed glob_t has null pointers and zero counts; glob initializes
    // it before use, and globfree releases only its own allocations afterwards.
    let mut matched: libc::glob_t = unsafe { std::mem::zeroed() };
    let status = unsafe { libc::glob(pattern.as_ptr(), libc::GLOB_ERR, None, &mut matched) };
    let mut paths = vec![];
    if status == 0 {
        for i in 0..matched.gl_pathc {
            // SAFETY: successful glob returns gl_pathc live NUL-terminated paths.
            let path = unsafe { CStr::from_ptr(*matched.gl_pathv.add(i)) };
            paths.push(PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes())));
        }
    }
    // SAFETY: matched was initialized for this glob call and is freed once.
    unsafe { libc::globfree(&mut matched) };
    anyhow::ensure!(status == 0 || status == libc::GLOB_NOMATCH, "cannot safely expand Include {} for copy preflight; check permissions or choose another destination and retry", pattern.to_string_lossy());
    Ok(paths)
}
#[cfg(not(unix))]
fn native_include_paths(pattern: &Path) -> Result<Vec<PathBuf>> {
    anyhow::ensure!(!pattern.to_string_lossy().contains(['*', '?', '[']), "glob Include preflight is unsupported on this platform; use literal absolute Include paths and retry");
    match fs::metadata(pattern) {
        Ok(_) => Ok(vec![pattern.to_path_buf()]),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
pub(super) fn native_home() -> Result<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    let mut buffer = vec![0u8; 16384];
    loop {
        let mut user = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found = std::ptr::null_mut();
        // SAFETY: getpwuid_r initializes user and returns pointers into the live
        // buffer; both remain alive until pw_dir is copied into an owned PathBuf.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                user.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        anyhow::ensure!(status == 0 && !found.is_null(), "cannot safely resolve OpenSSH's local home for Include preflight; use absolute Include paths and retry");
        // SAFETY: successful getpwuid_r initialized the passwd value.
        let user = unsafe { user.assume_init() };
        anyhow::ensure!(
            !user.pw_dir.is_null(),
            "OpenSSH local home is unavailable; use absolute Include paths and retry"
        );
        // SAFETY: pw_dir is non-null and points into the live getpwuid_r buffer.
        let directory = unsafe { CStr::from_ptr(user.pw_dir) };
        return Ok(PathBuf::from(std::ffi::OsStr::from_bytes(
            directory.to_bytes(),
        )));
    }
}
#[cfg(not(unix))]
pub(super) fn native_home() -> Result<PathBuf> {
    super::parser::home()
        .context("cannot resolve OpenSSH home; use absolute Include paths and retry")
}

/// The real access path matters independently of the canonical file's bytes:
/// retargeting a leaf or parent symlink can redirect OpenSSH to another file.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConfigAccess {
    resolved: PathBuf,
    permissions: fs::Permissions,
    #[cfg(unix)]
    owner: u32,
    #[cfg(unix)]
    group: u32,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}
impl ConfigAccess {
    fn read(path: &Path) -> Result<Self> {
        let resolved = fs::canonicalize(path)?;
        let metadata = fs::metadata(path)?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            resolved,
            permissions: metadata.permissions(),
            #[cfg(unix)]
            owner: metadata.uid(),
            #[cfg(unix)]
            group: metadata.gid(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }
}

/// An isolated OpenSSH Include graph. Only Include paths are rewritten; native
/// ssh -G continues to evaluate Host/Match/options, including their original order.
/// Real configuration files are read and checked, never temporarily published.
#[derive(Debug)]
pub(super) struct ConfigMirror {
    directory: tempfile::TempDir,
    root: PathBuf,
    snapshots: BTreeMap<PathBuf, Snapshot>,
    accesses: BTreeMap<PathBuf, ConfigAccess>,
    paths: BTreeMap<PathBuf, PathBuf>,
    includes: Vec<(PathBuf, Vec<PathBuf>)>,
}
impl ConfigMirror {
    pub fn new(root: &Path) -> Result<Self> {
        let mut mirror = Self {
            directory: tempfile::tempdir()?,
            root: PathBuf::new(),
            snapshots: BTreeMap::new(),
            accesses: BTreeMap::new(),
            paths: BTreeMap::new(),
            includes: vec![],
        };
        mirror.root = mirror.visit(root)?;
        Ok(mirror)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn snapshot(&mut self, path: &Path) -> Result<&Snapshot> {
        let access = self.record_access(path)?;
        self.snapshots
            .get(&access.resolved)
            .context("destination is not in the native Include graph; choose another destination")
    }
    fn record_access(&mut self, path: &Path) -> Result<ConfigAccess> {
        // Preserve lexical components: canonicalization below must follow every
        // real parent symlink, rather than discarding it before resolving '..'.
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let access = ConfigAccess::read(&path)?;
        if let Some(previous) = self.accesses.get(&path) {
            anyhow::ensure!(
                *previous == access,
                "SSH config access path or metadata changed at {}; reload and retry",
                path.display()
            );
        } else {
            self.accesses.insert(path, access.clone());
        }
        Ok(access)
    }
    fn visit(&mut self, path: &Path) -> Result<PathBuf> {
        // Record every spelling before canonical-file deduplication, including
        // repeated Includes which reach the same file through different links.
        let access = self.record_access(path)?;
        let snapshot = Snapshot::read(path)?;
        anyhow::ensure!(
            snapshot.path == access.resolved
                && snapshot.permissions.as_ref() == Some(&access.permissions),
            "SSH config access path or metadata changed while reading {}; reload and retry",
            path.display()
        );
        self.record_access(path)?;
        anyhow::ensure!(
            snapshot.bytes.is_some(),
            "cannot mirror absent SSH config {}",
            path.display()
        );
        if let Some(path) = self.paths.get(&snapshot.path) {
            return Ok(path.clone());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let owner = fs::metadata(&snapshot.path)?.uid();
            // SAFETY: getuid has no preconditions or borrowed output.
            let local_user = unsafe { libc::getuid() };
            anyhow::ensure!(owner == 0 || owner == local_user, "cannot safely mirror SSH config {} owned by another user; fix its ownership or choose another configuration and retry", snapshot.path.display());
        }
        let permissions = snapshot.permissions.clone();
        let destination = self
            .directory
            .path()
            .join(format!("config-{}", self.paths.len()));
        self.paths
            .insert(snapshot.path.clone(), destination.clone());
        let text = snapshot.text()?.to_owned();
        self.snapshots.insert(snapshot.path.clone(), snapshot);
        let rendered = self.render(&text)?;
        fs::write(&destination, rendered)?;
        if let Some(permissions) = permissions {
            fs::set_permissions(&destination, permissions)?;
        }
        Ok(destination)
    }
    fn render(&mut self, text: &str) -> Result<String> {
        let mut result = String::new();
        for text in text.split_inclusive('\n') {
            let Some(line) =
                split_line(text).filter(|line| line.keyword.eq_ignore_ascii_case("include"))
            else {
                result.push_str(text);
                continue;
            };
            let mut mirrored = vec![];
            for pattern in line.args {
                anyhow::ensure!(
                    !pattern.contains(['$', '%']) && (!pattern.starts_with('~') || pattern.starts_with("~/")),
                    "cannot safely preflight Include {pattern:?}: environment/token/named-user paths are unsupported; choose a configuration with literal or glob Includes, or expand that Include path and retry"
                );
                // OpenSSH uses the local passwd home on Unix, not an overridden HOME.
                // Relative user Includes are based at that home's .ssh even with -F.
                let path = if let Some(suffix) = pattern.strip_prefix("~/") {
                    native_home()?.join(suffix)
                } else {
                    let path = PathBuf::from(&pattern);
                    if path.is_absolute() {
                        path
                    } else {
                        native_home()?.join(".ssh").join(path)
                    }
                };
                let paths = native_include_paths(&path)?;
                self.includes.push((path, paths.clone()));
                for included in paths {
                    anyhow::ensure!(included.is_file(), "cannot safely preflight non-file Include {}; choose another destination or narrow the Include pattern", included.display());
                    mirrored.push(quote(&self.visit(&included)?.to_string_lossy()));
                }
            }
            if mirrored.is_empty() {
                mirrored.push(quote(
                    &self
                        .directory
                        .path()
                        .join("absent-include")
                        .to_string_lossy(),
                ));
            }
            result.push_str(&format!("Include {}\n", mirrored.join(" ")));
        }
        Ok(result)
    }
    pub fn replace(&mut self, target: &Path, text: &str) -> Result<()> {
        let target = fs::canonicalize(target)?;
        let path = self
            .paths
            .get(&target)
            .context("destination is not in the native Include graph; choose another destination")?
            .clone();
        let rendered = self.render(text)?;
        // The mirror may retain a read-only original mode. Make only this private
        // temporary file writable while replacing it, then restore the real mode
        // so native preflight does not hide OpenSSH's permission checks.
        private_file(&path)?;
        fs::write(&path, rendered)?;
        if let Some(permissions) = &self.snapshots[&target].permissions {
            fs::set_permissions(path, permissions.clone())?;
        }
        Ok(())
    }
    pub fn unchanged(&self) -> Result<()> {
        for snapshot in self.snapshots.values() {
            snapshot.unchanged()?;
        }
        for (pattern, paths) in &self.includes {
            anyhow::ensure!(
                native_include_paths(pattern)? == *paths,
                "SSH Include expansion changed while preparing the copy; reload and retry"
            );
        }
        for (path, expected) in &self.accesses {
            let actual = ConfigAccess::read(path).with_context(|| {
                format!(
                    "SSH config access path or metadata changed at {}; reload and retry",
                    path.display()
                )
            })?;
            anyhow::ensure!(
                *expected == actual,
                "SSH config access path or metadata changed at {}; reload and retry",
                path.display()
            );
        }
        Ok(())
    }
}

/// App-data writes must not traverse a symlink back into the legacy directory.
/// The application directory and what it contains are checked. Links above it
/// belong to the system layout (/var on macOS, /home on some Linux systems)
/// and are not ours to refuse.
pub(super) fn guard_path(app_dir: &Path, path: &Path) -> Result<()> {
    let app_dir = absolute(app_dir)?;
    let path = absolute(path)?;
    let below = path.strip_prefix(&app_dir).with_context(|| {
        format!(
            "{} is outside the application directory {}",
            path.display(),
            app_dir.display()
        )
    })?;
    let mut current = app_dir.clone();
    let mut components = below.components();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => bail!(
                "refusing redirected application write through symbolic link {}",
                current.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match components.next() {
            Some(Component::CurDir) => {}
            Some(component) => current.push(component.as_os_str()),
            None => return Ok(()),
        }
    }
}
pub(super) fn private_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
pub(super) fn secure_mkdir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path)?;
    Ok(())
}

/// Resolve existing ancestors without creating them. Preserve lexical symlink semantics.
fn resolve_destination(path: &Path) -> Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::ensure!(
                fs::symlink_metadata(path).is_err(),
                "SSH config {} is a broken symbolic link",
                path.display()
            );
            let parent = path
                .parent()
                .context("SSH config has no existing ancestor")?;
            let name = path.file_name().context("SSH config has no filename")?;
            Ok(resolve_destination(parent)?.join(name))
        }
        Err(error) => Err(error.into()),
    }
}
