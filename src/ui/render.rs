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
    // Persistent diagnostics have their own rows; notices and exact-file scope coexist.
    let area = if !m.config_warnings.is_empty() {
        let count = m
            .config_warnings
            .len()
            .min(usize::from((area.height / 4).max(1)));
        let parts =
            Layout::vertical([Constraint::Length(count as u16), Constraint::Min(0)]).split(area);
        let lines = m
            .config_warnings
            .iter()
            .take(count)
            .enumerate()
            .map(|(i, warning)| {
                let more = if i + 1 == count && count < m.config_warnings.len() {
                    format!(" (+{}; h for all)", m.config_warnings.len() - count)
                } else {
                    String::new()
                };
                Line::from(format!("FileTags warning{more}: {warning}"))
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(lines).style(Style::default().fg(Color::Yellow)),
            parts[0],
        );
        parts[1]
    } else {
        area
    };
    if m.quit_tunnels {
        // Name what leaving stops or loses, then the two possible decisions.
        let tunnels = m
            .tunnels
            .iter()
            .filter(|record| record.state == runtime::tunnels::State::Active)
            .count();
        let batch = m.batch.manager.has_active();
        let mut lines = vec![];
        if tunnels > 0 {
            lines.push(Line::from(format!(
                "• {tunnels} tunnel(s) actif(s) : ils seront arrêtés."
            )));
        }
        if batch {
            lines.push(Line::from(
                "• Lot en cours : il sera annulé ; un travail distant déjà détaché peut continuer.",
            ));
        }
        if m.has_unsaved_context() {
            lines.push(Line::from(
                "• Modifications non enregistrées : elles seront perdues.",
            ));
        }
        if lines.is_empty() {
            lines.push(Line::from("Aucun travail en cours."));
        }
        m.confirm.draw(
            f,
            area,
            "Quitter ssho ?",
            lines,
            "Revenir",
            if tunnels > 0 || batch {
                "Arrêter et quitter"
            } else {
                "Abandonner et quitter"
            },
        );
        return;
    }
    if m.discard {
        m.confirm.draw(
            f,
            area,
            "Modifications non enregistrées",
            vec![Line::from("Abandonner les modifications de ce brouillon ?")],
            "Continuer l'édition",
            "Abandonner",
        );
        return;
    }
    draw_view(f, m, area);
}
fn draw_view(f: &mut Frame, m: &mut Model, area: Rect) {
    match &m.view {
        View::List => {
            list(f, m, area);
        }
        View::Columns(_) => {
            let width = list(f, m, area);
            columns_selector(f, m, area, width);
        }
        View::Keys(_) => keys::draw(f, m, area),
        View::Batch(_) | View::BatchResults { .. } => batch::draw(f, m, area),
        View::Changes(_) | View::Backups { .. } => changes::draw(f, m, area),
        View::Actions(_) => {
            draw_parent(f, m, area);
            palette::draw(f, m, area);
        }
        View::Quick { .. } => {
            list(f, m, area);
            quick_actions(f, m, area);
        }
        View::Note(_) => note_editor(f, m, area),
        View::Host(_) => host_form(f, m, area),
        View::Forward(_) => forward_form(f, m, area),
        View::Active { .. } => active::draw(f, m, area),
        View::Profiles(_) => profile_list(f, m, area),
        View::Info(_) => info(f, m, area),
        View::GroupInfo { .. } => group_info(f, m, area),
        View::Help => help(f, m, area),
        View::Files { .. } => files(f, m, area),
        View::SourceFiles { .. } => source_files(f, m, area),
        View::Aliases { .. } => alias_chooser(f, m, area),
        View::GroupMode { .. } => group_mode(f, m, area),
        View::Availability { .. } => availability(f, m, area),
    }
}
fn list(f: &mut Frame, m: &mut Model, area: Rect) -> u16 {
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
        return 0;
    }
    let viewport_width = area.width;
    let area = if area.height >= 12 { inset(area) } else { area };
    let area = if let Some(path) = &m.source_filter {
        let parts = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
        f.render_widget(
            Paragraph::new(format!(
                "File: {}/F2 clear • {}",
                m.keymap().hint(PaletteAction::ClearSourceFilter),
                path.display()
            ))
            .style(accent()),
            parts[0],
        );
        parts[1]
    } else {
        area
    };
    let search_error = m.search.as_ref().err();
    let keymap = m.keymap();
    let notice_count = u16::from(search_error.is_some())
        + u16::from(!keymap.conflicts.is_empty())
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
    let mut help = list_footer(area.width, m.search_focus, &keymap);
    if m.details.shown && viewport_width < 110 {
        help.push(Line::from(details::narrow_hint(area.width, m.search_focus)));
    }
    let help_height = help.len() as u16;
    let status_height = browser_status(
        m,
        area.width,
        if m.rows.is_empty() { 0 } else { m.selected + 1 },
        m.rows.len(),
        m.rows.len(),
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
        .unwrap_or(if m.selected_group().is_some() {
            "Group • i: info • Enter: toggle"
        } else {
            "No selection"
        });
    let title = if !m.hint.is_empty() {
        &m.hint
    } else if m.connection_result.is_empty() {
        "SSHO • F4: details"
    } else {
        &m.connection_result
    };
    let method = m.preferences.availability_method.label();
    let heading = if m.connection_result.is_empty() || !m.hint.is_empty() {
        format!("{method} • {selected_state} • {title}")
    } else {
        format!("{method} • {title} • {selected_state}")
    };
    f.render_widget(Paragraph::new(heading).style(accent()), chunks[0]);
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
    if !keymap.conflicts.is_empty() {
        // The reasons are long: the help lists each refused preference in full.
        notices.push(Line::styled(
            format!(
                "Raccourcis : {} préférence(s) refusée(s) — détail dans l'aide ({})",
                keymap.conflicts.len(),
                keymap.hint(PaletteAction::Help)
            ),
            Style::default().fg(Color::Yellow),
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
        if !keymap.conflicts.is_empty() {
            badges.push("Raccourcis refusés");
        }
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
    let plan = columns::layout(&m.preferences.host_columns, inner.width);
    let widths: Vec<_> = plan.iter().map(|(_, width)| *width).collect();
    let header = Row::new(plan.iter().map(|(column, _)| {
        let sorted = match column {
            HostColumn::Name => !m.recent,
            HostColumn::LastAttempt => m.recent,
            _ => false,
        };
        format!("{}{}", column.label(), if sorted { " ↓" } else { "" })
    }))
    .style(Style::default().fg(if m.search_focus { GRAY } else { CYAN }));
    let row_capacity = inner.height.saturating_sub(1) as usize;
    m.page = row_capacity.max(1);
    // A stable source ordinal disambiguates even identical truncated path tails.
    let sources: BTreeMap<_, _> = m
        .all
        .iter()
        .map(|host| &host.source_file)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, path)| (path, index + 1))
        .collect();
    let rows = m.rows.iter().enumerate().map(|(position, row)| {
        let index = match row {
            groups::Row::Host(index) => *index,
            groups::Row::Group { key, depth, count } => {
                let marker = if m.group_projection.collapsed(key) {
                    "▶"
                } else {
                    "▼"
                };
                let ordinal = match key {
                    groups::Key::File(path) => format!("{} ", sources[path]),
                    _ => String::new(),
                };
                let prefix = format!("{}{marker} {ordinal}", "  ".repeat(*depth));
                let suffix = format!(" ({count})");
                let available =
                    usize::from(widths[0]).saturating_sub(prefix.width() + suffix.width());
                let label = group_label(&key.label(), available);
                return Row::new(plan.iter().map(|(column, _)| {
                    if *column == HostColumn::Name {
                        Cell::from(format!("{prefix}{label}{suffix}"))
                    } else {
                        Cell::from("")
                    }
                }))
                .style(
                    Style::default()
                        .fg(if position == m.selected {
                            Color::Black
                        } else {
                            CYAN
                        })
                        .add_modifier(Modifier::BOLD),
                );
            }
        };
        let h = &m.all[index];
        let (status, color) = match m.statuses.get(&h.name).map(|r| &r.status) {
            Some(PingStatus::Online) => ("+", Color::Green),
            Some(PingStatus::Offline) => ("!", Color::Red),
            Some(PingStatus::Connecting) => ("~", Color::Yellow),
            _ => ("?", GRAY),
        };
        let mut name_spans = vec![Span::styled(
            format!(
                "{status}{}{}",
                if m.batch.marked(h) { "x" } else { " " },
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
                &alias_label(
                    h,
                    m.preferences.group_aliases,
                    widths[0].saturating_sub(3) as usize,
                ),
                search::Field::Any,
                position == m.selected,
            )
            .spans,
        );
        let name = Cell::from(Line::from(name_spans));
        let tags = h.display_tags();
        let time = m
            .history
            .get(&h.name)
            .map(|c| time_ago(c.last_connect))
            .unwrap_or_default();
        let effective = m
            .effective
            .get(&h.name)
            .and_then(|result| result.as_ref().ok());
        let cells = plan.iter().map(|(column, width)| {
            if *column == HostColumn::Name {
                return name.clone();
            }
            let (text, field) = match column {
                HostColumn::Hostname => (
                    effective
                        .map(|v| v.hostname.as_str())
                        .unwrap_or(&h.hostname),
                    search::Field::Any,
                ),
                HostColumn::User => (
                    effective.map(|v| v.user.as_str()).unwrap_or(&h.user),
                    search::Field::User,
                ),
                HostColumn::Port => (
                    effective.map(|v| v.port.as_str()).unwrap_or(&h.port),
                    search::Field::Any,
                ),
                HostColumn::Tags => (tags.as_str(), search::Field::Tag),
                HostColumn::LastAttempt => (time.as_str(), search::Field::Any),
                HostColumn::Name => unreachable!(),
            };
            let text = if text.is_empty() && matches!(column, HostColumn::User | HostColumn::Port) {
                "—"
            } else {
                text
            };
            Cell::from(highlight(
                m,
                &ellipsize(text, *width as usize),
                field,
                position == m.selected,
            ))
        });
        Row::new(cells)
    });
    let mut state = TableState::default()
        .with_selected(if m.rows.is_empty() {
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
    if m.rows.is_empty() {
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
    let (position, first, last) = if m.rows.is_empty() {
        (0, 0, 0)
    } else if row_capacity == 0 {
        (m.selected + 1, 0, 0)
    } else {
        (
            m.selected + 1,
            m.offset + 1,
            (m.offset + row_capacity).min(m.rows.len()),
        )
    };
    let status = browser_status(m, chunks[4].width, position, first, last);
    f.render_widget(
        Paragraph::new(status).style(Style::default().fg(GRAY)),
        chunks[4],
    );
    f.render_widget(
        Paragraph::new(help).style(Style::default().fg(GRAY)),
        chunks[5],
    );
    inner.width
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

/// Keep both identities visible when a secondary alias is active, even in a
/// single narrow name column. The arrow identifies the actual action target.
fn alias_label(host: &Host, grouped: bool, width: usize) -> String {
    let primary = aliases::primary(host);
    if !grouped || primary == host.name {
        return ellipsize(&host.name, width);
    }
    let available = width.saturating_sub(1);
    let active_width = host.name.width().min(available.div_ceil(2));
    let primary_width = primary.width().min(available.saturating_sub(active_width));
    let active_width = available.saturating_sub(primary_width);
    format!(
        "{}→{}",
        ellipsize(primary, primary_width),
        ellipsize(&host.name, active_width)
    )
}

fn browser_status(
    m: &Model,
    width: u16,
    position: usize,
    first: usize,
    last: usize,
) -> Vec<Line<'static>> {
    if m.preferences.group_mode != GroupMode::Flat {
        let hosts = if m.preferences.group_aliases {
            format!(
                "{} blocks • {}/{} aliases",
                m.visible.len(),
                m.matching.len(),
                m.all.len()
            )
        } else {
            format!("Hosts: {}/{}", m.visible.len(), m.all.len())
        };
        let groups = format!("{} groups", m.group_projection.group_count);
        let rows = format!(
            "Selected: {position}/{} • Rows: {first}-{last}",
            m.rows.len()
        );
        let full = format!("{hosts} • {groups} • {rows}");
        if full.width() <= usize::from(width) {
            return vec![Line::from(full)];
        }
        let counts = format!("{hosts} • {groups}");
        if counts.width() <= usize::from(width) {
            return vec![Line::from(counts), Line::from(rows)];
        }
        return if m.preferences.group_aliases {
            vec![
                Line::from(format!("{} blocks • {groups}", m.visible.len())),
                Line::from(format!("{}/{} aliases", m.matching.len(), m.all.len())),
            ]
        } else {
            vec![Line::from(hosts), Line::from(groups)]
        };
    }
    if !m.preferences.group_aliases {
        return list_status(width, m.visible.len(), m.all.len(), position, first, last);
    }
    let blocks = format!("{} blocks", m.visible.len());
    let aliases = format!("{}/{} aliases", m.matching.len(), m.all.len());
    let counts = format!("{blocks} • {aliases}");
    let full = format!(
        "{counts} • Selected: {position}/{} • Rows: {first}-{last}",
        m.visible.len()
    );
    if full.width() <= usize::from(width) {
        vec![Line::from(full)]
    } else if counts.width() <= usize::from(width) {
        vec![Line::from(counts)]
    } else {
        vec![Line::from(blocks), Line::from(aliases)]
    }
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

fn list_footer(width: u16, search_focus: bool, keymap: &Keymap) -> Vec<Line<'static>> {
    use PaletteAction as A;
    let k = |action| keymap.hint(action);
    let lines = if width >= 105 {
        if search_focus {
            vec![format!(
                "{}: actions • {}: reload • ↑/↓: select • {}: connect • Tab: table • Esc: clear/unfocus",
                k(A::Palette), k(A::Reload), k(A::Connect)
            )]
        } else {
            vec![
                format!(
                    "{}: actions • {}: reload • ↑/↓: navigate • {}: connect • Tab: search • {}: info • {}: help • {}: quit",
                    k(A::Palette), k(A::Reload), k(A::Connect), k(A::Info), k(A::Help), k(A::Quit)
                ),
                format!(
                    "{}: favorite • {}: add • {}: edit • {}: del • {}: move • {}: tunnel • {}: test • {}/{}: file/all",
                    k(A::Favorite), k(A::Add), k(A::Edit), k(A::Delete), k(A::Move),
                    k(A::Tunnel), k(A::Test), k(A::SourceFilter), k(A::ClearSourceFilter)
                ),
            ]
        }
    } else if width >= 65 {
        if search_focus {
            vec![format!(
                "{}: actions • ↑↓: select • {}: connect • Tab: table • Esc: clear",
                k(A::Palette),
                k(A::Connect)
            )]
        } else {
            vec![
                format!(
                    "{}: actions • ↑↓: select • {}: connect • Tab: search • {}: quit",
                    k(A::Palette), k(A::Connect), k(A::Quit)
                ),
                format!(
                    "{}/{}: file/all • {}/{}/{}/{}: manage • {}: tunnel • {}: test • {}: method • {}: help",
                    k(A::SourceFilter), k(A::ClearSourceFilter), k(A::Add), k(A::Edit),
                    k(A::Delete), k(A::Move), k(A::Tunnel), k(A::Test), k(A::Availability), k(A::Help)
                ),
            ]
        }
    } else {
        vec![format!(
            "{}: actions • ↑↓ • {}",
            k(A::Palette),
            k(A::Connect)
        )]
    };
    let mut lines = lines;
    // The two-step prefix is announced wherever a row still has room for it.
    if let (Some(prefix), Some(last)) = (keymap.prefix(), lines.last_mut()) {
        let hint = format!(" • {prefix}: quick");
        if width >= 65 && last.width() + hint.width() <= usize::from(width) {
            last.push_str(&hint);
        }
    }
    lines.into_iter().map(Line::from).collect()
}
/// Draw the screen under an overlay, exactly as it will be found on return.
fn draw_parent(f: &mut Frame, m: &mut Model, area: Rect) {
    let Some((parent, scroll)) = m.parents.pop() else {
        return;
    };
    let overlay = std::mem::replace(&mut m.view, parent);
    let overlay_scroll = std::mem::replace(&mut m.scroll, scroll);
    draw_view(f, m, area);
    let parent = std::mem::replace(&mut m.view, overlay);
    let scroll = std::mem::replace(&mut m.scroll, overlay_scroll);
    m.parents.push((parent, scroll));
}
/// Second step of the prefix sequence: every letter is listed with its action.
fn quick_actions(f: &mut Frame, m: &Model, area: Rect) {
    let View::Quick { notice } = m.view else {
        return;
    };
    let host = m.selected_host();
    let context = m.action_context();
    let columns = if area.width >= 76 { 3 } else { 2 };
    let rows = actions::QUICK.len().div_ceil(columns);
    let area = panel(area, 76, rows as u16 + 5);
    f.render_widget(Clear, area);
    let title = format!(
        "Actions rapides — {}",
        host.as_ref().map_or("aucun hôte sélectionné", |h| &h.name)
    );
    let border = block(&title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let cells = actions::QUICK.chunks(rows).map(|column| {
        column
            .iter()
            .map(|(letter, action)| {
                let enabled = action.availability(&context).is_ok();
                Line::from(vec![
                    Span::styled(
                        format!("{letter} "),
                        if enabled {
                            accent()
                        } else {
                            Style::default().fg(GRAY)
                        },
                    ),
                    Span::styled(
                        action.label(),
                        if enabled {
                            Style::default()
                        } else {
                            Style::default().fg(GRAY)
                        },
                    ),
                ])
            })
            .collect::<Vec<_>>()
    });
    let areas = Layout::horizontal(vec![Constraint::Ratio(1, columns as u32); columns])
        .spacing(1)
        .split(parts[0]);
    for (lines, area) in cells.zip(areas.iter()) {
        f.render_widget(Paragraph::new(lines), *area);
    }
    f.render_widget(
        Paragraph::new(notice.map_or(String::new(), |reason| format!("Indisponible : {reason}")))
            .style(Style::default().fg(Color::Yellow)),
        parts[1],
    );
    f.render_widget(
        Paragraph::new("Lettre : lancer • Esc : annuler").style(Style::default().fg(GRAY)),
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
    if !form.file_tags.is_empty() {
        lines.push(Line::styled(
            format!(
                "Inherited (read only): {}",
                form.file_tags
                    .iter()
                    .map(|tag| format!("%{tag}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Style::default().fg(GRAY),
        ));
    }
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
        "F9: key picker • Ctrl+S: save • Ctrl+C/Esc: cancel • * Required fields",
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
fn source_files(f: &mut Frame, m: &Model, area: Rect) {
    let View::SourceFiles { files, selected } = &m.view else {
        return;
    };
    let area = inset(area);
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new("Filtrer par fichier source").style(accent()),
        parts[0],
    );
    let count = usize::from(parts[1].height).max(1);
    let offset = selected.saturating_sub(count - 1);
    let lines = std::iter::once("Tous les fichiers".to_owned())
        .chain(files.iter().map(|path| path.display().to_string()))
        .enumerate()
        .skip(offset)
        .map(|(i, label)| {
            Line::styled(
                format!("{} {label}", if i == *selected { "▶" } else { " " }),
                if i == *selected {
                    Style::default().bg(CYAN).fg(Color::Black)
                } else {
                    Style::default()
                },
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), parts[1]);
    f.render_widget(
        Paragraph::new("↑/↓: navigate • Enter: select • Esc: cancel")
            .style(Style::default().fg(GRAY)),
        parts[2],
    );
}

fn alias_chooser(f: &mut Frame, m: &Model, area: Rect) {
    let View::Aliases { hosts, selected } = &m.view else {
        return;
    };
    let area = inset(area);
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new("Choose alias • exact connection target").style(accent()),
        parts[0],
    );
    let count = usize::from(parts[1].height).max(1);
    let offset = selected.saturating_sub(count - 1);
    let lines = hosts
        .iter()
        .map(|host| {
            let effective = m
                .effective
                .get(&host.name)
                .and_then(|value| value.as_ref().ok());
            format!(
                "{} • {}@{}",
                host.name,
                effective.map_or(host.user.as_str(), |e| &e.user),
                effective.map_or(host.hostname.as_str(), |e| &e.hostname)
            )
        })
        .enumerate()
        .skip(offset)
        .map(|(i, label)| {
            Line::styled(
                format!("{} {label}", if i == *selected { "▶" } else { " " }),
                if i == *selected {
                    Style::default().bg(CYAN).fg(Color::Black)
                } else {
                    Style::default()
                },
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), parts[1]);
    f.render_widget(
        Paragraph::new("↑/↓: navigate • Enter: select • Esc: cancel")
            .style(Style::default().fg(GRAY)),
        parts[2],
    );
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
    let errors = if m.error.is_empty() {
        vec![]
    } else {
        details::wrapped(
            m.error
                .lines()
                .map(|line| Line::styled(line.to_owned(), Style::default().fg(Color::Red)))
                .collect(),
            area.width,
        )
    };
    let error_height = errors.len().min(usize::from(area.height.saturating_sub(3))) as u16;
    let parts = Layout::vertical([
        Constraint::Length(if errors.is_empty() { 3 } else { 1 }),
        Constraint::Min(1),
        Constraint::Length(error_height),
        Constraint::Length(1),
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
    f.render_widget(Paragraph::new(errors), parts[2]);
    f.render_widget(
        Paragraph::new("↑/↓: navigate • Enter: select • Esc: cancel")
            .style(Style::default().fg(GRAY)),
        parts[3],
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
    let area = panel(area, 96, 44);
    let border = block("SSHO - Commands", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(inner);
    // Action lines come from the catalogue and the active keymap; only screen
    // conventions that are not catalogue actions remain written here.
    let keymap = m.keymap();
    let section = |categories: &[actions::Category]| {
        let mut lines = Vec::new();
        for category in categories {
            lines.push(category.label().to_owned());
            for action in PaletteAction::CATALOGUE {
                if action.category() == *category {
                    let keys = keymap.keys(action);
                    let keys = if keys.is_empty() {
                        "F2 only".to_owned()
                    } else {
                        keys.join(", ")
                    };
                    lines.push(format!("{keys:<9} {}", action.label()));
                }
            }
            lines.push(String::new());
        }
        lines
    };
    use actions::Category as C;
    let mut left = section(&[C::Connection, C::Host, C::Organization]);
    left.extend(
        [
            "Search",
            "Words/phrases and fields combine with AND/ET",
            "tag:work AND user:deploy",
            "tag:work ET is:online",
            "file:\"Team Hosts\" is:favorite",
            "Tab/Shift+Tab switch search / table",
            "Esc    clear search / leave search / back one level",
        ]
        .map(String::from),
    );
    let mut right = section(&[C::Tools, C::Application]);
    if let Some(prefix) = keymap.prefix() {
        right.pop();
        right.push(format!("{prefix:<9} Actions rapides (puis une lettre)"));
        right.push(String::new());
    }
    right.extend(
        [
            "Navigation",
            "↑↓     select (Ctrl+P/N in search)",
            "Group Enter/Space: expand/collapse; ←/→ parent/open",
            "SSH exit returns here with its result",
            "Alt+↑↓ scroll details (Ctrl+PgUp/PgDn; width >=110)",
            "",
            "Forms and reviews",
            "Ctrl+S save • Ctrl+L launch tunnel/batch review",
            "Ctrl+J/K tabs; Alt+A/D aliases; F9 key picker",
            "Ctrl+T forwarding: shell session / tunnel only",
            "Ctrl+A/E start/end of input",
            "Review: Enter confirms, Esc returns",
            "Modified draft: Tab chooses, Enter confirms, Esc resumes",
            "Esc/Ctrl+C: back one level; quitting is asked from the list",
            "Columns: Space toggle, Shift+↑↓ order, r reset",
            "Batch results: s cancels the batch",
            "Keys: F4 full details/error; ↑↓ PgUp/Dn Home/End scroll",
            "ICMP local: no bastion / SSH auth",
        ]
        .map(String::from),
    );
    let blank = String::new();
    let mut text = if inner.width >= 77 {
        (0..left.len().max(right.len()))
            .map(|i| {
                format!(
                    "{:<45} {}",
                    left.get(i).unwrap_or(&blank),
                    right.get(i).unwrap_or(&blank)
                )
            })
            .collect::<Vec<_>>()
    } else {
        left.into_iter().chain(right).collect()
    };
    if let Some(path) = &m.source_filter {
        text.insert(
            0,
            format!(
                "File: {} • {}/F2: clear",
                path.display(),
                keymap.hint(PaletteAction::ClearSourceFilter)
            ),
        );
    }
    if !keymap.conflicts.is_empty() {
        text.push("Raccourcis refusés (key_bindings de config.json) :".into());
        text.extend(
            details::wrapped(
                keymap
                    .conflicts
                    .iter()
                    .map(|conflict| Line::from(conflict.clone()))
                    .collect(),
                parts[0].width,
            )
            .into_iter()
            .map(|line| line.to_string()),
        );
    }
    if !m.config_warnings.is_empty() {
        text.push("Configuration warnings (FileTags ignored):".into());
        text.extend(
            details::wrapped(
                m.config_warnings
                    .iter()
                    .map(|warning| Line::from(warning.to_string()))
                    .collect(),
                parts[0].width,
            )
            .into_iter()
            .map(|line| line.to_string()),
        );
    }
    m.scroll = m
        .scroll
        .min(text.len().saturating_sub(parts[0].height as usize));
    f.render_widget(
        Paragraph::new(text.join("\n")).scroll((m.scroll as u16, 0)),
        parts[0],
    );
    f.render_widget(
        Paragraph::new(format!(
            "↑↓/PgUp/PgDn: scroll • Esc, {}, q or Enter: close",
            keymap.hint(PaletteAction::Help)
        ))
        .style(Style::default().fg(GRAY)),
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

/// Keep the distinguishing path tail when headers have only one compact column.
fn group_label(label: &str, width: usize) -> String {
    if label.width() <= width {
        return label.into();
    }
    if width == 0 {
        return String::new();
    }
    let mut tail = String::new();
    for c in label.chars().rev() {
        let candidate = format!("{c}{tail}");
        if candidate.width() + 1 > width {
            break;
        }
        tail = candidate;
    }
    format!("…{tail}")
}

fn group_mode(f: &mut Frame, m: &Model, area: Rect) {
    let View::GroupMode { selected } = &m.view else {
        return;
    };
    let area = inset(area);
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(if m.error.is_empty() { 0 } else { 2 }),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new("Groupes • Group view").style(accent()),
        parts[0],
    );
    f.render_widget(
        Paragraph::new(m.error.as_str())
            .style(Style::default().fg(Color::LightRed))
            .wrap(Wrap { trim: false }),
        parts[1],
    );
    let capacity = usize::from(parts[2].height).max(1);
    let offset = selected.saturating_sub(capacity - 1);
    let lines: Vec<_> = groups::MODES
        .iter()
        .enumerate()
        .skip(offset)
        .map(|(index, (_, label))| {
            Line::styled(
                format!("{} {label}", if index == *selected { "▶" } else { " " }),
                if index == *selected {
                    Style::default().bg(CYAN).fg(Color::Black)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), parts[2]);
    f.render_widget(Paragraph::new("↑/↓ • Enter: save • Esc: cancel"), parts[3]);
}

pub(super) fn group_content(key: &groups::Key, count: usize, aliases: bool) -> Vec<Line<'static>> {
    let (path, tag) = match key {
        groups::Key::File(path) => (path, None),
        groups::Key::Tag(path, tag) => (path, Some(tag.as_deref().unwrap_or("Sans tag"))),
    };
    let mut lines = vec![
        Line::from("Source:"),
        Line::from(path.display().to_string()),
    ];
    if let Some(tag) = tag {
        lines.push(Line::from(format!("Primary tag: {tag}")));
    }
    lines.extend([
        Line::from(format!("{count} matching {}", if aliases { "blocks" } else { "hosts" })),
        Line::from("In list: Enter/Space toggles group; Left closes or selects parent; Right opens or selects child."),
        Line::from("Select a host for SSH and host actions."),
    ]);
    lines
}
fn group_info(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::GroupInfo { key, count } = &m.view else {
        return;
    };
    let lines = group_content(key, *count, m.preferences.group_aliases);
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inset(area));
    f.render_widget(Paragraph::new("Group info").style(accent()), parts[0]);
    m.page = usize::from(parts[1].height).max(1);
    details::render_rows(f, parts[1], lines, &mut m.scroll);
    f.render_widget(Paragraph::new("↑↓: scroll • Esc: back"), parts[2]);
}

fn availability(f: &mut Frame, m: &Model, area: Rect) {
    let View::Availability { selected } = m.view else {
        return;
    };
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new("Availability method").style(accent()),
        parts[0],
    );
    f.render_widget(
        Paragraph::new(m.error.as_str())
            .style(Style::default().fg(Color::LightRed))
            .wrap(Wrap { trim: false }),
        parts[1],
    );
    let lines = [AvailabilityMethod::Ssh, AvailabilityMethod::Icmp]
        .into_iter()
        .enumerate()
        .map(|(index, method)| {
            Line::styled(
                method.scope(),
                if index == selected {
                    Style::default().bg(CYAN).fg(Color::Black)
                } else {
                    Style::default()
                },
            )
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), parts[2]);
    f.render_widget(Paragraph::new("↑/↓ • Enter: save • Esc: cancel"), parts[3]);
}

fn columns_selector(f: &mut Frame, m: &Model, area: Rect, table_width: u16) {
    let View::Columns(editor) = &m.view else {
        return;
    };
    let area = panel(area, 76, 17);
    f.render_widget(Clear, area);
    let border = if area.height >= 10 {
        block("Columns", true)
    } else {
        Block::default()
    };
    let inner = border.inner(area);
    f.render_widget(border, area);
    let error_height = if m.error.is_empty() {
        0
    } else {
        2.min(inner.height.saturating_sub(5))
    };
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(error_height),
        Constraint::Length(3),
    ])
    .split(inner);
    let target = m
        .selected_host()
        .map(|h| h.name)
        .unwrap_or_else(|| "group / no host".into());
    f.render_widget(
        Paragraph::new(ellipsize(
            &format!("Columns • {target}"),
            parts[0].width as usize,
        ))
        .style(accent()),
        parts[0],
    );
    let plan = columns::layout(&editor.draft, table_width);
    let items = editor.items();
    let rows = items.iter().map(|column| {
        let active = editor.draft.contains(column);
        let hidden = active && !plan.iter().any(|(visible, _)| visible == column);
        let state = if *column == HostColumn::Name {
            "fixed"
        } else if hidden {
            "hidden: width"
        } else if active {
            "shown"
        } else {
            "off"
        };
        Row::new([format!(
            "{} {} ({state})",
            if active { "[x]" } else { "[ ]" },
            column.label()
        )])
    });
    let mut state = TableState::default().with_selected(Some(editor.selected));
    f.render_stateful_widget(
        Table::new(rows, [Constraint::Min(0)])
            .highlight_symbol("› ")
            .row_highlight_style(Style::default().bg(CYAN).fg(Color::Black)),
        parts[1],
        &mut state,
    );
    f.render_widget(
        Paragraph::new(m.error.as_str())
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false }),
        parts[2],
    );
    f.render_widget(
        Paragraph::new(
            "↑↓ select • Space toggle\nShift+↑↓ move • r reset\nEnter save • Esc cancel",
        ),
        parts[3],
    );
}
