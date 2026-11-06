//! Command catalogue: personal named commands, reused for the selected
//! host or for the marked targets of a batch. Using a command only fills the
//! batch preparation: targets and command are still reviewed there, and
//! nothing is ever launched from this screen.
use super::{forms::Input, nav, Action, Model, View};
use crate::storage::commands::{
    self, Commands, Kind, Parameter, SavedCommand, Scope, TemplateError,
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Browse,
    /// Creating or editing a command.
    Form,
    /// Giving a value to each parameter before the review.
    Params,
}

/// Two-choice confirmation in progress, focus on going back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pending {
    Delete,
    Discard,
}

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub(super) selected: usize,
    pub(super) button: usize,
    pub(super) mode: Mode,
    pub(super) pending: Option<Pending>,
    pub(super) notice: String,
    /// Form: name of the edited command (`None` for a new one), its three
    /// inputs (name, description, command) and the scope between them.
    pub(super) original: Option<String>,
    pub(super) inputs: [Input; 3],
    pub(super) scope: Scope,
    pub(super) focus: usize,
    initial: (String, String, String, Scope),
    /// Parameter step: one value per parameter, then two buttons.
    pub(super) params: Vec<Parameter>,
    pub(super) values: Vec<Input>,
    pub(super) param_focus: usize,
    /// The command will be prepared for the batch marks, not the selected host.
    pub(super) batch: bool,
}

pub(super) const NAME: usize = 0;
pub(super) const DESCRIPTION: usize = 1;
pub(super) const SCOPE: usize = 2;
pub(super) const TEMPLATE: usize = 3;
pub(super) const SAVE: usize = 4;
pub(super) const CANCEL: usize = 5;

const USE_HOST: usize = 0;
const USE_BATCH: usize = 1;
const NEW: usize = 2;
const EDIT: usize = 3;
const DELETE: usize = 4;

pub(super) fn buttons() -> [&'static str; 6] {
    [
        crate::t!("Use on this host", "Utiliser sur cet hôte"),
        crate::t!("Use on the batch", "Utiliser sur le lot"),
        crate::t!("New", "Nouvelle"),
        crate::t!("Edit", "Modifier"),
        crate::t!("Delete", "Supprimer"),
        crate::t!("Close", "Fermer"),
    ]
}

pub(super) fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Any => crate::t!("one host or a batch", "un hôte ou un lot"),
        Scope::Host => crate::t!("one host only", "un hôte seulement"),
        Scope::Batch => crate::t!("a batch only", "un lot seulement"),
    }
}

pub(super) fn kind_label(kind: &Kind) -> String {
    match kind {
        Kind::Text => crate::t!("text", "texte").to_owned(),
        Kind::Number => crate::t!("whole number", "nombre entier").to_owned(),
        Kind::Choice(choices) => {
            crate::tf!("choice: {}", "choix : {}", choices.join(" | "))
        }
    }
}

/// A refusal of the template engine, in the language of the interface.
pub(super) fn explain(error: &TemplateError) -> String {
    match error {
        TemplateError::Empty => {
            crate::t!("The command is empty", "La commande est vide").to_owned()
        }
        TemplateError::Unbalanced => crate::t!(
            "Unbalanced braces: a parameter is written {name}; literal braces are doubled, {{ and }}",
            "Accolades non appariées : un paramètre s'écrit {nom} ; une accolade littérale se double, {{ et }}"
        )
        .to_owned(),
        TemplateError::BadName(name) => crate::tf!(
            "Invalid parameter name \"{name}\": letters, digits and _, not starting with a digit",
            "Nom de paramètre invalide « {name} » : lettres, chiffres et _, sans commencer par un chiffre"
        ),
        TemplateError::UnknownKind(kind) => crate::tf!(
            "Unknown parameter type \"{kind}\": use text, number or choice",
            "Type de paramètre inconnu « {kind} » : utilisez text, number ou choice"
        ),
        TemplateError::BadDefault(name) => crate::tf!(
            "The default of {name} is not a whole number",
            "La valeur par défaut de {name} n'est pas un nombre entier"
        ),
        TemplateError::NoChoice(name) => crate::tf!(
            "{name} has no choice: write {{{name}:choice=a|b}}",
            "{name} n'a aucun choix : écrivez {{{name}:choice=a|b}}"
        ),
        TemplateError::Redeclared(name) => crate::tf!(
            "{name} is declared twice differently: declare it once, then reuse {{{name}}}",
            "{name} est déclaré deux fois différemment : déclarez-le une fois, puis réutilisez {{{name}}}"
        ),
        TemplateError::Missing(name) => {
            crate::tf!("{name}: a value is required", "{name} : une valeur est requise")
        }
        TemplateError::NotNumber(name) => crate::tf!(
            "{name}: a whole number is expected",
            "{name} : un nombre entier est attendu"
        ),
        TemplateError::NotChoice(name) => crate::tf!(
            "{name}: choose one of the proposed values",
            "{name} : choisissez une des valeurs proposées"
        ),
        TemplateError::ControlCharacter(name) => crate::tf!(
            "{name}: line breaks and control characters are refused",
            "{name} : les sauts de ligne et caractères de contrôle sont refusés"
        ),
    }
}

