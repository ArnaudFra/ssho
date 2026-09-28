//! Ratatui screens; labels and arrangement follow SSHM's terminal UI.
use super::*;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;
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
    if m.quit_tunnels {
        f.render_widget(
            Paragraph::new("Stop owned tunnels and quit?\nEnter: stop + quit • Esc: return")
                .block(block("Active tunnels", true))
                .wrap(Wrap { trim: false }),
            panel(area, 66, 7),
        );
        return;
    }
    if m.discard {
        let area = panel(area, 64, 7);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new("Discard changes?\nEnter: discard • Esc: resume editing")
                .block(block("Unsaved changes", true))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    match &m.view {
        View::List => list(f, m, area),
        View::Actions { .. } => {
            list(f, m, area);
            actions(f, m, area);
        }
        View::Note(_) => note_editor(f, m, area),
        View::Host(_) => host_form(f, m, area),
        View::Forward(_) => forward_form(f, m, area),
        View::Active { .. } => active::draw(f, m, area),
        View::Profiles(_) => profile_list(f, m, area),
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
    let viewport_width = area.width;
    let area = if area.height >= 12 { inset(area) } else { area };
    let search_error = m.search.as_ref().err();
    let notice_count = u16::from(search_error.is_some())
        + u16::from(!m.error.is_empty())
        + u16::from(m.update.as_ref().is_some_and(|u| u.available))
        + u16::from(m.show_hidden)
        + u16::from(m.favorites_only);
    let notice_height = if area.height < 12 {
        notice_count.min(1)
    } else {
        notice_count
    };
    let search_height = if area.height < 12 { 1 } else { 3 };
    let mut help = list_footer(area.width, m.search_focus);
    if m.details.shown && viewport_width < 110 {
        help.push(Line::from(details::narrow_hint(area.width, m.search_focus)));
    }
    let help_height = help.len() as u16;
    let status_height = list_status(
        area.width,
        m.visible.len(),
        m.all.len(),
        if m.visible.is_empty() {
            0
        } else {
            m.selected + 1
        },
        m.visible.len(),
        m.visible.len(),
    )
    .len() as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(notice_height),
            Constraint::Length(search_height),
            Constraint::Min(1),
            Constraint::Length(status_height),
            Constraint::Length(help_height),
        ])
        .split(area);
    // Keep the selected state readable even in a compact viewport or after SSH.
    let selected_state = m
        .selected_host()
        .map(|h| {
            m.statuses
                .get(&h.name)
                .map(|r| r.status)
                .unwrap_or_default()
                .label()
        })
        .unwrap_or("No selection");
    let title = if m.connection_result.is_empty() {
        "SSHO • F4: details"
    } else {
        &m.connection_result
    };
    f.render_widget(
        Paragraph::new(format!("{selected_state} • {title}")).style(accent()),
        chunks[0],
    );
    let mut notices = vec![];
    if let Some(error) = search_error {
        notices.push(Line::styled(
            format!("Search: {error}"),
            Style::default().fg(Color::LightRed),
        ));
    }
    if let Some(update) = &m.update {
        if update.available {
            notices.push(Line::styled(
                format!("Update available: {} → {}", update.current, update.latest),
                Style::default().fg(Color::Green),
            ));
        }
    }
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
    if m.favorites_only {
        notices.push(Line::styled(
            "[favorites only — F2: Favoris seuls to show all]",
            Style::default().fg(Color::Yellow),
        ));
    }
    if notice_height < notice_count {
        let mut badges = vec![];
        if m.update.as_ref().is_some_and(|u| u.available) {
            badges.push("Update");
        }
        if m.show_hidden {
            badges.push("Hidden");
        }
        if m.favorites_only {
            badges.push("Favorites (F2)");
        }
        let suffix = format!(" • {}", badges.join(" • "));
        let mut spans = vec![];
        if let Some(error) = search_error {
            spans.push(Span::styled(
                format!("Search: {error}"),
                Style::default().fg(Color::LightRed),
            ));
        } else if !m.error.is_empty() {
            spans.push(Span::styled(
                ellipsize(
                    &format!("Error: {}", m.error),
                    (area.width as usize).saturating_sub(suffix.width()),
                ),
                Style::default().fg(Color::Red),
            ));
            spans.push(Span::styled(suffix, Style::default().fg(Color::Yellow)));
        } else {
            spans.push(Span::styled(
                badges.join(" • "),
                Style::default().fg(Color::Yellow),
            ));
        }
        notices = vec![Line::from(spans)];
    }
    f.render_widget(Paragraph::new(notices), chunks[1]);
    let prompt = if chunks[2].width < 40 {
        if m.search_focus {
            "/ > "
        } else {
            "/: "
        }
    } else if m.search_focus {
        "Search: > "
    } else {
        "Search (/ or Tab to focus): "
    };
    let search_area = if chunks[2].height >= 3 {
        block("", m.search_focus).inner(chunks[2])
    } else {
        chunks[2]
    };
    let width = search_area.width.saturating_sub(prompt.len() as u16) as usize;
    let search = Paragraph::new(Line::from(vec![
        Span::raw(prompt),
        input_span(&m.query, m.search_focus, width),
    ]));
    if chunks[2].height >= 3 {
        f.render_widget(search.block(block("", m.search_focus)), chunks[2]);
    } else {
        f.render_widget(search, chunks[2]);
    }
    if m.search_focus && matches!(m.view, View::List) {
        input_cursor(f, &m.query, search_area, prompt.len() as u16, 0);
    }
    let (table_area, detail_area) = details::split(&mut m.details, viewport_width, chunks[3]);
    if let Some(area) = detail_area {
        details::draw(f, m, area);
    }
    let border = if area.height < 12 {
        Block::default()
    } else {
        block("", !m.search_focus)
    };
    let inner = border.inner(table_area);
    let widths = list_column_widths(inner.width);
    let header = Row::new(
        vec![
            if m.recent { "Name" } else { "Name ↓" },
            "Hostname",
            "Tags",
            if m.recent {
                "Last Attempt ↓"
            } else {
                "Last Attempt"
            },
        ]
        .into_iter()
        .take(widths.len()),
    )
    .style(Style::default().fg(if m.search_focus { GRAY } else { CYAN }));
    let row_capacity = inner.height.saturating_sub(1) as usize;
    m.page = row_capacity.max(1);
    let rows = m.visible.iter().enumerate().map(|(position, &index)| {
        let h = &m.all[index];
        let (status, color) = match m.statuses.get(&h.name).map(|r| &r.status) {
            Some(PingStatus::Online) => ("+", Color::Green),
            Some(PingStatus::Offline) => ("!", Color::Red),
            Some(PingStatus::Connecting) => ("~", Color::Yellow),
            _ => ("?", GRAY),
        };
        let mut name_spans = vec![Span::styled(
            format!(
                "{status}{} ",
                if m.metadata.get(&h.name).is_some_and(|entry| entry.favorite) {
                    "*"
                } else {
                    " "
                }
            ),
            Style::default().fg(if position == m.selected {
                Color::Black
            } else {
                color
            }),
        )];
        name_spans.extend(
            highlight(
                m,
                &ellipsize(&h.name, widths[0].saturating_sub(3) as usize),
                search::Field::Any,
                position == m.selected,
            )
            .spans,
        );
        let name = Cell::from(Line::from(name_spans));
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
        let cells = vec![
            name,
            Cell::from(highlight(
                m,
                &ellipsize(
                    m.effective
                        .get(&h.name)
                        .and_then(|result| result.as_ref().ok())
                        .map(|value| value.hostname.as_str())
                        .unwrap_or(&h.hostname),
                    widths.get(1).copied().unwrap_or(0) as usize,
                ),
                search::Field::Any,
                position == m.selected,
            )),
            Cell::from(highlight(
                m,
                &ellipsize(&tags, widths.get(2).copied().unwrap_or(0) as usize),
                search::Field::Tag,
                position == m.selected,
            )),
            Cell::from(ellipsize(
                &time,
                widths.get(3).copied().unwrap_or(0) as usize,
            )),
        ];
        Row::new(cells.into_iter().take(widths.len()))
    });
    let mut state = TableState::default()
        .with_selected(if m.visible.is_empty() {
            None
        } else {
            Some(m.selected)
        })
        .with_offset(m.offset);
    f.render_stateful_widget(
        Table::new(rows, widths.iter().copied().map(Constraint::Length))
            .header(header)
            .column_spacing(1)
            .row_highlight_style(Style::default().bg(CYAN).fg(Color::Black))
            .block(border),
        table_area,
        &mut state,
    );
    m.offset = state.offset();
    if m.visible.is_empty() {
        let empty = Rect::new(
            inner.x,
            inner.y.saturating_add(1),
            inner.width,
            inner.height.saturating_sub(1),
        );
        f.render_widget(
            Paragraph::new(if m.all.is_empty() {
                "No SSH hosts found. Press F2 to add a host."
            } else {
                if m.favorites_only {
                    "No favorite matches. F2: Favoris seuls to show all."
                } else {
                    "No matching hosts."
                }
            })
            .style(Style::default().fg(GRAY)),
            empty,
        );
    }
    let (position, first, last) = if m.visible.is_empty() {
        (0, 0, 0)
    } else if row_capacity == 0 {
        (m.selected + 1, 0, 0)
    } else {
        (
            m.selected + 1,
            m.offset + 1,
            (m.offset + row_capacity).min(m.visible.len()),
        )
    };
    let status = list_status(
        chunks[4].width,
        m.visible.len(),
        m.all.len(),
        position,
        first,
        last,
    );
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(GRAY)),
        chunks[4],
    );
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(GRAY)),
        chunks[5],
    );
}

