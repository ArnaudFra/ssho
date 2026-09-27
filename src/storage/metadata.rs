//! Favorites and notes scoped by the canonical root SSH configuration and alias.
use super::*;
use crate::types::Host;

/// Stable absolute config identity, also before a first config file is created.
/// Existing symlinks are resolved; missing suffixes use the nearest canonical ancestor.
pub fn config_scope_key(config: &Path) -> Result<String> {
    fn resolve(path: &Path) -> Result<PathBuf> {
        match fs::canonicalize(path) {
            Ok(path) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let parent = path.parent().context("configuration path has no parent")?;
                let parent = resolve(parent)?;
                match path.components().next_back() {
                    Some(std::path::Component::ParentDir) => {
                        Ok(parent.parent().unwrap_or(&parent).to_path_buf())
                    }
                    Some(std::path::Component::CurDir) => Ok(parent),
                    _ => Ok(parent.join(
                        path.file_name()
                            .context("configuration path has no filename")?,
                    )),
                }
            }
            Err(error) => {
                Err(error).with_context(|| format!("resolve configuration {}", path.display()))
            }
        }
    }
    let absolute = if config.is_absolute() {
        config.to_path_buf()
    } else {
        std::env::current_dir()?.join(config)
    };
    let path = resolve(&absolute)?;
    Ok(path
        .to_str()
        .context("configuration path is not valid UTF-8")?
        .to_owned())
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct HostMetadata {
    pub favorite: bool,
    pub note: String,
}

#[derive(Debug)]
pub struct Metadata {
    root: PathBuf,
    scope: String,
    hosts: BTreeMap<String, HostMetadata>,
}
impl Metadata {
    pub fn load(root: &Path, config: &Path) -> Result<Self> {
        ensure_root(root)?;
        let scope = config_scope_key(config)?;
        let original = Self::read(root)?;
        let hosts = Self::entries(&original, &scope)?;
        Ok(Self {
            root: root.into(),
            scope,
            hosts,
        })
    }
    pub fn hosts(&self) -> &BTreeMap<String, HostMetadata> {
        &self.hosts
    }
    fn read(root: &Path) -> Result<Value> {
        let value =
            read_json(root, "metadata.json")?.unwrap_or_else(|| serde_json::json!({"configs":{}}));
        let configs = value
            .as_object()
            .context("metadata root must be an object")?
            .get("configs");
        if let Some(configs) = configs {
            for scope in configs
                .as_object()
                .context("metadata configs must be an object")?
                .values()
            {
                let scope = scope
                    .as_object()
                    .context("metadata scope must be an object")?;
                if let Some(hosts) = scope.get("hosts") {
                    let hosts = hosts
                        .as_object()
                        .context("metadata hosts must be an object")?;
                    for entry in hosts.values() {
                        if !entry.is_object() {
                            bail!("metadata host must be an object");
                        }
                        let _: HostMetadata =
                            serde_json::from_value(entry.clone()).context("parse host metadata")?;
                    }
                }
            }
        }
        Ok(value)
    }
    fn entries(value: &Value, scope: &str) -> Result<BTreeMap<String, HostMetadata>> {
        value
            .get("configs")
            .and_then(|v| v.get(scope))
            .and_then(|v| v.get("hosts"))
            .map(|v| serde_json::from_value(v.clone()).context("parse host metadata"))
            .unwrap_or_else(|| Ok(BTreeMap::new()))
    }
    fn update(&mut self, edit: impl FnOnce(&mut Map<String, Value>) -> Result<()>) -> Result<()> {
        // Read the latest document so independent configs and unknown fields survive.
        let mut output = Self::read(&self.root)?;
        let configs = output
            .as_object_mut()
            .unwrap()
            .entry("configs")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap();
        let scope = configs
            .entry(self.scope.clone())
            .or_insert_with(|| serde_json::json!({"hosts":{}}));
        let hosts = scope
            .as_object_mut()
            .unwrap()
            .entry("hosts")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap();
        edit(hosts)?;
        let committed = Self::entries(&output, &self.scope)?;
        atomic_json(&self.root, "metadata.json", &output, 0o600)?;
        self.hosts = committed;
        Ok(())
    }
    pub fn set_favorite(&mut self, name: &str, favorite: bool) -> Result<()> {
        self.update(|hosts| {
            hosts
                .entry(name.to_owned())
                .or_insert_with(|| serde_json::json!({}))["favorite"] = Value::Bool(favorite);
            Ok(())
        })
    }
    pub fn set_note(&mut self, name: &str, note: &str) -> Result<()> {
        self.update(|hosts| {
            hosts
                .entry(name.to_owned())
                .or_insert_with(|| serde_json::json!({}))["note"] = Value::String(note.into());
            Ok(())
        })
    }
    /// Retained aliases keep their entries. Pair removed/new aliases in declaration
    /// order, matching form rename slots; deletion alone never targets a survivor.
    pub fn rename_hosts(&mut self, original: &Host, replacement: &Host) -> Result<()> {
        let mapping = alias_renames(original, replacement);
        if mapping.is_empty() {
            return Ok(());
        }
        self.update(|hosts| {
            for (old, new) in &mapping {
                if hosts.contains_key(old) && hosts.contains_key(new) {
                    bail!("metadata destination {new} is occupied; original {old} retained");
                }
            }
            for (old, new) in mapping {
                if let Some(entry) = hosts.remove(&old) {
                    let target = hosts
                        .entry(new.clone())
                        .or_insert_with(|| serde_json::json!({}));
                    merge_value(target, entry);
                }
            }
            Ok(())
        })
    }
}