/// Parameters of a template in words, or the reason it is refused.
pub(super) fn parameters_summary(template: &str) -> std::result::Result<String, String> {
    let parameters = commands::parameters(template).map_err(|error| explain(&error))?;
    if parameters.is_empty() {
        return Ok(crate::t!(
            "No parameter: the command is used as written.",
            "Aucun paramètre : la commande est utilisée telle quelle."
        )
        .to_owned());
    }
    let list = parameters
        .iter()
        .map(|p| {
            let default = if p.default.is_empty() {
                String::new()
            } else {
                crate::tf!(", default {}", ", défaut {}", p.default)
            };
            format!("{} ({}{default})", p.name, kind_label(&p.kind))
        })
        .collect::<Vec<_>>()
        .join(" • ");
    Ok(crate::tf!(
        "Asked at each use: {list}",
        "Demandé à chaque utilisation : {list}"
    ))
}

impl Editor {
    fn new(selected: usize, button: usize) -> Self {
        Self {
            selected,
            button,
            mode: Mode::Browse,
            pending: None,
            notice: String::new(),
            original: None,
            inputs: [Input::new(""), Input::new(""), Input::new("")],
            scope: Scope::Any,
            focus: NAME,
            initial: (String::new(), String::new(), String::new(), Scope::Any),
            params: Vec::new(),
            values: Vec::new(),
            param_focus: 0,
            batch: false,
        }
    }

    fn open_form(&mut self, original: Option<(&String, &SavedCommand)>) {
        let (name, command) = match original {
            Some((name, command)) => (name.clone(), command.clone()),
            None => (String::new(), SavedCommand::default()),
        };
        self.original = original.map(|(name, _)| name.clone());
        self.inputs = [
            Input::new(&name),
            Input::new(&command.description),
            Input::new(&command.template),
        ];
        self.inputs[NAME].limit = commands::MAX_NAME;
        self.inputs[1].limit = 200;
        self.inputs[2].limit = 4096;
        self.scope = command.scope;
        self.initial = (name, command.description, command.template, command.scope);
        self.focus = NAME;
        self.mode = Mode::Form;
    }

    /// The input a form stop edits; the scope and the buttons own no text.
    fn form_input(&mut self) -> Option<&mut Input> {
        match self.focus {
            NAME => Some(&mut self.inputs[0]),
            DESCRIPTION => Some(&mut self.inputs[1]),
            TEMPLATE => Some(&mut self.inputs[2]),
            _ => None,
        }
    }

    pub(super) fn dirty(&self) -> bool {
        self.mode == Mode::Form
            && (
                self.inputs[0].value.clone(),
                self.inputs[1].value.clone(),
                self.inputs[2].value.clone(),
                self.scope,
            ) != self.initial
    }

    /// The command the parameter step would hand to the review, or why not yet.
    pub(super) fn rendered(&self, template: &str) -> std::result::Result<String, String> {
        let values: BTreeMap<String, String> = self
            .params
            .iter()
            .zip(&self.values)
            .map(|(parameter, input)| (parameter.name.clone(), input.value.clone()))
            .collect();
        commands::render(template, &values).map_err(|error| explain(&error))
    }
}

pub(super) fn open(m: &mut Model) {
    let button = if m.commands.is_empty() { NEW } else { USE_HOST };
    m.open_view(View::Commands(Box::new(Editor::new(0, button))));
    m.error.clear();
}

fn selected(m: &Model, editor: &Editor) -> Option<(String, SavedCommand)> {
    m.commands
        .iter()
        .nth(editor.selected)
        .map(|(name, command)| (name.clone(), command.clone()))
}

