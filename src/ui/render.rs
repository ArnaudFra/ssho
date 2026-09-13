//! Ratatui screens; labels and arrangement follow SSHM's terminal UI.
use super::*;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
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
    match &m.view {
        View::List => list(f, m, area),
        View::Host(_) => host_form(f, m, area),
    }
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
        "↑/↓: navigate • Enter: connect • Tab: search • a: add • e: edit • q: quit"
    };
    f.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: false }),
        chunks[4],
    );
}
fn field_lines(
    lines: &mut Vec<Line<'static>>,
    label: String,
    input: &Input,
    focused: bool,
    width: usize,
) -> usize {
    let y = lines.len();
    lines.push(Line::styled(
        label,
        if focused { accent() } else { Style::default() },
    ));
    lines.push(Line::styled(
        format!(
            "{}{}",
            if focused { "> " } else { "  " },
            input.display(focused, width.saturating_sub(2))
        ),
        if input.value.is_empty() && !focused {
            Style::default().fg(GRAY)
        } else {
            Style::default()
        },
    ));
    lines.push(Line::from(""));
    y
}
fn scroll_focus(scroll: &mut usize, focus: usize, height: u16, total: usize) {
    let h = height as usize;
    if focus < *scroll {
        *scroll = focus;
    }
    if focus + 2 >= scroll.saturating_add(h) {
        *scroll = (focus + 3).saturating_sub(h);
    }
    *scroll = (*scroll).min(total.saturating_sub(h));
}
fn host_form(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Host(form) = &mut m.view else {
        return;
    };
    let area = inset(area);
    let footer_height = if area.height >= 12 { 4 } else { 2 };
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(if form.original.is_some() {
            "Edit SSH Host"
        } else {
            "Add SSH Host Configuration"
        })
        .style(accent()),
        parts[0],
    );
    let mut lines = vec![];
    let mut focus_y = 0;
    let width = parts[1].width as usize;
    if form.original.is_some() {
        lines.push(Line::styled(
            format!("Config file: {}", form.target.display()),
            Style::default().fg(GRAY),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled("Host Names", accent()));
        lines.push(Line::from(""));
        for (i, name) in form.names.iter().enumerate() {
            let selected = form.name_focus && form.focus == i;
            let y = field_lines(
                &mut lines,
                format!("Host Name {} *", i + 1),
                name,
                selected,
                width,
            );
            if selected {
                focus_y = y;
            }
        }
        lines.push(Line::styled("Common Properties", accent()));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled(
            if form.tab == 0 {
                "[ General ]"
            } else {
                "  General  "
            },
            if form.tab == 0 {
                accent()
            } else {
                Style::default().fg(GRAY)
            },
        ),
        Span::raw("  "),
        Span::styled(
            if form.tab == 1 {
                "[ Advanced ]"
            } else {
                "  Advanced  "
            },
            if form.tab == 1 {
                accent()
            } else {
                Style::default().fg(GRAY)
            },
        ),
    ]));
    lines.push(Line::from(""));
    for i in form.fields() {
        let selected = !form.name_focus && form.focus == i;
        let label = if form.original.is_none() && i == 5 {
            "ProxyJump"
        } else if form.original.is_none() && i == 6 {
            "ProxyCommand"
        } else {
            forms::LABELS[i]
        };
        let y = field_lines(&mut lines, label.into(), &form.inputs[i], selected, width);
        if selected {
            focus_y = y;
        }
        if i == 4 {
            if let Some(h) = &form.original {
                for identity in &h.extra_identities {
                    lines.push(Line::styled(
                        format!("  also: {identity} (read-only, kept on save)"),
                        Style::default().fg(GRAY),
                    ));
                }
            }
        }
        if i == 8 && selected {
            lines.push(Line::styled(
                "  tip: use \"hidden\" to hide this host from the list",
                Style::default().fg(GRAY),
            ));
        }
    }
    scroll_focus(&mut form.scroll, focus_y, parts[1].height, lines.len());
    f.render_widget(
        Paragraph::new(lines).scroll((form.scroll as u16, 0)),
        parts[1],
    );
    let mut footer = vec![];
    if !form.error.is_empty() {
        footer.push(Line::styled(
            format!("Error: {}", form.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::styled(
        if form.original.is_some() {
            "Tab/↑↓/Enter: navigate • Ctrl+J/K: switch tabs • Ctrl+A/D: add/delete host"
        } else {
            "Tab/Shift+Tab: navigate • Ctrl+J/K: switch tabs"
        },
        Style::default().fg(GRAY),
    ));
    footer.push(Line::styled(
        "Enter on last field: submit • Ctrl+S: save • Ctrl+C/Esc: cancel • * Required fields",
        Style::default().fg(GRAY),
    ));
    f.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), parts[2]);
}
