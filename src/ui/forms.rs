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
#[derive(Clone, Debug)]
pub(super) struct HostForm {
    pub inputs: Vec<Input>,
    pub names: Vec<Input>,
    pub original: Option<Host>,
    copy_source: Option<Host>,
    pub target: PathBuf,
    pub name_focus: bool,
    pub focus: usize,
    pub tab: usize,
    pub error: String,
    pub scroll: usize,
    pub default_user: String,
    baseline: (Vec<String>, Vec<String>),
    pub errors: Vec<FieldError>,
}
impl HostForm {
    pub fn add(name: String, target: PathBuf) -> Self {
        let mut inputs = vec![Input::default(); 11];
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "root".into());
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut identity = home.join(".ssh/id_rsa");
        for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
            let p = home.join(".ssh").join(name);
            if p.exists() {
                identity = p;
                break;
            }
        }
        let hints = [
            "server-name".into(),
            "192.168.1.100 or example.com".into(),
            user.clone(),
            "22".into(),
            identity.display().to_string(),
            "user@jump-host:port or existing-host-name".into(),
            "ssh -W %h:%p Jumphost".into(),
            "-o Compression=yes -o ServerAliveInterval=60".into(),
            "production, web, database".into(),
            "ls -la, htop, bash".into(),
            "yes, no, force, auto".into(),
        ];
        for (i, hint) in hints.into_iter().enumerate() {
            inputs[i].placeholder = hint;
            inputs[i].limit = [50, 100, 50, 5, 200, 200, 200, 500, 200, 300, 10][i];
        }
        inputs[0].set(&name);
        let baseline = (input_values(&inputs), vec![]);
        Self {
            baseline,
            errors: vec![],
            inputs,
            names: vec![],
            original: None,
            copy_source: None,
            target,
            name_focus: false,
            focus: 0,
            tab: 0,
            error: String::new(),
            scroll: 0,
            default_user: user,
        }
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
        f.baseline = (input_values(&f.inputs), input_values(&f.names));
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
        form.baseline = (input_values(&form.inputs), vec![]);
        form
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
    pub fn dirty(&self) -> bool {
        self.baseline != (input_values(&self.inputs), input_values(&self.names))
    }
    pub fn validate(&mut self) -> bool {
        self.errors = self.validation_errors();
        self.error.clear();
        if let Some(error) = self.errors.first() {
            match error.field {
                FormField::Name(i) => {
                    self.name_focus = true;
                    self.focus = i;
                }
                FormField::Input(i) => {
                    self.name_focus = false;
                    self.focus = i;
                    self.tab = usize::from([7, 9, 10].contains(&i));
                }
            }
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
        if self.original.is_some() {
            if self.names.is_empty() {
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
                        "copy requires a new concrete host name, distinct from every source alias"
                            .into(),
                });
            }
        }
        // Aliases, General (including tags), then Advanced: match navigation order.
        errors.sort_by_key(|error| match error.field {
            FormField::Name(i) => (0, i),
            FormField::Input(i) => (
                1,
                match i {
                    8 => 7,
                    7 => 8,
                    _ => i,
                },
            ),
        });
        errors
    }
    pub fn fields(&self) -> Vec<usize> {
        if self.tab == 1 {
            vec![7, 9, 10]
        } else if self.original.is_some() {
            vec![1, 2, 3, 4, 5, 6, 8]
        } else {
            vec![0, 1, 2, 3, 4, 5, 6, 8]
        }
    }
    pub fn key(&mut self, k: KeyEvent) -> FormEvent {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
            return FormEvent::Cancel;
        }
        if ctrl {
            match k.code {
                KeyCode::Char('s') => return FormEvent::Submit,
                KeyCode::Char('j' | 'k') => {
                    self.tab = 1 - self.tab;
                    if !self.name_focus {
                        self.focus = self.fields()[0];
                    }
                    return FormEvent::None;
                }
                _ => {}
            }
        }
        if k.modifiers.contains(KeyModifiers::ALT) {
            match k.code {
                KeyCode::Char('a') if self.original.is_some() => {
                    self.names.push(Input::default());
                    self.name_focus = true;
                    self.focus = self.names.len() - 1;
                    return FormEvent::None;
                }
                KeyCode::Char('d')
                    if self.original.is_some() && self.name_focus && self.names.len() > 1 =>
                {
                    self.names.remove(self.focus);
                    self.focus = self.focus.min(self.names.len() - 1);
                    return FormEvent::None;
                }
                _ => {}
            }
        }
        match k.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Enter | KeyCode::Up | KeyCode::Down => {
                let back = matches!(k.code, KeyCode::BackTab | KeyCode::Up);
                if self.name_focus {
                    if back {
                        self.focus = if self.focus == 0 {
                            self.names.len() - 1
                        } else {
                            self.focus - 1
                        };
                    } else if self.focus + 1 < self.names.len() {
                        self.focus += 1;
                    } else {
                        self.name_focus = false;
                        self.focus = self.fields()[0];
                    }
                } else {
                    let fields = self.fields();
                    let p = fields.iter().position(|&i| i == self.focus).unwrap_or(0);
                    if !back && p + 1 == fields.len() && self.tab == 1 && k.code == KeyCode::Enter {
                        return FormEvent::Submit;
                    }
                    if back && p == 0 {
                        if self.tab == 1 {
                            self.tab = 0;
                            self.focus = *self.fields().last().unwrap();
                        } else if !self.names.is_empty() {
                            self.name_focus = true;
                            self.focus = self.names.len() - 1;
                        } else {
                            self.focus = *fields.last().unwrap();
                        }
                    } else if !back && p + 1 == fields.len() {
                        if self.tab == 0 {
                            self.tab = 1;
                            self.focus = 7;
                        } else if !self.names.is_empty() {
                            self.name_focus = true;
                            self.focus = 0;
                        } else {
                            self.focus = 7;
                        }
                    } else {
                        self.focus = fields[if back { p - 1 } else { p + 1 }];
                    }
                }
            }
            _ => {
                if self.name_focus {
                    self.names[self.focus].key(k);
                } else {
                    self.inputs[self.focus].key(k);
                }
            }
        }
        FormEvent::None
    }
    pub fn host(&self) -> Result<Host> {
        if let Some(error) = self.validation_errors().first() {
            bail!("{}", error.message);
        }
        let value = |i: usize| self.inputs[i].value.trim().to_string();
        let names: Vec<String> = if self.original.is_some() {
            self.names
                .iter()
                .map(|i| i.value.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![value(0)]
        };
        if names.is_empty() || names[0].is_empty() {
            bail!("at least one host name is required");
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
        if self.original.is_none() && !self.is_duplicate() && host.user.is_empty() {
            host.user = self.default_user.clone();
        }
        if let Some(original) = &self.original {
            // The selected alias identifies the edited host until explicitly removed.
            if names.contains(&original.name) {
                host.name = original.name.clone();
            }
            host.extra_identities = original.extra_identities.clone();
            host.line_number = original.line_number;
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
    pub inputs: Vec<Input>,
    pub kind: usize,
    baseline: (usize, Vec<String>),
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
        ];
        for (i, hint) in [
            (1, "8080"),
            (2, "localhost"),
            (3, "80"),
            (4, "127.0.0.1 (optional)"),
        ] {
            inputs[i].placeholder = hint.into();
            inputs[i].limit = [0, 5, 100, 5, 50][i];
        }
        let baseline = (kind, input_values(&inputs));
        Self {
            baseline,
            errors: vec![],
            host,
            inputs,
            kind,
            focus: 0,
            error: String::new(),
            scroll: 0,
        }
    }
    pub fn dirty(&self) -> bool {
        self.baseline != (self.kind, input_values(&self.inputs))
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
        for i in self.fields().into_iter().skip(1) {
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
    pub fn fields(&self) -> Vec<usize> {
        if self.kind == 2 {
            vec![0, 1, 4]
        } else {
            vec![0, 1, 2, 3, 4]
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
        let fields = self.fields();
        let pos = fields.iter().position(|&i| i == self.focus).unwrap_or(0);
        match k.code {
            KeyCode::Left | KeyCode::Right if self.focus == 0 => {
                self.kind = (self.kind + if k.code == KeyCode::Left { 2 } else { 1 }) % 3;
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                if pos + 1 < fields.len() {
                    self.focus = fields[pos + 1];
                } else if k.code == KeyCode::Enter {
                    return FormEvent::Submit;
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if pos > 0 {
                    self.focus = fields[pos - 1];
                }
            }
            _ => {
                if self.focus != 0 {
                    self.inputs[self.focus].key(k);
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
pub(super) const LABELS: [&str; 11] = [
    "Host Name *",
    "Hostname/IP *",
    "User",
    "Port",
    "Identity File",
    "Proxy Jump",
    "Proxy Command",
    "SSH Options",
    "Tags (comma-separated)",
    "Remote Command",
    "Request TTY",
];
