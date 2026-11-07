//! Export and import of the application data. The export shows its
//! scope before writing a file; the import shows what it would do, asks how
//! conflicts are handled, and only writes after an explicit confirmation.
use super::{forms::Input, nav, Action, Model, View};
use crate::storage::transfer::{self, Applied, Export, Plan, Policy, Section};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Home,
    Export,
    /// Typing the path of the file to import.
    ImportPath,
    /// What the file would change here, with the choices of the import.
    Preview,
}

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub(super) mode: Mode,
    pub(super) focus: usize,
    pub(super) notice: String,
    /// Confirmation of the import in progress, focus on going back.
    pub(super) pending: bool,
    /// Export: entries per section, the chosen sections, the file to create.
    pub(super) counts: BTreeMap<Section, usize>,
    pub(super) chosen: [bool; 5],
    pub(super) path: Input,
    /// Import: the file read, its comparison with the data here, the choices.
    pub(super) import_path: Input,
    pub(super) export: Option<Export>,
    pub(super) plan: Plan,
    pub(super) include: [bool; 5],
    pub(super) policy: Policy,
    pub(super) keep_unknown: bool,
}

pub(super) fn section_label(section: Section) -> &'static str {
    match section {
        Section::Hosts => crate::t!("Favorites and notes", "Favoris et notes"),
        Section::Tunnels => crate::t!("Tunnel profiles", "Profils de tunnels"),
        Section::Views => crate::t!("Saved views", "Vues enregistrées"),
        Section::Commands => crate::t!("Saved commands", "Commandes enregistrées"),
        Section::Preferences => crate::t!("Preferences", "Préférences"),
    }
}

pub(super) fn policy_label(policy: Policy) -> &'static str {
    match policy {
        Policy::AddNew => crate::t!(
            "Add only what is new",
            "Ajouter seulement ce qui est nouveau"
        ),
        Policy::ReplaceConflicts => crate::t!(
            "Add, and replace what differs",
            "Ajouter, et remplacer ce qui diffère"
        ),
        Policy::ReplaceSections => crate::t!(
            "Replace the chosen sections entirely",
            "Remplacer entièrement les sections choisies"
        ),
    }
}

pub(super) fn policy_detail(policy: Policy) -> &'static str {
    match policy {
        Policy::AddNew => crate::t!(
            "Nothing that exists here is changed: an entry present on both sides keeps its local content.",
            "Rien de ce qui existe ici n'est modifié : une entrée présente des deux côtés garde son contenu local."
        ),
        Policy::ReplaceConflicts => crate::t!(
            "An entry present on both sides takes the content of the file; entries that exist only here are kept.",
            "Une entrée présente des deux côtés prend le contenu du fichier ; les entrées qui n'existent qu'ici sont conservées."
        ),
        Policy::ReplaceSections => crate::t!(
            "Each chosen section becomes the one of the file: the entries that exist only here are removed.",
            "Chaque section choisie devient celle du fichier : les entrées qui n'existent qu'ici sont supprimées."
        ),
    }
}

pub(super) fn home_buttons() -> [&'static str; 3] {
    [
        crate::t!("Export…", "Exporter…"),
        crate::t!("Import…", "Importer…"),
        crate::t!("Close", "Fermer"),
    ]
}

/// Sections of the file read, in display order.
pub(super) fn sections(editor: &Editor) -> Vec<Section> {
    Section::ALL
        .into_iter()
        .filter(|section| editor.plan.contains_key(section))
        .collect()
}

/// Whether the preview has an "unknown aliases" choice to show.
pub(super) fn has_unknown(editor: &Editor) -> bool {
    editor.plan.values().any(|plan| !plan.unknown.is_empty())
}