fn highlight(m: &Model, text: &str, field: search::Field, selected: bool) -> Line<'static> {
    let ranges = m
        .search
        .as_ref()
        .map(|q| q.ranges(text, field))
        .unwrap_or_default();
    let style = Style::default()
        .fg(if selected {
            Color::Black
        } else {
            Color::Yellow
        })
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    let mut spans = Vec::new();
    let mut end = 0;
    for range in ranges {
        spans.push(Span::raw(text[end..range.start].to_owned()));
        spans.push(Span::styled(text[range.clone()].to_owned(), style));
        end = range.end;
    }
    spans.push(Span::raw(text[end..].to_owned()));
    Line::from(spans)
}

/// Shared detail text has a styled label followed by value spans; only searchable
/// values participate, and continuation lines keep the same field (personal notes).
pub(super) fn highlight_details(m: &Model, lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let mut field = None;
    lines
        .into_iter()
        .map(|mut line| {
            let has_label = line.spans.len() >= 2;
            if has_label {
                field = match line.spans[0].content.as_ref() {
                    "Host: " | "Hostname/IP: " | "Personal note: " => Some(search::Field::Any),
                    "User: " => Some(search::Field::User),
                    "Tags: " => Some(search::Field::Tag),
                    "Declared source: " => Some(search::Field::File),
                    _ => None,
                };
            }
            if let Some(field) = field {
                let mut spans = Vec::new();
                for (index, span) in line.spans.into_iter().enumerate() {
                    if has_label && index == 0 {
                        spans.push(span);
                    } else {
                        spans.extend(highlight(m, &span.content, field, false).spans);
                    }
                }
                line.spans = spans;
            }
            line
        })
        .collect()
}

