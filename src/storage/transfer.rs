//! Portable export and import of the application data.
//!
//! An export holds what belongs to one SSH configuration (favorites and notes,
//! tunnel profiles, saved views) plus the personal catalogue and the portable
//! preferences. It never holds a key, a password, an SSH file or the history.
//! An import is always made into the configuration in use: the association is
//! "this configuration + alias", never the alias name alone across files.
use super::*;
use metadata::Metadata;
use std::collections::BTreeSet;
use tunnels::TunnelProfiles;

pub const FORMAT: &str = "ssho.export";
pub const VERSION: u32 = 1;
/// A larger file is not an export of this application.
const MAX_SIZE: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Hosts,
    Tunnels,
    Views,
    Commands,
    Preferences,
}

impl Section {
    pub const ALL: [Self; 5] = [
        Self::Hosts,
        Self::Tunnels,
        Self::Views,
        Self::Commands,
        Self::Preferences,
    ];
    fn key(self) -> &'static str {
        match self {
            Self::Hosts => "hosts",
            Self::Tunnels => "tunnels",
            Self::Views => "views",
            Self::Commands => "commands",
            Self::Preferences => "preferences",
        }
    }
    /// Sections whose entries belong to an alias of the configuration.
    fn by_alias(self) -> bool {
        matches!(self, Self::Hosts | Self::Tunnels)
    }
    fn file(self) -> &'static str {
        match self {
            Self::Hosts => "metadata.json",
            Self::Tunnels => "tunnel_profiles.json",
            Self::Views => "views.json",
            Self::Commands => "commands.json",
            Self::Preferences => "config.json",
        }
    }
}

/// Preferences that mean the same on another machine. The SSH launcher is a
/// local executable path and is never exported.
const PORTABLE_PREFERENCES: [&str; 11] = [
    "language",
    "colors",
    "ascii",
    "focus_search_on_start",
    "ping_on_start",
    "check_for_updates",
    "availability_method",
    "group_mode",
    "group_aliases",
    "host_columns",
    "key_bindings",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    pub application: String,
    /// Configuration the alias-bound sections were exported from.
    pub configuration: String,
    pub sections: BTreeMap<String, Map<String, Value>>,
}

impl Export {
    pub fn has(&self, section: Section) -> bool {
        self.sections.contains_key(section.key())
    }
}

/// One entry of a section: its display key, its alias when it has one, its value.
#[derive(Clone, Debug, PartialEq)]
struct Item {
    key: String,
    alias: Option<String>,
    value: Value,
}

fn scope_value<'a>(
    document: &'a Value,
    scope: &str,
    field: &str,
) -> Option<&'a Map<String, Value>> {
    document.get("configs")?.get(scope)?.get(field)?.as_object()
}

fn document(root: &Path, section: Section) -> Result<Value> {
    Ok(read_json(root, section.file())?.unwrap_or_else(|| serde_json::json!({})))
}

/// Favorite and note in one canonical shape, so that an omitted default and a
/// written one compare equal.
fn host_value(entry: &Value) -> Result<Value> {
    let host: metadata::HostMetadata =
        serde_json::from_value(entry.clone()).context("parse host metadata")?;
    Ok(serde_json::json!({"favorite": host.favorite, "note": host.note}))
}

/// Entries of a section as they are stored for `scope`.
fn local_items(root: &Path, scope: &str, section: Section) -> Result<Vec<Item>> {
    let document = document(root, section)?;
    let mut items = Vec::new();
    match section {
        Section::Hosts => {
            for (alias, entry) in scope_value(&document, scope, "hosts").into_iter().flatten() {
                let value = host_value(entry)?;
                // An entry without favorite nor note carries nothing.
                if value["favorite"] == true || value["note"] != "" {
                    items.push(Item {
                        key: alias.clone(),
                        alias: Some(alias.clone()),
                        value,
                    });
                }
            }
        }
        Section::Tunnels => {
            for (alias, entry) in scope_value(&document, scope, "hosts").into_iter().flatten() {
                let profiles = entry.get("profiles").and_then(Value::as_object);
                for (name, profile) in profiles.into_iter().flatten() {
                    items.push(Item {
                        key: format!("{alias} / {name}"),
                        alias: Some(alias.clone()),
                        value: profile.clone(),
                    });
                }
            }
        }
        Section::Views => {
            for (name, view) in scope_value(&document, scope, "views").into_iter().flatten() {
                items.push(Item {
                    key: name.clone(),
                    alias: None,
                    value: view.clone(),
                });
            }
        }
        Section::Commands => {
            let commands = document.get("commands").and_then(Value::as_object);
            for (name, command) in commands.into_iter().flatten() {
                items.push(Item {
                    key: name.clone(),
                    alias: None,
                    value: command.clone(),
                });
            }
        }
        Section::Preferences => {
            // Defaults included: a preference always has a value here.
            let config = serde_json::to_value(AppConfig::load(root)?)?;
            for name in PORTABLE_PREFERENCES {
                if let Some(value) = config.get(name) {
                    items.push(Item {
                        key: name.to_owned(),
                        alias: None,
                        value: value.clone(),
                    });
                }
            }
        }
    }
    Ok(items)
}