/// Text pasted in this screen only edits the focused input.
pub(super) fn input(editor: &mut Editor) -> Option<&mut Input> {
    if editor.pending.is_some() {
        return None;
    }
    match editor.mode {
        Mode::Form => editor.form_input(),
        Mode::Params => {
            let focus = editor.param_focus;
            let text = editor
                .params
                .get(focus)
                .is_some_and(|parameter| !matches!(parameter.kind, Kind::Choice(_)));
            if text {
                editor.values.get_mut(focus)
            } else {
                None
            }
        }
        Mode::Browse => None,
    }
}

pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Commands(editor) = &mut m.view else {
        return Action::None;
    };
    m.error.clear();
    if let Some(pending) = editor.pending {
        match m.confirm.key(k) {
            Some(nav::Answer::Proceed) => {
                editor.pending = None;
                match pending {
                    Pending::Discard => editor.mode = Mode::Browse,
                    Pending::Delete => {
                        let editor = editor.clone();
                        if let Some((name, _)) = selected(m, &editor) {
                            return Action::DeleteCommand(name);
                        }
                    }
                }
            }
            Some(nav::Answer::Stay) => editor.pending = None,
            None => {}
        }
        return Action::None;
    }
    editor.notice.clear();
    match editor.mode {
        Mode::Browse => browse_key(m, k),
        Mode::Form => form_key(m, k),
        Mode::Params => params_key(m, k),
    }
}

fn browse_key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Commands(editor) = &mut m.view else {
        return Action::None;
    };
    if nav::cancels(k) {
        m.back();
        return Action::None;
    }
    let count = m.commands.len();
    let labels = buttons().len();
    match k.code {
        KeyCode::Up if count > 0 => editor.selected = nav::wrap(editor.selected, count, -1),
        KeyCode::Down if count > 0 => editor.selected = nav::wrap(editor.selected, count, 1),
        KeyCode::Home => editor.selected = 0,
        KeyCode::End => editor.selected = count.saturating_sub(1),
        KeyCode::Tab | KeyCode::Right => editor.button = nav::wrap(editor.button, labels, 1),
        KeyCode::BackTab | KeyCode::Left => editor.button = nav::wrap(editor.button, labels, -1),
        KeyCode::Enter => {
            let button = editor.button;
            return run_button(m, button);
        }
        _ => {}
    }
    Action::None
}

fn notice(m: &mut Model, text: String) -> Action {
    if let View::Commands(editor) = &mut m.view {
        editor.notice = text;
    }
    Action::None
}

fn run_button(m: &mut Model, button: usize) -> Action {
    let View::Commands(editor) = &m.view else {
        return Action::None;
    };
    let chosen = selected(m, editor);
    match (button, chosen) {
        (NEW, _) => {
            if let View::Commands(editor) = &mut m.view {
                editor.open_form(None);
            }
            Action::None
        }
        (EDIT, Some((name, command))) => {
            if let View::Commands(editor) = &mut m.view {
                editor.open_form(Some((&name, &command)));
            }
            Action::None
        }
        (DELETE, Some(_)) => {
            m.confirm = nav::Confirm::default();
            if let View::Commands(editor) = &mut m.view {
                editor.pending = Some(Pending::Delete);
            }
            Action::None
        }
        (USE_HOST | USE_BATCH, Some((name, command))) => {
            let batch = button == USE_BATCH;
            // The intended scope is a promise made when the command was written.
            if batch && command.scope == Scope::Host {
                return notice(
                    m,
                    crate::tf!(
                        "\"{name}\" is meant for one host only: use it on the selected host, or edit its scope",
                        "« {name} » est prévue pour un hôte seulement : utilisez-la sur l'hôte sélectionné, ou modifiez sa portée"
                    ),
                );
            }
            if !batch && command.scope == Scope::Batch {
                return notice(
                    m,
                    crate::tf!(
                        "\"{name}\" is meant for a batch only: mark targets and use it on the batch, or edit its scope",
                        "« {name} » est prévue pour un lot seulement : marquez des cibles et utilisez-la sur le lot, ou modifiez sa portée"
                    ),
                );
            }
            if m.batch.manager.has_active() {
                return notice(
                    m,
                    crate::t!(
                        "A batch is running: follow or cancel it before preparing another command",
                        "Un lot est en cours : suivez-le ou annulez-le avant de préparer une autre commande"
                    )
                    .into(),
                );
            }
            if batch && m.batch.marks.is_empty() {
                // The catalogue never chooses targets: marking stays explicit.
                return notice(
                    m,
                    crate::t!(
                        "No target is marked: mark hosts in the list first, the catalogue never selects them for you",
                        "Aucune cible n'est marquée : marquez d'abord des hôtes dans la liste, le catalogue ne les choisit jamais à votre place"
                    )
                    .into(),
                );
            }
            if !batch && m.selected_host().is_none() {
                return notice(
                    m,
                    crate::t!(
                        "No host is selected in the list",
                        "Aucun hôte n'est sélectionné dans la liste"
                    )
                    .into(),
                );
            }
            let parameters = match commands::parameters(&command.template) {
                Ok(parameters) => parameters,
                Err(error) => return notice(m, explain(&error)),
            };
            if parameters.is_empty() {
                return Action::UseCommand {
                    command: command.template,
                    batch,
                };
            }
            if let View::Commands(editor) = &mut m.view {
                editor.values = parameters
                    .iter()
                    .map(|parameter| {
                        let mut input = Input::new(&parameter.default);
                        input.limit = 1024;
                        input
                    })
                    .collect();
                editor.params = parameters;
                editor.param_focus = 0;
                editor.batch = batch;
                editor.mode = Mode::Params;
            }
            Action::None
        }
        (USE_HOST | USE_BATCH | EDIT | DELETE, None) => notice(
            m,
            crate::t!(
                "No saved command yet: create one with New",
                "Aucune commande enregistrée : créez-en une avec Nouvelle"
            )
            .into(),
        ),
        _ => {
            m.back();
            Action::None
        }
    }
}

