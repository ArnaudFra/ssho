//! Saved views: the list context under a name. Applying a view sets the
//! search, the filters, the sort and the presentation; changing them afterwards
//! only marks the view as modified, the saved one is never overwritten silently.
use super::{forms::Input, nav, Action, Model, View};
use crate::storage::{
    views::{self, SavedView, Views},
    AppConfig,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Browse,
    /// Typing the name of a new view made from the current context.
    SaveAs,
    /// Typing the new name of the selected view.
    Rename,
    /// Two-choice confirmation, focus on going back.
    Delete,
}

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub(super) selected: usize,
    pub(super) button: usize,
    pub(super) mode: Mode,
    pub(super) name: Input,
    /// Focus inside the naming step: the input, then its two buttons.
    pub(super) name_focus: usize,
    pub(super) notice: String,
}

pub(super) fn buttons() -> [&'static str; 6] {
    [
        crate::t!("Apply", "Appliquer"),
        crate::t!("Save current as…", "Enregistrer sous…"),
        crate::t!("Update", "Mettre à jour"),
        crate::t!("Rename", "Renommer"),
        crate::t!("Delete", "Supprimer"),
        crate::t!("Close", "Fermer"),
    ]
}

const APPLY: usize = 0;
const SAVE_AS: usize = 1;
const UPDATE: usize = 2;
const RENAME: usize = 3;
const DELETE: usize = 4;

impl Model {
    /// The list context a view keeps. Batch marks and availability are left out
    /// on purpose: they describe a moment, not a view.
    pub(super) fn current_view(&self) -> SavedView {
        SavedView {
            query: self.query.value.clone(),
            source: self.source_filter.clone(),
            favorites_only: self.favorites_only,
            show_hidden: self.show_hidden,
            recent: self.recent,
            group_mode: self.preferences.group_mode,
            group_aliases: self.preferences.group_aliases,
            columns: self.preferences.host_columns.clone(),
        }
    }

    /// Name of the view in use and whether the context has moved away from it.
    pub(super) fn view_state(&self) -> Option<(&str, bool)> {
        let name = self.active_view.as_deref()?;
        let saved = self.views.get(name)?;
        Some((name, *saved != self.current_view()))
    }
}

