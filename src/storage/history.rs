//! Attempt and process-completion history, preserving unknown JSON fields.
use super::*;

#[derive(Debug)]
pub struct History {
    pub connections: BTreeMap<String, ConnectionInfo>,
    root: PathBuf,
    original: Value,
}

impl History {
    pub fn load(root: &Path) -> Result<Self> {
        ensure_root(root)?;
        let original = read_json(root, "sshm_history.json")?
            .unwrap_or_else(|| serde_json::json!({"connections": {}}));
        if !original.is_object() {
            bail!("history root must be an object");
        }
        let connections = match original.get("connections") {
            Some(Value::Null) | None => BTreeMap::new(),
            Some(value) => {
                let mut values = value
                    .as_object()
                    .context("history connections must be an object")?
                    .clone();
                for entry in values.values_mut() {
                    // Go initializes omitted struct fields to their zero values.
                    let mut defaults = serde_json::json!({"host_name":"", "last_connect":"0001-01-01T00:00:00Z", "connect_count":0});
                    merge_value(&mut defaults, entry.clone());
                    if let Some(forward) =
                        defaults.get_mut("port_forwarding").filter(|v| !v.is_null())
                    {
                        let mut zero = serde_json::to_value(Forward::default())?;
                        merge_value(&mut zero, forward.clone());
                        *forward = zero;
                    }
                    *entry = defaults;
                }
                serde_json::from_value(Value::Object(values)).context("parse connection history")?
            }
        };
        Ok(Self {
            connections,
            root: root.to_path_buf(),
            original,
        })
    }

    /// Call once at the same connection/tunnel attempt boundary as SSHM.
    pub fn record(&mut self, name: &str, forward: Option<&Forward>) -> Result<()> {
        self.record_at(name, forward, Utc::now())
    }

    pub fn record_at(
        &mut self,
        name: &str,
        forward: Option<&Forward>,
        at: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let mut connections = self.connections.clone();
        let connection = connections
            .entry(name.to_owned())
            .or_insert_with(|| ConnectionInfo {
                host_name: name.to_owned(),
                last_connect: at,
                connect_count: 0,
                port_forwarding: None,
                ..ConnectionInfo::default()
            });
        connection.last_connect = at;
        connection.connect_count = connection
            .connect_count
            .checked_add(1)
            .context("connection count overflow")?;
        if let Some(forward) = forward {
            connection.port_forwarding = Some(forward.clone());
        }
        self.save(connections)
    }

    pub fn complete(
        &mut self,
        name: &str,
        exit_code: Option<i32>,
        launch_error: Option<String>,
    ) -> Result<()> {
        self.complete_at(name, exit_code, launch_error, Utc::now())
    }

    pub fn complete_at(
        &mut self,
        name: &str,
        exit_code: Option<i32>,
        launch_error: Option<String>,
        now: chrono::DateTime<Utc>,
    ) -> Result<()> {
        let mut connections = self.connections.clone();
        let connection = connections
            .get_mut(name)
            .context("connection attempt is absent from history")?;
        connection.last_result = Some(crate::types::ConnectionResult {
            completed_at: now,
            exit_code,
            launch_error,
        });
        if exit_code == Some(0) {
            connection.last_successful_completion = Some(now);
        }
        self.save(connections)
    }

    fn save(&mut self, connections: BTreeMap<String, ConnectionInfo>) -> Result<()> {
        let mut output = self.original.clone();
        if !output.is_object() {
            bail!("history root must be an object");
        }
        let mut entries = Map::new();
        for (name, connection) in &connections {
            let mut entry = self
                .original
                .get("connections")
                .and_then(|v| v.get(name))
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()));
            merge_value(&mut entry, serde_json::to_value(connection)?);
            entries.insert(name.clone(), entry);
        }
        output["connections"] = Value::Object(entries);
        atomic_json(&self.root, "sshm_history.json", &output, 0o600)?;
        self.connections = connections;
        self.original = output;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_attempts_results_and_success_preserve_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("sshm_history.json"), r#"{"future":true,"connections":{"prod":{"host_name":"prod","last_connect":"2025-01-01T00:00:00Z","connect_count":4,"custom":42,"last_result":{"completed_at":"2025-01-01T00:00:00Z","exit_code":37,"launch_error":null,"future_result":true}}}}"#).unwrap();
        let mut history = History::load(temp.path()).unwrap();
        assert!(history.connections["prod"]
            .last_successful_completion
            .is_none());
        history.record("prod", None).unwrap();
        history.complete("prod", Some(0), None).unwrap();
        let success = history.connections["prod"]
            .last_successful_completion
            .unwrap();
        history.record("prod", None).unwrap();
        history.complete("prod", Some(37), None).unwrap();
        assert_eq!(
            history.connections["prod"].last_successful_completion,
            Some(success)
        );
        history.record("prod", None).unwrap();
        history
            .complete("prod", None, Some("missing ssh".into()))
            .unwrap();
        let loaded = History::load(temp.path()).unwrap();
        let connection = &loaded.connections["prod"];
        assert_eq!(connection.connect_count, 7);
        assert_eq!(connection.last_result.as_ref().unwrap().exit_code, None);
        assert_eq!(
            connection
                .last_result
                .as_ref()
                .unwrap()
                .launch_error
                .as_deref(),
            Some("missing ssh")
        );
        assert_eq!(loaded.original["future"], true);
        assert_eq!(loaded.original["connections"]["prod"]["custom"], 42);
        assert_eq!(
            loaded.original["connections"]["prod"]["last_result"]["future_result"],
            true
        );
    }
}

#[cfg(test)]
mod batch_timestamp_tests {
    use super::*;
    #[test]
    fn batch_history_uses_event_time_not_ui_poll_time() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = History::load(tmp.path()).unwrap();
        let attempted = "2020-01-01T00:00:00Z".parse().unwrap();
        let completed = "2020-01-01T00:00:07Z".parse().unwrap();
        h.record_at("frozen", None, attempted).unwrap();
        h.complete_at("frozen", Some(0), None, completed).unwrap();
        let h = History::load(tmp.path()).unwrap();
        assert_eq!(h.connections["frozen"].last_connect, attempted);
        assert_eq!(
            h.connections["frozen"]
                .last_result
                .as_ref()
                .unwrap()
                .completed_at,
            completed
        );
    }
}
