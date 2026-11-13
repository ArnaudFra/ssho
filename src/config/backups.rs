//! Versioned self-contained recovery records; legacy basenames carry no provenance.
use super::{
    files::{guard_path, private_file, secure_mkdir, Snapshot},
    preview::PreparedChange,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    version: u32,
    destination: PathBuf,
    access: PathBuf,
    created: String,
    #[cfg(unix)]
    mode: u32,
    bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Backup {
    path: PathBuf,
    pub destination: Option<PathBuf>,
    pub description: String,
}
impl Backup {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
pub(super) fn record(snapshot: &Snapshot, app_dir: &Path) -> Result<()> {
    let Some(bytes) = &snapshot.bytes else {
        return Ok(());
    };
    let directory = app_dir.join("backups");
    guard_path(app_dir, &directory)?;
    secure_mkdir(&directory)?;
    guard_path(app_dir, &directory)?;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let record = Record {
        version: 1,
        destination: snapshot.path.clone(),
        access: snapshot.access.clone(),
        created: chrono::Utc::now().to_rfc3339(),
        bytes: bytes.clone(),
        #[cfg(unix)]
        mode: snapshot
            .permissions
            .as_ref()
            .context("backup permissions missing")?
            .mode(),
    };
    // The tempfile's random suffix makes versions unique; noclobber publication
    // leaves every earlier record intact even within the same clock tick.
    let mut file = tempfile::Builder::new()
        .prefix("version-")
        .suffix(".json")
        .tempfile_in(&directory)?;
    private_file(file.path())?;
    serde_json::to_writer(&mut file, &record)?;
    file.flush()?;
    file.as_file().sync_all()?;
    let (_, path) = file.keep().map_err(|error| error.error)?;
    guard_path(app_dir, &path)?;
    Ok(())
}
fn read_record(path: &Path) -> Result<(Record, Snapshot)> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    anyhow::ensure!(
        name.starts_with("version-") && name.ends_with(".json"),
        "legacy backup has unknown provenance"
    );
    // A backup always lives in <application directory>/backups.
    let app_dir = path
        .parent()
        .and_then(Path::parent)
        .context("backup path has no application directory")?;
    guard_path(app_dir, path)?;
    let snapshot = Snapshot::read(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path)?;
        anyhow::ensure!(
            metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o077 == 0,
            "backup provenance is untrusted: record must be private and owned by this user"
        );
    }
    let record: Record =
        serde_json::from_slice(snapshot.bytes.as_deref().context("backup is missing")?)?;
    anyhow::ensure!(
        record.version == 1 && record.destination.is_absolute() && record.access.is_absolute(),
        "unknown backup provenance/version"
    );
    // Do not display or apply lossy UTF-8 diffs.
    std::str::from_utf8(&record.bytes).context("backup SSH config is not UTF-8")?;
    Ok((record, snapshot))
}
pub fn list(app_dir: &Path) -> Result<Vec<Backup>> {
    let directory = app_dir.join("backups");
    guard_path(app_dir, &directory)?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut backups = vec![];
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !name.ends_with(".backup") && !(name.starts_with("version-") && name.ends_with(".json"))
        {
            continue;
        }
        let (destination, description) = match read_record(&path) {
            Ok((record, _)) => (
                Some(record.destination.clone()),
                format!(
                    "{} → {} (recorded)",
                    record.created,
                    record.destination.display()
                ),
            ),
            Err(_) => (
                None,
                format!("{name}: legacy/unknown provenance — restore disabled"),
            ),
        };
        backups.push(Backup {
            path,
            destination,
            description,
        });
    }
    backups.sort_by(|a, b| {
        b.destination
            .is_some()
            .cmp(&a.destination.is_some())
            .then_with(|| b.description.cmp(&a.description))
    });
    Ok(backups)
}
pub fn prepare_restore(backup: &Backup) -> Result<PreparedChange> {
    anyhow::ensure!(
        backup.destination.is_some(),
        "unknown backup provenance; destination cannot be guessed"
    );
    let (record, evidence) = read_record(&backup.path)?;
    anyhow::ensure!(
        Some(&record.destination) == backup.destination.as_ref(),
        "backup destination changed; reload backups and retry"
    );
    let mut snapshot = Snapshot::read(&record.access)?;
    anyhow::ensure!(
        snapshot.path == record.destination,
        "recorded destination access changed; refusing redirected restoration"
    );
    snapshot.text()?;
    #[cfg(unix)]
    if snapshot.bytes.is_none() {
        use std::os::unix::fs::PermissionsExt;
        snapshot.permissions = Some(fs::Permissions::from_mode(record.mode & 0o777));
    }
    Ok(PreparedChange::single(snapshot, record.bytes).with_evidence(evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, types::Host};
    fn mutate(path: &Path, app: &Path) {
        config::add_host(
            &Host {
                name: "added".into(),
                hostname: "added.example".into(),
                ..Host::default()
            },
            path,
            app,
        )
        .unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn same_basename_restore_preserves_versions_modes_and_pre_restore_backup() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let a = tmp.path().join("a/config");
        let b = tmp.path().join("b/config");
        fs::create_dir_all(a.parent().unwrap()).unwrap();
        fs::create_dir_all(b.parent().unwrap()).unwrap();
        fs::write(&a, "# first A\r\n").unwrap();
        fs::write(&b, "# first B\n").unwrap();
        fs::set_permissions(&a, fs::Permissions::from_mode(0o640)).unwrap();
        mutate(&a, &app);
        mutate(&b, &app);
        let entries = list(&app).unwrap();
        assert!(entries.iter().any(|entry| entry.destination.is_none()));
        // The recorded destination is canonical: the temporary directory may be
        // reached through a link (/var on macOS).
        let recorded = fs::canonicalize(&a).unwrap();
        let backup = entries
            .iter()
            .find(|entry| entry.destination.as_ref() == Some(&recorded))
            .unwrap();
        let b_before = fs::read(&b).unwrap();
        let a_before = fs::read(&a).unwrap();
        let plan = prepare_restore(backup).unwrap();
        assert_eq!(plan.files()[0].after(), b"# first A\r\n");
        assert_eq!(fs::read(&a).unwrap(), a_before);
        plan.apply(&app).unwrap();
        assert_eq!(fs::read(&a).unwrap(), b"# first A\r\n");
        assert_eq!(fs::read(&b).unwrap(), b_before);
        assert_eq!(
            fs::metadata(&a).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let versions: Vec<_> = list(&app)
            .unwrap()
            .into_iter()
            .filter(|b| b.destination.is_some())
            .collect();
        assert_eq!(versions.len(), 3);
        assert!(versions
            .iter()
            .any(|b| read_record(b.path()).unwrap().0.bytes == a_before));
    }
    #[cfg(unix)]
    #[test]
    fn restore_rejects_changed_record_identity_even_with_identical_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let path = tmp.path().join("config");
        fs::write(&path, "# old\n").unwrap();
        mutate(&path, &app);
        let backup = list(&app)
            .unwrap()
            .into_iter()
            .find(|b| b.destination.is_some())
            .unwrap();
        let plan = prepare_restore(&backup).unwrap();
        let bytes = fs::read(backup.path()).unwrap();
        let replacement = tmp.path().join("replacement");
        fs::write(&replacement, bytes).unwrap();
        fs::set_permissions(
            &replacement,
            fs::metadata(backup.path()).unwrap().permissions(),
        )
        .unwrap();
        fs::rename(replacement, backup.path()).unwrap();
        let current = fs::read(&path).unwrap();
        assert!(
            plan.apply(&app).is_err(),
            "replacement of reviewed backup identity must invalidate confirmation"
        );
        assert_eq!(fs::read(path).unwrap(), current);
    }
    #[cfg(unix)]
    #[test]
    fn restore_rejects_changed_content_redirects_and_legacy_without_writes() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let path = tmp.path().join("config");
        fs::write(&path, "# old\n").unwrap();
        mutate(&path, &app);
        let entries = list(&app).unwrap();
        let legacy = entries.iter().find(|b| b.destination.is_none()).unwrap();
        assert!(prepare_restore(legacy).is_err());
        let backup = entries.iter().find(|b| b.destination.is_some()).unwrap();
        let plan = prepare_restore(backup).unwrap();
        fs::write(&path, "# external\n").unwrap();
        assert!(plan.apply(&app).is_err());
        let plan = prepare_restore(backup).unwrap();
        fs::write(backup.path(), "{}").unwrap();
        assert!(plan.apply(&app).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"# external\n");
        fs::remove_file(&path).unwrap();
        let other = tmp.path().join("other");
        fs::write(&other, "# safe\n").unwrap();
        symlink(&other, &path).unwrap();
        assert!(plan.apply(&app).is_err());
        assert_eq!(fs::read(other).unwrap(), b"# safe\n");
    }
    #[test]
    fn restore_missing_known_target_is_inert_and_safe() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let path = tmp.path().join("ssh/config");
        fs::create_dir(path.parent().unwrap()).unwrap();
        fs::write(&path, "# saved\n").unwrap();
        mutate(&path, &app);
        let backup = list(&app)
            .unwrap()
            .into_iter()
            .find(|b| b.destination.is_some())
            .unwrap();
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
        let plan = prepare_restore(&backup).unwrap();
        assert!(!path.parent().unwrap().exists());
        plan.apply(&app).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"# saved\n");
    }
    #[cfg(unix)]
    #[test]
    fn restore_backup_failure_leaves_target_and_redirected_file_unchanged() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("app");
        let path = tmp.path().join("config");
        fs::write(&path, "# old\n").unwrap();
        mutate(&path, &app);
        let backup = list(&app)
            .unwrap()
            .into_iter()
            .find(|b| b.destination.is_some())
            .unwrap();
        let plan = prepare_restore(&backup).unwrap();
        let before = fs::read(&path).unwrap();
        let external = tmp.path().join("external");
        fs::write(&external, "# safe\n").unwrap();
        let legacy = app.join("backups/config.backup");
        fs::remove_file(&legacy).unwrap();
        symlink(&external, &legacy).unwrap();
        assert!(plan.apply(&app).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(external).unwrap(), b"# safe\n");
    }
}
