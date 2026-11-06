//! Application data copied from SSHM without modifying the legacy directory.
use crate::types::{ConnectionInfo, Forward};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub legacy: PathBuf,
    pub ssh_config: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let home = home_directory()?;
        let base = config_base(&home, |name| std::env::var_os(name));
        Ok(Self::new(base, home))
    }

    pub fn new(config_base: PathBuf, home: PathBuf) -> Self {
        Self {
            root: config_base.join("ssho"),
            legacy: config_base.join("sshm"),
            ssh_config: home.join(".ssh/config"),
        }
    }

    /// Stage the complete copy beside its destination, then publish without replacing anything.
    /// Returns `true` when this call copied the data of the legacy `sshm/` folder.
    pub fn initialize(&self) -> Result<bool> {
        if existing_directory(&self.root)? {
            return Ok(false);
        }
        let parent = self
            .root
            .parent()
            .context("application directory has no parent")?;
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        let staging = tempfile::Builder::new()
            .prefix(".ssho-migration-")
            .tempdir_in(parent)?;
        let mut copied = false;
        match fs::metadata(&self.legacy) {
            Ok(metadata) if metadata.is_dir() => {
                copy_tree(&self.legacy, staging.path(), &mut HashSet::new())?;
                copied = true;
            }
            Ok(_) => bail!("legacy path {} is not a directory", self.legacy.display()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A broken source link is an error, rather than silently treating it as absent.
                if fs::symlink_metadata(&self.legacy).is_ok() {
                    bail!(
                        "legacy path {} is a broken symbolic link",
                        self.legacy.display()
                    );
                }
                atomic_json(
                    staging.path(),
                    "config.json",
                    &serde_json::to_value(AppConfig::default())?,
                    0o644,
                )?;
            }
            Err(error) => return Err(error).context("inspect legacy directory"),
        }
        match publish_directory(staging.path(), &self.root) {
            Ok(()) => Ok(copied),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                // Another process published first: this call copied nothing.
                existing_directory(&self.root)?;
                Ok(false)
            }
            Err(error) => Err(error).with_context(|| {
                format!(
                    "publish {} without replacing existing data",
                    self.root.display()
                )
            }),
        }
    }
}

fn home_directory() -> Result<PathBuf> {
    #[cfg(windows)]
    let key = "USERPROFILE";
    #[cfg(not(windows))]
    let key = "HOME";
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .with_context(|| format!("{key} is not set"))
}