fn form_key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Commands(editor) = &mut m.view else {
        return Action::None;
    };
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if nav::cancels(k) {
        if editor.dirty() {
            m.confirm = nav::Confirm::default();
            editor.pending = Some(Pending::Discard);
        } else {
            editor.mode = Mode::Browse;
        }
        return Action::None;
    }
    let save = (ctrl && k.code == KeyCode::Char('s'))
        || (k.code == KeyCode::Enter && editor.focus == SAVE);
    if !save {
        match (k.code, editor.focus) {
            (KeyCode::Tab | KeyCode::Down, focus) => editor.focus = (focus + 1) % 6,
            (KeyCode::BackTab | KeyCode::Up, focus) => editor.focus = (focus + 5) % 6,
            (KeyCode::Enter, CANCEL) => {
                if editor.dirty() {
                    m.confirm = nav::Confirm::default();
                    editor.pending = Some(Pending::Discard);
                } else {
                    editor.mode = Mode::Browse;
                }
            }
            // Enter in a field goes to the next one: it never saves.
            (KeyCode::Enter, focus) => editor.focus = focus + 1,
            (KeyCode::Left | KeyCode::Right | KeyCode::Char(' '), SCOPE) => {
                let all = [Scope::Any, Scope::Host, Scope::Batch];
                let at = all.iter().position(|s| *s == editor.scope).unwrap_or(0);
                let delta = if k.code == KeyCode::Left { -1 } else { 1 };
                editor.scope = all[nav::wrap(at, all.len(), delta)];
            }
            (KeyCode::Left | KeyCode::Right, SAVE | CANCEL) => {
                editor.focus = if editor.focus == SAVE { CANCEL } else { SAVE };
            }
            _ => {
                if let Some(input) = editor.form_input() {
                    input.key(k);
                }
            }
        }
        return Action::None;
    }
    let name = editor.inputs[0].value.clone();
    let command = SavedCommand {
        description: editor.inputs[1].value.clone(),
        scope: editor.scope,
        template: editor.inputs[2].value.clone(),
    };
    if commands::validate_name(&name).is_err() {
        editor.focus = NAME;
        editor.notice = crate::tf!(
            "A name is required: 1 to {} characters, without leading or trailing space",
            "Un nom est requis : 1 à {} caractères, sans espace au début ni à la fin",
            commands::MAX_NAME
        );
        return Action::None;
    }
    if let Err(error) = commands::parameters(&command.template) {
        editor.focus = TEMPLATE;
        editor.notice = explain(&error);
        return Action::None;
    }
    Action::SaveCommand {
        original: editor.original.clone(),
        name,
        command: Box::new(command),
    }
}

