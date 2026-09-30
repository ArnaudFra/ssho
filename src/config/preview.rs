//! Immutable, inert transformations shared by CLI mutations and interactive review.
use super::files::{ConfigMirror, Snapshot, WRITE_LOCK};
use super::{check_add, document, load_hosts_checked, safe_fields, Document};
use crate::types::Host;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FileChange {
    pub(super) snapshot: Snapshot,
    after: Vec<u8>,
}
impl FileChange {
    pub fn path(&self) -> &Path {
        &self.snapshot.path
    }
    pub fn access_path(&self) -> &Path {
        &self.snapshot.access
    }
    pub fn before(&self) -> Option<&[u8]> {
        self.snapshot.bytes.as_deref()
    }
    pub fn after(&self) -> &[u8] {
        &self.after
    }
    pub fn assumptions(&self) -> String {
        format!("Access: {}", self.snapshot.access_description())
    }
}

#[derive(Debug)]
pub struct PreparedChange {
    files: Vec<FileChange>,
    mirror: Option<ConfigMirror>,
    evidence: Vec<Snapshot>,
    browser_graph: Option<(PathBuf, Vec<PathBuf>)>,
}
impl PreparedChange {
    pub(super) fn single(snapshot: Snapshot, after: Vec<u8>) -> Self {
        Self {
            files: vec![FileChange { snapshot, after }],
            mirror: None,
            evidence: vec![],
            browser_graph: None,
        }
    }
    pub(super) fn with_evidence(mut self, snapshot: Snapshot) -> Self {
        self.evidence.push(snapshot);
        self
    }
    pub fn files(&self) -> &[FileChange] {
        &self.files
    }
    pub fn diff(&self) -> String {
        let mut output = String::new();
        for file in &self.files {
            output.push_str(&format!("{}\n", file.assumptions()));
            let before = String::from_utf8_lossy(file.before().unwrap_or_default());
            let after = String::from_utf8_lossy(file.after());
            let path = file.path().display().to_string();
            let diff = similar::TextDiff::from_lines(&before, &after);
            output.push_str(
                &diff
                    .unified_diff()
                    .context_radius(3)
                    .header(&format!("before/{path}"), &format!("after/{path}"))
                    .to_string(),
            );
            if before == after {
                output.push_str("(no content changes)\n");
            }
            output.push('\n');
        }
        output
    }
    fn unchanged(&self) -> Result<()> {
        if let Some((root, files)) = &self.browser_graph {
            anyhow::ensure!(
                super::config_files(root)? == *files,
                "SSH browser Include graph changed; reload and retry"
            );
        }
        if let Some(mirror) = &self.mirror {
            mirror.unchanged()?;
        }
        for file in &self.files {
            file.snapshot.unchanged()?;
        }
        for snapshot in &self.evidence {
            snapshot.unchanged()?;
        }
        Ok(())
    }
    pub fn apply(&self, app_dir: &Path) -> Result<()> {
        self.apply_with_hook(app_dir, || {})
    }
    pub(super) fn apply_with_hook(
        &self,
        app_dir: &Path,
        before_source: impl FnOnce(),
    ) -> Result<()> {
        let _lock = WRITE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("SSH configuration lock poisoned"))?;
        self.unchanged()?;
        if self.files.len() == 1 {
            let file = &self.files[0];
            if file.before() == Some(file.after()) {
                return Ok(());
            }
            let staged = file.snapshot.stage(file.after())?;
            self.unchanged()?;
            file.snapshot.backup(app_dir)?;
            self.unchanged()?;
            return file.snapshot.commit(staged);
        }
        // A move publishes the destination first; its rollback never overwrites
        // a concurrent edit. Both stages and recovery copies precede publication.
        let source = &self.files[0];
        let target = &self.files[1];
        let source_staged = source.snapshot.stage(source.after())?;
        let target_staged = target.snapshot.stage(target.after())?;
        let rollback = target
            .before()
            .map(|bytes| target.snapshot.stage(bytes))
            .transpose()?;
        self.unchanged()?;
        source.snapshot.backup(app_dir)?;
        target.snapshot.backup(app_dir)?;
        self.unchanged()?;
        let published = target
            .snapshot
            .staged_snapshot(&target_staged, target.after())?;
        target.snapshot.commit(target_staged)?;
        before_source();
        if let Err(error) = source.snapshot.commit(source_staged) {
            return match target.snapshot.restore(rollback, &published) {
                Ok(()) => Err(error).context("move failed; target restored and host retained in source"),
                Err(rollback_error) => Err(error).context(format!("move failed; rollback also failed: {rollback_error:#}; recovery copies remain in {}", app_dir.join("backups").display())),
            };
        }
        Ok(())
    }
}