/// Shared declaration-order alias transition used by scoped app data.
pub(crate) fn alias_renames(original: &Host, replacement: &Host) -> Vec<(String, String)> {
    let old = if original.aliases.is_empty() {
        vec![original.name.clone()]
    } else {
        original.aliases.clone()
    };
    let mut new = if replacement.aliases.is_empty() {
        vec![replacement.name.clone()]
    } else {
        replacement.aliases.clone()
    };
    if replacement.aliases == original.aliases {
        for name in &mut new {
            if name == &original.name {
                *name = replacement.name.clone();
            }
        }
    }
    let removed = old.iter().filter(|name| !new.contains(name));
    let added = new.iter().filter(|name| !old.contains(name));
    removed
        .zip(added)
        .map(|(old, new)| (old.clone(), new.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn metadata_persists_separates_configs_and_preserves_unknown_fields() {
        let temp = tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::write(&a, "Host prod\n HostName a\n").unwrap();
        fs::write(&b, "Host prod\n HostName b\n").unwrap();
        let scope = config_scope_key(&a).unwrap();
        let raw = serde_json::json!({"future":42,"configs":{scope.clone():{"future_scope":true,"hosts":{"prod":{"favorite":false,"note":"old","custom":17}}}}});
        fs::write(temp.path().join("metadata.json"), raw.to_string()).unwrap();
        let mut metadata = Metadata::load(temp.path(), &a).unwrap();
        metadata.set_favorite("prod", true).unwrap();
        metadata.set_note("prod", "réseau\nsecond line").unwrap();
        let loaded = Metadata::load(temp.path(), &a).unwrap();
        assert!(loaded.hosts()["prod"].favorite);
        assert_eq!(loaded.hosts()["prod"].note, "réseau\nsecond line");
        assert!(Metadata::load(temp.path(), &b).unwrap().hosts().is_empty());
        let json: Value =
            serde_json::from_slice(&fs::read(temp.path().join("metadata.json")).unwrap()).unwrap();
        assert_eq!(json["future"], 42);
        assert_eq!(json["configs"][&scope]["future_scope"], true);
        assert_eq!(json["configs"][&scope]["hosts"]["prod"]["custom"], 17);
        assert_eq!(fs::read_to_string(a).unwrap(), "Host prod\n HostName a\n");
    }

    #[test]
    fn metadata_scope_is_stable_for_missing_root_and_equivalent_paths() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("missing/ssh/config");
        let before = config_scope_key(&root).unwrap();
        fs::create_dir_all(root.parent().unwrap()).unwrap();
        fs::write(&root, "").unwrap();
        assert_eq!(before, config_scope_key(&root).unwrap());
        assert_eq!(
            before,
            config_scope_key(&root.parent().unwrap().join("./config")).unwrap()
        );
        #[cfg(unix)]
        {
            let alias = temp.path().join("linked");
            std::os::unix::fs::symlink(&root, &alias).unwrap();
            assert_eq!(before, config_scope_key(&alias).unwrap());
        }
    }

    #[test]
    fn metadata_failed_write_retains_committed_values_and_rejects_symlinks() {
        let temp = tempdir().unwrap();
        let mut metadata = Metadata::load(temp.path(), &temp.path().join("config")).unwrap();
        metadata.set_note("prod", "kept").unwrap();
        fs::remove_file(temp.path().join("metadata.json")).unwrap();
        fs::create_dir(temp.path().join("metadata.json")).unwrap();
        assert!(metadata.set_favorite("prod", true).is_err());
        assert!(!metadata.hosts()["prod"].favorite);
        assert_eq!(metadata.hosts()["prod"].note, "kept");
        assert!(Metadata::load(temp.path(), &temp.path().join("config")).is_err());
    }

    #[test]
    fn metadata_rename_secondary_alias_transfers_only_removed_to_new_names() {
        let temp = tempdir().unwrap();
        let mut metadata = Metadata::load(temp.path(), &temp.path().join("config")).unwrap();
        metadata.set_note("primary", "primary note").unwrap();
        metadata.set_note("secondary", "secondary note").unwrap();
        metadata.set_favorite("secondary", true).unwrap();
        let old = Host {
            name: "secondary".into(),
            aliases: vec!["primary".into(), "secondary".into()],
            ..Host::default()
        };
        let new = Host {
            name: "primary".into(),
            aliases: vec!["primary".into(), "new-secondary".into()],
            ..old.clone()
        };
        metadata.rename_hosts(&old, &new).unwrap();
        assert_eq!(metadata.hosts()["primary"].note, "primary note");
        assert_eq!(metadata.hosts()["new-secondary"].note, "secondary note");
        assert!(metadata.hosts()["new-secondary"].favorite);
        assert!(!metadata.hosts().contains_key("secondary"));
        // Removing a secondary alias does not overwrite a retained primary.
        let removed = Host {
            aliases: vec!["primary".into()],
            ..new.clone()
        };
        metadata.rename_hosts(&new, &removed).unwrap();
        assert_eq!(metadata.hosts()["primary"].note, "primary note");
    }
}

#[cfg(test)]
mod collision_tests {
    use super::*;
    #[test]
    fn metadata_rename_refuses_occupied_destination_without_overwriting_notes() {
        let temp = tempfile::tempdir().unwrap();
        let mut metadata = Metadata::load(temp.path(), &temp.path().join("config")).unwrap();
        metadata.set_note("old", "recover me").unwrap();
        metadata.set_note("new", "keep existing").unwrap();
        let old = Host {
            name: "old".into(),
            ..Host::default()
        };
        let new = Host {
            name: "new".into(),
            ..Host::default()
        };
        assert!(metadata.rename_hosts(&old, &new).is_err());
        assert_eq!(metadata.hosts()["old"].note, "recover me");
        assert_eq!(metadata.hosts()["new"].note, "keep existing");
        let loaded = Metadata::load(temp.path(), &temp.path().join("config")).unwrap();
        assert_eq!(loaded.hosts()["old"].note, "recover me");
        assert_eq!(loaded.hosts()["new"].note, "keep existing");
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    #[test]
    fn metadata_scope_normalizes_missing_parent_components() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            config_scope_key(&temp.path().join("config")).unwrap(),
            config_scope_key(&temp.path().join("missing/../config")).unwrap()
        );
    }
    #[cfg(unix)]
    #[test]
    fn metadata_atomic_writes_are_private_and_never_follow_destination_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let mut metadata = Metadata::load(temp.path(), &temp.path().join("config")).unwrap();
        metadata.set_favorite("prod", true).unwrap();
        assert_eq!(
            fs::metadata(temp.path().join("metadata.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::rename(
            temp.path().join("metadata.json"),
            temp.path().join("external"),
        )
        .unwrap();
        let before = fs::read(temp.path().join("external")).unwrap();
        symlink(
            temp.path().join("external"),
            temp.path().join("metadata.json"),
        )
        .unwrap();
        assert!(metadata.set_note("prod", "no").is_err());
        assert_eq!(metadata.hosts()["prod"].note, "");
        assert_eq!(fs::read(temp.path().join("external")).unwrap(), before);
    }
}