/// Directory against which the source file of a view is made portable.
fn config_directory(scope: &str) -> PathBuf {
    Path::new(scope)
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf)
}

/// A view source inside the configuration directory travels as a relative path.
fn portable_view(view: &Value, directory: &Path) -> Value {
    let mut view = view.clone();
    if let Some(source) = view.get("source").and_then(Value::as_str) {
        if let Ok(relative) = Path::new(source).strip_prefix(directory) {
            view["source"] = Value::from(relative.to_string_lossy().into_owned());
        }
    }
    view
}

/// The reverse: a relative source is looked for under the importing configuration.
fn local_view(view: &Value, directory: &Path) -> Value {
    let mut view = view.clone();
    if let Some(source) = view.get("source").and_then(Value::as_str) {
        if Path::new(source).is_relative() {
            view["source"] = Value::from(directory.join(source).to_string_lossy().into_owned());
        }
    }
    view
}

/// How many entries each section of the current data would export.
pub fn inventory(root: &Path, config: &Path) -> Result<BTreeMap<Section, usize>> {
    let scope = metadata::config_scope_key(config)?;
    Section::ALL
        .into_iter()
        .map(|section| Ok((section, local_items(root, &scope, section)?.len())))
        .collect()
}

/// Build the export of `sections` for the configuration in use.
pub fn export(root: &Path, config: &Path, sections: &[Section]) -> Result<Export> {
    anyhow::ensure!(!sections.is_empty(), "nothing is selected for the export");
    let scope = metadata::config_scope_key(config)?;
    let directory = config_directory(&scope);
    let mut output = BTreeMap::new();
    for section in sections {
        let mut entries = Map::new();
        for item in local_items(root, &scope, *section)? {
            match section {
                Section::Tunnels => {
                    let alias = item.alias.clone().unwrap_or_default();
                    let name = item
                        .key
                        .strip_prefix(&format!("{alias} / "))
                        .unwrap_or(&item.key)
                        .to_owned();
                    entries
                        .entry(alias)
                        .or_insert_with(|| serde_json::json!({"profiles": {}}))["profiles"][name] =
                        item.value;
                }
                Section::Views => {
                    entries.insert(item.key, portable_view(&item.value, &directory));
                }
                _ => {
                    entries.insert(item.key, item.value);
                }
            }
        }
        output.insert(section.key().to_owned(), entries);
    }
    Ok(Export {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        application: format!("ssho {}", env!("CARGO_PKG_VERSION")),
        configuration: scope,
        sections: output,
    })
}

/// Write an export to a new file; an existing file is never replaced.
pub fn write(path: &Path, export: &Export) -> Result<()> {
    use std::io::Write as _;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Notes and commands may be private: readable by the owner only.
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            anyhow::anyhow!("{} already exists: choose another name", path.display())
        } else {
            anyhow::Error::new(error).context(format!("create {}", path.display()))
        }
    })?;
    let mut bytes = serde_json::to_vec_pretty(export)?;
    bytes.push(b'\n');
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Read and check an export file: format, version and shape of every section.
pub fn read(path: &Path) -> Result<Export> {
    let size = fs::metadata(path)
        .with_context(|| format!("read {}", path.display()))?
        .len();
    anyhow::ensure!(
        size <= MAX_SIZE,
        "{} is too large to be an export",
        path.display()
    );
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let export: Export = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} is not a ssho export", path.display()))?;
    anyhow::ensure!(
        export.format == FORMAT,
        "{} is not a ssho export",
        path.display()
    );
    anyhow::ensure!(
        export.version == VERSION,
        "export version {} is not supported by this version (expected {VERSION})",
        export.version
    );
    for name in export.sections.keys() {
        anyhow::ensure!(
            Section::ALL.iter().any(|section| section.key() == name),
            "unknown export section {name:?}"
        );
    }
    Ok(export)
}

