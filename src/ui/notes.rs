//! Personal note draft; persistence is owned by the application action boundary.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub host: String,
    pub input: Input,
    baseline: String,
    pub error: String,
}

impl Editor {
    pub fn new(host: String, note: &str) -> Self {
        Self {
            host,
            input: Input::new(note),
            baseline: note.into(),
            error: String::new(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.input.value != self.baseline
    }
    pub fn key(&mut self, mut key: KeyEvent) -> FormEvent {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl {
            key.code = match key.code {
                KeyCode::Char('a') => KeyCode::Home,
                KeyCode::Char('e') => KeyCode::End,
                code => code,
            };
        }
        match key.code {
            KeyCode::Esc => FormEvent::Cancel,
            KeyCode::Char('c') if ctrl => FormEvent::Cancel,
            KeyCode::Char('s') if ctrl => FormEvent::Submit,
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
                let chars: Vec<char> = self.input.value.chars().collect();
                let cursor = self.input.cursor.min(chars.len());
                let start = chars[..cursor]
                    .iter()
                    .rposition(|&c| c == '\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let end = cursor
                    + chars[cursor..]
                        .iter()
                        .position(|&c| c == '\n')
                        .unwrap_or(chars.len() - cursor);
                self.input.cursor = match key.code {
                    KeyCode::Home => start,
                    KeyCode::End => end,
                    KeyCode::Up if start > 0 => {
                        let previous = chars[..start - 1]
                            .iter()
                            .rposition(|&c| c == '\n')
                            .map(|i| i + 1)
                            .unwrap_or(0);
                        previous + (cursor - start).min(start - 1 - previous)
                    }
                    KeyCode::Down if end < chars.len() => {
                        let next = end + 1;
                        let length = chars[next..]
                            .iter()
                            .position(|&c| c == '\n')
                            .unwrap_or(chars.len() - next);
                        next + (cursor - start).min(length)
                    }
                    _ => cursor,
                };
                FormEvent::None
            }
            KeyCode::Enter => {
                self.input
                    .key(KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::NONE));
                FormEvent::None
            }
            _ => {
                self.input.key(key);
                FormEvent::None
            }
        }
    }
}