pub fn prepare_add(host: &Host, target: &Path) -> Result<PreparedChange> {
    safe_fields(host)?;
    let snapshot = Snapshot::read(target)?;
    let mut document = Document::new(snapshot.text()?);
    check_add(&document, host)?;
    document.append(&document::serialize(host, document.newline));
    Ok(PreparedChange::single(
        snapshot,
        document.text().into_bytes(),
    ))
}
pub fn prepare_update(original: &Host, replacement: &Host) -> Result<PreparedChange> {
    safe_fields(replacement)?;
    let snapshot = Snapshot::read(&original.source_file)?;
    let mut document = Document::new(snapshot.text()?);
    let block = document.locate(original)?;
    document.replace(&block, original, replacement)?;
    Ok(PreparedChange::single(
        snapshot,
        document.text().into_bytes(),
    ))
}
pub fn prepare_delete(host: &Host) -> Result<PreparedChange> {
    let snapshot = Snapshot::read(&host.source_file)?;
    let mut document = Document::new(snapshot.text()?);
    let block = document.locate(host)?;
    document.remove(&block, &host.name);
    Ok(PreparedChange::single(
        snapshot,
        document.text().into_bytes(),
    ))
}
pub fn prepare_move(host: &Host, target: &Path) -> Result<PreparedChange> {
    let source = Snapshot::read(&host.source_file)?;
    let target = Snapshot::read(target)?;
    anyhow::ensure!(
        source.path != target.path,
        "host '{}' is already in the target config file",
        host.name
    );
    let mut source_doc = Document::new(source.text()?);
    let mut target_doc = Document::new(target.text()?);
    anyhow::ensure!(
        !target_doc.contains(&host.name),
        "host '{}' already exists in target config",
        host.name
    );
    let block = source_doc.locate(host)?;
    let raw = source_doc.extracted(&block, &host.name);
    source_doc.remove(&block, &host.name);
    target_doc.append(&raw);
    Ok(PreparedChange {
        files: vec![
            FileChange {
                snapshot: source,
                after: source_doc.text().into_bytes(),
            },
            FileChange {
                snapshot: target,
                after: target_doc.text().into_bytes(),
            },
        ],
        mirror: None,
        evidence: vec![],
        browser_graph: None,
    })
}
pub fn prepare_copy(host: &Host, target: &Path, root: &Path) -> Result<PreparedChange> {
    safe_fields(host)?;
    // Preserve the UI's full browser-graph collision rule even where the legacy
    // browser and native OpenSSH resolve relative Include paths differently.
    let browser_files = super::config_files(root)?;
    let browser_snapshots = browser_files
        .iter()
        .map(|path| Snapshot::read(path))
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(
        !load_hosts_checked(root)?
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&host.name)),
        "host '{}' already exists in SSH configuration (including Includes and hidden hosts)",
        host.name
    );
    let mut mirror = ConfigMirror::new(root)?;
    anyhow::ensure!(
        !load_hosts_checked(mirror.root())?
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&host.name)),
        "host '{}' already exists in SSH configuration (including Includes and hidden hosts)",
        host.name
    );
    let snapshot = mirror.snapshot(target)?.clone();
    let mut document = Document::new(snapshot.text()?);
    check_add(&document, host)?;
    let boundary =
        document.insert_before_first_block(&document::serialize_copy(host, document.newline));
    anyhow::ensure!(boundary || snapshot.path == std::fs::canonicalize(root)?, "destination Include has no Host/Match boundary; choose the root file or another destination and retry");
    let target: PathBuf = snapshot.path.clone();
    mirror.replace(&target, &document.text())?;
    let effective = crate::runtime::effective::resolve(mirror.root(), &host.name)
        .map_err(|error| anyhow::anyhow!("cannot preflight copy destination; choose another destination or fix SSH configuration and retry: {error:#}"))?;
    anyhow::ensure!(effective.hostname == host.hostname && (host.user.is_empty() || effective.user == host.user) && (host.port.is_empty() || effective.port == host.port),
        "destination precedence changes the requested copy target (resolved {}@{}:{}); choose an earlier destination, usually the root file, or adjust global/Include defaults and retry", effective.user, effective.hostname, effective.port);
    mirror.unchanged()?;
    let mut prepared = PreparedChange::single(snapshot, document.text().into_bytes());
    prepared.mirror = Some(mirror);
    prepared.evidence = browser_snapshots;
    prepared.browser_graph = Some((root.to_owned(), browser_files));
    prepared.unchanged()?;
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn copy_preview_checks_full_native_graph_again_at_confirmation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("config");
        let include = tmp.path().join("include");
        let app = tmp.path().join("app");
        fs::write(
            &root,
            format!(
                "Include {}\nHost source\n HostName source.example\n",
                include.display()
            ),
        )
        .unwrap();
        fs::write(&include, "Host included\n HostName included.example\n").unwrap();
        let host = Host {
            name: "copy".into(),
            hostname: "copy.example".into(),
            port: "22".into(),
            ..Host::default()
        };
        let before = fs::read(&root).unwrap();
        let plan = prepare_copy(&host, &root, &root).unwrap();
        assert!(!app.exists());
        assert_eq!(fs::read(&root).unwrap(), before);
        fs::write(&include, "Host added-externally\n HostName other.example\n").unwrap();
        assert!(plan.apply(&app).is_err());
        assert_eq!(fs::read(&root).unwrap(), before);
        assert!(!app.exists());
    }
    #[cfg(unix)]
    #[test]
    fn move_rollback_does_not_overwrite_external_permission_change() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("config");
        let target = tmp.path().join("other");
        let app = tmp.path().join("app");
        fs::write(&root, "Host source\n HostName source.example\n").unwrap();
        fs::write(&target, "# original\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let host = super::super::find_host(&root, "source").unwrap();
        let plan = prepare_move(&host, &target).unwrap();
        let output = plan.files()[1].after().to_vec();
        let error = plan
            .apply_with_hook(&app, || {
                fs::write(&root, "# external\n").unwrap();
                fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
            })
            .unwrap_err();
        assert!(error.to_string().contains("rollback also failed"));
        assert_eq!(fs::read(&target).unwrap(), output);
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}

