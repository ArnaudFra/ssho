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
use nav::{FORM_WIDTH, TABLE_WIDTH};
/// Smallest zone kept for a value before the button that follows it.
const VALUE_ZONE: usize = 24;
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
    // One line for owned work still running, on every screen: a tunnel, a batch
    // or a probe stays findable after leaving the screen that started it.
    let jobs = m.jobs();
    let area = if !jobs.is_empty() && area.height >= 8 {
        let parts = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);
        f.render_widget(
            Paragraph::new(ellipsize(
                &format!("Jobs — {}", jobs.join(" • ")),
                usize::from(parts[1].width),
            ))
            .style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            parts[1],
        );
        parts[0]
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
        View::CopyValue { .. } => copy_value(f, m, area),
        View::GroupInfo { .. } => group_info(f, m, area),
        View::Help => help(f, m, area),
        View::Messages => messages(f, m, area),
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
    let search_error = m.search.as_ref().err();
    let keymap = m.keymap();
    let notice_count = u16::from(search_error.is_some())
        + u16::from(!keymap.conflicts.is_empty())
        + u16::from(!m.error.is_empty())
        + u16::from(m.update.as_ref().is_some_and(|u| u.available));
    let notice_height = if area.height < 12 {
        notice_count.min(1)
    } else {
        notice_count
    };
    // While editing, the query lives in an input with a caret; every other
    // criterion, and the query itself once the table has the focus, is listed
    // in a band so no active filter is ever out of sight.
    let compact = area.height < 12;
    let band = filter_band(m, &keymap, area.width, !m.search_focus, compact);
    let edit_height = match (m.search_focus, compact) {
        (false, _) => 0,
        (true, true) => 1,
        (true, false) => 3,
    };
    let search_height = edit_height + band.len() as u16;
    let mut help = list_footer(
        area.width,
        m.search_focus,
        &keymap,
        m.selected_group().is_some(),
    );
    // A legend of the signs in use, where the terminal has rows to spare.
    if area.height >= 20 {
        if let Some(line) = legend(m, area.width) {
            help.insert(0, line);
        }
    }
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
        notices.push(message_line(m, &keymap, area.width));
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
    if notice_height < notice_count {
        let mut badges = vec![];
        if !keymap.conflicts.is_empty() {
            badges.push("Raccourcis refusés");
        }
        if m.update.as_ref().is_some_and(|u| u.available) {
            badges.push("Update");
        }
        let suffix = format!(" • {}", badges.join(" • "));
        let mut spans = vec![];
        if let Some(error) = search_error {
            spans.push(Span::styled(
                format!("Search: {error}"),
                Style::default().fg(Color::LightRed),
            ));
        } else if !m.error.is_empty() {
            let warning = m.error.starts_with("Warning");
            spans.push(Span::styled(
                ellipsize(
                    &if warning {
                        m.error.clone()
                    } else {
                        format!("Error: {}", m.error)
                    },
                    (area.width as usize).saturating_sub(suffix.width()),
                ),
                Style::default().fg(if warning { Color::Yellow } else { Color::Red }),
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
    let zone =
        Layout::vertical([Constraint::Length(edit_height), Constraint::Min(0)]).split(chunks[2]);
    if m.search_focus {
        let prompt = if zone[0].width < 40 {
            "/ > "
        } else {
            "Search: > "
        };
        // The focus is named, not only coloured.
        let border = block("● Recherche", true).title_bottom(
            Line::from(format!(" {} ", host_count(m))).style(Style::default().fg(GRAY)),
        );
        let search_area = if zone[0].height >= 3 {
            border.inner(zone[0])
        } else {
            zone[0]
        };
        let width = search_area.width.saturating_sub(prompt.len() as u16) as usize;
        let search = Paragraph::new(Line::from(vec![
            Span::raw(prompt),
            input_span(&m.query, true, width),
        ]));
        if zone[0].height >= 3 {
            f.render_widget(search.block(border), zone[0]);
        } else {
            f.render_widget(search, zone[0]);
        }
        if matches!(m.view, View::List) {
            input_cursor(f, &m.query, search_area, prompt.len() as u16, 0);
        }
    }
    f.render_widget(Paragraph::new(band), zone[1]);
    let (table_area, detail_area) = details::split(&mut m.details, viewport_width, chunks[3]);
    if let Some(area) = detail_area {
        details::draw(f, m, area);
    }
    let border = if area.height < 12 {
        Block::default()
    } else if m.search_focus {
        block("Hôtes", false)
    } else {
        block("● Hôtes", true)
    };
    let inner = border.inner(table_area);
    let plan = columns::layout(&m.preferences.host_columns, inner.width.min(TABLE_WIDTH));
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
    let marking = !m.batch.marks.is_empty();
    let mark_width = if marking { 4 } else { 0 };
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
                // A header is never a target; it only counts those of its hosts,
                // including the ones a collapsed group hides.
                let marked = if marking {
                    m.visible
                        .iter()
                        .map(|&index| &m.all[index])
                        .filter(|host| {
                            m.batch.marked(host)
                                && groups::ancestors(host, m.preferences.group_mode).contains(key)
                        })
                        .count()
                } else {
                    0
                };
                let suffix = match marked {
                    0 => format!(" ({count})"),
                    1 => format!(" ({count} • 1 cible)"),
                    marked => format!(" ({count} • {marked} cibles)"),
                };
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
        // Each sign has its own zone: batch target, availability, favorite, name.
        // The target zone only exists while targets are marked.
        let mut name_spans = vec![];
        if marking {
            let marked = m.batch.marked(h);
            name_spans.push(Span::styled(
                if marked { "[x] " } else { "[ ] " },
                if position == m.selected {
                    Style::default().fg(Color::Black)
                } else if marked {
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(GRAY)
                },
            ));
        }
        name_spans.push(Span::styled(
            format!(
                "{status}{}{}",
                // A discreet sign, not a column: the note itself is in the sheet.
                if m.note(&h.name).is_empty() {
                    " "
                } else {
                    "≡"
                },
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
        ));
        name_spans.extend(
            highlight(
                m,
                &alias_label(
                    h,
                    m.preferences.group_aliases,
                    usize::from(widths[0]).saturating_sub(3 + mark_width),
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
    inner.width.min(TABLE_WIDTH)
}

/// Hosts kept by the filters out of all loaded hosts. Group headers are rows of
/// the table, never SSH targets, so they are not part of this count.
fn host_count(m: &Model) -> String {
    let (shown, total) = (m.matching.len(), m.all.len());
    if shown == total {
        format!("{total} hôte{}", if total > 1 { "s" } else { "" })
    } else {
        format!(
            "{shown} hôte{} sur {total}",
            if shown > 1 { "s" } else { "" }
        )
    }
}

/// Every active criterion of the list in one place: search text (when it is not
/// being edited), exact file, favorites, hidden hosts and grouping.
fn filter_band(
    m: &Model,
    keymap: &Keymap,
    width: u16,
    with_query: bool,
    compact: bool,
) -> Vec<Line<'static>> {
    use PaletteAction as A;
    let width = usize::from(width);
    let query = with_query && !m.query.value.is_empty();
    let mut filters = vec![];
    if query {
        filters.push(m.query.value.clone());
    }
    if m.favorites_only {
        filters.push("Favoris seuls".to_owned());
    }
    if m.show_hidden {
        filters.push(format!(
            "Masqués visibles ({})",
            keymap.hint(A::ToggleHidden)
        ));
    }
    let file = m
        .source_filter
        .as_ref()
        .map(|path| format!("Fichier : {}", path.display()));
    if !m.batch.marks.is_empty() && compact {
        filters.push(format!("Lot : {} cible(s)", m.batch.marks.len()));
    }
    if let (Some(path), true) = (&m.source_filter, compact) {
        // One row only: the file scope leads the criteria, with its clear key.
        filters.insert(
            0,
            format!(
                "Fichier ({}/F2) : {}",
                keymap.hint(A::ClearSourceFilter),
                path.display()
            ),
        );
    }
    let grouping = groups::MODES
        .iter()
        .find(|(mode, _)| *mode == m.preferences.group_mode && *mode != GroupMode::Flat)
        .map(|(_, label)| format!("Groupes : {label}"));
    let mut lines = vec![];
    let count = host_count(m);
    if with_query || !filters.is_empty() || grouping.is_some() {
        let mut left = if filters.is_empty() {
            format!("{} Rechercher", keymap.hint(A::Search))
        } else {
            format!("Filtres : {}", filters.join(" • "))
        };
        if let Some(grouping) = &grouping {
            left.push_str(" • ");
            left.push_str(grouping);
        }
        if query {
            let hint = format!(
                "  ({} : modifier • {} : effacer)",
                keymap.hint(A::Search),
                keymap.hint(A::ClearSearch)
            );
            if left.width() + hint.width() + count.width() + 3 <= width {
                left.push_str(&hint);
            }
        }
        // The count follows the filters, never pushed to the far edge of a wide
        // terminal; when the row is too short the filters win.
        let room = width.saturating_sub(count.width() + 3);
        let active = !filters.is_empty();
        let style = if active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(GRAY)
        };
        if with_query && left.width() <= room {
            lines.push(Line::from(vec![
                Span::styled(left, style),
                Span::styled(format!(" • {count}"), Style::default().fg(GRAY)),
            ]));
        } else {
            lines.push(Line::styled(ellipsize(&left, width), style));
        }
    }
    if !m.batch.marks.is_empty() && !compact {
        // Marks are frozen targets: a search or a collapsed group never drops them.
        let marks = m.batch.marks.len();
        let shown = m
            .matching
            .iter()
            .filter(|&&index| m.batch.marked(&m.all[index]))
            .count();
        let outside = marks.saturating_sub(shown);
        let mut text = format!(
            "Lot : {marks} cible{} marquée{}",
            if marks > 1 { "s" } else { "" },
            if marks > 1 { "s" } else { "" }
        );
        if outside > 0 {
            text.push_str(&format!(", dont {outside} hors du filtre"));
        }
        let hint = format!(
            " • {} : voir / préparer • F2 : tout retirer",
            keymap.hint(A::BatchOpen)
        );
        if text.width() + hint.width() <= width {
            text.push_str(&hint);
        }
        lines.push(Line::styled(
            ellipsize(&text, width),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let (Some(file), false) = (file, compact) {
        let hint = format!(
            "  ({}/F2 : tous les fichiers)",
            keymap.hint(A::ClearSourceFilter)
        );
        let text = if file.width() + hint.width() <= width {
            format!("{file}{hint}")
        } else if let Some(path) = &m.source_filter {
            // Keep the distinguishing tail of a long path.
            let label = "Fichier : ";
            format!(
                "{label}{}",
                group_label(
                    &path.display().to_string(),
                    width.saturating_sub(label.width())
                )
            )
        } else {
            file
        };
        lines.push(Line::styled(text, Style::default().fg(Color::Yellow)));
    }
    lines
}

/// The current error or warning on one row: its level in words, and when it
/// does not fit, the key that shows it in full.
fn message_line(m: &Model, keymap: &Keymap, width: u16) -> Line<'static> {
    let warning = m.error.starts_with("Warning");
    let text = if warning {
        m.error.clone()
    } else {
        format!("Error: {}", m.error)
    };
    let style = Style::default().fg(if warning { Color::Yellow } else { Color::Red });
    let width = usize::from(width);
    if text.width() <= width {
        return Line::styled(text, style);
    }
    let more = format!(" ({}: full text)", keymap.hint(PaletteAction::Messages));
    Line::styled(
        format!(
            "{}{more}",
            ellipsize(&text, width.saturating_sub(more.width()))
        ),
        style,
    )
}

/// Every current message, wrapped and scrollable, each with its level in words.
fn messages(f: &mut Frame, m: &mut Model, area: Rect) {
    let area = nav::bounded(inset(area), nav::TABLE_WIDTH);
    let border = block("Messages", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let mut lines = vec![];
    for (level, text) in m.messages() {
        let color = match level {
            "Error" => Color::Red,
            "Warning" => Color::Yellow,
            _ => CYAN,
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("[{level}] "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(""),
        ]));
        lines.extend(text.lines().map(|line| Line::from(line.to_owned())));
        lines.push(Line::from(""));
    }
    if lines.is_empty() {
        lines.push(Line::from("No message."));
    }
    m.page = usize::from(parts[0].height).max(1);
    details::render_rows(f, parts[0], lines, &mut m.scroll);
    f.render_widget(
        Paragraph::new("↑↓/PgUp/PgDn: scroll • Esc, q or Enter: back")
            .style(Style::default().fg(GRAY)),
        parts[1],
    );
}

/// What the screen under the help is for: shown first, before the shortcuts.
fn screen_guide(view: Option<&View>) -> Vec<&'static str> {
    match view {
        Some(View::Info(_)) => vec![
            "This screen: a host sheet. Read its sections; Tab or ←/→ picks a named action",
            "(Connexion, Modifier, Note, Copier, Fermer) and Enter runs it. Enter alone does nothing.",
        ],
        Some(View::Host(_)) => vec![
            "This screen: the host form. Tab/↑↓ walk fields, section headers and buttons;",
            "Enter moves to the next field or activates; Ctrl+S previews, nothing is written before.",
        ],
        Some(View::Forward(_)) => vec![
            "This screen: a tunnel. Choose its type with ←/→, read the route in words,",
            "then Tab to Launch or Save profile. Enter never saves or launches from a field.",
        ],
        Some(View::Note(_)) => vec![
            "This screen: a personal note, kept in the ssho data and never in the SSH file.",
            "Enter adds a line; Tab reaches Save and Cancel; Ctrl+S saves.",
        ],
        _ => vec![
            "This screen: the host list. Type / to search, ↑↓ to choose, Enter to connect.",
            "F2 lists every action with its shortcut; Esc goes back one level and never quits.",
        ],
    }
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
    // One row stands for the whole block: say how many other aliases it holds,
    // when that fits without shortening the alias used for the connection.
    let others = host.aliases.len().saturating_sub(1);
    let with_others = |label: String| {
        let suffix = format!(" +{others} alias");
        if grouped && others > 0 && label.width() + suffix.width() <= width {
            format!("{label}{suffix}")
        } else {
            label
        }
    };
    if !grouped || primary == host.name {
        return with_others(ellipsize(&host.name, width));
    }
    let available = width.saturating_sub(1);
    let active_width = host.name.width().min(available.div_ceil(2));
    let primary_width = primary.width().min(available.saturating_sub(active_width));
    let active_width = available.saturating_sub(primary_width);
    with_others(format!(
        "{}→{}",
        ellipsize(primary, primary_width),
        ellipsize(&host.name, active_width)
    ))
}

/// Signs currently on screen, beyond the availability ones every row carries.
fn legend(m: &Model, width: u16) -> Option<Line<'static>> {
    let mut signs = vec![];
    if !m.batch.marks.is_empty() {
        signs.push("[x] cible de lot");
    }
    if m.visible
        .iter()
        .any(|&index| m.is_favorite(&m.all[index].name))
    {
        signs.push("* favori");
    }
    if m.visible
        .iter()
        .any(|&index| !m.note(&m.all[index].name).is_empty())
    {
        signs.push("≡ note personnelle");
    }
    if m.preferences.group_mode != GroupMode::Flat {
        signs.push("▼ ▶ groupe ouvert / replié");
    }
    if m.preferences.group_aliases {
        signs.push("a→b alias actif d'un bloc");
    }
    if signs.is_empty() {
        return None;
    }
    signs.push("+ en ligne  ! hors ligne  ~ en cours  ? inconnu");
    Some(Line::from(ellipsize(
        &format!("Légende : {}", signs.join(" · ")),
        usize::from(width),
    )))
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

fn list_footer(width: u16, search_focus: bool, keymap: &Keymap, group: bool) -> Vec<Line<'static>> {
    use PaletteAction as A;
    let k = |action| keymap.hint(action);
    // Enter connects to a host but opens or closes a group header.
    let enter = if group { "open/close group" } else { "connect" };
    let lines = if width >= 105 {
        if search_focus {
            vec![format!(
                "{}: actions • {}: reload • ↑/↓: select • {}: {enter} • Tab/Esc: table, filter kept",
                k(A::Palette), k(A::Reload), k(A::Connect)
            )]
        } else {
            vec![
                format!(
                    "{}: actions • {}: reload • ↑/↓: navigate • {}: {enter} • Tab: search • {}: info • {}: help • {}: quit",
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
                "{}: actions • ↑↓: select • {}: {enter} • Tab/Esc: table",
                k(A::Palette),
                k(A::Connect)
            )]
        } else {
            vec![
                format!(
                    "{}: actions • ↑↓: select • {}: {enter} • Tab: search • {}: quit",
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
/// One page: destination file, essential fields, collapsible sections and the
/// buttons, all in a single Tab order. Each row is built with the stop it draws.
fn host_form(f: &mut Frame, m: &mut Model, area: Rect) {
    use forms::{Control, FormField, Section, Stop};
    let note = match &m.view {
        View::Host(form) => form.original.as_ref().map(|host| {
            m.note(&host.name)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned()
        }),
        _ => None,
    };
    let View::Host(form) = &mut m.view else {
        return;
    };
    let area = inset(area);
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
    let title = if let Some(original) = &form.original {
        format!("Hôtes > Modifier {}", original.name)
    } else if let Some(source) = form.copy_source() {
        format!("Hôtes > Dupliquer {} — saisir un NOUVEAU nom", source.name)
    } else {
        "Hôtes > Ajouter un hôte".to_owned()
    };
    f.render_widget(Paragraph::new(title).style(accent()), parts[0]);

    let width = usize::from(parts[1].width).min(FORM_WIDTH);
    // Narrow terminals stack the label above its input.
    let stacked = width < 44;
    let label_width = if stacked { 0 } else { 18 };
    let current = form.current();
    let marker = |focused: bool| {
        if width < 3 {
            ""
        } else if focused {
            "> "
        } else {
            "  "
        }
    };
    let label_style = |focused: bool| if focused { accent() } else { Style::default() };
    let mut lines: Vec<Line<'static>> = vec![];
    let mut focus_y = 0;
    // Where the caret goes: line, column offset and the width left to the input.
    let mut caret: Option<(usize, usize, usize)> = None;

    if !form.errors.is_empty() {
        lines.push(Line::styled(
            ellipsize(
                &format!(
                    "{} champ(s) à corriger — {}",
                    form.errors.len(),
                    form.errors[0].message
                ),
                width,
            ),
            Style::default().fg(Color::Red),
        ));
    }
    let labels = [
        "Alias *",
        "Adresse *",
        "Utilisateur",
        "Port",
        "Clé SSH",
        "ProxyJump",
        "ProxyCommand",
        "Options SSH",
        "Tags",
        "Commande distante",
        "RequestTTY",
    ];
    for stop in form.stops() {
        let focused = stop == current;
        if focused {
            focus_y = lines.len();
        }
        match stop {
            Stop::Control(Control::File) => {
                let button = nav::button("Changer", focused);
                // A path may use the whole row: its button follows it directly.
                let room = usize::from(parts[1].width)
                    .saturating_sub(marker(focused).width() + 10 + button.width() + 1);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}Fichier : ", marker(focused)),
                        label_style(focused),
                    ),
                    Span::raw(format!(
                        "{} ",
                        group_label(&form.target.display().to_string(), room)
                    )),
                    button,
                ]));
                lines.push(Line::from(""));
            }
            Stop::Field(field) => {
                if field == FormField::Name(0) {
                    // An existing block is moved by its own action, not from here.
                    lines.push(Line::styled(
                        ellipsize(&format!("  Fichier : {}", form.target.display()), width),
                        Style::default().fg(GRAY),
                    ));
                    lines.push(Line::from(""));
                    if focused {
                        focus_y = lines.len();
                    }
                }
                let (input, label, trailing) = match field {
                    FormField::Input(i) => (
                        &form.inputs[i],
                        labels[i].to_owned(),
                        (i == 4).then(|| {
                            nav::button("Choisir", current == Stop::Control(Control::KeyPicker))
                        }),
                    ),
                    FormField::Name(0) if form.original.is_some() => {
                        (&form.names[0], "Alias *".to_owned(), None)
                    }
                    FormField::Name(i) => (
                        &form.names[i],
                        format!("  Alias {}", i + 1 + usize::from(form.original.is_none())),
                        Some(nav::button(
                            "Retirer",
                            current == Stop::Control(Control::RemoveAlias(i)),
                        )),
                    ),
                };
                let prefix = marker(focused).width() + label_width;
                let reserved = trailing.as_ref().map_or(0, |button| button.width() + 2);
                let room = width.saturating_sub(prefix + reserved);
                let mut row = vec![];
                if stacked {
                    lines.push(Line::styled(label, label_style(focused)));
                    row.push(Span::raw(marker(focused)));
                } else {
                    row.push(Span::styled(
                        format!("{}{label:<label_width$}", marker(focused)),
                        label_style(focused),
                    ));
                }
                let text = input_span(input, focused, room);
                // The button stays next to its value: after a short zone that
                // only grows with the text, never at the edge of the terminal.
                let zone = text.width().max(VALUE_ZONE).min(room);
                let padding = zone.saturating_sub(text.width());
                row.push(text);
                if let Some(button) = trailing {
                    row.push(Span::raw(" ".repeat(padding + 2)));
                    row.push(button);
                }
                if focused {
                    focus_y = lines.len() - usize::from(stacked);
                    caret = Some((lines.len(), prefix, room));
                }
                lines.push(Line::from(row));
                if let Some(error) = field_error(&form.errors, field) {
                    // Explicit wrapping keeps the caret row aligned with later inputs.
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
                }
                if field == FormField::Input(4) {
                    for identity in form.extra_identities() {
                        lines.push(Line::styled(
                            format!("  aussi : {identity} (lecture seule, conservée)"),
                            Style::default().fg(GRAY),
                        ));
                    }
                    lines.push(Line::from(""));
                }
            }
            // Drawn on the row of the input they belong to.
            Stop::Control(Control::KeyPicker | Control::RemoveAlias(_)) => {
                if focused {
                    focus_y = lines.len().saturating_sub(1);
                    if matches!(stop, Stop::Control(Control::KeyPicker)) {
                        // The key row is followed by read-only identities and a blank.
                        focus_y = lines
                            .iter()
                            .rposition(|line| {
                                line.spans
                                    .iter()
                                    .any(|span| span.content.contains("Choisir"))
                            })
                            .unwrap_or(focus_y);
                    }
                }
            }
            Stop::Control(Control::Section(section)) => {
                let open = form.is_open(section);
                let summary = match section {
                    Section::Aliases => {
                        format!("Alias supplémentaires ({})", form.extra_aliases().len())
                    }
                    Section::Advanced => format!(
                        "Options avancées ({} renseignée(s))",
                        [5, 6, 7, 9, 10]
                            .iter()
                            .filter(|&&i| !form.inputs[i].value.is_empty())
                            .count()
                    ),
                    Section::Organization => {
                        let inherited = form
                            .file_tags
                            .iter()
                            .map(|tag| format!("%{tag}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if inherited.is_empty() {
                            "Tags et note".to_owned()
                        } else {
                            format!("Tags et note — hérités : {inherited}")
                        }
                    }
                };
                lines.push(Line::styled(
                    ellipsize(
                        &format!(
                            "{}{} {summary}",
                            marker(focused),
                            if open { "▾" } else { "▸" }
                        ),
                        width,
                    ),
                    label_style(focused),
                ));
            }
            Stop::Control(Control::AddAlias) => {
                lines.push(Line::from(vec![
                    Span::raw(format!("{}  ", marker(focused))),
                    nav::button("+ Ajouter un alias", focused),
                ]));
                lines.push(Line::styled(
                    ellipsize(
                        "    Les alias partagent ce bloc ; favoris, notes et connexions visent un alias précis.",
                        width,
                    ),
                    Style::default().fg(GRAY),
                ));
            }
            Stop::Control(Control::Note) => {
                let preview = match note.as_deref() {
                    Some("") | None => "(vide)".to_owned(),
                    Some(text) => text.to_owned(),
                };
                let button = nav::button("Modifier", focused);
                let head = format!("{}  {:<16}", marker(focused), "Note personnelle");
                let room = width.saturating_sub(head.width() + button.width() + 2);
                lines.push(Line::from(vec![
                    Span::styled(head, label_style(focused)),
                    Span::styled(
                        format!("{}  ", ellipsize(&preview, room)),
                        Style::default().fg(GRAY),
                    ),
                    button,
                ]));
            }
            Stop::Control(Control::Preview) => {
                lines.push(Line::from(""));
                if focused {
                    focus_y = lines.len();
                }
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    nav::button("Prévisualiser les modifications", focused),
                    Span::raw("   "),
                    nav::button("Annuler", current == Stop::Control(Control::Cancel)),
                ]));
            }
            Stop::Control(Control::Cancel) => {
                if focused {
                    focus_y = lines.len() - 1;
                }
            }
        }
    }
    let focused_error = match current {
        Stop::Field(field) => field_error(&form.errors, field).is_some(),
        Stop::Control(_) => false,
    };
    scroll_focus(
        &mut form.scroll,
        focus_y,
        parts[1].height,
        lines.len(),
        focused_error,
    );
    f.render_widget(
        Paragraph::new(lines).scroll((form.scroll as u16, 0)),
        parts[1],
    );
    let input = match current {
        Stop::Field(FormField::Name(i)) => form.names.get(i),
        Stop::Field(FormField::Input(i)) => form.inputs.get(i),
        Stop::Control(_) => None,
    };
    if let (Some(input), Some((line, prefix, room))) = (input, caret) {
        if let Some(row) = line.checked_sub(form.scroll) {
            let field = Rect::new(
                parts[1].x,
                parts[1].y,
                (prefix + room).min(width) as u16,
                parts[1].height,
            );
            input_cursor(f, input, field, prefix as u16, row);
        }
    }
    // What the focused element is or does, then the keys that always apply.
    let hint = match current {
        Stop::Field(field) => forms::HostForm::example(field),
        Stop::Control(Control::File) => {
            "Entrée : choisir un autre fichier ; le brouillon est conservé"
        }
        Stop::Control(Control::KeyPicker) => {
            "Entrée ou F9 : choisir une clé existante ; seul ce champ est rempli"
        }
        Stop::Control(Control::Section(_)) => "Entrée ou Espace : déplier / replier",
        Stop::Control(Control::AddAlias) => "Entrée : ajouter un alias à ce bloc (Alt+A)",
        Stop::Control(Control::RemoveAlias(_)) => "Entrée : retirer cet alias du bloc (Alt+D)",
        Stop::Control(Control::Note) => {
            "Entrée : éditer la note, conservée dans les données de ssho, hors du fichier SSH"
        }
        Stop::Control(Control::Preview) => {
            "Entrée : afficher l'aperçu ; rien n'est écrit avant « Enregistrer »"
        }
        Stop::Control(Control::Cancel) => "Entrée : fermer ce formulaire",
    };
    let mut footer = vec![];
    if !form.error.is_empty() {
        footer.push(Line::styled(
            format!("Error: {}", form.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::styled(hint, Style::default().fg(GRAY)));
    footer.push(Line::styled(
        "Tab/↓ suivant • Maj+Tab/↑ précédent • Entrée : champ suivant / activer • Ctrl+S : prévisualiser • Échap : annuler • * requis",
        Style::default().fg(GRAY),
    ));
    f.render_widget(Paragraph::new(footer).wrap(Wrap { trim: false }), parts[2]);
}
fn forward_form(f: &mut Frame, m: &mut Model, area: Rect) {
    use forms::ForwardForm as F;
    let View::Forward(form) = &mut m.view else {
        return;
    };
    let area = panel(area, 96, 36);
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
        "Profile name (required to save only):".into(),
        &form.inputs[5],
        form.focus == 5,
        inner.width as usize,
        compact,
        field_error(&form.errors, forms::FormField::Input(5)),
    );
    // Three named choices; the selected one is marked, not only coloured.
    let choice =
        |label: &str, selected: bool| format!("({}) {label}", if selected { "•" } else { " " });
    if form.focus == 0 {
        focus_y = lines.len();
    }
    lines.push(Line::styled(
        "Forward Type:",
        if form.focus == 0 {
            accent()
        } else {
            Style::default()
        },
    ));
    lines.push(Line::from(format!(
        "{}{}   {}   {}",
        if form.focus == 0 { "> " } else { "  " },
        choice("Local (-L)", form.kind == 0),
        choice("Remote (-R)", form.kind == 1),
        choice("Dynamic (-D)", form.kind == 2),
    )));
    lines.push(Line::styled(
        match form.kind {
            0 => "Local forwarding: ssh -L [bind_address:]local_port:remote_host:remote_port",
            1 => "Remote forwarding: ssh -R [bind_address:]remote_port:local_host:local_port",
            _ => "Dynamic forwarding (SOCKS proxy): ssh -D [bind_address:]port",
        },
        Style::default().fg(GRAY),
    ));
    // The direction in words: where it listens, what it reaches, from where.
    lines.push(Line::styled(
        ellipsize(&form.route(), usize::from(inner.width)),
        Style::default().fg(Color::Green),
    ));
    lines.push(Line::from(""));
    let labels = match form.kind {
        0 => [
            "",
            "Local Port (listens on this machine):",
            "Remote Host (destination, seen from the server):",
            "Remote Port (destination):",
            "Bind Address (listening address, optional):",
        ],
        1 => [
            "",
            "Remote Port (listens on the SSH server):",
            "Local Host (destination, seen from this machine):",
            "Local Port (destination):",
            "Bind Address (listening address, optional):",
        ],
        _ => [
            "",
            "SOCKS Port (listens on this machine):",
            "",
            "",
            "Bind Address (listening address, optional):",
        ],
    };
    for i in form.fields().into_iter().filter(|&i| (1..=4).contains(&i)) {
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
    use crate::storage::tunnels::LaunchMode;
    if form.focus == F::MODE {
        focus_y = lines.len();
    }
    lines.push(Line::styled(
        "Launch mode:",
        if form.focus == F::MODE {
            accent()
        } else {
            Style::default()
        },
    ));
    lines.push(Line::from(format!(
        "{}{}   {}",
        if form.focus == F::MODE { "> " } else { "  " },
        choice("Shell session", form.mode == LaunchMode::Shell),
        choice("Tunnel only", form.mode == LaunchMode::TunnelOnly),
    )));
    lines.push(Line::styled(
        ellipsize(
            if form.mode == LaunchMode::Shell {
                "Opens a shell with the forwarding; the tunnel ends when you leave that shell."
            } else {
                "Runs in the background, owned by ssho: follow or stop it in F7; it stops when ssho quits."
            },
            usize::from(inner.width),
        ),
        Style::default().fg(GRAY),
    ));
    lines.push(Line::from(""));
    if form.focus >= F::LAUNCH {
        focus_y = lines.len();
    }
    lines.push(Line::from(vec![
        Span::raw("  "),
        nav::button("Launch", form.focus == F::LAUNCH),
        Span::raw("  "),
        nav::button("Save profile", form.focus == F::SAVE),
        Span::raw("  "),
        nav::button("Cancel", form.focus == F::CANCEL),
    ]));
    let typing = (1..=5).contains(&form.focus);
    scroll_focus(
        &mut form.scroll,
        focus_y,
        parts[1].height,
        lines.len(),
        typing && field_error(&form.errors, forms::FormField::Input(form.focus)).is_some(),
    );
    f.render_widget(
        Paragraph::new(lines).scroll((form.scroll as u16, 0)),
        parts[1],
    );
    if typing {
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
        "Tab/↓: next field • Shift+Tab/↑: previous field • ←/→: type, mode or button",
        Style::default().fg(GRAY),
    ));
    footer.push(Line::styled(
        "Ctrl+S: save profile • Ctrl+L: launch • Ctrl+T: mode • Esc: cancel",
        Style::default().fg(GRAY),
    ));
    f.render_widget(Paragraph::new(footer), parts[2]);
}
/// Hosts declared in each file, for the selector's detail zone.
fn hosts_in(m: &Model, path: &Path) -> usize {
    m.all.iter().filter(|host| host.source_file == path).count()
}

fn source_files(f: &mut Frame, m: &Model, area: Rect) {
    let View::SourceFiles { files, selected } = &m.view else {
        return;
    };
    let labels = select::path_labels(files, area.width);
    let all = select::Item {
        label: "Tous les fichiers".into(),
        aside: String::new(),
        detail: vec![
            "Aucun filtre par fichier.".into(),
            format!("{} hôte(s) dans la configuration", m.all.len()),
        ],
        current: m.source_filter.is_none(),
        search: String::new(),
    };
    let items = std::iter::once(all)
        .chain(files.iter().zip(labels).map(|(path, label)| select::Item {
            label,
            aside: String::new(),
            detail: select::file_detail(path, hosts_in(m, path)),
            current: m.source_filter.as_ref() == Some(path),
            search: path.display().to_string(),
        }))
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            title: "Filtrer par fichier source".into(),
            subject: "Limiter la liste aux hôtes déclarés dans un fichier".into(),
            items,
            selected: *selected,
            error: "",
            apply: "appliquer",
            query: Some(&m.selector_query),
            empty: "Aucun fichier de configuration SSH.",
        },
    );
}

fn alias_chooser(f: &mut Frame, m: &Model, area: Rect) {
    let View::Aliases { hosts, selected } = &m.view else {
        return;
    };
    let active = m.selected_host().map(|host| host.name);
    let primary = hosts.first().map(aliases::primary).unwrap_or_default();
    let items = hosts
        .iter()
        .map(|host| {
            let effective = m
                .effective
                .get(&host.name)
                .and_then(|value| value.as_ref().ok());
            select::Item {
                label: host.name.clone(),
                aside: format!(
                    "{}@{}",
                    effective.map_or(host.user.as_str(), |e| &e.user),
                    effective.map_or(host.hostname.as_str(), |e| &e.hostname)
                ),
                detail: vec![
                    format!("Fichier : {}", host.source_file.display()),
                    "Connexion, favori et note visent cet alias précis ; le bloc SSH est partagé."
                        .into(),
                ],
                current: active.as_deref() == Some(host.name.as_str()),
                search: String::new(),
            }
        })
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            title: "Choisir un alias — cible exacte de connexion".into(),
            subject: format!("Bloc de {primary} : {} alias", hosts.len()),
            items,
            selected: *selected,
            error: "",
            apply: "choisir",
            query: Some(&m.selector_query),
            empty: "Ce bloc n'a aucun alias.",
        },
    );
}

fn files(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Files {
        files,
        selected,
        host,
        duplicate,
        retarget,
        ..
    } = &m.view
    else {
        return;
    };
    // The draft under a retarget selector knows the destination in effect.
    let current = match (retarget, m.parents.last()) {
        (true, Some((View::Host(form), _))) => Some(form.target.clone()),
        _ => None,
    };
    let (title, subject) = if *retarget {
        (
            "Changer le fichier de destination".to_owned(),
            "Le brouillon est conservé ; seul son fichier cible change".to_owned(),
        )
    } else if let Some(host) = host {
        (
            format!("Hôtes > Déplacer {} — fichier de destination", host.name),
            format!("Source : {}", host.source_file.display()),
        )
    } else if let Some(source) = duplicate.as_deref() {
        (
            format!("Hôtes > Dupliquer {} — fichier de destination", source.name),
            "Le fichier de l'original est aussi une destination possible".to_owned(),
        )
    } else {
        (
            "Hôtes > Ajouter un hôte — fichier de destination".to_owned(),
            "Fichier SSH dans lequel le nouveau bloc sera écrit".to_owned(),
        )
    };
    let labels = select::path_labels(files, area.width);
    let items = files
        .iter()
        .zip(labels)
        .map(|(path, label)| select::Item {
            label,
            aside: String::new(),
            detail: select::file_detail(path, hosts_in(m, path)),
            current: current.as_ref() == Some(path),
            search: path.display().to_string(),
        })
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            title,
            subject,
            items,
            selected: *selected,
            error: &m.error,
            apply: "choisir",
            query: Some(&m.selector_query),
            empty: "Aucun fichier de configuration SSH.",
        },
    );
}
fn info(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Info(h) = &m.view else {
        return;
    };
    let area = panel(area, 96, 34);
    let title = format!("SSH Host Information: {}", h.name);
    let border = block(&title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    // Under the content: the named actions, the keys, then what the last key did.
    let notice: Vec<Line> = if let Some((label, value)) = &m.copied {
        // Each on its own row: the value can be selected by hand if the
        // terminal ignored the clipboard request.
        vec![
            Line::styled(
                format!("Copie demandée au terminal (OSC 52) : {label}"),
                Style::default().fg(Color::Green),
            ),
            Line::styled(
                "Si le collage reste vide, sélectionnez la valeur ci-dessous :",
                Style::default().fg(GRAY),
            ),
            Line::from(value.clone()),
        ]
    } else if !m.hint.is_empty() {
        vec![Line::styled(
            m.hint.clone(),
            Style::default().fg(Color::Yellow),
        )]
    } else {
        vec![]
    };
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(if inner.height >= 6 { 2 } else { 0 }),
        Constraint::Length((notice.len() as u16).min(inner.height.saturating_sub(4))),
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
    let mut buttons = vec![];
    for (index, label) in details::SHEET_ACTIONS.into_iter().enumerate() {
        buttons.push(nav::button(label, m.sheet_focus == Some(index)));
        buttons.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(buttons),
            Line::styled(
                "↑↓/PgUp/PgDn défiler • Tab/←→ : actions • F5 : recharger • F2 : autres actions • Échap : retour",
                Style::default().fg(GRAY),
            ),
        ]),
        parts[1],
    );
    f.render_widget(Paragraph::new(notice).wrap(Wrap { trim: false }), parts[2]);
    if !m.error.is_empty() {
        f.render_widget(
            Paragraph::new(format!("Error: {}", m.error))
                .style(Style::default().fg(Color::Red))
                .wrap(Wrap { trim: false }),
            parts[3],
        );
    }
}

