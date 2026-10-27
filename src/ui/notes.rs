//! Personal note draft; persistence is owned by the application action boundary.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub host: String,
    pub input: Input,
    baseline: String,
    pub error: String,
    /// Focused button under the text: 0 Save, 1 Cancel. `None` while typing.
    pub button: Option<usize>,
}

impl Editor {
    pub fn new(host: String, note: &str) -> Self {
        Self {
            host,
            input: Input::new(note),
            baseline: note.into(),
            error: String::new(),
            button: None,
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
            // Tab walks text, Save, Cancel; a newline stays the meaning of Enter
            // in the text, so only a focused button is activated by it.
            KeyCode::Tab => {
                self.button = match self.button {
                    None => Some(0),
                    Some(0) => Some(1),
                    Some(_) => None,
                };
                FormEvent::None
            }
            KeyCode::BackTab => {
                self.button = match self.button {
                    None => Some(1),
                    Some(1) => Some(0),
                    Some(_) => None,
                };
                FormEvent::None
            }
            _ if self.button.is_some() => match (key.code, self.button) {
                (KeyCode::Enter | KeyCode::Char(' '), Some(0)) => FormEvent::Submit,
                (KeyCode::Enter | KeyCode::Char(' '), Some(_)) => FormEvent::Cancel,
                (KeyCode::Left | KeyCode::Right, Some(button)) => {
                    self.button = Some(1 - button);
                    FormEvent::None
                }
                _ => FormEvent::None,
            },
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