/// Criteria of a view in words, in the order the list shows them.
pub(super) fn summary(view: &SavedView) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(if view.query.is_empty() {
        crate::t!("Search: none", "Recherche : aucune").to_owned()
    } else {
        crate::tf!("Search: {}", "Recherche : {}", view.query)
    });
    lines.push(match &view.source {
        Some(path) => crate::tf!("File: {}", "Fichier : {}", path.display()),
        None => crate::t!("File: all files", "Fichier : tous les fichiers").to_owned(),
    });
    let mut flags = Vec::new();
    if view.favorites_only {
        flags.push(crate::t!("favorites only", "favoris seuls"));
    }
    if view.show_hidden {
        flags.push(crate::t!("hidden hosts shown", "hôtes masqués visibles"));
    }
    flags.push(if view.recent {
        crate::t!("sorted by recent connection", "tri par connexion récente")
    } else {
        crate::t!("sorted by name", "tri par nom")
    });
    lines.push(flags.join(" • "));
    let groups = super::groups::modes()
        .iter()
        .find(|(mode, _)| *mode == view.group_mode)
        .map_or("", |(_, label)| *label);
    lines.push(crate::tf!(
        "Groups: {groups}{}",
        "Groupes : {groups}{}",
        if view.group_aliases {
            crate::t!(" • grouped aliases", " • alias regroupés")
        } else {
            ""
        }
    ));
    if !view.columns.is_empty() {
        lines.push(crate::tf!(
            "Columns: {}",
            "Colonnes : {}",
            view.columns
                .iter()
                .map(|column| column.label())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines
}

pub(super) fn open(m: &mut Model) {
    let selected = m
        .active_view
        .as_ref()
        .and_then(|name| m.views.keys().position(|key| key == name))
        .unwrap_or(0);
    // Without any view the only useful action is to save the current context.
    let button = if m.views.is_empty() { SAVE_AS } else { APPLY };
    m.open_view(View::Views(Editor {
        selected,
        button,
        mode: Mode::Browse,
        name: Input::new(""),
        name_focus: 0,
        notice: String::new(),
    }));
    m.error.clear();
}

fn selected_name(m: &Model, editor: &Editor) -> Option<String> {
    m.views.keys().nth(editor.selected).cloned()
}

pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Views(editor) = &mut m.view else {
        return Action::None;
    };
    editor.notice.clear();
    // A refusal stays readable until the next key, like a notice.
    m.error.clear();
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match editor.mode {
        Mode::Delete => {
            match m.confirm.key(k) {
                Some(nav::Answer::Proceed) => {
                    editor.mode = Mode::Browse;
                    let editor = editor.clone();
                    if let Some(name) = selected_name(m, &editor) {
                        return Action::DeleteView(name);
                    }
                }
                Some(nav::Answer::Stay) => editor.mode = Mode::Browse,
                None => {}
            }
            Action::None
        }
        Mode::SaveAs | Mode::Rename => {
            let save = ctrl && k.code == KeyCode::Char('s');
            if nav::cancels(k) {
                editor.mode = Mode::Browse;
                return Action::None;
            }
            match (k.code, editor.name_focus) {
                _ if save => {}
                (KeyCode::Tab | KeyCode::Down, focus) => {
                    editor.name_focus = (focus + 1) % 3;
                    return Action::None;
                }
                (KeyCode::BackTab | KeyCode::Up, focus) => {
                    editor.name_focus = (focus + 2) % 3;
                    return Action::None;
                }
                // Enter in the name goes to the Save button: it never writes.
                (KeyCode::Enter, 0) => {
                    editor.name_focus = 1;
                    return Action::None;
                }
                (KeyCode::Enter, 1) => {}
                (KeyCode::Enter, _) => {
                    editor.mode = Mode::Browse;
                    return Action::None;
                }
                (KeyCode::Left | KeyCode::Right, 1 | 2) => {
                    editor.name_focus = 3 - editor.name_focus;
                    return Action::None;
                }
                (_, 0) => {
                    editor.name.key(k);
                    return Action::None;
                }
                _ => return Action::None,
            }
            let name = editor.name.value.clone();
            if let Err(error) = views::validate_name(&name) {
                editor.notice = name_error(&error.to_string());
                editor.name_focus = 0;
                return Action::None;
            }
            let renaming = editor.mode == Mode::Rename;
            let editor = editor.clone();
            let original = renaming.then(|| selected_name(m, &editor)).flatten();
            let view = match &original {
                // A rename keeps the saved criteria; only the name changes.
                Some(original) => m.views.get(original).cloned().unwrap_or_default(),
                None => m.current_view(),
            };
            Action::SaveView {
                original,
                name,
                view: Box::new(view),
            }
        }
        Mode::Browse => {
            if nav::cancels(k) {
                m.back();
                return Action::None;
            }
            let count = m.views.len();
            let labels = buttons().len();
            match k.code {
                KeyCode::Up if count > 0 => editor.selected = nav::wrap(editor.selected, count, -1),
                KeyCode::Down if count > 0 => {
                    editor.selected = nav::wrap(editor.selected, count, 1)
                }
                KeyCode::Home => editor.selected = 0,
                KeyCode::End => editor.selected = count.saturating_sub(1),
                KeyCode::Tab | KeyCode::Right => {
                    editor.button = nav::wrap(editor.button, labels, 1);
                }
                KeyCode::BackTab | KeyCode::Left => {
                    editor.button = nav::wrap(editor.button, labels, -1);
                }
                KeyCode::Enter => {
                    let editor = editor.clone();
                    return run_button(m, &editor);
                }
                _ => {}
            }
            Action::None
        }
    }
}

fn name_error(error: &str) -> String {
    if error.contains("required") {
        crate::t!("A name is required", "Un nom est requis").into()
    } else if error.contains("limited") {
        crate::tf!(
            "The name is limited to {} characters",
            "Le nom est limité à {} caractères",
            views::MAX_NAME
        )
    } else {
        crate::t!(
            "The name cannot start or end with a space, nor hold control characters",
            "Le nom ne peut ni commencer ni finir par une espace, ni contenir de caractère de contrôle"
        )
        .into()
    }
}

fn run_button(m: &mut Model, editor: &Editor) -> Action {
    let name = selected_name(m, editor);
    let needs_view = |m: &mut Model| {
        if let View::Views(editor) = &mut m.view {
            editor.notice = crate::t!(
                "No saved view yet: save the current context first",
                "Aucune vue enregistrée : enregistrez d'abord le contexte courant"
            )
            .into();
        }
        Action::None
    };
    match (editor.button, name) {
        (SAVE_AS, _) => {
            if let View::Views(editor) = &mut m.view {
                editor.mode = Mode::SaveAs;
                editor.name = Input::new("");
                editor.name.limit = views::MAX_NAME;
                editor.name_focus = 0;
            }
            Action::None
        }
        (APPLY, Some(name)) => Action::ApplyView(name),
        (UPDATE, Some(name)) => Action::SaveView {
            original: Some(name.clone()),
            name,
            view: Box::new(m.current_view()),
        },
        (RENAME, Some(name)) => {
            if let View::Views(editor) = &mut m.view {
                editor.mode = Mode::Rename;
                editor.name = Input::new(&name);
                editor.name.limit = views::MAX_NAME;
                editor.name_focus = 0;
            }
            Action::None
        }
        (DELETE, Some(_)) => {
            m.confirm = nav::Confirm::default();
            if let View::Views(editor) = &mut m.view {
                editor.mode = Mode::Delete;
            }
            Action::None
        }
        (APPLY | UPDATE | RENAME | DELETE, None) => needs_view(m),
        _ => {
            m.back();
            Action::None
        }
    }
}

/// Text pasted while naming a view goes to the name, and nowhere else.
pub(super) fn input(editor: &mut Editor) -> Option<&mut Input> {
    (matches!(editor.mode, Mode::SaveAs | Mode::Rename) && editor.name_focus == 0)
        .then_some(&mut editor.name)
}

/// Apply a saved view. Its source file must still belong to the configuration:
/// a missing source is reported and nothing is applied, rather than widening
/// the filter to every file.
pub(super) fn apply(m: &mut Model, root: &Path, config: &Path, name: &str) -> Result<()> {
    let view = m
        .views
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!(gone(name)))?;
    if let Some(source) = &view.source {
        let files = crate::config::config_files(config)?;
        anyhow::ensure!(
            files.contains(source),
            crate::tf!(
                "View \"{name}\" filters on {}, which is no longer part of the SSH configuration. Nothing was applied: update or delete this view.",
                "La vue « {name} » filtre sur {}, qui ne fait plus partie de la configuration SSH. Rien n'a été appliqué : mettez à jour ou supprimez cette vue.",
                source.display()
            )
        );
    }
    // The presentation is a preference: it is saved like a choice made by hand.
    m.preferences = AppConfig::update(root, |config| {
        config.group_mode = view.group_mode;
        config.group_aliases = view.group_aliases;
        if !view.columns.is_empty() {
            config.host_columns.clone_from(&view.columns);
        }
    })?;
    m.query.set(&view.query);
    m.source_filter.clone_from(&view.source);
    m.favorites_only = view.favorites_only;
    m.show_hidden = view.show_hidden;
    m.recent = view.recent;
    m.active_view = Some(name.to_owned());
    m.back();
    m.search_focus = false;
    m.filter(true);
    m.error.clear();
    m.hint = crate::tf!("✓ View \"{name}\" applied", "✓ Vue « {name} » appliquée");
    Ok(())
}