/// Entries of one section of an export, shaped like the local ones.
fn imported_items(export: &Export, section: Section, directory: &Path) -> Result<Vec<Item>> {
    let Some(entries) = export.sections.get(section.key()) else {
        return Ok(Vec::new());
    };
    let mut items = Vec::new();
    for (key, value) in entries {
        match section {
            Section::Hosts => items.push(Item {
                key: key.clone(),
                alias: Some(key.clone()),
                value: host_value(value)?,
            }),
            Section::Tunnels => {
                let profiles = value
                    .get("profiles")
                    .and_then(Value::as_object)
                    .context("tunnel export entry must hold profiles")?;
                for (name, profile) in profiles {
                    items.push(Item {
                        key: format!("{key} / {name}"),
                        alias: Some(key.clone()),
                        value: profile.clone(),
                    });
                }
            }
            Section::Views => items.push(Item {
                key: key.clone(),
                alias: None,
                value: local_view(value, directory),
            }),
            Section::Commands => items.push(Item {
                key: key.clone(),
                alias: None,
                value: value.clone(),
            }),
            Section::Preferences => {
                // A preference this version does not export is ignored, not written.
                if PORTABLE_PREFERENCES.contains(&key.as_str()) {
                    items.push(Item {
                        key: key.clone(),
                        alias: None,
                        value: value.clone(),
                    });
                }
            }
        }
    }
    Ok(items)
}

/// What an import would do to one section, before any choice is made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SectionPlan {
    /// Entries absent here.
    pub new: Vec<String>,
    /// Entries already present with the same content.
    pub same: usize,
    /// Entries present here with a different content.
    pub conflicts: Vec<String>,
    /// Entries bound to an alias this configuration does not declare.
    pub unknown: Vec<String>,
    /// Local entries the file does not hold (removed only by a full replacement).
    pub local_only: usize,
    /// Views whose source file is not part of this configuration.
    pub missing_sources: Vec<String>,
}

pub type Plan = BTreeMap<Section, SectionPlan>;

/// Compare an export with the data of the configuration in use.
pub fn plan(
    root: &Path,
    config: &Path,
    export: &Export,
    aliases: &BTreeSet<String>,
    files: &[PathBuf],
) -> Result<Plan> {
    let scope = metadata::config_scope_key(config)?;
    let directory = config_directory(&scope);
    let mut plan = Plan::new();
    for section in Section::ALL {
        if !export.has(section) {
            continue;
        }
        let local = local_items(root, &scope, section)?;
        let imported = imported_items(export, section, &directory)?;
        let mut entry = SectionPlan::default();
        for item in &imported {
            if item
                .alias
                .as_ref()
                .is_some_and(|alias| !aliases.contains(alias))
            {
                entry.unknown.push(item.key.clone());
                continue;
            }
            match local.iter().find(|existing| existing.key == item.key) {
                None => entry.new.push(item.key.clone()),
                Some(existing) if existing.value == item.value => entry.same += 1,
                Some(_) => entry.conflicts.push(item.key.clone()),
            }
            if section == Section::Views {
                if let Some(source) = item.value.get("source").and_then(Value::as_str) {
                    if !files.iter().any(|file| file == Path::new(source)) {
                        entry.missing_sources.push(item.key.clone());
                    }
                }
            }
        }
        entry.local_only = local
            .iter()
            .filter(|existing| !imported.iter().any(|item| item.key == existing.key))
            .count();
        plan.insert(section, entry);
    }
    Ok(plan)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// Add what is absent here; never change an existing entry.
    #[default]
    AddNew,
    /// Add what is absent and replace the entries that differ.
    ReplaceConflicts,
    /// Make each chosen section equal to the file: local-only entries are removed.
    ReplaceSections,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// Folder holding the files as they were before the import.
    pub backup: PathBuf,
    pub written: usize,
    pub removed: usize,
    pub skipped: usize,
}

