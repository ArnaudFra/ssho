//! Named forwarding drafts, scoped to the canonical root config and SSH alias.
use super::*;
use crate::types::Host;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    #[default]
    Shell,
    TunnelOnly,
}
impl LaunchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shell => "Shell session",
            Self::TunnelOnly => "Tunnel only",
        }
    }
}
pub type HostModes = BTreeMap<String, BTreeMap<String, LaunchMode>>;
pub type HostProfiles = BTreeMap<String, BTreeMap<String, Forward>>;

pub fn validate_name(name: &str) -> Result<()> {
    anyhow::ensure!(!name.trim().is_empty(), "profile name is required");
    anyhow::ensure!(
        name == name.trim(),
        "profile name cannot start or end with whitespace"
    );
    anyhow::ensure!(
        name.chars().count() <= 100,
        "profile name is limited to 100 characters"
    );
    anyhow::ensure!(
        !name.chars().any(char::is_control),
        "profile name cannot contain control characters"
    );
    Ok(())
}

#[derive(Debug)]
pub struct TunnelProfiles {
    root: PathBuf,
    scope: String,
    hosts: HostProfiles,
    modes: HostModes,
}
impl TunnelProfiles {
    pub fn load(root: &Path, config: &Path) -> Result<Self> {
        ensure_root(root)?;
        let scope = metadata::config_scope_key(config)?;
        let document = Self::read(root)?;
        let hosts = Self::entries(&document, &scope)?;
        let modes = Self::mode_entries(&document, &scope)?;
        Ok(Self {
            root: root.into(),
            scope,
            hosts,
            modes,
        })
    }
    pub fn hosts(&self) -> &HostProfiles {
        &self.hosts
    }
    pub fn modes(&self) -> &HostModes {
        &self.modes
    }
    fn read(root: &Path) -> Result<Value> {
        let value = read_json(root, "tunnel_profiles.json")?
            .unwrap_or_else(|| serde_json::json!({"configs":{}}));
        let configs = value
            .as_object()
            .context("tunnel profiles root must be an object")?
            .get("configs");
        if let Some(configs) = configs {
            for scope in configs
                .as_object()
                .context("tunnel configs must be an object")?
                .values()
            {
                let scope = scope
                    .as_object()
                    .context("tunnel scope must be an object")?;
                if let Some(hosts) = scope.get("hosts") {
                    for host in hosts
                        .as_object()
                        .context("tunnel hosts must be an object")?
                        .values()
                    {
                        let host = host.as_object().context("tunnel host must be an object")?;
                        if let Some(profiles) = host.get("profiles") {
                            for (name, entry) in
                                profiles.as_object().context("profiles must be an object")?
                            {
                                validate_name(name)?;
                                let entry =
                                    entry.as_object().context("profile must be an object")?;
                                let forward: Forward = serde_json::from_value(
                                    entry
                                        .get("forward")
                                        .context("profile forward is required")?
                                        .clone(),
                                )?;
                                forward.validate()?;
                                let _: LaunchMode = serde_json::from_value(
                                    entry
                                        .get("mode")
                                        .cloned()
                                        .unwrap_or(serde_json::json!("shell")),
                                )?;
                            }
                        }
                    }
                }
            }
        }
        Ok(value)
    }
    fn entries(value: &Value, scope: &str) -> Result<HostProfiles> {
        let mut result = BTreeMap::new();
        if let Some(hosts) = value
            .get("configs")
            .and_then(|v| v.get(scope))
            .and_then(|v| v.get("hosts"))
            .and_then(Value::as_object)
        {
            for (host, entry) in hosts {
                let mut profiles = BTreeMap::new();
                if let Some(entries) = entry.get("profiles").and_then(Value::as_object) {
                    for (name, profile) in entries {
                        profiles.insert(
                            name.clone(),
                            serde_json::from_value(profile["forward"].clone())?,
                        );
                    }
                }
                result.insert(host.clone(), profiles);
            }
        }
        Ok(result)
    }
    fn mode_entries(value: &Value, scope: &str) -> Result<HostModes> {
        let mut result = BTreeMap::new();
        if let Some(hosts) = value
            .get("configs")
            .and_then(|v| v.get(scope))
            .and_then(|v| v.get("hosts"))
            .and_then(Value::as_object)
        {
            for (host, entry) in hosts {
                let mut modes = BTreeMap::new();
                if let Some(profiles) = entry.get("profiles").and_then(Value::as_object) {
                    for (name, profile) in profiles {
                        modes.insert(
                            name.clone(),
                            serde_json::from_value(
                                profile
                                    .get("mode")
                                    .cloned()
                                    .unwrap_or(serde_json::json!("shell")),
                            )?,
                        );
                    }
                }
                result.insert(host.clone(), modes);
            }
        }
        Ok(result)
    }
    fn update(&mut self, edit: impl FnOnce(&mut Map<String, Value>) -> Result<()>) -> Result<()> {
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
            .or_insert_with(|| serde_json::json!({}));
        let hosts = scope
            .as_object_mut()
            .unwrap()
            .entry("hosts")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .unwrap();
        edit(hosts)?;
        let committed = Self::entries(&output, &self.scope)?;
        let modes = Self::mode_entries(&output, &self.scope)?;
        atomic_json(&self.root, "tunnel_profiles.json", &output, 0o600)?;
        self.modes = modes;
        self.hosts = committed;
        Ok(())
    }
    /// `original` is None for creation; Some for replacement or explicit rename.
    /// Names are exact/case-sensitive. Never overwrite another named profile.
    pub fn save(
        &mut self,
        host: &str,
        original: Option<&str>,
        name: &str,
        forward: &Forward,
    ) -> Result<()> {
        self.save_internal(host, original, name, forward, None)
    }
    pub fn save_with_mode(
        &mut self,
        host: &str,
        original: Option<&str>,
        name: &str,
        forward: &Forward,
        mode: LaunchMode,
    ) -> Result<()> {
        self.save_internal(host, original, name, forward, Some(mode))
    }
    fn save_internal(
        &mut self,
        host: &str,
        original: Option<&str>,
        name: &str,
        forward: &Forward,
        mode: Option<LaunchMode>,
    ) -> Result<()> {
        validate_name(name)?;
        forward.validate()?;
        self.update(|hosts| {
            let host = hosts
                .entry(host.to_owned())
                .or_insert_with(|| serde_json::json!({}));
            let profiles = host
                .as_object_mut()
                .unwrap()
                .entry("profiles")
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
                .unwrap();
            anyhow::ensure!(
                original == Some(name) || !profiles.contains_key(name),
                "profile '{name}' already exists"
            );
            if let Some(old) = original {
                anyhow::ensure!(
                    profiles.contains_key(old),
                    "original profile '{old}' no longer exists"
                );
            }
            let mut entry = original
                .and_then(|old| profiles.remove(old))
                .unwrap_or_else(|| serde_json::json!({}));
            let forward_value = entry
                .as_object_mut()
                .unwrap()
                .entry("forward")
                .or_insert_with(|| serde_json::json!({}));
            merge_value(forward_value, serde_json::to_value(forward)?);
            if let Some(mode) = mode {
                entry
                    .as_object_mut()
                    .unwrap()
                    .insert("mode".into(), serde_json::to_value(mode)?);
            }
            profiles.insert(name.to_owned(), entry);
            Ok(())
        })
    }
    pub fn delete(&mut self, host: &str, name: &str) -> Result<()> {
        self.update(|hosts| {
            let profiles = hosts
                .get_mut(host)
                .and_then(|v| v.get_mut("profiles"))
                .and_then(Value::as_object_mut)
                .context("host has no profiles")?;
            anyhow::ensure!(
                profiles.remove(name).is_some(),
                "profile '{name}' no longer exists"
            );
            Ok(())
        })
    }
    /// A destination alias with stored data is a conflict; retain both for recovery.
    pub fn rename_hosts(&mut self, original: &Host, replacement: &Host) -> Result<()> {
        let mapping = metadata::alias_renames(original, replacement);
        if mapping.is_empty() {
            return Ok(());
        }
        self.update(|hosts| {
            for (old, new) in &mapping {
                anyhow::ensure!(
                    !(hosts.contains_key(old) && hosts.contains_key(new)),
                    "tunnel profile destination {new} is occupied; original {old} retained"
                );
            }
            for (old, new) in mapping {
                if let Some(entry) = hosts.remove(&old) {
                    hosts.insert(new, entry);
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn forward(kind: &str) -> Forward {
        Forward {
            kind: kind.into(),
            local_port: "8080".into(),
            remote_port: "80".into(),
            remote_host: "localhost".into(),
            bind_address: "127.0.0.1".into(),
        }
    }
    #[test]
    fn profiles_persist_multiple_hosts_scopes_and_all_types() {
        let dir = tempdir().unwrap();
        let config = dir.path().join("missing/config");
        let mut store = TunnelProfiles::load(dir.path(), &config).unwrap();
        for kind in ["local", "remote", "dynamic"] {
            store.save("prod", None, kind, &forward(kind)).unwrap();
        }
        store
            .save("other", None, "local", &forward("dynamic"))
            .unwrap();
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "").unwrap();
        let loaded = TunnelProfiles::load(dir.path(), &config).unwrap();
        assert_eq!(loaded.hosts()["prod"].len(), 3);
        assert_eq!(loaded.hosts()["other"]["local"], forward("dynamic"));
        for kind in ["local", "remote", "dynamic"] {
            assert_eq!(loaded.hosts()["prod"][kind], forward(kind));
        }
        assert!(
            TunnelProfiles::load(dir.path(), &dir.path().join("different"))
                .unwrap()
                .hosts()
                .is_empty()
        );
    }
    #[test]
    fn profiles_validate_names_conflicts_rename_delete_and_preserve_unknowns() {
        let dir = tempdir().unwrap();
        let config = dir.path().join("config");
        let scope = metadata::config_scope_key(&config).unwrap();
        let path = dir.path().join("tunnel_profiles.json");
        let original = serde_json::json!({"future":1,"configs":{scope.clone():{"extra":2,"hosts":{"prod":{"extra":3,"profiles":{"old":{"extra":4,"forward":{"future":5,"type":"local","local_port":"8080","remote_host":"localhost","remote_port":"80","bind_address":""}}}}}}}});
        fs::write(&path, original.to_string()).unwrap();
        let mut store = TunnelProfiles::load(dir.path(), &config).unwrap();
        for name in ["", " ", " padded", "bad\nname"] {
            assert!(store.save("prod", None, name, &forward("local")).is_err());
        }
        store
            .save("prod", None, "taken", &forward("dynamic"))
            .unwrap();
        let before = fs::read(&path).unwrap();
        assert!(store
            .save("prod", Some("old"), "taken", &forward("remote"))
            .is_err());
        assert!(store
            .save("prod", None, "taken", &forward("remote"))
            .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        store
            .save("prod", Some("old"), "renamed", &forward("remote"))
            .unwrap();
        let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["future"], 1);
        assert_eq!(value["configs"][&scope]["extra"], 2);
        let host = &value["configs"][&scope]["hosts"]["prod"];
        assert_eq!(host["extra"], 3);
        assert_eq!(host["profiles"]["renamed"]["extra"], 4);
        assert_eq!(host["profiles"]["renamed"]["forward"]["future"], 5);
        assert_eq!(store.hosts()["prod"]["renamed"], forward("remote"));
        assert!(!store.hosts()["prod"].contains_key("old"));
        store.delete("prod", "renamed").unwrap();
        assert_eq!(
            TunnelProfiles::load(dir.path(), &config).unwrap().hosts()["prod"].len(),
            1
        );
    }
    #[test]
    fn profile_alias_rename_preserves_secondary_and_refuses_collisions() {
        let dir = tempdir().unwrap();
        let mut store = TunnelProfiles::load(dir.path(), &dir.path().join("config")).unwrap();
        store
            .save("primary", None, "primary", &forward("local"))
            .unwrap();
        store
            .save("secondary", None, "secondary", &forward("remote"))
            .unwrap();
        let old = Host {
            name: "secondary".into(),
            aliases: vec!["primary".into(), "secondary".into()],
            ..Host::default()
        };
        let new = Host {
            name: "primary".into(),
            aliases: vec!["primary".into(), "new".into()],
            ..old.clone()
        };
        store.rename_hosts(&old, &new).unwrap();
        assert!(store.hosts()["primary"].contains_key("primary"));
        assert!(store.hosts()["new"].contains_key("secondary"));
        let old = Host {
            name: "new".into(),
            ..Host::default()
        };
        let new = Host {
            name: "primary".into(),
            ..Host::default()
        };
        assert!(store.rename_hosts(&old, &new).is_err());
        assert!(store.hosts()["new"].contains_key("secondary"));
    }
    #[cfg(unix)]
    #[test]
    fn profiles_private_atomic_no_follow_and_failed_write_keeps_memory() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempdir().unwrap();
        let config = dir.path().join("config");
        let path = dir.path().join("tunnel_profiles.json");
        let mut store = TunnelProfiles::load(dir.path(), &config).unwrap();
        store.save("prod", None, "keep", &forward("local")).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let external = dir.path().join("external");
        fs::rename(&path, &external).unwrap();
        let before = fs::read(&external).unwrap();
        symlink(&external, &path).unwrap();
        assert!(store.delete("prod", "keep").is_err());
        assert!(store.hosts()["prod"].contains_key("keep"));
        assert_eq!(fs::read(external).unwrap(), before);
        assert!(TunnelProfiles::load(dir.path(), &config).is_err());
    }
}
