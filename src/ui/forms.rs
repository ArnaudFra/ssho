//! Form state and editing; adapted from SSHM (MIT, Guillaume Archambault).
use crate::{
    config,
    types::{Forward, Host},
};
use anyhow::{bail, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{style::Style, text::Span};
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default)]
pub(super) struct Input {
    pub value: String,
    pub cursor: usize,
    pub placeholder: String,
    pub limit: usize,
}
impl Input {
    pub fn new(s: &str) -> Self {
        Self {
            value: s.into(),
            cursor: s.chars().count(),
            placeholder: String::new(),
            limit: 0,
        }
    }
    pub fn set(&mut self, s: &str) {
        self.value = s.into();
        self.cursor = s.chars().count();
    }
    pub fn key(&mut self, mut k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) {
            let code = match k.code {
                KeyCode::Char('b') => Some(KeyCode::Left),
                KeyCode::Char('f') => Some(KeyCode::Right),
                KeyCode::Char('h') => Some(KeyCode::Backspace),
                KeyCode::Char('d') => Some(KeyCode::Delete),
                _ => None,
            };
            if let Some(code) = code {
                k.code = code;
                k.modifiers.remove(KeyModifiers::CONTROL);
            }
        } else if k.modifiers.contains(KeyModifiers::ALT) {
            match k.code {
                KeyCode::Char('b') => k.code = KeyCode::Left,
                KeyCode::Char('f') => k.code = KeyCode::Right,
                KeyCode::Backspace => {
                    k.code = KeyCode::Char('w');
                    k.modifiers = KeyModifiers::CONTROL;
                }
                _ => {}
            }
        }
        let mut chars: Vec<char> = self.value.chars().collect();
        self.cursor = self.cursor.min(chars.len());
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = chars.len(),
            KeyCode::Left => {
                if ctrl || alt {
                    while self.cursor > 0 && chars[self.cursor - 1].is_whitespace() {
                        self.cursor -= 1;
                    }
                    while self.cursor > 0 && !chars[self.cursor - 1].is_whitespace() {
                        self.cursor -= 1;
                    }
                } else {
                    self.cursor = self.cursor.saturating_sub(1);
                }
            }
            KeyCode::Right => {
                if ctrl || alt {
                    while self.cursor < chars.len() && !chars[self.cursor].is_whitespace() {
                        self.cursor += 1;
                    }
                    while self.cursor < chars.len() && chars[self.cursor].is_whitespace() {
                        self.cursor += 1;
                    }
                } else {
                    self.cursor = (self.cursor + 1).min(chars.len());
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    chars.remove(self.cursor - 1);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < chars.len() {
                    chars.remove(self.cursor);
                }
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = chars.len(),
            KeyCode::Char('u') if ctrl => {
                chars.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char('k') if ctrl => {
                chars.truncate(self.cursor);
            }
            KeyCode::Char('w') if ctrl => {
                let end = self.cursor;
                while self.cursor > 0 && chars[self.cursor - 1].is_whitespace() {
                    self.cursor -= 1;
                }
                while self.cursor > 0 && !chars[self.cursor - 1].is_whitespace() {
                    self.cursor -= 1;
                }
                chars.drain(self.cursor..end);
            }
            KeyCode::Char(c) if !ctrl && !alt && (self.limit == 0 || chars.len() < self.limit) => {
                chars.insert(self.cursor, c);
                self.cursor += 1;
            }
            _ => {}
        }
        self.value = chars.into_iter().collect();
    }
    /// Visible text and terminal cursor column; never insert a fake cursor into the value.
    pub fn viewport(&self, focused: bool, width: usize) -> (String, Option<u16>) {
        if width == 0 {
            return (String::new(), None);
        }
        let text = if self.value.is_empty() {
            &self.placeholder
        } else {
            &self.value
        };
        let span = Span::raw(text);
        let graphemes: Vec<_> = span
            .styled_graphemes(Style::default())
            .map(|g| g.symbol)
            .collect();
        let cursor = if self.value.is_empty() {
            0
        } else {
            self.cursor
        };
        // The editor uses character offsets; a cursor inside a grapheme belongs at its
        // leading cell, never on a combining mark or half of a joined emoji.
        let mut offset = 0;
        let mut cursor_grapheme = 0;
        for grapheme in &graphemes {
            offset += grapheme.chars().count();
            if offset > cursor {
                break;
            }
            cursor_grapheme += 1;
        }
        let mut start = 0;
        let mut column: usize = graphemes[..cursor_grapheme].iter().map(|g| g.width()).sum();
        if focused {
            while column >= width && start < cursor_grapheme {
                column -= graphemes[start].width();
                start += 1;
            }
        }
        let mut visible = String::new();
        let mut used = 0;
        for grapheme in &graphemes[start..] {
            used += grapheme.width();
            if used > width {
                break;
            }
            visible.push_str(grapheme);
        }
        (visible, focused.then_some(column as u16))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FormEvent {
    None,
    Cancel,
    Submit,
    /// Start what the form describes, without saving it.
    Launch,
    /// Choose another destination file, then come back to the same draft.
    ChooseFile,
    PickKey,
    EditNote,
}
/// Structured location shared by validation, focus and inline rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FormField {
    Input(usize),
    Name(usize),
}
#[derive(Clone, Debug)]
pub(super) struct FieldError {
    pub field: FormField,
    pub message: String,
}
fn input_values(inputs: &[Input]) -> Vec<String> {
    inputs.iter().map(|i| i.value.clone()).collect()
}

/// Collapsible parts of the host form. Essential fields are outside any section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Section {
    Aliases,
    Advanced,
    Organization,
}

/// Focusable controls of the host form that are not text inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    File,
    KeyPicker,
    Section(Section),
    AddAlias,
    RemoveAlias(usize),
    Note,
    Preview,
    Cancel,
}