fn list_status(
    width: u16,
    count: usize,
    total: usize,
    position: usize,
    first: usize,
    last: usize,
) -> Vec<Line<'static>> {
    let counts = format!("{count}/{total}");
    let range = format!("{first}-{last}");
    let candidates = [
        format!("Hosts: {counts} • Selected: {position}/{count} • Rows: {range}"),
        format!("{counts} | {position}/{count} | {range}"),
        format!("{counts}|{position}/{count}|{range}"),
        format!("{counts} @{position} {range}"),
    ];
    if let Some(text) = candidates
        .into_iter()
        .find(|text| text.width() <= width as usize)
    {
        vec![Line::from(text)]
    } else {
        vec![
            Line::from(counts),
            Line::from(format!("@{position}")),
            Line::from(range),
        ]
    }
}

fn list_column_widths(width: u16) -> Vec<u16> {
    let percentages: &[u16] = match width {
        0..=36 => &[100],
        37..=64 => &[45, 55],
        65..=94 => &[40, 38, 22],
        _ => &[26, 30, 23, 21],
    };
    let available = width.saturating_sub(percentages.len() as u16 - 1);
    let mut widths: Vec<_> = percentages
        .iter()
        .map(|p| (u32::from(available) * u32::from(*p) / 100) as u16)
        .collect();
    let remainder = available - widths.iter().sum::<u16>();
    *widths.last_mut().unwrap() += remainder;
    widths
}