#[cfg(test)]
mod collision_tests {
    use super::*;
    use std::fs;
    #[test]
    fn copy_keeps_browser_graph_collision_check_for_relative_include() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("config");
        let relative = format!(
            "restore-{}",
            tmp.path().file_name().unwrap().to_string_lossy()
        );
        let parts = tmp.path().join(&relative);
        fs::write(&parts, "Host occupied\n HostName hidden.example\n").unwrap();
        fs::write(
            &root,
            format!("Include {relative}\nHost source\n HostName source.example\n"),
        )
        .unwrap();
        assert!(load_hosts_checked(&root)
            .unwrap()
            .iter()
            .any(|host| host.name == "occupied"));
        let host = Host {
            name: "occupied".into(),
            hostname: "copy.example".into(),
            port: "22".into(),
            ..Host::default()
        };
        assert!(prepare_copy(&host, &root, &root).is_err(), "copy must retain the browser graph collision check even when native relative Include resolves elsewhere");
        fs::write(&parts, "Host unrelated\n HostName other.example\n").unwrap();
        let plan = prepare_copy(&host, &root, &root).unwrap();
        let before = fs::read(&root).unwrap();
        fs::write(&parts, "Host occupied\n HostName hidden.example\n").unwrap();
        let app = tmp.path().join("app");
        assert!(plan.apply(&app).is_err());
        assert!(!app.exists());
        assert_eq!(fs::read(&root).unwrap(), before);
    }
}