/// One stop of the Tab order, in the order the form is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    Field(FormField),
    Control(Control),
}

/// Input indices: 0 alias (add/copy), 1 address, 2 user, 3 port, 4 key, 5 ProxyJump,
/// 6 ProxyCommand, 7 options, 8 tags, 9 remote command, 10 RequestTTY.
const ADVANCED: [usize; 5] = [5, 6, 7, 9, 10];
/// Example of each input, shown under the form for the focused field only.
fn example_of(index: usize) -> &'static str {
    match index {
        0 => crate::t!("e.g. api-prod", "ex. : api-prod"),
        1 => crate::t!(
            "e.g. 192.168.1.100 or example.com",
            "ex. : 192.168.1.100 ou example.com"
        ),
        2 => crate::t!("e.g. deploy", "ex. : deploy"),
        3 => crate::t!("e.g. 2222", "ex. : 2222"),
        4 => crate::t!(
            "e.g. ~/.ssh/id_ed25519 — F9 or [Choose] lists the keys",
            "ex. : ~/.ssh/id_ed25519 — F9 ou [Choisir] liste les clés"
        ),
        5 => crate::t!(
            "e.g. user@bastion:22 or an existing alias",
            "ex. : user@bastion:22 ou un alias existant"
        ),
        6 => crate::t!("e.g. ssh -W %h:%p bastion", "ex. : ssh -W %h:%p bastion"),
        7 => crate::t!(
            "e.g. -o Compression=yes -o ServerAliveInterval=60",
            "ex. : -o Compression=yes -o ServerAliveInterval=60"
        ),
        8 => crate::t!(
            "e.g. production, web — \"hidden\" hides the host from the list",
            "ex. : production, web — « hidden » masque l'hôte dans la liste"
        ),
        9 => crate::t!(
            "e.g. htop — left empty: login shell",
            "ex. : htop — laissé vide : shell de connexion"
        ),
        _ => crate::t!("yes, no, force or auto", "yes, no, force ou auto"),
    }
}

