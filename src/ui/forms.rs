//! Text input state; adapted from SSHM (MIT, Guillaume Archambault).
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, Default)]
pub(super) struct Input {
    pub value: String,
    pub cursor: usize,
    pub placeholder: String,
    pub limit: usize,
}
impl Input {
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
