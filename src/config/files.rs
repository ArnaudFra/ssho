use super::parser::absolute;
use anyhow::{bail, Context, Result};
use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};
use tempfile::NamedTempFile;

pub(super) static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub(super) struct Snapshot {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
    permissions: Option<fs::Permissions>,
}
impl Snapshot {
    pub fn read(path: &Path) -> Result<Self> {
        let path = absolute(path)?;
        match fs::metadata(&path) {
            Ok(metadata) => {
                if !metadata.is_file() {
                    bail!("SSH config {} is not a regular file", path.display());
                }
                let resolved = fs::canonicalize(&path)?;
                Ok(Self {
                    bytes: Some(fs::read(&resolved)?),
                    permissions: Some(metadata.permissions()),
                    path: resolved,
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A dangling config symlink is an error, not a new file to replace.
                if fs::symlink_metadata(&path).is_ok() {
                    bail!("SSH config {} is a broken symbolic link", path.display());
                }
                Ok(Self {
                    path,
                    bytes: None,
                    permissions: None,
                })
            }
            Err(error) => Err(error).with_context(|| format!("opening {}", path.display())),
        }
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
        let backup_dir = app_dir.join("backups");
        guard_path(&backup_dir)?;
        secure_mkdir(&backup_dir)?;
        guard_path(&backup_dir)?;
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
            guard_path(&target)?;
            let mut file = NamedTempFile::new_in(&backup_dir)?;
            private_file(file.path())?;
            file.write_all(bytes)?;
            file.as_file().sync_all()?;
            file.persist(&target).map_err(|error| error.error)?;
        }
        Ok(())
    }
    pub fn restore(&self, staged: Option<NamedTempFile>, expected: &[u8]) -> Result<()> {
        // Never overwrite an external edit made after the target was published.
        if fs::read(&self.path)? != expected {
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

/// App-data writes must not traverse a symlink back into the legacy directory.
fn guard_path(path: &Path) -> Result<()> {
    let path = absolute(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::CurDir) {
            continue;
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => bail!(
                "refusing redirected application write through symbolic link {}",
                current.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn private_file(path: &Path) -> Result<()> {
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
fn secure_mkdir(path: &Path) -> Result<()> {
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