fn gone(name: &str) -> String {
    crate::tf!(
        "View \"{name}\" no longer exists",
        "La vue « {name} » n'existe plus"
    )
}

/// Create, update or rename; the screen stays open on the saved view.
pub(super) fn save(
    m: &mut Model,
    store: &mut Views,
    original: Option<&str>,
    name: &str,
    view: &SavedView,
) -> Result<()> {
    let outcome = store.save(original, name, view);
    m.views.clone_from(store.all());
    if let Err(error) = outcome {
        let text = error.to_string();
        let message = if text.contains("already exists") {
            crate::tf!(
                "A view named \"{name}\" already exists: choose another name, or use Update",
                "Une vue nommée « {name} » existe déjà : choisissez un autre nom, ou utilisez Mettre à jour"
            )
        } else {
            text
        };
        if let View::Views(editor) = &mut m.view {
            editor.notice = message;
            editor.name_focus = 0;
        }
        return Ok(());
    }
    // The view in use follows its new name; a new or updated view becomes the one in use.
    let renamed = original.is_some_and(|original| original != name);
    if !renamed || m.active_view.as_deref() == original {
        m.active_view = Some(name.to_owned());
    }
    let position = m.views.keys().position(|key| key == name).unwrap_or(0);
    if let View::Views(editor) = &mut m.view {
        editor.mode = Mode::Browse;
        editor.selected = position;
        editor.button = APPLY;
        editor.notice = match original {
            None => crate::tf!("✓ View \"{name}\" saved", "✓ Vue « {name} » enregistrée"),
            Some(_) if renamed => {
                crate::tf!("✓ View renamed \"{name}\"", "✓ Vue renommée « {name} »")
            }
            Some(_) => crate::tf!(
                "✓ View \"{name}\" updated with the current context",
                "✓ Vue « {name} » mise à jour avec le contexte courant"
            ),
        };
    }
    Ok(())
}

pub(super) fn delete(m: &mut Model, store: &mut Views, name: &str) -> Result<()> {
    let outcome = store.delete(name);
    m.views.clone_from(store.all());
    outcome?;
    if m.active_view.as_deref() == Some(name) {
        m.active_view = None;
    }
    if let View::Views(editor) = &mut m.view {
        editor.selected = editor.selected.min(m.views.len().saturating_sub(1));
        editor.button = if m.views.is_empty() { SAVE_AS } else { APPLY };
        editor.notice = crate::tf!("✓ View \"{name}\" deleted", "✓ Vue « {name} » supprimée");
    }
    Ok(())
}