fn ellipsize(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let span = Span::raw(text);
    let mut used = 0;
    let mut result = String::new();
    for grapheme in span.styled_graphemes(Style::default()) {
        let columns = UnicodeWidthStr::width(grapheme.symbol);
        if used + columns > width - 1 {
            break;
        }
        result.push_str(grapheme.symbol);
        used += columns;
    }
    result.push('…');
    result
}

fn list_footer(width: u16, search_focus: bool) -> Vec<Line<'static>> {
    let lines: Vec<&str> = if width >= 105 {
        if search_focus {
            vec!["F2: actions • F5: reload • ↑/↓: select • Enter: connect • Tab: table • Esc: clear/unfocus"]
        } else {
            vec![
                "F2: actions • F5: reload • ↑/↓: navigate • Enter: connect • Tab: search • i: info • h: help • q: quit",
                "*: favorite • N: note • a: add • e: edit • d: del • m: move • f: tunnel • p: test • s: sort • H: hidden",
            ]
        }
    } else if width >= 65 {
        if search_focus {
            vec!["F2: actions • ↑↓: select • Enter: connect • Tab: table • Esc: clear"]
        } else {
            vec![
                "F2: actions • ↑↓: select • Enter: connect • Tab: search • q: quit",
                "a/e/d/m: add/edit/delete/move • f: tunnel • p: test • h: help",
            ]
        }
    } else {
        vec!["F2: actions • ↑↓ • Enter"]
    };
    lines.into_iter().map(Line::from).collect()
}
fn actions(f: &mut Frame, m: &Model, area: Rect) {
    let View::Actions { selected } = m.view else {
        return;
    };
    let area = panel(area, 62, 17);
    f.render_widget(Clear, area);
    let border = block("Actions", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    let host = m.selected_host();
    f.render_widget(
        Paragraph::new(
            host.as_ref()
                .map(|host| format!("Host: {}", host.name))
                .unwrap_or_else(|| "No selected host".into()),
        ),
        parts[0],
    );
    let rows = PaletteAction::ALL.into_iter().map(|action| {
        let enabled = action.enabled(host.is_some());
        Row::new(vec![
            Cell::from(action.label()),
            Cell::from(if enabled { "" } else { "Unavailable: no host" }),
        ])
        .style(if enabled {
            Style::default()
        } else {
            Style::default().fg(GRAY)
        })
    });
    let mut state = TableState::default().with_selected(Some(selected));
    f.render_stateful_widget(
        Table::new(rows, [Constraint::Length(18), Constraint::Min(0)])
            .highlight_symbol("▶ ")
            .row_highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
        parts[1],
        &mut state,
    );
    f.render_widget(
        Paragraph::new("↑/↓: select • Enter: run • Esc: cancel").style(Style::default().fg(GRAY)),
        parts[2],
    );
}
fn input_span(input: &Input, focused: bool, width: usize) -> Span<'static> {
    Span::styled(
        input.viewport(focused, width).0,
        if input.value.is_empty() {
            Style::default().fg(GRAY)
        } else {
            Style::default()
        },
    )
}