fn object<'a>(value: &'a mut Value, key: &str) -> &'a mut Value {
    if !value.is_object() {
        *value = serde_json::json!({});
    }
    let map = value.as_object_mut().expect("just made an object");
    let entry = map
        .entry(key.to_owned())
        .or_insert_with(|| serde_json::json!({}));
    if !entry.is_object() {
        *entry = serde_json::json!({});
    }
    entry
}

/// Import the chosen sections into the configuration in use. The new files are
/// first written apart and read back by the application itself; the current
/// files are then copied to a backup folder, and only then replaced.
pub fn apply(
    root: &Path,
    config: &Path,
    export: &Export,
    sections: &[Section],
    policy: Policy,
    keep_unknown: bool,
    aliases: &BTreeSet<String>,
) -> Result<Applied> {
    anyhow::ensure!(!sections.is_empty(), "nothing is selected for the import");
    ensure_root(root)?;
    let scope = metadata::config_scope_key(config)?;
    let directory = config_directory(&scope);
    let (mut written, mut removed, mut skipped) = (0, 0, 0);
    let mut documents: Vec<(Section, Value)> = Vec::new();
    for section in sections {
        anyhow::ensure!(
            export.has(*section),
            "the export has no {} section",
            section.key()
        );
        let mut output = document(root, *section)?;
        let local = local_items(root, &scope, *section)?;
        let mut imported = imported_items(export, *section, &directory)?;
        let before = imported.len();
        if section.by_alias() && !keep_unknown {
            imported.retain(|item| item.alias.as_ref().is_some_and(|a| aliases.contains(a)));
        }
        skipped += before - imported.len();
        if policy == Policy::AddNew {
            let before = imported.len();
            imported.retain(|item| !local.iter().any(|existing| existing.key == item.key));
            skipped += before - imported.len();
        }
        let replace = policy == Policy::ReplaceSections;
        if replace {
            removed += local
                .iter()
                .filter(|existing| !imported.iter().any(|item| item.key == existing.key))
                .count();
        }
        written += imported.len();
        match section {
            Section::Hosts => {
                let hosts = object(object(object(&mut output, "configs"), &scope), "hosts");
                if replace {
                    *hosts = serde_json::json!({});
                }
                for item in imported {
                    hosts[item.key] = item.value;
                }
            }
            Section::Tunnels => {
                let hosts = object(object(object(&mut output, "configs"), &scope), "hosts");
                if replace {
                    *hosts = serde_json::json!({});
                }
                for item in imported {
                    let alias = item.alias.unwrap_or_default();
                    let name = item
                        .key
                        .strip_prefix(&format!("{alias} / "))
                        .unwrap_or(&item.key)
                        .to_owned();
                    object(object(hosts, &alias), "profiles")[name] = item.value;
                }
            }
            Section::Views => {
                let views = object(object(object(&mut output, "configs"), &scope), "views");
                if replace {
                    *views = serde_json::json!({});
                }
                for item in imported {
                    views[item.key] = item.value;
                }
            }
            Section::Commands => {
                let commands = object(&mut output, "commands");
                if replace {
                    *commands = serde_json::json!({});
                }
                for item in imported {
                    commands[item.key] = item.value;
                }
            }
            Section::Preferences => {
                // Every preference always has a value: nothing is ever removed.
                if !output.is_object() {
                    output = serde_json::json!({});
                }
                for item in imported {
                    output[item.key] = item.value;
                }
            }
        }
        documents.push((*section, output));
    }
    // Prove the result with the loaders of the application, away from the real files.
    let stage = tempfile::Builder::new()
        .prefix(".ssho-import-")
        .tempdir_in(root)?;
    for (section, value) in &documents {
        atomic_json(stage.path(), section.file(), value, 0o600)?;
    }
    for (section, _) in &documents {
        let checked = match section {
            Section::Hosts => Metadata::load(stage.path(), config).map(|_| ()),
            Section::Tunnels => TunnelProfiles::load(stage.path(), config).map(|_| ()),
            Section::Views => views::Views::load(stage.path(), config).map(|_| ()),
            Section::Commands => commands::Commands::load(stage.path()).map(|_| ()),
            Section::Preferences => AppConfig::load(stage.path()).and_then(|config| {
                validate_host_columns(&config.host_columns)?;
                Ok(())
            }),
        };
        checked.with_context(|| format!("the imported {} section is not valid", section.key()))?;
    }
    // Keep the current files before replacing any of them.
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ").to_string();
    let backups = root.join("data-backups");
    fs::create_dir_all(&backups)?;
    // Two imports in the same instant never share, nor overwrite, a backup folder.
    let mut backup = backups.join(&stamp);
    let mut attempt = 1;
    loop {
        match fs::create_dir(&backup) {
            Ok(()) => break,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempt < 1000 => {
                attempt += 1;
                backup = backups.join(format!("{stamp}-{attempt}"));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("create {}", backup.display()));
            }
        }
    }
    for (section, _) in &documents {
        let current = root.join(section.file());
        if current.exists() {
            fs::copy(&current, backup.join(section.file()))
                .with_context(|| format!("back up {}", current.display()))?;
        }
    }
    for (section, value) in &documents {
        let mode = if *section == Section::Preferences {
            0o644
        } else {
            0o600
        };
        atomic_json(root, section.file(), value, mode)?;
    }
    Ok(Applied {
        backup,
        written,
        removed,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::commands::{Commands, SavedCommand};
    use crate::storage::views::{SavedView, Views};

    struct Machine {
        _temp: tempfile::TempDir,
        root: PathBuf,
        config: PathBuf,
    }

    fn machine(hosts: &[&str]) -> Machine {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let ssh = temp.path().join("ssh");
        fs::create_dir_all(ssh.join("config.d")).unwrap();
        let config = ssh.join("config");
        let body: String = hosts
            .iter()
            .map(|host| format!("Host {host}\n HostName {host}.example\n"))
            .collect();
        fs::write(&config, format!("Include config.d/*\n{body}")).unwrap();
        fs::write(ssh.join("config.d/work"), "Host work\n").unwrap();
        AppConfig::default().save_new(&root);
        Machine {
            _temp: temp,
            root,
            config,
        }
    }

    impl AppConfig {
        fn save_new(&self, root: &Path) {
            fs::create_dir_all(root).unwrap();
            self.save(root).unwrap();
        }
    }

    fn aliases(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn files(machine: &Machine) -> Vec<PathBuf> {
        let directory = machine.config.parent().unwrap().canonicalize().unwrap();
        vec![directory.join("config"), directory.join("config.d/work")]
    }

    fn populate(machine: &Machine) {
        let mut metadata = Metadata::load(&machine.root, &machine.config).unwrap();
        metadata.set_favorite("alpha", true).unwrap();
        metadata.set_note("beta", "Bastion requis").unwrap();
        let mut views = Views::load(&machine.root, &machine.config).unwrap();
        let directory = machine.config.parent().unwrap().canonicalize().unwrap();
        views
            .save(
                None,
                "Travail",
                &SavedView {
                    query: "tag:work".into(),
                    source: Some(directory.join("config.d/work")),
                    ..SavedView::default()
                },
            )
            .unwrap();
        let mut commands = Commands::load(&machine.root).unwrap();
        commands
            .save(
                None,
                "Charge",
                &SavedCommand {
                    template: "uptime".into(),
                    ..SavedCommand::default()
                },
            )
            .unwrap();
        AppConfig::update(&machine.root, |config| {
            config.language = crate::i18n::Language::Fr;
            config.ssh_command = vec!["/opt/local/wrapper".into()];
        })
        .unwrap();
    }

    #[test]
    fn export_holds_the_chosen_sections_and_nothing_private_by_default() {
        let source = machine(&["alpha", "beta"]);
        populate(&source);
        let counts = inventory(&source.root, &source.config).unwrap();
        assert_eq!(counts[&Section::Hosts], 2);
        assert_eq!(counts[&Section::Views], 1);
        assert_eq!(counts[&Section::Commands], 1);
        assert_eq!(counts[&Section::Tunnels], 0);
        let all = export(&source.root, &source.config, &Section::ALL).unwrap();
        assert_eq!((all.format.as_str(), all.version), (FORMAT, VERSION));
        let text = serde_json::to_string(&all).unwrap();
        // The launcher is a local path: it never travels. Nor do history or SSH files.
        assert!(
            !text.contains("wrapper") && !text.contains("ssh_command"),
            "{text}"
        );
        assert!(
            !text.contains("HostName") && !text.contains("connect_count"),
            "{text}"
        );
        // A source file inside the configuration directory is exported relative to it.
        assert_eq!(all.sections["views"]["Travail"]["source"], "config.d/work");
        assert_eq!(all.sections["preferences"]["language"], "fr");
        // Only what was chosen is exported.
        let notes = export(&source.root, &source.config, &[Section::Hosts]).unwrap();
        assert_eq!(notes.sections.keys().collect::<Vec<_>>(), ["hosts"]);
        assert!(export(&source.root, &source.config, &[]).is_err());
    }

    #[test]
    fn file_is_private_never_overwritten_and_checked_when_read() {
        let source = machine(&["alpha", "beta"]);
        populate(&source);
        let data = export(&source.root, &source.config, &Section::ALL).unwrap();
        let path = source.root.join("export.json");
        write(&path, &data).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(read(&path).unwrap(), data);
        let error = write(&path, &data).unwrap_err().to_string();
        assert!(error.contains("already exists"), "{error}");
        for (name, content) in [
            ("not-json", "hello".to_owned()),
            ("other", r#"{"format":"other","version":1,"exported_at":"","application":"","configuration":"","sections":{}}"#.to_owned()),
            ("future", serde_json::to_string(&Export { version: VERSION + 1, ..data.clone() }).unwrap()),
        ] {
            let path = source.root.join(name);
            fs::write(&path, content).unwrap();
            assert!(read(&path).is_err(), "{name}");
        }
        let mut odd = data;
        odd.sections.insert("keys".into(), Map::new());
        let path = source.root.join("odd");
        fs::write(&path, serde_json::to_string(&odd).unwrap()).unwrap();
        assert!(read(&path)
            .unwrap_err()
            .to_string()
            .contains("unknown export section"));
    }

    #[test]
    fn plan_tells_new_identical_conflicting_and_unknown_entries_apart() {
        let source = machine(&["alpha", "beta"]);
        populate(&source);
        let data = export(&source.root, &source.config, &Section::ALL).unwrap();
        // Another machine: another path, alpha only, and its own note on alpha.
        let target = machine(&["alpha", "gamma"]);
        let mut metadata = Metadata::load(&target.root, &target.config).unwrap();
        metadata.set_note("alpha", "note locale").unwrap();
        metadata.set_favorite("gamma", true).unwrap();
        let known = aliases(&["alpha", "gamma"]);
        let plan = plan(&target.root, &target.config, &data, &known, &files(&target)).unwrap();
        let hosts = &plan[&Section::Hosts];
        assert_eq!(hosts.conflicts, ["alpha"]);
        assert_eq!(hosts.unknown, ["beta"], "beta is not declared here");
        assert!(hosts.new.is_empty());
        assert_eq!(hosts.local_only, 1, "gamma exists only here");
        // The view found its source again under the other configuration directory.
        let views = &plan[&Section::Views];
        assert_eq!(views.new, ["Travail"]);
        assert!(views.missing_sources.is_empty());
        assert_eq!(plan[&Section::Commands].new, ["Charge"]);
        let preferences = &plan[&Section::Preferences];
        assert_eq!(preferences.conflicts, ["language"]);
        assert!(preferences.new.is_empty());
        // Planning writes nothing.
        assert!(!target.root.join("views.json").exists());
        // A configuration without that file reports the view as needing an update.
        let plan = super::plan(&target.root, &target.config, &data, &known, &[]).unwrap();
        assert_eq!(plan[&Section::Views].missing_sources, ["Travail"]);
    }

    #[test]
    fn import_adds_only_what_is_new_unless_told_otherwise_and_backs_up_first() {
        let source = machine(&["alpha", "beta"]);
        populate(&source);
        let data = export(&source.root, &source.config, &Section::ALL).unwrap();
        let target = machine(&["alpha", "gamma"]);
        let mut metadata = Metadata::load(&target.root, &target.config).unwrap();
        metadata.set_note("alpha", "note locale").unwrap();
        metadata.set_favorite("gamma", true).unwrap();
        let known = aliases(&["alpha", "gamma"]);
        let before = fs::read(target.root.join("metadata.json")).unwrap();
        // Default policy: the local note wins, the unknown alias is left out.
        let applied = apply(
            &target.root,
            &target.config,
            &data,
            &Section::ALL,
            Policy::AddNew,
            false,
            &known,
        )
        .unwrap();
        let metadata = Metadata::load(&target.root, &target.config).unwrap();
        assert_eq!(metadata.hosts()["alpha"].note, "note locale");
        assert!(!metadata.hosts().contains_key("beta"));
        assert!(metadata.hosts()["gamma"].favorite);
        assert_eq!(
            AppConfig::load(&target.root).unwrap().language,
            crate::i18n::Language::En
        );
        let views = Views::load(&target.root, &target.config).unwrap();
        let directory = target.config.parent().unwrap().canonicalize().unwrap();
        assert_eq!(
            views.all()["Travail"].source.as_deref(),
            Some(directory.join("config.d/work").as_path())
        );
        assert!(Commands::load(&target.root)
            .unwrap()
            .all()
            .contains_key("Charge"));
        // The files as they were are kept apart.
        assert_eq!(
            fs::read(applied.backup.join("metadata.json")).unwrap(),
            before
        );
        assert!(applied.backup.starts_with(target.root.join("data-backups")));
        assert_eq!(applied.removed, 0);
        // Explicit replacement of conflicts, keeping the unknown alias on request.
        let applied = apply(
            &target.root,
            &target.config,
            &data,
            &[Section::Hosts, Section::Preferences],
            Policy::ReplaceConflicts,
            true,
            &known,
        )
        .unwrap();
        let metadata = Metadata::load(&target.root, &target.config).unwrap();
        assert!(metadata.hosts()["alpha"].favorite);
        assert_eq!(metadata.hosts()["alpha"].note, "");
        assert_eq!(metadata.hosts()["beta"].note, "Bastion requis");
        assert!(
            metadata.hosts()["gamma"].favorite,
            "local-only entries stay"
        );
        assert_eq!(applied.removed, 0);
        let config = AppConfig::load(&target.root).unwrap();
        assert_eq!(config.language, crate::i18n::Language::Fr);
        assert_eq!(
            config.ssh_command,
            ["ssh"],
            "the launcher is never imported"
        );
        // Full replacement of a section removes what the file does not hold.
        let applied = apply(
            &target.root,
            &target.config,
            &data,
            &[Section::Hosts],
            Policy::ReplaceSections,
            false,
            &known,
        )
        .unwrap();
        let metadata = Metadata::load(&target.root, &target.config).unwrap();
        assert_eq!(metadata.hosts().keys().collect::<Vec<_>>(), ["alpha"]);
        assert_eq!(applied.removed, 2);
    }

    #[test]
    fn an_invalid_import_changes_nothing() {
        let source = machine(&["alpha"]);
        populate(&source);
        let mut data = export(&source.root, &source.config, &Section::ALL).unwrap();
        data.sections
            .get_mut("commands")
            .unwrap()
            .insert("Cassée".into(), serde_json::json!({"template": "echo {"}));
        let target = machine(&["alpha"]);
        let known = aliases(&["alpha"]);
        let error = apply(
            &target.root,
            &target.config,
            &data,
            &Section::ALL,
            Policy::ReplaceConflicts,
            false,
            &known,
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("commands section is not valid"),
            "{error:#}"
        );
        assert!(!target.root.join("commands.json").exists());
        assert!(!target.root.join("metadata.json").exists());
        assert!(!target.root.join("data-backups").exists());
        assert_eq!(
            AppConfig::load(&target.root).unwrap().language,
            crate::i18n::Language::En
        );
        // Another configuration of the same machine keeps its own data untouched.
        assert!(apply(
            &target.root,
            &target.config,
            &export(&source.root, &source.config, &[Section::Hosts]).unwrap(),
            &[Section::Views],
            Policy::AddNew,
            false,
            &known,
        )
        .is_err());
    }
}