fn copy_value(f: &mut Frame, m: &Model, area: Rect) {
    let View::CopyValue {
        host,
        values,
        selected,
    } = &m.view
    else {
        return;
    };
    let items = values
        .iter()
        .map(|(label, value)| select::Item {
            label: label.clone(),
            aside: value.clone(),
            detail: vec![value.clone()],
            current: false,
            search: String::new(),
        })
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            title: format!("Copier une valeur — {host}"),
            subject: "Une seule valeur est copiée ; rien n'est exporté".into(),
            items,
            selected: *selected,
            error: "",
            apply: "copier",
            query: None,
            empty: "Aucune valeur à copier.",
        },
    );
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
            "Esc    leave search, filter kept / back one level",
            "Backspace (table) or Ctrl+U (input) clears the search",
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
            "Signes de la liste",
            "[x] cible de lot  * favori  ≡ note  ▼ ▶ groupe",
            "+ en ligne  ! hors ligne  ~ en cours  ? inconnu",
            "a→b alias actif d'un bloc  +N alias : autres alias du bloc",
            "",
            "Navigation",
            "↑↓     select (Ctrl+P/N in search)",
            "Group Enter/Space: expand/collapse; ←/→ parent/open",
            "SSH exit returns here with its result",
            "Alt+↑↓ scroll details (Ctrl+PgUp/PgDn; width >=110)",
            "",
            "Forms and reviews",
            "Ctrl+S preview/save • Ctrl+L launch tunnel/batch review",
            "Host form: Tab/↑↓ fields, sections, buttons; F9 keys",
            "Enter: next field or activate; never submits a field",
            "Alt+A/D add/remove an additional alias",
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
    let query = actions::fold(&m.selector_query.value);
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut text = if !words.is_empty() {
        // Searching lists every matching line, whatever its column.
        let found: Vec<String> = left
            .into_iter()
            .chain(right)
            .filter(|line| {
                let line = actions::fold(line);
                !line.trim().is_empty() && words.iter().all(|word| line.contains(word))
            })
            .collect();
        if found.is_empty() {
            vec!["No line of the help matches.".to_owned()]
        } else {
            found
        }
    } else if inner.width >= 77 {
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
    if words.is_empty() {
        // What the screen underneath is for comes before the list of shortcuts.
        let guide = screen_guide(m.parents.last().map(|(view, _)| view));
        text.splice(
            0..0,
            guide.into_iter().map(String::from).chain([String::new()]),
        );
    }
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
        Paragraph::new(if m.help_search || !m.selector_query.value.is_empty() {
            format!(
                "Search: {}{} • Enter: keep • Esc: clear",
                m.selector_query.value,
                if m.help_search { "_" } else { "" }
            )
        } else {
            format!(
                "/: search • ↑↓/PgUp/PgDn: scroll • Esc, {}, q or Enter: close",
                keymap.hint(PaletteAction::Help)
            )
        })
        .style(Style::default().fg(GRAY)),
        parts[1],
    );
}