fn input_cursor(f: &mut Frame, input: &Input, area: Rect, prefix: u16, row: usize) {
    if row >= area.height as usize || prefix >= area.width {
        return;
    }
    if let Some(column) = input.viewport(true, (area.width - prefix) as usize).1 {
        f.set_cursor_position((area.x + prefix + column, area.y + row as u16));
    }
}

fn field_lines(
    lines: &mut Vec<Line<'static>>,
    label: String,
    input: &Input,
    focused: bool,
    width: usize,
    compact: bool,
    error: Option<&str>,
) -> usize {
    let y = lines.len();
    lines.push(Line::styled(
        label,
        if focused { accent() } else { Style::default() },
    ));
    lines.push(Line::from(vec![
        Span::raw(if width < 3 {
            ""
        } else if focused {
            "> "
        } else {
            "  "
        }),
        input_span(
            input,
            focused,
            width.saturating_sub(if width < 3 { 0 } else { 2 }),
        ),
    ]));
    if let Some(error) = error {
        // Explicit wrapping keeps the native cursor's row aligned with later inputs.
        let mut line = String::new();
        let mut columns = 0;
        let span = Span::raw(error);
        for grapheme in span.styled_graphemes(Style::default()) {
            let size = grapheme.symbol.width();
            if columns + size > width.max(1) && !line.is_empty() {
                lines.push(Line::styled(
                    std::mem::take(&mut line),
                    Style::default().fg(Color::Red),
                ));
                columns = 0;
            }
            line.push_str(grapheme.symbol);
            columns += size;
        }
        lines.push(Line::styled(line, Style::default().fg(Color::Red)));
    } else if !compact {
        lines.push(Line::from(""));
    }
    y
}
fn scroll_focus(scroll: &mut usize, focus: usize, height: u16, total: usize, error: bool) {
    let h = height as usize;
    // Keep label + input + the beginning of its error together where space allows.
    // With only one row, the actual input and native caret take precedence.
    let end = focus + 2 + usize::from(error && h >= 3);
    if focus < *scroll {
        *scroll = focus;
    }
    if end > scroll.saturating_add(h) {
        *scroll = end.saturating_sub(h);
    }
    *scroll = (*scroll).min(total.saturating_sub(h));
}
fn field_error(errors: &[forms::FieldError], field: forms::FormField) -> Option<&str> {
    errors
        .iter()
        .find(|e| e.field == field)
        .map(|e| e.message.as_str())
}
fn host_form(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Host(form) = &mut m.view else {
        return;
    };
    let area = inset(area);
    let compact = area.height < 30;
    let footer_height = if area.height >= 12 {
        4
    } else if area.height >= 6 {
        2
    } else {
        0
    };
    let parts = Layout::vertical([
        Constraint::Length(if area.height >= 5 { 2 } else { 0 }),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(if form.original.is_some() {
            "Edit SSH Host"
        } else if form.is_duplicate() {
            "Duplicate SSH Host - enter a NEW name"
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
                compact,
                field_error(&form.errors, forms::FormField::Name(i)),
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
        let y = field_lines(
            &mut lines,
            label.into(),
            &form.inputs[i],
            selected,
            width,
            compact,
            field_error(&form.errors, forms::FormField::Input(i)),
        );
        if selected {
            focus_y = y;
        }
        if i == 4 {
            for identity in form.extra_identities() {
                lines.push(Line::styled(
                    format!("  also: {identity} (read-only, kept on save)"),
                    Style::default().fg(GRAY),
                ));
            }
        }
        if i == 8 && selected {
            lines.push(Line::styled(
                "  tip: use \"hidden\" to hide this host from the list",
                Style::default().fg(GRAY),
            ));
        }
    }
    let focused_field = if form.name_focus {
        forms::FormField::Name(form.focus)
    } else {
        forms::FormField::Input(form.focus)
    };
    scroll_focus(
        &mut form.scroll,
        focus_y,
        parts[1].height,
        lines.len(),
        field_error(&form.errors, focused_field).is_some(),
    );
    f.render_widget(
        Paragraph::new(lines).scroll((form.scroll as u16, 0)),
        parts[1],
    );
    let input = if form.name_focus {
        form.names.get(form.focus)
    } else {
        form.inputs.get(form.focus)
    };
    if let (Some(input), Some(row)) = (input, (focus_y + 1).checked_sub(form.scroll)) {
        input_cursor(
            f,
            input,
            parts[1],
            if parts[1].width < 3 { 0 } else { 2 },
            row,
        );
    }
    let mut footer = vec![];
    if !form.error.is_empty() {
        footer.push(Line::styled(
            format!("Error: {}", form.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::styled(
        if form.original.is_some() {
            "Tab/↑↓/Enter: navigate • Ctrl+J/K: switch tabs • Alt+A/D: add/delete alias"
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
fn forward_form(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Forward(form) = &mut m.view else {
        return;
    };
    let area = panel(area, 88, 30);
    let compact = area.height < 30;
    let border = if area.height < 5 || area.width < 4 {
        Block::default()
    } else {
        block("Port Forwarding Setup", true)
    };
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([
        Constraint::Length(if inner.height >= 5 { 2 } else { 0 }),
        Constraint::Min(1),
        Constraint::Length(if inner.height > 12 {
            4
        } else if inner.height >= 6 {
            2
        } else {
            0
        }),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(format!(
            "Host: {} • {} • {} (Ctrl+T)",
            form.host,
            form.mode.label(),
            if form.dirty() {
                "Unsaved changes"
            } else if form.original_name.is_some() {
                "Profile saved"
            } else {
                "Draft"
            }
        ))
        .style(accent()),
        parts[0],
    );
    let mut lines = vec![];
    let mut focus_y = field_lines(
        &mut lines,
        "Profile name (required to save):".into(),
        &form.inputs[5],
        form.focus == 5,
        inner.width as usize,
        compact,
        field_error(&form.errors, forms::FormField::Input(5)),
    );
    let kind = ["Local (-L)", "Remote (-R)", "Dynamic (-D)"][form.kind];
    let input = Input::new(kind);
    let kind_y = field_lines(
        &mut lines,
        "Forward Type:".into(),
        &input,
        form.focus == 0,
        inner.width as usize,
        compact,
        None,
    );
    if form.focus == 0 {
        focus_y = kind_y;
    }
    lines.push(Line::styled(
        "Use ←/→ to change type",
        Style::default().fg(GRAY),
    ));
    lines.push(Line::from(""));
    lines.push(Line::styled(
        match form.kind {
            0 => "Local forwarding: ssh -L [bind_address:]local_port:remote_host:remote_port",
            1 => "Remote forwarding: ssh -R [bind_address:]remote_port:local_host:local_port",
            _ => "Dynamic forwarding (SOCKS proxy): ssh -D [bind_address:]port",
        },
        Style::default().fg(GRAY),
    ));
    lines.push(Line::from(""));
    let labels = match form.kind {
        0 => [
            "",
            "Local Port:",
            "Remote Host:",
            "Remote Port:",
            "Bind Address (optional):",
        ],
        1 => [
            "",
            "Remote Port:",
            "Local Host:",
            "Local Port:",
            "Bind Address (optional):",
        ],
        _ => ["", "SOCKS Port:", "", "", "Bind Address (optional):"],
    };
    for i in form.fields().into_iter().filter(|&i| i != 0 && i != 5) {
        let selected = form.focus == i;
        let y = field_lines(
            &mut lines,
            labels[i].into(),
            &form.inputs[i],
            selected,
            inner.width as usize,
            compact,
            field_error(&form.errors, forms::FormField::Input(i)),
        );
        if selected {
            focus_y = y;
        }
    }
    scroll_focus(
        &mut form.scroll,
        focus_y,
        parts[1].height,
        lines.len(),
        field_error(&form.errors, forms::FormField::Input(form.focus)).is_some(),
    );
    f.render_widget(
        Paragraph::new(lines).scroll((form.scroll as u16, 0)),
        parts[1],
    );
    if form.focus != 0 {
        if let Some(row) = (focus_y + 1).checked_sub(form.scroll) {
            input_cursor(
                f,
                &form.inputs[form.focus],
                parts[1],
                if parts[1].width < 3 { 0 } else { 2 },
                row,
            );
        }
    }
    let mut footer = vec![];
    if !form.error.is_empty() {
        footer.push(Line::styled(
            format!("Error: {}", form.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::styled(
        "Tab/↓: next field • Shift+Tab/↑: previous field",
        Style::default().fg(GRAY),
    ));
    footer.push(Line::styled(
        "Ctrl+S: save profile • Ctrl+L: launch • Esc: cancel",
        Style::default().fg(GRAY),
    ));
    f.render_widget(Paragraph::new(footer), parts[2]);
}
fn files(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Files {
        files,
        selected,
        host,
        duplicate,
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
    let title = duplicate
        .as_deref()
        .or(host.as_ref())
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
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(if m.error.is_empty() { 0 } else { 3 }),
    ])
    .split(inner);
    let lines = details::content(
        h,
        m.statuses.get(&h.name),
        m.effective.get(&h.name),
        m.history.get(&h.name),
        m.note(&h.name),
    );
    let lines = highlight_details(m, lines);
    m.page = usize::from(parts[0].height).max(1);
    details::render_rows(f, parts[0], lines, &mut m.scroll);
    f.render_widget(
        Paragraph::new(
            "Actions: ↑/↓/PgUp/PgDn scroll • F5: reload\n  e/Enter - Switch to edit mode\n  q/Esc   - Return to host list",
        )
        .style(Style::default().fg(GRAY)),
        parts[1],
    );
    if !m.error.is_empty() {
        f.render_widget(
            Paragraph::new(format!("Error: {}", m.error))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: false }),
            parts[2],
        );
    }
}
fn help(f: &mut Frame, m: &mut Model, area: Rect) {
    let area = panel(area, 96, 28);
    let border = block("SSHO - Commands", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
    let left = [
        "Navigation & Connection",
        "",
        "Enter  connect (also from search)",
        "SSH exit returns here with its result",
        "↑↓     select (Ctrl+P/N in search)",
        "i      show host information",
        "/      search words/phrases (AND/ET)",
        "Tab/Shift+Tab switch search / table",
        "F2     actions (also from search)",
        "F4     toggle details (width >=110)",
        "F5     reload config and Includes",
        "Alt+↑↓ scroll details (Ctrl+PgUp/PgDn)",
        "",
        "Host Management",
        "",
        "a      add new host",
        "e      edit selected host",
        "D      duplicate to a new name/config",
        "m      move host to another config",
        "d      delete selected host",
        "tag:work AND user:deploy",
        "tag:work ET is:online",
        "file:\"Team Hosts\" is:favorite",
        "Quotes: phrases; fields/words use AND",
    ];
    let right = [
        "Advanced Features",
        "*      toggle favorite (pinned first)",
        "N      edit personal note",
        "F2     Favoris seuls: favorites filter",
        "p      ping all hosts",
        "H      toggle hidden hosts visibility",
        "f      last forwarding draft (Ctrl+L launch, Ctrl+S save profile)",
        "F7     owned tunnels: state, cause and explicit stop",
        "Ctrl+T forwarding form: shell session / tunnel only",
        "F6     named tunnel profiles (add/edit/rename/delete/launch)",
        "s      cycle sort modes",
        "n      sort by name",
        "r      sort by recent connection",
        "       Tester: check all hosts",
        "Forms: Ctrl+S save, Ctrl+L launch tunnel",
        "Ctrl+J/K tabs; Alt+A/D aliases",
        "Ctrl+A/E start/end of input",
        "Discard: Enter discard, Esc resume",
        "System",
        "",
        "h      show this help",
        "q      quit application",
        "Esc    clear search / leave search / exit view",
    ];
    let text = if inner.width >= 77 {
        (0..left.len().max(right.len()))
            .map(|i| {
                format!(
                    "{:<39}  {}",
                    left.get(i).unwrap_or(&""),
                    right.get(i).unwrap_or(&"")
                )
            })
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

fn note_editor(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Note(editor) = &m.view else {
        return;
    };
    let area = inset(area);
    let border = block("Personal note", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(if inner.height >= 5 { 3 } else { 1 }),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(format!("Host: {}", editor.host)).style(accent()),
        parts[0],
    );
    let before: String = editor
        .input
        .value
        .chars()
        .take(editor.input.cursor)
        .collect();
    let cursor_row = before.chars().filter(|&c| c == '\n').count();
    let cursor_col = before.rsplit('\n').next().unwrap_or("").chars().count();
    let offset = cursor_row.saturating_sub(parts[1].height.saturating_sub(1) as usize);
    let mut lines = vec![];
    for (row, text) in editor
        .input
        .value
        .split('\n')
        .enumerate()
        .skip(offset)
        .take(parts[1].height as usize)
    {
        let mut input = Input::new(text);
        let focused = row == cursor_row;
        if focused {
            input.cursor = cursor_col;
        }
        lines.push(Line::from(input_span(
            &input,
            focused,
            parts[1].width as usize,
        )));
        if focused {
            input_cursor(f, &input, parts[1], 0, row - offset);
        }
    }
    f.render_widget(Paragraph::new(lines), parts[1]);
    let mut footer = vec![];
    if !editor.error.is_empty() {
        footer.push(Line::styled(
            format!("Error: {}", editor.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::from(
        "Ctrl+S: save • Esc/Ctrl+C: cancel • Enter: newline",
    ));
    f.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), parts[2]);
}

fn profile_list(f: &mut Frame, m: &Model, area: Rect) {
    let View::Profiles(list) = &m.view else {
        return;
    };
    let area = inset(area);
    let parts = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(format!("Host: {}", list.host)).block(block("Tunnel profiles", true)),
        parts[0],
    );
    if let Some(name) = &list.delete {
        f.render_widget(
            Paragraph::new(format!(
                "Delete profile '{name}'?\nEnter: confirm • Esc: cancel"
            ))
            .block(block("Delete tunnel profile", true)),
            parts[1],
        );
    } else {
        let profiles = m.tunnel_profiles.get(&list.host);
        if profiles.is_none_or(BTreeMap::is_empty) {
            f.render_widget(
                Paragraph::new("No profiles. Press a to create one; f resumes last forwarding."),
                parts[1],
            );
        } else {
            let rows = profiles
                .into_iter()
                .flat_map(|p| p.iter())
                .map(|(name, forward)| {
                    Row::new(vec![
                        name.clone(),
                        forward.kind.clone(),
                        m.tunnel_modes
                            .get(&list.host)
                            .and_then(|m| m.get(name))
                            .copied()
                            .unwrap_or_default()
                            .label()
                            .into(),
                        forward.ssh_args().unwrap_or_default().join(" "),
                    ])
                });
            let mut state = TableState::default().with_selected(Some(list.selected));
            f.render_stateful_widget(
                Table::new(
                    rows,
                    [
                        Constraint::Percentage(25),
                        Constraint::Length(9),
                        Constraint::Length(14),
                        Constraint::Min(0),
                    ],
                )
                .header(Row::new(["Name", "Type", "Mode", "Forwarding"]).style(accent()))
                .highlight_symbol("▶ ")
                .row_highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                parts[1],
                &mut state,
            );
        }
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(&list.error, Style::default().fg(Color::Red)),
            Line::from("a: create • Enter/e/r: edit or rename • d: delete • Ctrl+L: launch"),
            Line::from("f: last forwarding draft • Esc: back"),
        ]),
        parts[2],
    );
}
