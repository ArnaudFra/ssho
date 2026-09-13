//! Form state and editing; adapted from SSHM (MIT, Guillaume Archambault).
use crate::{config, types::Host};
use anyhow::{bail, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

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
    pub fn display(&self, focused: bool, width: usize) -> String {
        if self.value.is_empty() {
            let prefix = if focused { "▏" } else { "" };
            return format!(
                "{prefix}{}",
                self.placeholder
                    .chars()
                    .take(width.saturating_sub(usize::from(focused)))
                    .collect::<String>()
            );
        }
        let chars: Vec<char> = self.value.chars().collect();
        let start = if focused {
            self.cursor.saturating_sub(width.saturating_sub(2))
        } else {
            0
        };
        let mut out = String::new();
        for (i, c) in chars.iter().enumerate().skip(start).take(width) {
            if focused && i == self.cursor {
                out.push('▏');
            }
            out.push(*c);
        }
        if focused && self.cursor == chars.len() {
            out.push('▏');
        }
        out
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum FormEvent {
    None,
    Cancel,
    Submit,
}
#[derive(Clone, Debug)]
pub(super) struct HostForm {
    pub inputs: Vec<Input>,
    pub names: Vec<Input>,
    pub original: Option<Host>,
    pub target: PathBuf,
    pub name_focus: bool,
    pub focus: usize,
    pub tab: usize,
    pub error: String,
    pub scroll: usize,
    pub default_user: String,
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
        Self {
            inputs,
            names: vec![],
            original: None,
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
            f.inputs[i].set(value);
        }
        f.original = Some(host);
        f.name_focus = true;
        f
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
        if host.port.is_empty() {
            host.port = "22".into();
        }
        if self.original.is_none() && host.user.is_empty() {
            host.user = self.default_user.clone();
        }
        if let Some(original) = &self.original {
            host.extra_identities = original.extra_identities.clone();
            host.line_number = original.line_number;
            host.aliases = names.clone();
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
