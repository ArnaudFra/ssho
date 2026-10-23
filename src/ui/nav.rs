//! Navigation rules shared by every screen: what cancels a local step, how a
//! simple menu wraps, and how a confirmation names its two choices.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

/// Esc and Ctrl+C follow one rule everywhere: leave the local step, one level up.
pub(super) fn cancels(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c'))
}

/// Move in a simple menu or selector of `len` entries, wrapping at both ends.
/// Tables, trees, logs and diffs stay bounded to keep a spatial landmark.
pub(super) fn wrap(selected: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    (selected.min(len as usize - 1) as isize + delta).rem_euclid(len) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Answer {
    /// Keep the draft or the running work; nothing is lost.
    Stay,
    Proceed,
}

/// A named action. The focused one carries a marker in addition to its colours.
pub(super) fn button(label: &str, focused: bool) -> Span<'static> {
    Span::styled(
        format!("{} {label} ]", if focused { "[▶" } else { "[ " }),
        if focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(0, 173, 216))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        },
    )
}

/// Two named choices. The safe one has the focus first, so a repeated Enter
/// never discards a draft or stops owned work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Confirm {
    pub proceed: bool,
}

impl Confirm {
    pub(super) fn key(&mut self, key: KeyEvent) -> Option<Answer> {
        if cancels(key) {
            return Some(Answer::Stay);
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                self.proceed = !self.proceed;
                None
            }
            KeyCode::Enter if self.proceed => Some(Answer::Proceed),
            KeyCode::Enter => Some(Answer::Stay),
            _ => None,
        }
    }

    /// The focused choice carries a marker in addition to its colours.
    pub(super) fn draw(
        self,
        f: &mut Frame,
        area: Rect,
        title: &str,
        mut lines: Vec<Line<'static>>,
        stay: &str,
        proceed: &str,
    ) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            button(stay, !self.proceed),
            Span::raw("  "),
            button(proceed, self.proceed),
        ]));
        let hint = format!("Tab/←→ : choisir • Entrée : valider • Échap : {stay}");
        let width = lines
            .iter()
            .map(Line::width)
            .chain([hint.width(), title.width() + 2])
            .max()
            .unwrap_or(0) as u16
            + 4;
        let width = width.clamp(40, 76).min(area.width);
        // Wrapped summaries need their real height on narrow terminals.
        let inner_width = usize::from(width.saturating_sub(2)).max(1);
        let rows: usize = lines
            .iter()
            .map(|line| line.width().max(1).div_ceil(inner_width))
            .sum();
        let height = (rows as u16 + 4).min(area.height);
        let area = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(title.to_owned())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(0, 173, 216)));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let parts = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), parts[0]);
        f.render_widget(
            Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
            parts[1],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn menus_wrap_at_both_ends_and_tolerate_empty_or_stale_positions() {
        assert_eq!(wrap(0, 3, -1), 2);
        assert_eq!(wrap(2, 3, 1), 0);
        assert_eq!(wrap(1, 3, 1), 2);
        assert_eq!(wrap(0, 1, 1), 0);
        assert_eq!(wrap(0, 0, 1), 0);
        assert_eq!(wrap(9, 3, 1), 0);
    }

    #[test]
    fn confirmation_keeps_the_safe_choice_until_the_other_is_selected() {
        let mut confirm = Confirm::default();
        assert_eq!(confirm.key(key(KeyCode::Enter)), Some(Answer::Stay));
        assert_eq!(confirm.key(key(KeyCode::Char('y'))), None);
        assert_eq!(confirm.key(key(KeyCode::Tab)), None);
        assert_eq!(confirm.key(key(KeyCode::Enter)), Some(Answer::Proceed));
        assert_eq!(confirm.key(key(KeyCode::Left)), None);
        assert!(!confirm.proceed);
        confirm.proceed = true;
        // Esc and Ctrl+C never proceed, whatever has the focus.
        assert_eq!(confirm.key(key(KeyCode::Esc)), Some(Answer::Stay));
        assert_eq!(
            confirm.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Answer::Stay)
        );
    }
}