fn config_base(home: &Path, env: impl Fn(&str) -> Option<std::ffi::OsString>) -> PathBuf {
    #[cfg(windows)]
    let key = "APPDATA";
    #[cfg(not(windows))]
    let key = "XDG_CONFIG_HOME";
    env(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
}

/// Inspect links themselves: even a link to a directory must never become an application root.
fn existing_directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => bail!(
            "{} must be a real directory, not a file or symbolic link",
            path.display()
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn ensure_root(root: &Path) -> Result<()> {
    if !existing_directory(root)? {
        fs::create_dir_all(root)?;
        existing_directory(root)?;
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path, ancestors: &mut HashSet<PathBuf>) -> Result<()> {
    let physical = fs::canonicalize(source).with_context(|| {
        format!(
            "resolve legacy entry {} (broken link or cycle)",
            source.display()
        )
    })?;
    let metadata = fs::metadata(&physical)?;
    if metadata.is_dir() {
        if !ancestors.insert(physical.clone()) {
            bail!("symbolic link cycle at {}", source.display());
        }
        if !target.exists() {
            fs::create_dir(target)?;
        }
        for entry in fs::read_dir(&physical)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()), ancestors)?;
        }
        ancestors.remove(&physical);
        fs::set_permissions(target, metadata.permissions())?;
    } else if metadata.is_file() {
        fs::copy(&physical, target).with_context(|| format!("copy {}", source.display()))?;
        fs::set_permissions(target, metadata.permissions())?;
    } else {
        bail!("unsupported legacy entry type: {}", source.display());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn publish_directory(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
    // A plain rename would overwrite an empty directory that appeared during copying.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if matches!(
        error.raw_os_error(),
        Some(libc::ENOSYS) | Some(libc::EINVAL)
    ) {
        // Fail closed on old kernels/filesystems; a check followed by rename is unsafe.
        return Err(io::Error::new(io::ErrorKind::Unsupported, "atomic non-replacing directory publication is unsupported; source is intact and initialization may be retried"));
    }
    Err(error)
}

#[cfg(target_os = "macos")]
fn publish_directory(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let source = std::ffi::CString::new(source.as_os_str().as_bytes())?;
    let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())?;
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn publish_directory(source: &Path, destination: &Path) -> io::Result<()> {
    // Windows rename does not replace an existing destination directory.
    fs::rename(source, destination)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn publish_directory(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic non-replacing directory publication is unavailable on this platform",
    ))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBindings {
    #[serde(deserialize_with = "nullable_vec")]
    pub quit_keys: Vec<String>,
    #[serde(deserialize_with = "nullable_false")]
    pub disable_esc_quit: bool,
    /// Two-step prefix opening the quick actions panel; an empty string disables it.
    #[serde(deserialize_with = "nullable_prefix")]
    pub prefix_key: String,
    /// Direct list shortcuts by stable action identifier, replacing the historical
    /// keys of that action. An empty list leaves the action to the palette.
    #[serde(deserialize_with = "nullable_shortcuts")]
    pub shortcuts: BTreeMap<String, Vec<String>>,
}

pub const DEFAULT_PREFIX_KEY: &str = "ctrl+g";

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            quit_keys: vec!["q".into(), "ctrl+c".into()],
            disable_esc_quit: false,
            prefix_key: DEFAULT_PREFIX_KEY.into(),
            shortcuts: BTreeMap::new(),
        }
    }
}

impl KeyBindings {
    pub fn should_quit_on_key(&self, key: &str) -> bool {
        // Esc goes back one level; at the list it quits only when listed among
        // the quit keys, and `disable_esc_quit` still forbids it.
        self.quit_keys.iter().any(|candidate| candidate == key)
            && !(key == "esc" && self.disable_esc_quit)
    }
}

fn nullable_vec<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}
fn nullable_prefix<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_else(|| DEFAULT_PREFIX_KEY.into()))
}
fn nullable_shortcuts<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Vec<String>>, D::Error> {
    Ok(Option::<BTreeMap<String, Vec<String>>>::deserialize(deserializer)?.unwrap_or_default())
}
fn nullable_true<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(deserializer)?.unwrap_or(true))
}

fn nullable_false<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(deserializer)?.unwrap_or(false))
}
fn nullable_bindings<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<KeyBindings, D::Error> {
    Ok(Option::<KeyBindings>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_ssh_command() -> Vec<String> {
    vec!["ssh".into()]
}

fn validate_ssh_command(command: &[String]) -> Result<()> {
    anyhow::ensure!(
        command.first().is_some_and(|program| !program.trim().is_empty()),
        "ssh_command must be a non-empty array of string arguments with a non-blank executable first; use [\"ssh\"] for OpenSSH"
    );
    anyhow::ensure!(
        !command.iter().any(|argument| argument.contains('\0')),
        "ssh_command arguments must not contain NUL characters; remove the NUL or use [\"ssh\"]"
    );
    Ok(())
}

fn nullable_ssh_command<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error> {
    let command = Option::<Vec<String>>::deserialize(deserializer)
        .map_err(|error| {
            serde::de::Error::custom(format!(
                "ssh_command must be an array of string arguments, for example [\"ssh\"]: {error}"
            ))
        })?
        .unwrap_or_else(default_ssh_command);
    validate_ssh_command(&command).map_err(serde::de::Error::custom)?;
    Ok(command)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    #[default]
    Flat,
    Files,
    FileTags,
}
fn nullable_group_mode<'de, D: Deserializer<'de>>(deserializer: D) -> Result<GroupMode, D::Error> {
    Ok(Option::<GroupMode>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityMethod {
    #[default]
    Ssh,
    Icmp,
}
impl AvailabilityMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ssh => "SSH",
            Self::Icmp => "ICMP local",
        }
    }
    pub fn scope(self) -> &'static str {
        match self {
            Self::Ssh => crate::t!(
                "SSH: direct handshake or configured proxy check",
                "SSH : poignée de main directe ou contrôle via le proxy configuré"
            ),
            Self::Icmp => crate::t!(
                "ICMP local: network availability; no bastion, no SSH authentication",
                "ICMP local : disponibilité réseau ; ni bastion ni authentification SSH"
            ),
        }
    }
}
fn nullable_availability_method<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<AvailabilityMethod, D::Error> {
    Ok(Option::<AvailabilityMethod>::deserialize(deserializer)?.unwrap_or_default())
}

