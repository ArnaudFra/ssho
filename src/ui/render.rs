//! Ratatui screens; labels and arrangement follow SSHM's terminal UI.
use super::*;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};
const CYAN: Color = Color::Rgb(0, 173, 216);
const GRAY: Color = Color::DarkGray;
fn accent() -> Style {
    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
}
fn block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { CYAN } else { GRAY }))
}
fn inset(area: Rect) -> Rect {
    if area.width > 4 && area.height > 4 {
        area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        })
    } else {
        area
    }
}
pub(super) fn draw(f: &mut Frame, m: &mut Model) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    list(f, m, area);
}
fn list(f: &mut Frame, m: &mut Model, area: Rect) {
    let area = inset(area);
    let title_height = if area.height >= 20 { 5 } else { 1 };
    let notice_height = u16::from(!m.error.is_empty()) + u16::from(m.show_hidden);
    let search_height = if area.height < 9 { 1 } else { 3 };
    let help_height = if area.height > 13 { 2 } else { 1 };
    let available_table_height = area
        .height
        .saturating_sub(title_height + notice_height + search_height + help_height)
        .max(1);
    // Two border lines, header plus its separator, then only the existing rows.
    let table_height = u16::try_from(m.visible.len().max(1))
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .min(available_table_height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(title_height),
            Constraint::Length(notice_height),
            Constraint::Length(search_height),
            Constraint::Length(table_height),
            Constraint::Length(help_height),
            Constraint::Min(0),
        ])
        .split(area);
    let title = if title_height == 5 {
        " _____ _____ __ __ _____\n|   __|   __|  |  |     |\n|__   |__   |     |  |  |\n|_____|_____|__|__|_____|\n          SSHO"
    } else {
        "SSHO"
    };
    f.render_widget(Paragraph::new(title).style(accent()), chunks[0]);
    let mut notices = vec![];
    if !m.error.is_empty() {
        notices.push(Line::styled(
            format!("Error: {}", m.error),
            Style::default().fg(Color::Red),
        ));
    }
    if m.show_hidden {
        notices.push(Line::styled(
            "[showing hidden hosts — press H to hide]",
            Style::default().fg(Color::Yellow),
        ));
    }
    f.render_widget(Paragraph::new(notices), chunks[1]);
    let prompt = if m.search_focus {
        "Search: "
    } else {
        "Search (/ or Tab to focus): "
    };
    let width = chunks[2].width.saturating_sub(prompt.len() as u16 + 3) as usize;
    let search = format!("{prompt}{}", m.query.display(m.search_focus, width));
    let search = Paragraph::new(search);
    if chunks[2].height >= 3 {
        f.render_widget(search.block(block("", m.search_focus)), chunks[2]);
    } else {
        f.render_widget(search, chunks[2]);
    }
    let table_area = chunks[3];
    let header = Row::new(vec![
        if m.recent { "Name" } else { "Name ↓" },
        "Hostname",
        "Tags",
        if m.recent {
            "Last Login ↓"
        } else {
            "Last Login"
        },
    ])
    .style(Style::default().fg(if m.search_focus { GRAY } else { CYAN }))
    .bottom_margin(1);
    let rows = m.visible.iter().map(|&index| {
        let h = &m.all[index];
        let tags = h
            .tags
            .iter()
            .map(|s| format!("#{s}"))
            .collect::<Vec<_>>()
            .join(" ");
        let time = m
            .history
            .get(&h.name)
            .map(|c| time_ago(c.last_connect))
            .unwrap_or_default();
        Row::new(vec![
            Cell::from(h.name.clone()),
            Cell::from(h.hostname.clone()),
            Cell::from(tags),
            Cell::from(time),
        ])
    });
    let widths = if table_area.width >= 75 {
        vec![
            Constraint::Percentage(26),
            Constraint::Percentage(30),
            Constraint::Percentage(23),
            Constraint::Percentage(21),
        ]
    } else {
        vec![
            Constraint::Percentage(25),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
        ]
    };
    m.page = table_area.height.saturating_sub(4).max(1) as usize;
    let mut state = TableState::default()
        .with_selected(if m.visible.is_empty() {
            None
        } else {
            Some(m.selected)
        })
        .with_offset(m.offset);
    f.render_stateful_widget(
        Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .row_highlight_style(Style::default().bg(CYAN).fg(Color::Indexed(229)))
            .block(block("", !m.search_focus)),
        table_area,
        &mut state,
    );
    m.offset = state.offset();
    if m.visible.is_empty() {
        let empty = table_area.inner(Margin {
            horizontal: 1,
            vertical: 2,
        });
        f.render_widget(
            Paragraph::new(if m.all.is_empty() {
                "No SSH hosts found. Press Tab, then a to add a host."
            } else {
                "No matching hosts."
            })
            .style(Style::default().fg(GRAY)),
            empty,
        );
    }
    let help = if m.search_focus {
        "↑/↓: select • Enter: connect • Tab: table shortcuts • Esc: clear/quit"
    } else {
        "↑/↓: navigate • Enter: connect • Tab: search • q: quit"
    };
    f.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: false }),
        chunks[4],
    );
}