/// What the import would do with the current choices: written, replaced,
/// removed, left out.
pub(super) fn totals(editor: &Editor) -> (usize, usize, usize, usize) {
    let (mut added, mut replaced, mut removed, mut skipped) = (0, 0, 0, 0);
    for (index, section) in Section::ALL.into_iter().enumerate() {
        let Some(plan) = editor.plan.get(&section) else {
            continue;
        };
        if !editor.include[index] {
            continue;
        }
        added += plan.new.len();
        if editor.keep_unknown {
            added += plan.unknown.len();
        } else {
            skipped += plan.unknown.len();
        }
        match editor.policy {
            Policy::AddNew => skipped += plan.conflicts.len(),
            Policy::ReplaceConflicts => replaced += plan.conflicts.len(),
            Policy::ReplaceSections => {
                replaced += plan.conflicts.len();
                // Preferences always keep a value: nothing is removed there.
                if section != Section::Preferences {
                    removed += plan.local_only;
                }
            }
        }
    }
    (added, replaced, removed, skipped)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// `~/` is the home directory, as in a shell; everything else is taken as typed.
fn expand(text: &str) -> PathBuf {
    match (text.strip_prefix("~/"), home()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(text),
    }
}

fn roots(m: &Model) -> Result<(PathBuf, PathBuf)> {
    let root = m.batch.history_root.clone().ok_or_else(|| {
        anyhow::anyhow!(crate::t!(
            "the application folder is unknown",
            "le dossier de l'application est inconnu"
        ))
    })?;
    Ok((root, m.batch.root.clone()))
}

pub(super) fn open(m: &mut Model) {
    let (counts, notice) =
        match roots(m).and_then(|(root, config)| transfer::inventory(&root, &config)) {
            Ok(counts) => (counts, String::new()),
            Err(error) => (BTreeMap::new(), format!("{error:#}")),
        };
    let default = format!(
        "~/ssho-export-{}.json",
        chrono::Local::now().format("%Y%m%d")
    );
    let mut path = Input::new(&default);
    path.limit = 1024;
    let mut import_path = Input::new("");
    import_path.limit = 1024;
    // Empty sections are not chosen: an export says what it really holds.
    let chosen = Section::ALL.map(|section| counts.get(&section).copied().unwrap_or(0) > 0);
    m.open_view(View::Transfer(Box::new(Editor {
        mode: Mode::Home,
        focus: 0,
        notice,
        pending: false,
        counts,
        chosen,
        path,
        import_path,
        export: None,
        plan: Plan::new(),
        include: [true; 5],
        policy: Policy::AddNew,
        keep_unknown: false,
    })));
    m.error.clear();
}

/// Text pasted in this screen only edits the focused path.
pub(super) fn input(editor: &mut Editor) -> Option<&mut Input> {
    if editor.pending {
        return None;
    }
    match (editor.mode, editor.focus) {
        (Mode::Export, 5) => Some(&mut editor.path),
        (Mode::ImportPath, 0) => Some(&mut editor.import_path),
        _ => None,
    }
}

pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Transfer(editor) = &mut m.view else {
        return Action::None;
    };
    m.error.clear();
    if editor.pending {
        match m.confirm.key(k) {
            Some(nav::Answer::Proceed) => {
                editor.pending = false;
                return Action::ImportData;
            }
            Some(nav::Answer::Stay) => editor.pending = false,
            None => {}
        }
        return Action::None;
    }
    editor.notice.clear();
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if nav::cancels(k) {
        match editor.mode {
            Mode::Home => m.back(),
            // One level at a time: a step returns to the one before it.
            Mode::Preview => {
                editor.mode = Mode::ImportPath;
                editor.focus = 0;
            }
            Mode::Export | Mode::ImportPath => {
                editor.mode = Mode::Home;
                editor.focus = 0;
            }
        }
        return Action::None;
    }
    match editor.mode {
        Mode::Home => match k.code {
            KeyCode::Tab | KeyCode::Right | KeyCode::Down => {
                editor.focus = nav::wrap(editor.focus, 3, 1);
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Up => {
                editor.focus = nav::wrap(editor.focus, 3, -1);
            }
            KeyCode::Enter => match editor.focus {
                0 => {
                    editor.mode = Mode::Export;
                    editor.focus = 0;
                }
                1 => {
                    editor.mode = Mode::ImportPath;
                    editor.focus = 0;
                }
                _ => m.back(),
            },
            _ => {}
        },
        Mode::Export => {
            // Stops: five sections, the path, Export, Back.
            let write = (ctrl && k.code == KeyCode::Char('s'))
                || (k.code == KeyCode::Enter && editor.focus == 6);
            if write {
                let editor = editor.clone();
                write_export(m, &editor);
                return Action::None;
            }
            match (k.code, editor.focus) {
                (KeyCode::Tab | KeyCode::Down, focus) => editor.focus = (focus + 1) % 8,
                (KeyCode::BackTab | KeyCode::Up, focus) => editor.focus = (focus + 7) % 8,
                (KeyCode::Char(' '), focus) if focus < 5 => {
                    editor.chosen[focus] = !editor.chosen[focus];
                }
                // Enter goes on to the next stop: it never writes from a row.
                (KeyCode::Enter, focus) if focus < 6 => editor.focus = focus + 1,
                (KeyCode::Enter, _) => {
                    editor.mode = Mode::Home;
                    editor.focus = 0;
                }
                (KeyCode::Left | KeyCode::Right, 6 | 7) => editor.focus = 13 - editor.focus,
                (_, 5) => editor.path.key(k),
                _ => {}
            }
        }
        Mode::ImportPath => {
            // Stops: the path, Read, Back.
            let read = (ctrl && k.code == KeyCode::Char('l'))
                || (k.code == KeyCode::Enter && editor.focus == 1);
            if read {
                let editor = editor.clone();
                read_import(m, &editor);
                return Action::None;
            }
            match (k.code, editor.focus) {
                (KeyCode::Tab | KeyCode::Down, focus) => editor.focus = (focus + 1) % 3,
                (KeyCode::BackTab | KeyCode::Up, focus) => editor.focus = (focus + 2) % 3,
                (KeyCode::Enter, 0) => editor.focus = 1,
                (KeyCode::Enter, _) => {
                    editor.mode = Mode::Home;
                    editor.focus = 1;
                }
                (KeyCode::Left | KeyCode::Right, 1 | 2) => editor.focus = 3 - editor.focus,
                (_, 0) => editor.import_path.key(k),
                _ => {}
            }
        }
        Mode::Preview => {
            // Stops: one per section of the file, the policy, the unknown
            // aliases when there are some, then Import and Back.
            let listed = sections(editor);
            let unknown = usize::from(has_unknown(editor));
            let policy = listed.len();
            let import = policy + 1 + unknown;
            let stops = import + 2;
            match (k.code, editor.focus) {
                (KeyCode::Tab | KeyCode::Down, focus) => editor.focus = (focus + 1) % stops,
                (KeyCode::BackTab | KeyCode::Up, focus) => {
                    editor.focus = (focus + stops - 1) % stops;
                }
                (KeyCode::Char(' '), focus) if focus < policy => {
                    let index = Section::ALL
                        .iter()
                        .position(|section| *section == listed[focus])
                        .unwrap_or(0);
                    editor.include[index] = !editor.include[index];
                }
                (KeyCode::Left | KeyCode::Right | KeyCode::Char(' '), focus) if focus == policy => {
                    let all = [
                        Policy::AddNew,
                        Policy::ReplaceConflicts,
                        Policy::ReplaceSections,
                    ];
                    let at = all.iter().position(|p| *p == editor.policy).unwrap_or(0);
                    let delta = if k.code == KeyCode::Left { -1 } else { 1 };
                    editor.policy = all[nav::wrap(at, all.len(), delta)];
                }
                (KeyCode::Left | KeyCode::Right | KeyCode::Char(' '), focus)
                    if unknown == 1 && focus == policy + 1 =>
                {
                    editor.keep_unknown = !editor.keep_unknown;
                }
                (KeyCode::Left | KeyCode::Right, focus) if focus >= import => {
                    editor.focus = if focus == import { import + 1 } else { import };
                }
                // Enter on a choice goes on to the next stop: it never imports.
                (KeyCode::Enter, focus) if focus < import => editor.focus = focus + 1,
                (KeyCode::Enter, focus) if focus == import => {
                    if listed.iter().all(|section| !included(editor, *section)) {
                        editor.notice = crate::t!(
                            "No section is chosen: nothing to import",
                            "Aucune section n'est choisie : rien à importer"
                        )
                        .into();
                    } else {
                        m.confirm = nav::Confirm::default();
                        editor.pending = true;
                    }
                }
                (KeyCode::Enter, _) => {
                    editor.mode = Mode::ImportPath;
                    editor.focus = 0;
                }
                _ => {}
            }
        }
    }
    Action::None
}

fn included(editor: &Editor, section: Section) -> bool {
    Section::ALL
        .iter()
        .position(|s| *s == section)
        .is_some_and(|index| editor.include[index])
}

fn notice(m: &mut Model, text: String) {
    if let View::Transfer(editor) = &mut m.view {
        editor.notice = text;
    }
}

fn write_export(m: &mut Model, editor: &Editor) {
    let chosen: Vec<Section> = Section::ALL
        .into_iter()
        .zip(editor.chosen)
        .filter_map(|(section, on)| on.then_some(section))
        .collect();
    if chosen.is_empty() {
        return notice(
            m,
            crate::t!(
                "No section is chosen: nothing to export",
                "Aucune section n'est choisie : rien à exporter"
            )
            .into(),
        );
    }
    if editor.path.value.trim().is_empty() {
        if let View::Transfer(editor) = &mut m.view {
            editor.focus = 5;
        }
        return notice(
            m,
            crate::t!("Give the file to create", "Indiquez le fichier à créer").into(),
        );
    }
    let path = expand(editor.path.value.trim());
    let outcome = roots(m).and_then(|(root, config)| {
        let export = transfer::export(&root, &config, &chosen)?;
        transfer::write(&path, &export)?;
        Ok(export
            .sections
            .values()
            .map(|entries| entries.len())
            .sum::<usize>())
    });
    match outcome {
        Ok(entries) => {
            if let View::Transfer(editor) = &mut m.view {
                editor.mode = Mode::Home;
                editor.focus = 0;
            }
            notice(
                m,
                crate::tf!(
                    "✓ Exported to {} ({} section(s), {entries} entries), readable by you only",
                    "✓ Exporté vers {} ({} section(s), {entries} entrées), lisible par vous seul",
                    path.display(),
                    chosen.len()
                ),
            );
        }
        Err(error) => {
            let text = error.to_string();
            let message = if text.contains("already exists") {
                crate::tf!(
                    "{} already exists: an export never replaces a file, choose another name",
                    "{} existe déjà : un export ne remplace jamais un fichier, choisissez un autre nom",
                    path.display()
                )
            } else {
                format!("{error:#}")
            };
            if let View::Transfer(editor) = &mut m.view {
                editor.focus = 5;
            }
            notice(m, message);
        }
    }
}

/// Aliases the configuration in use declares: an imported favorite, note or
/// tunnel profile is only attached to one of them.
fn aliases(m: &Model) -> BTreeSet<String> {
    m.all.iter().map(|host| host.name.clone()).collect()
}

fn read_import(m: &mut Model, editor: &Editor) {
    if editor.import_path.value.trim().is_empty() {
        if let View::Transfer(editor) = &mut m.view {
            editor.focus = 0;
        }
        return notice(
            m,
            crate::t!(
                "Give the export file to read",
                "Indiquez le fichier d'export à lire"
            )
            .into(),
        );
    }
    let path = expand(editor.import_path.value.trim());
    let outcome = roots(m).and_then(|(root, config)| {
        let export = transfer::read(&path)?;
        let files = crate::config::config_files(&config)?;
        let plan = transfer::plan(&root, &config, &export, &aliases(m), &files)?;
        Ok((export, plan))
    });
    match outcome {
        Ok((export, plan)) => {
            if let View::Transfer(editor) = &mut m.view {
                editor.export = Some(export);
                editor.plan = plan;
                editor.include = [true; 5];
                editor.policy = Policy::AddNew;
                editor.keep_unknown = false;
                editor.mode = Mode::Preview;
                editor.focus = 0;
            }
        }
        Err(error) => {
            let text = format!("{error:#}");
            let message = if text.contains("is not a ssho export") {
                crate::tf!(
                    "{} is not a ssho export",
                    "{} n'est pas un export de ssho",
                    path.display()
                )
            } else if text.contains("is not supported") {
                crate::tf!(
                    "This export was made by a newer version: {text}",
                    "Cet export vient d'une version plus récente : {text}"
                )
            } else {
                text
            };
            if let View::Transfer(editor) = &mut m.view {
                editor.focus = 0;
            }
            notice(m, message);
        }
    }
}

/// Apply the import that was previewed and confirmed. The event loop then
/// reloads every store from the files that were just written.
pub(super) fn apply(m: &mut Model, root: &Path, config: &Path) -> Result<Applied> {
    let View::Transfer(editor) = &m.view else {
        anyhow::bail!("import preview is absent")
    };
    let export = editor
        .export
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("import preview is absent"))?;
    let chosen: Vec<Section> = sections(editor)
        .into_iter()
        .filter(|section| included(editor, *section))
        .collect();
    let applied = transfer::apply(
        root,
        config,
        export,
        &chosen,
        editor.policy,
        editor.keep_unknown,
        &aliases(m),
    )?;
    let message = crate::tf!(
        "✓ Imported: {} written, {} removed, {} left out. The previous files are kept in {}",
        "✓ Importé : {} écrites, {} supprimées, {} laissées de côté. Les fichiers précédents sont conservés dans {}",
        applied.written,
        applied.removed,
        applied.skipped,
        applied.backup.display()
    );
    if let View::Transfer(editor) = &mut m.view {
        editor.mode = Mode::Home;
        editor.focus = 2;
        editor.export = None;
        editor.plan = Plan::new();
        editor.notice = message;
    }
    Ok(applied)
}

/// Counts shown after an import or on reopening the export step.
pub(super) fn refresh_counts(m: &mut Model) {
    let counts = roots(m).and_then(|(root, config)| transfer::inventory(&root, &config));
    if let (Ok(counts), View::Transfer(editor)) = (counts, &mut m.view) {
        editor.chosen = Section::ALL.map(|section| counts.get(&section).copied().unwrap_or(0) > 0);
        editor.counts = counts;
    }
}