#[derive(Clone, Debug)]
pub(super) struct HostForm {
    pub inputs: Vec<Input>,
    /// Edit: every alias of the block, the first being the primary one.
    /// Add: only the additional aliases; the primary one is `inputs[0]`.
    pub names: Vec<Input>,
    pub original: Option<Host>,
    copy_source: Option<Host>,
    pub target: PathBuf,
    pub file_tags: Vec<String>,
    pub name_focus: bool,
    pub focus: usize,
    /// A focused button or section header; text inputs then have no caret.
    pub control: Option<Control>,
    open: [bool; 3],
    pub error: String,
    pub scroll: usize,
    baseline: (Vec<String>, Vec<String>, PathBuf),
    pub errors: Vec<FieldError>,
}
impl HostForm {
    pub fn add(name: String, target: PathBuf) -> Self {
        let mut inputs = vec![Input::default(); 11];
        for (i, input) in inputs.iter_mut().enumerate() {
            input.limit = [50, 100, 50, 5, 200, 200, 200, 500, 200, 300, 10][i];
        }
        inputs[0].set(&name);
        let mut form = Self {
            baseline: (vec![], vec![], target.clone()),
            errors: vec![],
            inputs,
            names: vec![],
            original: None,
            copy_source: None,
            file_tags: config::file_tags(&target).unwrap_or_default(),
            target,
            name_focus: false,
            focus: 0,
            control: None,
            open: [false; 3],
            error: String::new(),
            scroll: 0,
        };
        form.describe_empty_fields();
        form.rebase();
        form
    }
    pub fn edit(host: Host) -> Self {
        let mut f = Self::add(String::new(), host.source_file.clone());
        let names = if host.aliases.is_empty() {
            vec![host.name.clone()]
        } else {
            host.aliases.clone()
        };
        f.names = names.iter().map(|n| Input::new(n)).collect();
        f.prefill(&host);
        f.original = Some(host);
        f.name_focus = true;
        f.describe_empty_fields();
        f.open_filled_sections();
        f.rebase();
        f
    }
    fn prefill(&mut self, host: &Host) {
        let values = [
            host.name.clone(),
            host.hostname.clone(),
            host.user.clone(),
            host.port.clone(),
            host.identity.clone(),
            host.proxy_jump.clone(),
            host.proxy_command.clone(),
            config::options_to_command(&host.options),
            host.tags.join(", "),
            host.remote_command.clone(),
            host.request_tty.clone(),
        ];
        for (i, value) in values.iter().enumerate() {
            self.inputs[i].set(value);
        }
    }
    /// A copy is an add form with a separate read-only template, never an edit identity.
    pub fn duplicate(source: Host, target: PathBuf) -> Self {
        let mut form = Self::add(String::new(), target.clone());
        form.prefill(&source);
        form.copy_source = Some(source);
        form.inputs[0].set("");
        form.describe_empty_fields();
        form.open_filled_sections();
        form.rebase();
        form
    }
    fn rebase(&mut self) {
        self.baseline = (
            input_values(&self.inputs),
            input_values(&self.names),
            self.target.clone(),
        );
    }
    /// What an empty field means, shown in its place: a value is never suggested
    /// by a greyed example that looks like one already entered.
    fn describe_empty_fields(&mut self) {
        let openssh = crate::t!("left to OpenSSH", "selon OpenSSH");
        // An empty field writes nothing: OpenSSH then uses the local user name,
        // or whatever an earlier `Host *` block declares.
        let user = openssh.to_owned();
        let port = if self.copy_source.is_some() {
            openssh.to_owned()
        } else {
            crate::t!(
                "22 — OpenSSH default, not written",
                "22 — défaut OpenSSH, non écrit"
            )
            .to_owned()
        };
        let texts = [
            crate::t!("required", "requis").to_owned(),
            crate::t!("required", "requis").to_owned(),
            user,
            port,
            openssh.to_owned(),
            openssh.to_owned(),
            openssh.to_owned(),
            crate::t!("no additional option", "aucune option supplémentaire").to_owned(),
            crate::t!("no tag", "aucun tag").to_owned(),
            crate::t!("none: login shell", "aucune : shell de connexion").to_owned(),
            openssh.to_owned(),
        ];
        for (input, text) in self.inputs.iter_mut().zip(texts) {
            input.placeholder = text;
        }
        for name in &mut self.names {
            name.placeholder = crate::t!("required", "requis").into();
        }
    }
    /// Existing values are never hidden behind a closed section.
    fn open_filled_sections(&mut self) {
        self.open = [
            !self.extra_aliases().is_empty(),
            ADVANCED.iter().any(|&i| !self.inputs[i].value.is_empty()),
            !self.inputs[8].value.is_empty() || !self.file_tags.is_empty(),
        ];
    }
    pub fn example(field: FormField) -> &'static str {
        match field {
            FormField::Input(i) => example_of(i),
            FormField::Name(_) => crate::t!(
                "shares the configuration block of the main alias",
                "partage le bloc de configuration de l'alias principal"
            ),
        }
    }
    pub fn extra_identities(&self) -> &[String] {
        self.original
            .as_ref()
            .or(self.copy_source.as_ref())
            .map(|host| host.extra_identities.as_slice())
            .unwrap_or_default()
    }
    pub fn is_duplicate(&self) -> bool {
        self.copy_source.is_some()
    }
    pub fn copy_source(&self) -> Option<&Host> {
        self.copy_source.as_ref()
    }
    pub fn dirty(&self) -> bool {
        self.baseline
            != (
                input_values(&self.inputs),
                input_values(&self.names),
                self.target.clone(),
            )
    }
    /// The destination can be changed from an add or copy draft; an existing
    /// block is moved by its own action.
    pub fn retarget(&mut self, target: PathBuf) {
        self.file_tags = config::file_tags(&target).unwrap_or_default();
        self.target = target;
        self.control = Some(Control::File);
    }
    pub fn is_open(&self, section: Section) -> bool {
        self.open[section as usize]
    }
    #[cfg(test)]
    pub fn reveal(&mut self, section: Section) {
        self.open[section as usize] = true;
    }
    /// Indices in `names` of the aliases listed under "additional aliases".
    pub fn extra_aliases(&self) -> std::ops::Range<usize> {
        usize::from(self.original.is_some()).min(self.names.len())..self.names.len()
    }
    fn primary(&self) -> FormField {
        if self.original.is_some() {
            FormField::Name(0)
        } else {
            FormField::Input(0)
        }
    }
    fn section_of(&self, field: FormField) -> Option<Section> {
        match field {
            FormField::Name(i) if self.extra_aliases().contains(&i) => Some(Section::Aliases),
            FormField::Input(i) if ADVANCED.contains(&i) => Some(Section::Advanced),
            FormField::Input(8) => Some(Section::Organization),
            _ => None,
        }
    }
    /// Every focusable element in visual order. Tab never opens a section: closed
    /// sections contribute their header only.
    pub fn stops(&self) -> Vec<Stop> {
        use Control as C;
        let mut stops = vec![];
        if self.original.is_none() {
            stops.push(Stop::Control(C::File));
        }
        stops.push(Stop::Field(self.primary()));
        stops.extend((1..=4).map(|i| Stop::Field(FormField::Input(i))));
        stops.push(Stop::Control(C::KeyPicker));
        if !self.is_duplicate() {
            stops.push(Stop::Control(C::Section(Section::Aliases)));
            if self.is_open(Section::Aliases) {
                for i in self.extra_aliases() {
                    stops.push(Stop::Field(FormField::Name(i)));
                    stops.push(Stop::Control(C::RemoveAlias(i)));
                }
                stops.push(Stop::Control(C::AddAlias));
            }
        }
        stops.push(Stop::Control(C::Section(Section::Advanced)));
        if self.is_open(Section::Advanced) {
            stops.extend(ADVANCED.map(|i| Stop::Field(FormField::Input(i))));
        }
        stops.push(Stop::Control(C::Section(Section::Organization)));
        if self.is_open(Section::Organization) {
            stops.push(Stop::Field(FormField::Input(8)));
            if self.original.is_some() {
                stops.push(Stop::Control(C::Note));
            }
        }
        stops.push(Stop::Control(C::Preview));
        stops.push(Stop::Control(C::Cancel));
        stops
    }
    pub fn current(&self) -> Stop {
        match self.control {
            Some(control) => Stop::Control(control),
            None if self.name_focus => Stop::Field(FormField::Name(self.focus)),
            None => Stop::Field(FormField::Input(self.focus)),
        }
    }
    pub fn focus_on(&mut self, stop: Stop) {
        match stop {
            Stop::Control(control) => self.control = Some(control),
            Stop::Field(FormField::Name(i)) => {
                self.control = None;
                self.name_focus = true;
                self.focus = i;
            }
            Stop::Field(FormField::Input(i)) => {
                self.control = None;
                self.name_focus = false;
                self.focus = i;
            }
        }
    }
    /// The text input owning the caret, if a field has the focus.
    pub fn focused_input(&mut self) -> Option<&mut Input> {
        match self.current() {
            Stop::Field(FormField::Name(i)) => self.names.get_mut(i),
            Stop::Field(FormField::Input(i)) => self.inputs.get_mut(i),
            Stop::Control(_) => None,
        }
    }
    fn step(&mut self, back: bool) {
        let stops = self.stops();
        let position = stops
            .iter()
            .position(|stop| *stop == self.current())
            .unwrap_or(0);
        let next = if back {
            (position + stops.len() - 1) % stops.len()
        } else {
            (position + 1) % stops.len()
        };
        self.focus_on(stops[next]);
    }
    fn add_alias(&mut self) {
        self.names.push(Input {
            placeholder: crate::t!("required", "requis").into(),
            limit: 50,
            ..Input::default()
        });
        self.open[Section::Aliases as usize] = true;
        self.focus_on(Stop::Field(FormField::Name(self.names.len() - 1)));
    }
    fn remove_alias(&mut self, index: usize) {
        if !self.extra_aliases().contains(&index) {
            return;
        }
        self.names.remove(index);
        self.errors.clear();
        // Stay in the aliases: on the one that took this place, else on Add.
        if self.extra_aliases().contains(&index) {
            self.focus_on(Stop::Field(FormField::Name(index)));
        } else {
            self.focus_on(Stop::Control(Control::AddAlias));
        }
    }
    fn toggle(&mut self, section: Section) {
        self.open[section as usize] = !self.open[section as usize];
    }
    pub fn validate(&mut self) -> bool {
        self.errors = self.validation_errors();
        self.error.clear();
        if let Some(field) = self.errors.first().map(|error| error.field) {
            // Only the section holding the first error is opened to show it.
            if let Some(section) = self.section_of(field) {
                self.open[section as usize] = true;
            }
            self.focus_on(Stop::Field(field));
        }
        self.errors.is_empty()
    }
    fn validation_errors(&self) -> Vec<FieldError> {
        let mut errors = vec![];
        // Validate isolated fields against a known-good host so the existing rules
        // assign their errors structurally, without interpreting their display text.
        let valid = || Host {
            name: "host".into(),
            hostname: "localhost".into(),
            ..Host::default()
        };
        let mut check = |field, host: Host| {
            if let Err(error) = config::validate_host(&host) {
                errors.push(FieldError {
                    field,
                    message: error.to_string(),
                });
            }
        };
        if self.original.is_some() && self.names.is_empty() {
            check(
                FormField::Input(0),
                Host {
                    name: String::new(),
                    ..valid()
                },
            );
        }
        for (i, name) in self.names.iter().enumerate() {
            check(
                FormField::Name(i),
                Host {
                    name: name.value.trim().into(),
                    ..valid()
                },
            );
        }
        for (i, input) in self.inputs.iter().enumerate() {
            if i == 0 && self.original.is_some() {
                continue;
            }
            let value = input.value.trim().to_string();
            let mut host = valid();
            match i {
                0 => host.name = value,
                1 => host.hostname = value,
                2 => host.user = value,
                3 => host.port = value,
                4 => host.identity = value,
                5 => host.proxy_jump = value,
                6 => host.proxy_command = value,
                7 => host.options = config::options_to_config(&value),
                8 => host.tags = vec![value],
                9 => host.remote_command = value,
                10 => host.request_tty = value,
                _ => unreachable!(),
            }
            check(FormField::Input(i), host);
        }
        if let Some(source) = &self.copy_source {
            let name = self.inputs[0].value.trim();
            if name.contains(['*', '?', '!', '[', ']'])
                || name.eq_ignore_ascii_case(&source.name)
                || source
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(name))
            {
                errors.push(FieldError {
                    field: FormField::Input(0),
                    message:
                        crate::t!("copy requires a new concrete host name, distinct from every source alias", "la copie exige un nouveau nom d'hôte concret, distinct de tous les alias de la source")
                            .into(),
                });
            }
        }
        // Report in the order the form is read, whatever is currently unfolded.
        let primary = self.primary();
        errors.sort_by_key(|error| match error.field {
            field if field == primary => (0, 0),
            FormField::Input(i @ 1..=4) => (1, i),
            FormField::Name(i) => (2, i),
            FormField::Input(8) => (4, 0),
            FormField::Input(i) => (3, i),
        });
        errors
    }
    pub fn key(&mut self, k: KeyEvent) -> FormEvent {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
            return FormEvent::Cancel;
        }
        if ctrl && k.code == KeyCode::Char('s') {
            return FormEvent::Submit;
        }
        let current = self.current();
        if k.modifiers.contains(KeyModifiers::ALT) && !self.is_duplicate() {
            match (k.code, current) {
                (KeyCode::Char('a'), _) => {
                    self.add_alias();
                    return FormEvent::None;
                }
                (KeyCode::Char('d'), Stop::Field(FormField::Name(i))) => {
                    self.remove_alias(i);
                    return FormEvent::None;
                }
                _ => {}
            }
        }
        match (k.code, current) {
            (KeyCode::Tab | KeyCode::Down, _) => self.step(false),
            (KeyCode::BackTab | KeyCode::Up, _) => self.step(true),
            // Enter walks the fields and stops on the main action; only a button
            // continues the journey, so reaching the end never submits.
            (KeyCode::Enter, Stop::Field(_)) => {
                let stops = self.stops();
                let position = stops.iter().position(|stop| *stop == current).unwrap_or(0);
                let next = stops[position + 1..]
                    .iter()
                    .find(|stop| matches!(stop, Stop::Field(_)))
                    .copied()
                    .unwrap_or(Stop::Control(Control::Preview));
                self.focus_on(next);
            }
            (KeyCode::Enter | KeyCode::Char(' '), Stop::Control(control)) => match control {
                Control::Preview => return FormEvent::Submit,
                Control::Cancel => return FormEvent::Cancel,
                Control::File => return FormEvent::ChooseFile,
                Control::KeyPicker => return FormEvent::PickKey,
                Control::Note => return FormEvent::EditNote,
                Control::Section(section) => self.toggle(section),
                Control::AddAlias => self.add_alias(),
                Control::RemoveAlias(i) => self.remove_alias(i),
            },
            (KeyCode::Right, Stop::Control(Control::Section(section))) => {
                self.open[section as usize] = true;
            }
            (KeyCode::Left, Stop::Control(Control::Section(section))) => {
                self.open[section as usize] = false;
            }
            (KeyCode::Left | KeyCode::Right, Stop::Control(Control::Preview)) => {
                self.control = Some(Control::Cancel);
            }
            (KeyCode::Left | KeyCode::Right, Stop::Control(Control::Cancel)) => {
                self.control = Some(Control::Preview);
            }
            (_, Stop::Field(_)) => {
                if let Some(input) = self.focused_input() {
                    input.key(k);
                }
            }
            _ => {}
        }
        FormEvent::None
    }
    pub fn host(&self) -> Result<Host> {
        if let Some(error) = self.validation_errors().first() {
            bail!("{}", error.message);
        }
        let value = |i: usize| self.inputs[i].value.trim().to_string();
        let listed = self
            .names
            .iter()
            .map(|i| i.value.trim().to_string())
            .filter(|s| !s.is_empty());
        let names: Vec<String> = if self.original.is_some() {
            listed.collect()
        } else {
            std::iter::once(value(0)).chain(listed).collect()
        };
        if names.is_empty() || names[0].is_empty() {
            bail!(crate::t!(
                "at least one host name is required",
                "au moins un nom d'hôte est requis"
            ));
        }
        let mut host = Host {
            name: names[0].clone(),
            hostname: value(1),
            user: value(2),
            port: value(3),
            identity: value(4),
            proxy_jump: value(5),
            proxy_command: value(6),
            options: config::options_to_config(&value(7)),
            file_tags: self.file_tags.clone(),
            tags: value(8)
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
            remote_command: value(9),
            request_tty: value(10),
            source_file: self.target.clone(),
            ..Host::default()
        };
        if host.port.is_empty() && !self.is_duplicate() {
            host.port = "22".into();
        }
        if let Some(original) = &self.original {
            // The selected alias identifies the edited host until explicitly removed.
            if names.contains(&original.name) {
                host.name = original.name.clone();
            }
            host.extra_identities = original.extra_identities.clone();
            host.line_number = original.line_number;
            host.aliases = names.clone();
        } else if names.len() > 1 {
            // A new block declared with several aliases shares one configuration.
            host.aliases = names.clone();
        }
        if let Some(source) = &self.copy_source {
            host.extra_identities = source.extra_identities.clone();
        }
        for name in names {
            let mut check = host.clone();
            check.name = name;
            // Existing extra identities are read-only and survive edits even if unavailable here.
            check.extra_identities.clear();
            config::validate_host(&check)?;
        }
        Ok(host)
    }
}
#[derive(Clone, Debug)]
pub(super) struct ForwardForm {
    pub host: String,
    pub original_name: Option<String>,
    pub inputs: Vec<Input>,
    pub kind: usize,
    pub mode: crate::storage::tunnels::LaunchMode,
    baseline: (usize, Vec<String>, crate::storage::tunnels::LaunchMode),
    pub errors: Vec<FieldError>,
    pub focus: usize,
    pub error: String,
    pub scroll: usize,
}
impl ForwardForm {
    pub fn new(host: String, previous: Option<Forward>) -> Self {
        let old = previous.unwrap_or_else(|| Forward {
            kind: "local".into(),
            remote_host: "localhost".into(),
            ..Forward::default()
        });
        let kind = match old.kind.as_str() {
            "remote" => 1,
            "dynamic" => 2,
            _ => 0,
        };
        let mut inputs = vec![
            Input::default(),
            Input::new(&old.local_port),
            Input::new(&old.remote_host),
            Input::new(&old.remote_port),
            Input::new(&old.bind_address),
            Input::default(), // Named profile; index 5 preserves existing forwarding fields.
        ];
        for (i, hint) in [
            (1, "8080"),
            (2, "localhost"),
            (3, "80"),
            (
                4,
                crate::t!("127.0.0.1 (optional)", "127.0.0.1 (facultatif)"),
            ),
        ] {
            inputs[i].placeholder = hint.into();
            inputs[i].limit = [0, 5, 100, 5, 50][i];
        }
        let mode = crate::storage::tunnels::LaunchMode::Shell;
        let baseline = (kind, input_values(&inputs), mode);
        Self {
            baseline,
            mode,
            original_name: None,
            errors: vec![],
            host,
            inputs,
            kind,
            focus: 0,
            error: String::new(),
            scroll: 0,
        }
    }
    pub fn named(host: String, name: String, forward: Forward) -> Self {
        let mut form = Self::new(host, Some(forward));
        form.inputs[5].set(&name);
        form.original_name = Some(name);
        form.focus = 5;
        form.mark_saved();
        form
    }
    pub fn mark_saved(&mut self) {
        self.baseline = (self.kind, input_values(&self.inputs), self.mode);
        self.original_name = Some(self.inputs[5].value.clone());
        self.error.clear();
    }
    pub fn validate_save(&mut self) -> bool {
        self.validate();
        if let Err(error) = crate::storage::tunnels::validate_name(&self.inputs[5].value) {
            self.errors.insert(
                0,
                FieldError {
                    field: FormField::Input(5),
                    message: error.to_string(),
                },
            );
            self.focus = 5;
        }
        self.errors.is_empty()
    }
    pub fn dirty(&self) -> bool {
        self.baseline != (self.kind, input_values(&self.inputs), self.mode)
    }
    pub fn validate(&mut self) -> bool {
        self.errors = self.validation_errors();
        self.error.clear();
        if let Some(error) = self.errors.first() {
            if let FormField::Input(i) = error.field {
                self.focus = i;
            }
        }
        self.errors.is_empty()
    }
    fn validation_errors(&self) -> Vec<FieldError> {
        let mut errors = vec![];
        for i in self.fields().into_iter().filter(|i| (1..=4).contains(i)) {
            let mut f = Forward {
                kind: ["local", "remote", "dynamic"][self.kind].into(),
                local_port: "1".into(),
                remote_port: "1".into(),
                remote_host: "localhost".into(),
                ..Forward::default()
            };
            let value = self.inputs[i].value.trim().to_string();
            match i {
                1 => f.local_port = value,
                2 => {
                    if !value.is_empty() {
                        f.remote_host = value;
                    }
                }
                3 => f.remote_port = value,
                4 => f.bind_address = value,
                _ => unreachable!(),
            }
            if let Err(error) = f.validate() {
                errors.push(FieldError {
                    field: FormField::Input(i),
                    message: error.to_string(),
                });
            }
        }
        errors
    }
    /// Stops in visual order: inputs 1 to 5, the type selector (0), then the
    /// launch mode and the three buttons.
    pub fn fields(&self) -> Vec<usize> {
        use ForwardForm as F;
        if self.kind == 2 {
            vec![5, 0, 1, 4, F::MODE, F::LAUNCH, F::SAVE, F::CANCEL]
        } else {
            vec![5, 0, 1, 2, 3, 4, F::MODE, F::LAUNCH, F::SAVE, F::CANCEL]
        }
    }
    pub const MODE: usize = 6;
    pub const LAUNCH: usize = 7;
    pub const SAVE: usize = 8;
    pub const CANCEL: usize = 9;
    /// The text input owning the caret, if one has the focus.
    pub fn focused_input(&mut self) -> Option<&mut Input> {
        (1..=5)
            .contains(&self.focus)
            .then(|| &mut self.inputs[self.focus])
    }
    fn toggle_mode(&mut self) {
        use crate::storage::tunnels::LaunchMode;
        self.mode = if self.mode == LaunchMode::Shell {
            LaunchMode::TunnelOnly
        } else {
            LaunchMode::Shell
        };
    }
    /// The route in words, so the direction never depends on knowing -L from -R.
    pub fn route(&self) -> String {
        let value = |i: usize, empty: &str| {
            let text = self.inputs[i].value.trim();
            if text.is_empty() {
                empty.to_owned()
            } else {
                text.to_owned()
            }
        };
        let bind = self.inputs[4].value.trim();
        match self.kind {
            0 => crate::tf!(
                "Listen on this machine, {}port {} → reach {}:{} from the SSH server",
                "Écouter sur ce poste, {}port {} → joindre {}:{} depuis le serveur SSH",
                if bind.is_empty() {
                    String::new()
                } else {
                    crate::tf!("address {bind}, ", "adresse {bind}, ")
                },
                value(1, "?"),
                value(2, "localhost"),
                value(3, "?")
            ),
            1 => crate::tf!(
                "Listen on the SSH server, {}port {} → reach {}:{} from this machine",
                "Écouter sur le serveur SSH, {}port {} → joindre {}:{} depuis ce poste",
                if bind.is_empty() {
                    String::new()
                } else {
                    crate::tf!("address {bind}, ", "adresse {bind}, ")
                },
                value(1, "?"),
                value(2, "localhost"),
                value(3, "?")
            ),
            _ => crate::tf!(
                "SOCKS proxy on this machine, {}port {} → connections leave from the SSH server",
                "Proxy SOCKS sur ce poste, {}port {} → les connexions sortent par le serveur SSH",
                if bind.is_empty() {
                    String::new()
                } else {
                    crate::tf!("address {bind}, ", "adresse {bind}, ")
                },
                value(1, "?")
            ),
        }
    }
    pub fn key(&mut self, k: KeyEvent) -> FormEvent {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
            return FormEvent::Cancel;
        }
        if ctrl && k.code == KeyCode::Char('s') {
            return FormEvent::Submit;
        }
        if ctrl && k.code == KeyCode::Char('l') {
            return FormEvent::Launch;
        }
        if ctrl && k.code == KeyCode::Char('t') {
            self.toggle_mode();
            return FormEvent::None;
        }
        let fields = self.fields();
        let pos = fields.iter().position(|&i| i == self.focus).unwrap_or(0);
        let button = self.focus >= Self::LAUNCH;
        match k.code {
            KeyCode::Left | KeyCode::Right if self.focus == 0 => {
                self.kind = (self.kind + if k.code == KeyCode::Left { 2 } else { 1 }) % 3;
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if self.focus == Self::MODE => {
                self.toggle_mode();
            }
            KeyCode::Left if button => {
                self.focus = Self::LAUNCH + (self.focus - Self::LAUNCH + 2) % 3;
            }
            KeyCode::Right if button => {
                self.focus = Self::LAUNCH + (self.focus - Self::LAUNCH + 1) % 3;
            }
            // Saving and launching are two named buttons; only they act.
            KeyCode::Enter | KeyCode::Char(' ') if button => {
                return match self.focus {
                    Self::LAUNCH => FormEvent::Launch,
                    Self::SAVE => FormEvent::Submit,
                    _ => FormEvent::Cancel,
                };
            }
            // Enter walks the fields and stops on the last one: repeating it
            // never reaches a button, so it can neither save nor launch.
            KeyCode::Enter => {
                if pos + 1 < fields.len() && fields[pos + 1] < Self::MODE {
                    self.focus = fields[pos + 1];
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                if pos + 1 < fields.len() {
                    self.focus = fields[pos + 1];
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if pos > 0 {
                    self.focus = fields[pos - 1];
                }
            }
            _ => {
                if let Some(input) = self.focused_input() {
                    input.key(k);
                }
            }
        }
        FormEvent::None
    }
    pub fn forward(&self) -> Result<Forward> {
        let f = Forward {
            kind: ["local", "remote", "dynamic"][self.kind].into(),
            local_port: self.inputs[1].value.trim().into(),
            remote_host: if self.inputs[2].value.trim().is_empty() {
                "localhost".into()
            } else {
                self.inputs[2].value.trim().into()
            },
            remote_port: self.inputs[3].value.trim().into(),
            bind_address: self.inputs[4].value.trim().into(),
        };
        f.validate()?;
        Ok(f)
    }
}