fn params_key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Commands(editor) = &mut m.view else {
        return Action::None;
    };
    if nav::cancels(k) {
        editor.mode = Mode::Browse;
        return Action::None;
    }
    let count = editor.params.len();
    let stops = count + 2;
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let go = (ctrl && k.code == KeyCode::Char('l'))
        || (k.code == KeyCode::Enter && editor.param_focus == count);
    if !go {
        let choice =
            editor
                .params
                .get(editor.param_focus)
                .and_then(|parameter| match &parameter.kind {
                    Kind::Choice(choices) => Some(choices.clone()),
                    _ => None,
                });
        match (k.code, editor.param_focus) {
            (KeyCode::Tab | KeyCode::Down, focus) => editor.param_focus = (focus + 1) % stops,
            (KeyCode::BackTab | KeyCode::Up, focus) => {
                editor.param_focus = (focus + stops - 1) % stops;
            }
            (KeyCode::Enter, focus) if focus == count + 1 => editor.mode = Mode::Browse,
            // Enter on a value goes to the next stop: it never launches.
            (KeyCode::Enter, focus) => editor.param_focus = focus + 1,
            (KeyCode::Left | KeyCode::Right, focus) if focus >= count => {
                editor.param_focus = if focus == count { count + 1 } else { count };
            }
            (KeyCode::Left | KeyCode::Right | KeyCode::Char(' '), focus) if choice.is_some() => {
                let choices = choice.unwrap_or_default();
                let at = choices
                    .iter()
                    .position(|c| *c == editor.values[focus].value)
                    .unwrap_or(0);
                let delta = if k.code == KeyCode::Left { -1 } else { 1 };
                editor.values[focus].set(&choices[nav::wrap(at, choices.len(), delta)]);
            }
            (_, focus) if focus < count && choice.is_none() => editor.values[focus].key(k),
            _ => {}
        }
        return Action::None;
    }
    let editor = editor.clone();
    let Some((_, command)) = selected(m, &editor) else {
        return Action::None;
    };
    match editor.rendered(&command.template) {
        Ok(command) => Action::UseCommand {
            command,
            batch: editor.batch,
        },
        Err(reason) => {
            // Put the focus on the first value that is refused.
            let wrong = editor
                .params
                .iter()
                .zip(&editor.values)
                .position(|(parameter, input)| commands::check(parameter, &input.value).is_err());
            if let View::Commands(editor) = &mut m.view {
                editor.notice = reason;
                if let Some(index) = wrong {
                    editor.param_focus = index;
                }
            }
            Action::None
        }
    }
}

/// Create, edit or rename; the screen returns to the list on the saved command.
pub(super) fn save(
    m: &mut Model,
    store: &mut Commands,
    original: Option<&str>,
    name: &str,
    command: &SavedCommand,
) -> Result<()> {
    let outcome = store.save(original, name, command);
    m.commands.clone_from(store.all());
    if let Err(error) = outcome {
        let text = error.to_string();
        let message = if text.contains("already exists") {
            crate::tf!(
                "A command named \"{name}\" already exists: choose another name",
                "Une commande nommée « {name} » existe déjà : choisissez un autre nom"
            )
        } else {
            text
        };
        if let View::Commands(editor) = &mut m.view {
            editor.notice = message;
            editor.focus = NAME;
        }
        return Ok(());
    }
    let position = m.commands.keys().position(|key| key == name).unwrap_or(0);
    if let View::Commands(editor) = &mut m.view {
        editor.mode = Mode::Browse;
        editor.selected = position;
        editor.button = USE_HOST;
        editor.notice = crate::tf!(
            "✓ Command \"{name}\" saved",
            "✓ Commande « {name} » enregistrée"
        );
    }
    Ok(())
}

pub(super) fn delete(m: &mut Model, store: &mut Commands, name: &str) -> Result<()> {
    let outcome = store.delete(name);
    m.commands.clone_from(store.all());
    outcome?;
    if let View::Commands(editor) = &mut m.view {
        editor.selected = editor.selected.min(m.commands.len().saturating_sub(1));
        editor.button = if m.commands.is_empty() { NEW } else { USE_HOST };
        editor.notice = crate::tf!(
            "✓ Command \"{name}\" deleted",
            "✓ Commande « {name} » supprimée"
        );
    }
    Ok(())
}

/// Hand the command to the batch preparation: the selected host alone, or the
/// marked targets. The preparation keeps its own review and launch steps.
pub(super) fn prepare(m: &mut Model, command: &str, batch: bool) -> Result<()> {
    let targets = if batch {
        m.batch.marks.clone()
    } else {
        let host = m.selected_host().ok_or_else(|| {
            anyhow::anyhow!(crate::t!(
                "No host is selected in the list",
                "Aucun hôte n'est sélectionné dans la liste"
            ))
        })?;
        vec![super::batch::freeze(m, &host)?]
    };
    m.back();
    super::batch::open_with(m, targets, command);
    m.error.clear();
    Ok(())
}