/// Ordered host table fields; Name is mandatory and always first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostColumn {
    Name,
    Hostname,
    User,
    Port,
    Tags,
    LastAttempt,
}
pub fn default_host_columns() -> Vec<HostColumn> {
    use HostColumn::*;
    vec![Name, Hostname, Tags, LastAttempt]
}
fn validate_host_columns(columns: &[HostColumn]) -> Result<()> {
    anyhow::ensure!(
        columns.first() == Some(&HostColumn::Name),
        "host_columns must start with name and cannot be empty"
    );
    for (index, column) in columns.iter().enumerate() {
        anyhow::ensure!(
            !columns[..index].contains(column),
            "host_columns contains duplicate {column:?}"
        );
    }
    Ok(())
}
fn nullable_host_columns<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<HostColumn>, D::Error> {
    let columns = Option::<Vec<HostColumn>>::deserialize(deserializer)
        .map_err(|error| serde::de::Error::custom(format!("host_columns: {error}")))?
        .unwrap_or_else(default_host_columns);
    validate_host_columns(&columns).map_err(serde::de::Error::custom)?;
    Ok(columns)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    #[serde(deserialize_with = "nullable_host_columns")]
    pub host_columns: Vec<HostColumn>,
    #[serde(deserialize_with = "nullable_availability_method")]
    pub availability_method: AvailabilityMethod,
    #[serde(deserialize_with = "nullable_true")]
    pub check_for_updates: bool,
    #[serde(deserialize_with = "nullable_true")]
    pub focus_search_on_start: bool,
    #[serde(deserialize_with = "nullable_true")]
    pub ping_on_start: bool,
    #[serde(deserialize_with = "nullable_bindings")]
    pub key_bindings: KeyBindings,
    #[serde(deserialize_with = "nullable_ssh_command")]
    pub ssh_command: Vec<String>,
    #[serde(deserialize_with = "nullable_false")]
    pub group_aliases: bool,
    #[serde(deserialize_with = "nullable_group_mode")]
    pub group_mode: GroupMode,
    /// Interface language: `"en"` (default, also for null or unknown) or `"fr"`.
    pub language: crate::i18n::Language,
    /// `false` draws without any colour: states keep their text, sign or emphasis.
    #[serde(deserialize_with = "nullable_true")]
    pub colors: bool,
    /// `true` replaces box drawing and symbols with ASCII characters.
    #[serde(deserialize_with = "nullable_false")]
    pub ascii: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host_columns: default_host_columns(),
            availability_method: AvailabilityMethod::Ssh,
            check_for_updates: true,
            focus_search_on_start: true,
            ping_on_start: true,
            key_bindings: KeyBindings::default(),
            ssh_command: default_ssh_command(),
            group_aliases: false,
            group_mode: GroupMode::Flat,
            language: crate::i18n::Language::En,
            colors: true,
            ascii: false,
        }
    }
}

