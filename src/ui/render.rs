//! Ratatui screens; labels and arrangement follow SSHM's terminal UI.
use super::*;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
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
fn panel(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
pub(super) fn draw(f: &mut Frame, m: &mut Model) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    match &m.view {
        View::List => list(f, m, area),
        View::Host(_) => host_form(f, m, area),
        View::Info(_) => info(f, m, area),
        View::Help => help(f, m, area),
        View::Files { .. } => files(f, m, area),
    }
}
fn list(f: &mut Frame, m: &mut Model, area: Rect) {
    if let Some(h) = &m.delete {
        let area = panel(area, 72, 11);
        f.render_widget(Clear, area);
        let text = vec![
            Line::styled(
                "DELETE SSH HOST",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::from(format!(
                "Are you sure you want to delete host '{}' ?",
                h.name
            )),
            Line::from(""),
            Line::styled(
                "This action cannot be undone.",
                Style::default().fg(Color::LightRed),
            ),
            Line::from(""),
            Line::styled("Enter: confirm • Esc: cancel", Style::default().fg(GRAY)),
        ];
        f.render_widget(
            Paragraph::new(text)
                .block(block("", false).border_style(Style::default().fg(Color::Red)))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
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
        "↑/↓: navigate • Enter: connect • Tab: search • i: info • h: help • q: quit"
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
fn files(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Files {
        files,
        selected,
        host,
        ..
    } = &m.view
    else {
        return;
    };
    let area = inset(area);
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    let title = host
        .as_ref()
        .map(|h| format!("Select destination config file for host '{}':", h.name))
        .unwrap_or_else(|| "Select config file to add host to:".into());
    f.render_widget(Paragraph::new(title).style(accent()), parts[0]);
    let count = parts[1].height.max(1) as usize;
    let offset = selected.saturating_sub(count - 1);
    let lines = files
        .iter()
        .enumerate()
        .skip(offset)
        .map(|(i, path)| {
            Line::styled(
                format!(
                    "{} {}",
                    if i == *selected { "▶" } else { " " },
                    path.display()
                ),
                if i == *selected {
                    Style::default().bg(CYAN).fg(Color::Indexed(229))
                } else {
                    Style::default()
                },
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(
        Paragraph::new(if files.is_empty() {
            vec![Line::from("No SSH config files found.")]
        } else {
            lines
        }),
        parts[1],
    );
    f.render_widget(
        Paragraph::new("↑/↓: navigate • Enter: select • Esc: cancel")
            .style(Style::default().fg(GRAY)),
        parts[2],
    );
}
fn info(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Info(h) = &m.view else {
        return;
    };
    let area = panel(area, 92, 28);
    let title = format!("SSH Host Information: {}", h.name);
    let border = block(&title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(4)]).split(inner);
    let optional = |s: &str| {
        if s.is_empty() {
            "Not set".to_string()
        } else {
            s.to_string()
        }
    };
    let mut identities = vec![optional(&h.identity)];
    identities.extend(h.extra_identities.clone());
    let sections = [
        ("Host Name", h.name.clone()),
        ("Config File", h.source_file.display().to_string()),
        ("Hostname/IP", h.hostname.clone()),
        ("User", optional(&h.user)),
        ("Port", optional(&h.port)),
        ("Identity File", identities.join("\n")),
        ("ProxyJump", optional(&h.proxy_jump)),
        ("ProxyCommand", optional(&h.proxy_command)),
        ("SSH Options", optional(&h.options)),
        ("Tags", optional(&h.tags.join(", "))),
    ];
    let mut lines = vec![Line::from("")];
    for (label, value) in sections {
        for (i, line) in value.lines().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{:>15} ",
                        if i == 0 {
                            format!("{label}:")
                        } else {
                            String::new()
                        }
                    ),
                    accent(),
                ),
                Span::raw(line.to_string()),
            ]));
        }
    }
    m.scroll = m
        .scroll
        .min(lines.len().saturating_sub(parts[0].height as usize));
    f.render_widget(
        Paragraph::new(lines)
            .scroll((m.scroll as u16, 0))
            .wrap(Wrap { trim: false }),
        parts[0],
    );
    f.render_widget(
        Paragraph::new(
            "Actions:\n  e/Enter - Switch to edit mode\n  q/Esc   - Return to host list",
        )
        .style(Style::default().fg(GRAY)),
        parts[1],
    );
}
fn help(f: &mut Frame, m: &mut Model, area: Rect) {
    let area = panel(area, 96, 25);
    let border = block("SSHO - Commands", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
    let left = [
        "Navigation & Connection",
        "",
        "Enter  connect (also from search)",
        "↑↓     select (Ctrl+P/N in search)",
        "i      show host information",
        "/      search hosts",
        "Tab    switch search / table",
        "",
        "Host Management",
        "",
        "a      add new host",
        "e      edit selected host",
        "m      move host to another config",
        "d      delete selected host",
    ];
    let right = [
        "Advanced Features",
        "",
        "H      toggle hidden hosts visibility",
        "s      cycle sort modes",
        "n      sort by name",
        "r      sort by recent connection",
        "",
        "System",
        "",
        "h      show this help",
        "q      quit application",
        "Esc    clear search / exit view",
        "",
        "",
    ];
    let text = if inner.width >= 77 {
        left.iter()
            .zip(right.iter())
            .map(|(a, b)| format!("{a:<39}  {b}"))
            .collect::<Vec<_>>()
    } else {
        left.iter()
            .chain(right.iter())
            .map(|s| s.to_string())
            .collect()
    };
    m.scroll = m
        .scroll
        .min(text.len().saturating_sub(parts[0].height as usize));
    f.render_widget(
        Paragraph::new(text.join("\n")).scroll((m.scroll as u16, 0)),
        parts[0],
    );
    f.render_widget(
        Paragraph::new("Press ESC, h, q or Enter to close").style(Style::default().fg(GRAY)),
        parts[1],
    );
}
