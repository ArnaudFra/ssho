//! Saved views: named list contexts, scoped to the canonical root config.
//! A view keeps how the list is searched, filtered, sorted and presented. It
//! never keeps batch marks or availability results, which do not last.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedView {
    pub query: String,
    /// Exact source file filter; the file must still belong to the configuration.
    pub source: Option<PathBuf>,
    pub favorites_only: bool,
    pub show_hidden: bool,
    /// Sort by recent connection instead of by name.
    pub recent: bool,
    pub group_mode: GroupMode,
    pub group_aliases: bool,
    /// Ordered columns; empty leaves the columns in use untouched.
    pub columns: Vec<HostColumn>,
}

impl Default for SavedView {
    fn default() -> Self {
        Self {
            query: String::new(),
            source: None,
            favorites_only: false,
            show_hidden: false,
            recent: false,
            group_mode: GroupMode::Flat,
            group_aliases: false,
            columns: Vec::new(),
        }
    }
}

/// Longest accepted name, in characters: it must stay readable in the list.
pub const MAX_NAME: usize = 40;

pub fn validate_name(name: &str) -> Result<()> {
    anyhow::ensure!(!name.trim().is_empty(), "view name is required");
    anyhow::ensure!(
        name == name.trim(),
        "view name cannot start or end with whitespace"
    );
    anyhow::ensure!(
        !name.chars().any(char::is_control),
        "view name cannot contain control characters"
    );
    anyhow::ensure!(
        name.chars().count() <= MAX_NAME,
        "view name is limited to {MAX_NAME} characters"
    );
    Ok(())
}

#[derive(Debug)]
pub struct Views {
    root: PathBuf,
    scope: String,
    views: BTreeMap<String, SavedView>,
}

impl Views {
    pub fn load(root: &Path, config: &Path) -> Result<Self> {
        ensure_root(root)?;
        let scope = metadata::config_scope_key(config)?;
        let views = Self::entries(&Self::read(root)?, &scope)?;
        Ok(Self {
            root: root.into(),
            scope,
            views,
        })
    }

    pub fn all(&self) -> &BTreeMap<String, SavedView> {
        &self.views
    }

    fn read(root: &Path) -> Result<Value> {
        let value =
            read_json(root, "views.json")?.unwrap_or_else(|| serde_json::json!({"configs":{}}));
        anyhow::ensure!(value.is_object(), "saved views root must be an object");
        Ok(value)
    }

    fn entries(value: &Value, scope: &str) -> Result<BTreeMap<String, SavedView>> {
        let mut result = BTreeMap::new();
        if let Some(views) = value
            .get("configs")
            .and_then(|configs| configs.get(scope))
            .and_then(|scope| scope.get("views"))
        {
            for (name, entry) in views.as_object().context("saved views must be an object")? {
                validate_name(name)?;
                let view: SavedView = serde_json::from_value(entry.clone())
                    .with_context(|| format!("parse saved view {name:?}"))?;
                validate_host_columns_or_empty(&view.columns)?;
                result.insert(name.clone(), view);
            }
        }
        Ok(result)
    }

    /// Re-read the file, edit this scope only, then publish and adopt the result.
    fn update(&mut self, edit: impl FnOnce(&mut Map<String, Value>) -> Result<()>) -> Result<()> {
        let mut output = Self::read(&self.root)?;
        let configs = output
            .as_object_mut()
            .context("saved views root must be an object")?
            .entry("configs")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .context("saved views configs must be an object")?;
        let views = configs
            .entry(self.scope.clone())
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .context("saved views scope must be an object")?
            .entry("views")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .context("saved views must be an object")?;
        edit(views)?;
        let committed = Self::entries(&output, &self.scope)?;
        atomic_json(&self.root, "views.json", &output, 0o600)?;
        self.views = committed;
        Ok(())
    }