fn note_editor(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Note(editor) = &m.view else {
        return;
    };
    let area = nav::bounded(inset(area), nav::TABLE_WIDTH);
    let border = block("Personal note", true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    // Whose note this is, and where it lives: application data, not the SSH file.
    let source = m
        .all
        .iter()
        .find(|host| host.name == editor.host)
        .map(|host| host.source_file.display().to_string());
    let data = m
        .batch
        .history_root
        .as_ref()
        .map(|root| root.join("metadata.json").display().to_string())
        .unwrap_or_else(|| "metadata.json".into());
    let mut header = vec![Line::from(vec![
        Span::styled(format!("Host: {}", editor.host), accent()),
        Span::raw("  "),
        if editor.dirty() {
            Span::styled("● Modifiée", Style::default().fg(Color::Yellow))
        } else {
            Span::styled("Enregistrée", Style::default().fg(GRAY))
        },
    ])];
    if inner.height >= 10 {
        if let Some(source) = source {
            header.push(Line::styled(
                ellipsize(
                    &format!("Fichier SSH de l'hôte : {source}"),
                    usize::from(inner.width),
                ),
                Style::default().fg(GRAY),
            ));
        }
        header.push(Line::styled(
            ellipsize(
                &format!("Donnée de ssho : {data}"),
                usize::from(inner.width),
            ),
            Style::default().fg(GRAY),
        ));
        header.push(Line::styled(
            ellipsize(
                "Jamais écrite dans le fichier SSH ; à ne pas confondre avec un tag.",
                usize::from(inner.width),
            ),
            Style::default().fg(GRAY),
        ));
    }
    let footer_height = if inner.height >= 6 { 3 } else { 1 };
    let parts = Layout::vertical([
        Constraint::Length(header.len() as u16),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    f.render_widget(Paragraph::new(header), parts[0]);
    let before: String = editor
        .input
        .value
        .chars()
        .take(editor.input.cursor)
        .collect();
    let cursor_row = before.chars().filter(|&c| c == '\n').count();
    let cursor_col = before.rsplit('\n').next().unwrap_or("").chars().count();
    let offset = cursor_row.saturating_sub(parts[1].height.saturating_sub(1) as usize);
    let typing = editor.button.is_none();
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
        let focused = typing && row == cursor_row;
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
    if footer_height > 1 {
        footer.push(Line::from(vec![
            nav::button("Enregistrer", editor.button == Some(0)),
            Span::raw("  "),
            nav::button("Annuler", editor.button == Some(1)),
        ]));
    }
    footer.push(Line::styled(
        "Enter: newline • Tab: buttons • Ctrl+S: save • Esc/Ctrl+C: cancel",
        Style::default().fg(GRAY),
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
                nav::bounded(parts[1], TABLE_WIDTH),
                &mut state,
            );
        }
    }
    // The route of the selected profile in words, then the named actions.
    let empty = m
        .tunnel_profiles
        .get(&list.host)
        .is_none_or(BTreeMap::is_empty);
    let route = m
        .tunnel_profiles
        .get(&list.host)
        .and_then(|profiles| profiles.iter().nth(list.selected))
        .map(|(name, forward)| {
            forms::ForwardForm::named(list.host.clone(), name.clone(), forward.clone()).route()
        })
        .unwrap_or_default();
    let focused = if empty {
        profiles::ACTIONS.len() - 1
    } else {
        list.button
    };
    let mut buttons = vec![];
    for (index, label) in profiles::ACTIONS.into_iter().enumerate() {
        buttons.push(nav::button(label, index == focused));
        buttons.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(vec![
            if list.error.is_empty() {
                Line::styled(
                    ellipsize(&route, usize::from(parts[2].width)),
                    Style::default().fg(Color::Green),
                )
            } else {
                Line::styled(list.error.clone(), Style::default().fg(Color::Red))
            },
            Line::from(buttons),
            Line::styled(
                "↑↓: profile • ←/→: action • Enter: run it • f: last forwarding draft • Esc: back",
                Style::default().fg(GRAY),
            ),
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
    let items = groups::MODES
        .iter()
        .map(|(mode, label)| select::Item {
            label: (*label).to_owned(),
            aside: String::new(),
            detail: vec![match mode {
                GroupMode::Flat => "Tous les hôtes dans une seule liste triée.",
                GroupMode::Files => "Un groupe repliable par fichier de configuration.",
                GroupMode::FileTags => {
                    "Par fichier, puis par tag principal : premier tag propre, sinon premier tag hérité, sinon « Sans tag »."
                }
            }
            .to_owned()],
            current: *mode == m.preferences.group_mode,
            search: String::new(),
        })
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            title: "Groupes — présentation de la liste".into(),
            subject: "Préférence enregistrée ; les replis restent ceux de la session".into(),
            items,
            selected: *selected,
            error: &m.error,
            apply: "enregistrer",
            query: None,
            empty: "",
        },
    );
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
    // Why these hosts are together, in the terms of the grouping mode.
    lines.push(Line::from(match tag {
        None => "Critère : hôtes déclarés dans ce fichier de configuration.",
        Some(_) => {
            "Critère : hôtes de ce fichier dont le tag principal est celui-ci (premier tag propre, sinon premier tag hérité, sinon « Sans tag »)."
        }
    }));
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
    let items = [AvailabilityMethod::Ssh, AvailabilityMethod::Icmp]
        .into_iter()
        .map(|method| select::Item {
            label: method.label().to_owned(),
            aside: String::new(),
            detail: match method {
                AvailabilityMethod::Ssh => vec![
                    "Vérifie qu'un serveur SSH répond : poignée de main directe, ou contrôle via le proxy configuré.".to_owned(),
                    "Ne prouve pas l'authentification.".to_owned(),
                ],
                AvailabilityMethod::Icmp => vec![
                    "Ping depuis ce poste : disponibilité réseau seulement, sans bastion ni authentification SSH.".to_owned(),
                    "État « inconnu » si la commande ping manque ou est refusée.".to_owned(),
                ],
            },
            current: method == m.preferences.availability_method,
            search: String::new(),
        })
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            title: "Méthode de disponibilité".into(),
            subject: "Ce que mesure l'état affiché dans la liste".into(),
            items,
            selected,
            error: &m.error,
            apply: "enregistrer",
            query: None,
            empty: "",
        },
    );
}

fn columns_selector(f: &mut Frame, m: &Model, area: Rect, table_width: u16) {
    let View::Columns(editor) = &m.view else {
        return;
    };
    let area = panel(area, 76, 20);
    f.render_widget(Clear, area);
    let border = if area.height >= 10 {
        block("Columns", true)
    } else {
        Block::default()
    };
    let inner = border.inner(area);
    f.render_widget(border, area);
    // Roomy panels add the outcome at the current width and the named actions.
    let roomy = inner.height >= 14;
    // An error is shown whole: it takes the rows its wrapped text needs.
    let error_rows: usize = m
        .error
        .lines()
        .map(|line| select::wrapped_rows(line, usize::from(inner.width).max(1)))
        .sum();
    let error_height =
        (error_rows as u16).min(inner.height.saturating_sub(if roomy { 10 } else { 5 }));
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(error_height),
        Constraint::Length(if roomy { 3 } else { 0 }),
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
    // The final order as the table will show it, and what the width leaves out:
    // a column switched off and a column short of room are two different things.
    let names = |columns: Vec<HostColumn>| {
        columns
            .iter()
            .map(|column| column.label())
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let visible: Vec<HostColumn> = plan.iter().map(|(column, _)| *column).collect();
    let waiting: Vec<HostColumn> = editor
        .draft
        .iter()
        .copied()
        .filter(|column| !visible.contains(column))
        .collect();
    let mut buttons = vec![];
    for (index, label) in columns::ACTIONS.into_iter().enumerate() {
        buttons.push(nav::button(label, editor.button == Some(index)));
        buttons.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(ellipsize(
                &format!("Shown at this width: {}", names(visible)),
                usize::from(parts[3].width),
            )),
            Line::styled(
                ellipsize(
                    &if waiting.is_empty() {
                        "Every active column fits.".to_owned()
                    } else {
                        format!("Active but short of room: {}", names(waiting))
                    },
                    usize::from(parts[3].width),
                ),
                Style::default().fg(GRAY),
            ),
            Line::from(buttons),
        ]),
        parts[3],
    );
    f.render_widget(
        Paragraph::new(
            "↑↓ select • Space toggle • Tab: actions\nShift+↑↓ move • r reset\nEnter save • Esc cancel",
        ),
        parts[4],
    );
}