impl AppConfig {
    /// Read current preferences, change only the requested fields, then publish.
    /// Callers commit their UI state only after this succeeds.
    pub fn update(root: &Path, change: impl FnOnce(&mut Self)) -> Result<Self> {
        let mut config = Self::load(root)?;
        change(&mut config);
        config.save(root)?;
        Ok(config)
    }

    pub fn load(root: &Path) -> Result<Self> {
        ensure_root(root)?;
        let mut config: Self = match read_json(root, "config.json")? {
            Some(Value::Null) => Self::default(),
            Some(value) => {
                serde_json::from_value(value).context("parse application configuration")?
            }
            None => {
                let config = Self::default();
                // SSHM remains usable with defaults when a new preferences file cannot be saved.
                let _ = config.save(root);
                return Ok(config);
            }
        };
        if config.key_bindings.quit_keys.is_empty() {
            config.key_bindings.quit_keys = KeyBindings::default().quit_keys;
        }
        Ok(config)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        validate_ssh_command(&self.ssh_command)?;
        validate_host_columns(&self.host_columns)?;
        ensure_root(root)?;
        let mut value =
            read_json(root, "config.json")?.unwrap_or_else(|| Value::Object(Map::new()));
        merge_value(&mut value, serde_json::to_value(self)?);
        atomic_json(root, "config.json", &value, 0o644)
    }
}

pub mod commands;
mod history;
pub mod metadata;
pub mod tunnels;
pub mod views;
pub use history::History;