    /// `original` is `None` for a creation, `Some` for an update or a rename.
    /// Names are exact and case-sensitive; another view is never overwritten.
    pub fn save(&mut self, original: Option<&str>, name: &str, view: &SavedView) -> Result<()> {
        validate_name(name)?;
        validate_host_columns_or_empty(&view.columns)?;
        let value = serde_json::to_value(view)?;
        self.update(|views| {
            if original != Some(name) {
                anyhow::ensure!(
                    !views.contains_key(name),
                    "a view named {name:?} already exists"
                );
            }
            if let Some(original) = original {
                anyhow::ensure!(
                    views.remove(original).is_some(),
                    "view {original:?} no longer exists"
                );
            }
            views.insert(name.to_owned(), value);
            Ok(())
        })
    }

    pub fn delete(&mut self, name: &str) -> Result<()> {
        self.update(|views| {
            anyhow::ensure!(
                views.remove(name).is_some(),
                "view {name:?} no longer exists"
            );
            Ok(())
        })
    }
}

fn validate_host_columns_or_empty(columns: &[HostColumn]) -> Result<()> {
    if columns.is_empty() {
        Ok(())
    } else {
        validate_host_columns(columns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let config = temp.path().join("config");
        fs::write(&config, "Host a\n").unwrap();
        (temp, root, config)
    }

    fn sample() -> SavedView {
        SavedView {
            query: "tag:prod".into(),
            favorites_only: true,
            recent: true,
            group_mode: GroupMode::Files,
            columns: vec![HostColumn::Name, HostColumn::Tags],
            ..SavedView::default()
        }
    }

    #[test]
    fn views_are_saved_per_configuration_and_reloaded() {
        let (temp, root, config) = fixture();
        let mut views = Views::load(&root, &config).unwrap();
        assert!(views.all().is_empty());
        views.save(None, "Production", &sample()).unwrap();
        let reloaded = Views::load(&root, &config).unwrap();
        assert_eq!(reloaded.all()["Production"], sample());
        // Another configuration has its own, empty, set of views.
        let other = temp.path().join("other");
        fs::write(&other, "Host b\n").unwrap();
        assert!(Views::load(&root, &other).unwrap().all().is_empty());
    }

    #[test]
    fn rename_update_and_delete_never_overwrite_another_view() {
        let (_temp, root, config) = fixture();
        let mut views = Views::load(&root, &config).unwrap();
        views.save(None, "Production", &sample()).unwrap();
        views
            .save(None, "Databases", &SavedView::default())
            .unwrap();
        // A creation or a rename onto an existing name is refused, nothing changes.
        assert!(views
            .save(None, "Production", &SavedView::default())
            .is_err());
        assert!(views
            .save(Some("Databases"), "Production", &SavedView::default())
            .is_err());
        assert_eq!(views.all()["Production"], sample());
        assert_eq!(views.all().len(), 2);
        // Update in place, then rename.
        let mut changed = sample();
        changed.query = "tag:db".into();
        views
            .save(Some("Production"), "Production", &changed)
            .unwrap();
        assert_eq!(views.all()["Production"].query, "tag:db");
        views.save(Some("Production"), "Prod", &changed).unwrap();
        assert!(!views.all().contains_key("Production"));
        assert_eq!(views.all()["Prod"], changed);
        views.delete("Prod").unwrap();
        assert!(views.delete("Prod").is_err());
        assert_eq!(views.all().len(), 1);
    }

    #[test]
    fn names_are_validated_and_a_damaged_file_is_an_error() {
        let (_temp, root, config) = fixture();
        let mut views = Views::load(&root, &config).unwrap();
        for name in ["", "  ", " lead", "trail ", "a\nb"] {
            assert!(views.save(None, name, &sample()).is_err(), "{name:?}");
        }
        assert!(views
            .save(None, &"x".repeat(MAX_NAME + 1), &sample())
            .is_err());
        views.save(None, &"x".repeat(MAX_NAME), &sample()).unwrap();
        // A view never widens silently: unknown or duplicate columns are refused.
        let mut wrong = sample();
        wrong.columns = vec![HostColumn::Tags];
        assert!(views.save(None, "wrong", &wrong).is_err());
        fs::write(root.join("views.json"), b"[]").unwrap();
        assert!(Views::load(&root, &config).is_err());
    }
}