fn merge_value(target: &mut Value, incoming: Value) {
    if let (Some(target), Some(incoming)) = (target.as_object_mut(), incoming.as_object()) {
        for (key, value) in incoming {
            if let Some(previous) = target.get_mut(key) {
                merge_value(previous, value.clone());
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = incoming;
    }
}

fn check_file(root: &Path, name: &str) -> Result<()> {
    existing_directory(root)?
        .then_some(())
        .context("application root no longer exists")?;
    match fs::symlink_metadata(root.join(name)) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => bail!(
            "refusing application file {}: expected a regular file, not a symbolic link",
            root.join(name).display()
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn read_json(root: &Path, name: &str) -> Result<Option<Value>> {
    check_file(root, name)?;
    match fs::read(root.join(name)) {
        Ok(bytes) => {
            Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
                format!("parse {}", root.join(name).display())
            })?))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
fn atomic_json(root: &Path, name: &str, value: &Value, default_mode: u32) -> Result<()> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    check_file(root, name)?;
    let directory = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)?;
    let fd = directory.as_raw_fd();
    let target = CString::new(name)?;
    let temporary = CString::new(format!(
        ".{name}.{}-{}-{}.tmp",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ))?;
    let mode = fs::symlink_metadata(root.join(name))
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(default_mode);
    let raw = unsafe {
        libc::openat(
            fd,
            temporary.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            mode as libc::mode_t,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let result = (|| -> Result<()> {
        let mut file = unsafe { fs::File::from_raw_fd(raw) };
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        let stat = unsafe {
            libc::fstatat(
                fd,
                target.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if stat == 0 {
            let metadata = unsafe { metadata.assume_init() };
            if metadata.st_mode & libc::S_IFMT != libc::S_IFREG {
                bail!("refusing symbolic link or non-regular application file {name}");
            }
        } else if io::Error::last_os_error().kind() != io::ErrorKind::NotFound {
            return Err(io::Error::last_os_error().into());
        }
        if unsafe { libc::renameat(fd, temporary.as_ptr(), fd, target.as_ptr()) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        directory.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        unsafe {
            libc::unlinkat(fd, temporary.as_ptr(), 0);
        }
    }
    result
}

#[cfg(windows)]
fn lock_windows_root(root: &Path) -> Result<fs::File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    // Opening the link itself closes the check/open race. Omitting FILE_SHARE_DELETE
    // prevents replacement of the verified directory until this handle is dropped.
    let directory = fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(root)
        .with_context(|| format!("lock application directory {}", root.display()))?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!(
            "{} must be a real directory, not a reparse point or junction",
            root.display()
        );
    }
    Ok(directory)
}

#[cfg(not(unix))]
fn atomic_json(root: &Path, name: &str, value: &Value, _default_mode: u32) -> Result<()> {
    // Keep the verified, non-renamable root open before the first temporary write.
    #[cfg(windows)]
    let directory = lock_windows_root(root)?;
    check_file(root, name)?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    check_file(root, name)?;
    file.persist(root.join(name)).map_err(|error| error.error)?;
    #[cfg(windows)]
    drop(directory);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, AppPaths) {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(temp.path().join("config"), temp.path().join("home"));
        (temp, paths)
    }

    #[cfg(windows)]
    #[test]
    fn windows_root_handle_prevents_swap_until_publication_finishes() {
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.root).unwrap();
        let displaced = paths.root.with_file_name("displaced");
        let guard = lock_windows_root(&paths.root).unwrap();
        assert!(fs::rename(&paths.root, &displaced).is_err());
        atomic_json(
            &paths.root,
            "sshm_history.json",
            &serde_json::json!({"connections":{}}),
            0o600,
        )
        .unwrap();
        assert!(paths.root.join("sshm_history.json").is_file());
        drop(guard);
        fs::rename(&paths.root, &displaced).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_root_swapped_for_symlink_is_rejected_before_creating_a_tempfile() {
        use std::os::windows::fs::symlink_dir;
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.legacy).unwrap();
        fs::create_dir(&paths.root).unwrap();
        check_file(&paths.root, "sshm_history.json").unwrap();
        fs::remove_dir(&paths.root).unwrap();
        match symlink_dir(&paths.legacy, &paths.root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                eprintln!("directory symlink requires Windows Developer Mode or symlink privileges: {error}");
                return;
            }
            Err(error) => panic!("create directory symlink: {error}"),
        }
        assert!(lock_windows_root(&paths.root).is_err());
        assert!(atomic_json(
            &paths.root,
            "sshm_history.json",
            &serde_json::json!({"connections":{}}),
            0o600
        )
        .is_err());
        assert_eq!(fs::read_dir(&paths.legacy).unwrap().count(), 0);
        fs::remove_dir(&paths.root).unwrap();
    }

    #[test]
    fn config_accepts_go_null_fields() {
        let temp = tempfile::tempdir().unwrap();
        for input in [
            r#"{"key_bindings":null}"#,
            r#"{"key_bindings":{"quit_keys":null,"disable_esc_quit":null}}"#,
            r#"{"key_bindings":{"prefix_key":null,"shortcuts":null}}"#,
            "null",
        ] {
            fs::write(temp.path().join("config.json"), input).unwrap();
            let config = AppConfig::load(temp.path()).unwrap();
            assert_eq!(config, AppConfig::default());
        }
    }

    #[test]
    fn configured_config_base_is_used_without_mutating_environment() {
        let home = Path::new("/test-home");
        let base = config_base(home, |key| {
            #[cfg(windows)]
            assert_eq!(key, "APPDATA");
            #[cfg(not(windows))]
            assert_eq!(key, "XDG_CONFIG_HOME");
            Some("/custom-config".into())
        });
        let paths = AppPaths::new(base, home.into());
        assert_eq!(paths.root, Path::new("/custom-config/ssho"));
        assert_eq!(paths.legacy, Path::new("/custom-config/sshm"));
        assert_eq!(paths.ssh_config, home.join(".ssh/config"));
        assert_eq!(config_base(home, |_| None), home.join(".config"));
        assert_eq!(config_base(home, |_| Some("".into())), home.join(".config"));
    }

    #[test]
    fn atomic_publication_does_not_replace_a_destination_created_after_staging() {
        let temp = tempfile::tempdir().unwrap();
        let stage = temp.path().join("stage");
        let target = temp.path().join("target");
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("incoming"), b"copy").unwrap();
        fs::create_dir(&target).unwrap();
        assert!(publish_directory(&stage, &target).is_err());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        assert!(stage.join("incoming").is_file());
    }

    #[test]
    fn concurrent_initializers_publish_one_complete_independent_copy() {
        let (_temp, paths) = fixture();
        fs::create_dir_all(paths.legacy.join("nested")).unwrap();
        for index in 0..20 {
            fs::write(
                paths.legacy.join(format!("nested/{index}")),
                format!("payload-{index}"),
            )
            .unwrap();
        }
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let paths = paths.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    paths.initialize()
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        assert_eq!(fs::read_dir(paths.root.join("nested")).unwrap().count(), 20);
        for index in 0..20 {
            assert_eq!(
                fs::read_to_string(paths.root.join(format!("nested/{index}"))).unwrap(),
                format!("payload-{index}")
            );
        }
        assert_eq!(
            fs::read_dir(paths.root.parent().unwrap()).unwrap().count(),
            2
        );
    }

    #[cfg(unix)]
    #[test]
    fn missing_preferences_return_defaults_when_default_file_cannot_be_saved() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let loaded = AppConfig::load(temp.path());
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(loaded.unwrap(), AppConfig::default());
        assert!(!temp.path().join("config.json").exists());
    }

    #[test]
    fn config_save_preserves_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("config.json"),
            r#"{"future":true,"key_bindings":{"custom":42}}"#,
        )
        .unwrap();
        let mut config = AppConfig::load(temp.path()).unwrap();
        config.ping_on_start = false;
        config.save(temp.path()).unwrap();
        let value: Value =
            serde_json::from_slice(&fs::read(temp.path().join("config.json")).unwrap()).unwrap();
        assert_eq!(value["future"], true);
        assert_eq!(value["key_bindings"]["custom"], 42);
        assert_eq!(value["ping_on_start"], false);
        // Esc goes back; it quits only when listed, and the legacy switch still forbids it.
        assert!(!config.key_bindings.should_quit_on_key("esc"));
        config.key_bindings.quit_keys.push("esc".into());
        assert!(config.key_bindings.should_quit_on_key("esc"));
        config.key_bindings.disable_esc_quit = true;
        assert!(!config.key_bindings.should_quit_on_key("esc"));
        assert!(config.key_bindings.should_quit_on_key("ctrl+c"));
    }

    #[cfg(unix)]
    #[test]
    fn unsupported_source_entry_never_publishes() {
        use std::os::unix::ffi::OsStrExt;
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.legacy).unwrap();
        let fifo_path =
            std::ffi::CString::new(paths.legacy.join("fifo").as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
        assert!(paths
            .initialize()
            .unwrap_err()
            .to_string()
            .contains("unsupported"));
        assert!(!paths.root.exists());
        assert!(paths.legacy.join("fifo").exists());
    }

    #[test]
    fn migration_copies_every_file_preserves_source_and_is_independent() {
        let (_temp, paths) = fixture();
        fs::create_dir_all(paths.legacy.join("backups/nested")).unwrap();
        fs::write(paths.legacy.join("config.json"), b"{}").unwrap();
        fs::write(paths.legacy.join("backups/nested/backup"), b"original").unwrap();
        paths.initialize().unwrap();
        assert_eq!(
            fs::read(paths.root.join("backups/nested/backup")).unwrap(),
            b"original"
        );
        fs::write(paths.root.join("backups/nested/backup"), b"changed").unwrap();
        assert_eq!(
            fs::read(paths.legacy.join("backups/nested/backup")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn initialization_reports_a_copy_only_when_it_made_one() {
        // A legacy folder and no destination: the copy is made and reported.
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.legacy).unwrap();
        fs::write(paths.legacy.join("config.json"), b"{}").unwrap();
        assert!(paths.initialize().unwrap());
        // The destination now exists: nothing is copied or announced again.
        assert!(!paths.initialize().unwrap());
        // A fresh start without SSHM creates defaults and announces nothing.
        let (_temp, paths) = fixture();
        assert!(!paths.initialize().unwrap());
        assert!(paths.root.join("config.json").exists());
    }

    #[test]
    fn existing_empty_destination_does_not_import() {
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.legacy).unwrap();
        fs::write(paths.legacy.join("only-old"), b"old").unwrap();
        fs::create_dir(&paths.root).unwrap();
        paths.initialize().unwrap();
        assert_eq!(fs::read_dir(&paths.root).unwrap().count(), 0);
    }

    #[test]
    fn absent_source_creates_defaults_and_leaves_ssh_history_untouched() {
        let (_temp, paths) = fixture();
        let ssh_dir = paths.ssh_config.parent().unwrap();
        fs::create_dir_all(ssh_dir).unwrap();
        fs::write(ssh_dir.join("sshm_history.json"), b"untouched").unwrap();
        paths.initialize().unwrap();
        let config = AppConfig::load(&paths.root).unwrap();
        assert!(config.check_for_updates && config.focus_search_on_start && config.ping_on_start);
        assert_eq!(config.key_bindings.quit_keys, ["q", "ctrl+c"]);
        assert!(History::load(&paths.root).unwrap().connections.is_empty());
        assert_eq!(
            fs::read(ssh_dir.join("sshm_history.json")).unwrap(),
            b"untouched"
        );
    }

    #[test]
    fn config_merges_nullable_go_defaults_and_preserves_false() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("config.json"), r#"{"check_for_updates":null,"focus_search_on_start":false,"key_bindings":{"quit_keys":[],"disable_esc_quit":true}}"#).unwrap();
        let config = AppConfig::load(temp.path()).unwrap();
        assert!(config.check_for_updates && config.ping_on_start);
        assert!(!config.focus_search_on_start);
        assert!(config.key_bindings.disable_esc_quit);
        assert_eq!(config.key_bindings.quit_keys, ["q", "ctrl+c"]);
    }

    #[test]
    fn invalid_history_is_rejected_without_rewriting_data() {
        let temp = tempfile::tempdir().unwrap();
        for input in ["[]", "17", r#"{"connections":[]}"#, "{broken"] {
            fs::write(temp.path().join("sshm_history.json"), input).unwrap();
            assert!(History::load(temp.path()).is_err());
            assert_eq!(
                fs::read_to_string(temp.path().join("sshm_history.json")).unwrap(),
                input
            );
        }
    }

    #[test]
    fn history_loads_go_json_and_preserves_tunnel_on_normal_connection() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("sshm_history.json"), r#"{"future":42,"connections":{"prod":{"host_name":"prod","last_connect":"2026-01-02T03:04:05.123456789+01:00","connect_count":3,"custom":"keep","port_forwarding":{"type":"dynamic","local_port":"1080","remote_host":"","remote_port":"","bind_address":"127.0.0.1","future":true}}}}"#).unwrap();
        let mut history = History::load(temp.path()).unwrap();
        let previous = history.connections["prod"].last_connect;
        history.record("prod", None).unwrap();
        assert_eq!(history.connections["prod"].connect_count, 4);
        assert!(history.connections["prod"].last_connect > previous);
        assert_eq!(
            history.connections["prod"]
                .port_forwarding
                .as_ref()
                .unwrap()
                .local_port,
            "1080"
        );
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(temp.path().join("sshm_history.json")).unwrap())
                .unwrap();
        assert_eq!(json["future"], 42);
        assert_eq!(json["connections"]["prod"]["custom"], "keep");
        assert_eq!(
            json["connections"]["prod"]["port_forwarding"]["future"],
            true
        );
        let forward = Forward {
            kind: "local".into(),
            local_port: "8080".into(),
            remote_host: "localhost".into(),
            remote_port: "80".into(),
            bind_address: "".into(),
        };
        history.record("prod", Some(&forward)).unwrap();
        let loaded = History::load(temp.path()).unwrap();
        assert_eq!(loaded.connections["prod"].connect_count, 5);
        assert_eq!(
            loaded.connections["prod"].port_forwarding.as_ref(),
            Some(&forward)
        );
    }

    #[cfg(unix)]
    #[test]
    fn migration_materializes_symlinks_and_preserves_permissions() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let (_temp, paths) = fixture();
        fs::create_dir_all(paths.legacy.join("real")).unwrap();
        fs::write(paths.legacy.join("real/key"), b"private").unwrap();
        fs::set_permissions(
            paths.legacy.join("real/key"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        symlink("real", paths.legacy.join("linked")).unwrap();
        symlink("real/key", paths.legacy.join("key-link")).unwrap();
        paths.initialize().unwrap();
        assert!(!fs::symlink_metadata(paths.root.join("linked"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!fs::symlink_metadata(paths.root.join("key-link"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::metadata(paths.root.join("linked/key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::write(paths.root.join("key-link"), b"new").unwrap();
        assert_eq!(fs::read(paths.legacy.join("real/key")).unwrap(), b"private");
    }

    #[cfg(unix)]
    #[test]
    fn migration_cycle_or_broken_link_never_publishes_and_can_retry() {
        use std::os::unix::fs::symlink;
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.legacy).unwrap();
        fs::write(paths.legacy.join("valid"), b"safe").unwrap();
        symlink(".", paths.legacy.join("cycle")).unwrap();
        assert!(paths.initialize().is_err());
        assert!(!paths.root.exists());
        fs::remove_file(paths.legacy.join("cycle")).unwrap();
        symlink("missing", paths.legacy.join("broken")).unwrap();
        assert!(paths.initialize().is_err());
        assert!(!paths.root.exists());
        fs::remove_file(paths.legacy.join("broken")).unwrap();
        paths.initialize().unwrap();
        assert_eq!(fs::read(paths.root.join("valid")).unwrap(), b"safe");
    }

    #[cfg(unix)]
    #[test]
    fn application_rejects_destination_and_history_symlinks() {
        use std::os::unix::fs::symlink;
        let (_temp, paths) = fixture();
        fs::create_dir_all(&paths.legacy).unwrap();
        symlink(&paths.legacy, &paths.root).unwrap();
        assert!(paths.initialize().is_err());
        assert!(AppConfig::load(&paths.root).is_err());
        assert!(History::load(&paths.root).is_err());
        fs::remove_file(&paths.root).unwrap();
        fs::create_dir(&paths.root).unwrap();
        fs::write(paths.legacy.join("sshm_history.json"), b"{}").unwrap();
        let mut history = History::load(&paths.root).unwrap();
        symlink(
            paths.legacy.join("sshm_history.json"),
            paths.root.join("sshm_history.json"),
        )
        .unwrap();
        assert!(history.record("prod", None).is_err());
        assert!(history.connections.is_empty());
        assert_eq!(
            fs::read(paths.legacy.join("sshm_history.json")).unwrap(),
            b"{}"
        );
    }

    #[test]
    fn file_destination_is_rejected_without_replacement() {
        let (_temp, paths) = fixture();
        fs::create_dir_all(paths.root.parent().unwrap()).unwrap();
        fs::write(&paths.root, b"keep").unwrap();
        assert!(paths.initialize().is_err());
        assert_eq!(fs::read(&paths.root).unwrap(), b"keep");
    }
}

#[cfg(test)]
mod availability_tests {
    use super::*;
    #[test]
    fn availability_defaults_and_null_are_ssh_and_invalid_is_rejected() {
        for json in ["{}", r#"{"availability_method":null}"#] {
            let config: AppConfig = serde_json::from_str(json).unwrap();
            assert_eq!(
                serde_json::to_value(config).unwrap()["availability_method"],
                "ssh"
            );
        }
        assert!(serde_json::from_str::<AppConfig>(r#"{"availability_method":"tcp"}"#).is_err());
    }
}
