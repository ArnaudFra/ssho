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
/// Below this size the host list can no longer show a host and its legend.
/// Forms and confirmations keep their own compact layout, caret included.
pub(super) const MIN_WIDTH: u16 = 24;
pub(super) const MIN_HEIGHT: u16 = 6;

pub(super) fn draw(f: &mut Frame, m: &mut Model) {
    // Every text of the frame is drawn in the language of this model.
    crate::i18n::set(m.preferences.language);
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let too_small = area.width < MIN_WIDTH || area.height < MIN_HEIGHT;
    if too_small && matches!(m.view, View::List) && !m.quit_tunnels {
        // Said plainly rather than drawing a list whose rows are cut off.
        let text = crate::tf!(
            "Terminal too small ({}x{}): enlarge it to {MIN_WIDTH}x{MIN_HEIGHT} or more.",
            "Terminal trop petit ({}x{}) : agrandissez-le à {MIN_WIDTH}x{MIN_HEIGHT} ou plus.",
            area.width,
            area.height
        );
        f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), area);
    } else {
        draw_frame(f, m, area);
    }
    plain_display(f, !m.preferences.colors || m.no_color, m.preferences.ascii);
}

/// Display preferences applied to the finished frame, so that no screen can
/// forget them: colours removed, symbols replaced by ASCII characters.
fn plain_display(f: &mut Frame, no_colors: bool, ascii: bool) {
    if !no_colors && !ascii {
        return;
    }
    for cell in &mut f.buffer_mut().content {
        if no_colors {
            // A coloured background was a selection: it becomes a reversed cell.
            // Grey was an indicative value: it becomes a dimmed one.
            if cell.bg != Color::Reset {
                cell.modifier.insert(Modifier::REVERSED);
            }
            if cell.fg == Color::DarkGray || cell.fg == Color::Gray {
                cell.modifier.insert(Modifier::DIM);
            }
            cell.fg = Color::Reset;
            cell.bg = Color::Reset;
        }
        if ascii {
            let mut symbols = cell.symbol().chars();
            if let (Some(symbol), None) = (symbols.next(), symbols.next()) {
                if let Some(plain) = ascii_symbol(symbol) {
                    cell.set_char(plain);
                }
            }
        }
    }
}

/// ASCII stand-in of an interface symbol; text and user data are left alone.
fn ascii_symbol(symbol: char) -> Option<char> {
    Some(match symbol {
        '─' | '━' | '═' | '—' | '–' => '-',
        '│' | '┃' | '║' | '•' => '|',
        '╭' | '╮' | '╰' | '╯' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' => {
            '+'
        }
        '▶' | '▸' | '→' | '›' => '>',
        '←' => '<',
        '↑' => '^',
        '▼' | '▾' | '↓' => 'v',
        '●' => 'o',
        '✓' => '+',
        '≡' => '=',
        '…' | '·' => '.',
        '«' | '»' => '"',
        '’' => '\'',
        _ => return None,
    })
}

fn draw_frame(f: &mut Frame, m: &mut Model, area: Rect) {
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
                    crate::tf!(
                        " (+{}; ? for all)",
                        " (+{} ; ? pour tout voir)",
                        m.config_warnings.len() - count
                    )
                } else {
                    String::new()
                };
                Line::from(crate::tf!(
                    "FileTags warning{more}: {warning}",
                    "Avertissement FileTags{more} : {warning}"
                ))
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
                &crate::tf!("Jobs — {}", "Travaux — {}", jobs.join(" • ")),
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
            lines.push(Line::from(crate::tf!(
                "• {tunnels} active tunnel(s): they will be stopped.",
                "• {tunnels} tunnel(s) actif(s) : ils seront arrêtés."
            )));
        }
        if batch {
            lines.push(Line::from(crate::t!(
                "• Running batch: it will be cancelled; remote work already detached may go on.",
                "• Lot en cours : il sera annulé ; un travail distant déjà détaché peut continuer."
            )));
        }
        if m.has_unsaved_context() {
            lines.push(Line::from(crate::t!(
                "• Unsaved changes: they will be lost.",
                "• Modifications non enregistrées : elles seront perdues."
            )));
        }
        if lines.is_empty() {
            lines.push(Line::from(crate::t!(
                "No work in progress.",
                "Aucun travail en cours."
            )));
        }
        m.confirm.draw(
            f,
            area,
            crate::t!("Quit ssho?", "Quitter ssho ?"),
            lines,
            crate::t!("Go back", "Revenir"),
            if tunnels > 0 || batch {
                crate::t!("Stop and quit", "Arrêter et quitter")
            } else {
                crate::t!("Discard and quit", "Abandonner et quitter")
            },
        );
        return;
    }
    if m.discard {
        m.confirm.draw(
            f,
            area,
            crate::t!("Unsaved changes", "Modifications non enregistrées"),
            vec![Line::from(crate::t!(
                "Discard the changes of this draft?",
                "Abandonner les modifications de ce brouillon ?"
            ))],
            crate::t!("Keep editing", "Continuer l'édition"),
            crate::t!("Discard", "Abandonner"),
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
        View::Preferences(_) => preferences(f, m, area),
        View::Views(_) => saved_views(f, m, area),
        View::Commands(_) => saved_commands(f, m, area),
        View::Transfer(_) => data_transfer(f, m, area),
        View::Shortcuts(_) => shortcuts(f, m, area),
        View::Files { .. } => files(f, m, area),
        View::SourceFiles { .. } => source_files(f, m, area),
        View::Aliases { .. } => alias_chooser(f, m, area),
        View::GroupMode { .. } => group_mode(f, m, area),
        View::Availability { .. } => availability(f, m, area),
        View::Language { .. } => language(f, m, area),
    }
}
fn list(f: &mut Frame, m: &mut Model, area: Rect) -> u16 {
    if let Some(h) = &m.delete {
        let area = panel(area, 72, 11);
        f.render_widget(Clear, area);
        let text = vec![
            Line::styled(
                crate::t!("DELETE SSH HOST", "SUPPRIMER L'HÔTE SSH"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::from(crate::tf!(
                "Are you sure you want to delete host '{}' ?",
                "Voulez-vous vraiment supprimer l'hôte « {} » ?",
                h.name
            )),
            Line::from(""),
            Line::styled(
                crate::t!(
                    "This action cannot be undone.",
                    "Cette action est irréversible."
                ),
                Style::default().fg(Color::LightRed),
            ),
            Line::from(""),
            Line::styled(
                crate::t!(
                    "Enter: confirm • Esc: cancel",
                    "Entrée : confirmer • Échap : annuler"
                ),
                Style::default().fg(GRAY),
            ),
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
            crate::t!(
                "Group • i: info • Enter: toggle",
                "Groupe • i : fiche • Entrée : déplier"
            )
        } else {
            crate::t!("No selection", "Aucune sélection")
        });
    let title = if !m.hint.is_empty() {
        &m.hint
    } else if m.connection_result.is_empty() {
        crate::t!("SSHO • F4: details", "SSHO • F4 : détails")
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
            crate::tf!("Search: {error}", "Recherche : {error}"),
            Style::default().fg(Color::LightRed),
        ));
    }
    if let Some(update) = &m.update {
        if update.available {
            notices.push(Line::styled(
                crate::tf!(
                    "Update available: {} → {}",
                    "Mise à jour disponible : {} → {}",
                    update.current,
                    update.latest
                ),
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
            crate::tf!(
                "Shortcuts: {} preference(s) refused — details in the help ({})",
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
            badges.push(crate::t!("Shortcuts refused", "Raccourcis refusés"));
        }
        if m.update.as_ref().is_some_and(|u| u.available) {
            badges.push(crate::t!("Update", "Mise à jour"));
        }
        let suffix = format!(" • {}", badges.join(" • "));
        let mut spans = vec![];
        if let Some(error) = search_error {
            spans.push(Span::styled(
                crate::tf!("Search: {error}", "Recherche : {error}"),
                Style::default().fg(Color::LightRed),
            ));
        } else if !m.error.is_empty() {
            let warning = m.error.starts_with("Warning");
            spans.push(Span::styled(
                ellipsize(
                    &if warning {
                        m.error.clone()
                    } else {
                        crate::tf!("Error: {}", "Erreur : {}", m.error)
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
            crate::t!("Search: > ", "Recherche : > ")
        };
        // The focus is named, not only coloured.
        let border = block(crate::t!("● Search", "● Recherche"), true).title_bottom(
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
        block(crate::t!("Hosts", "Hôtes"), false)
    } else {
        block(crate::t!("● Hosts", "● Hôtes"), true)
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
                    1 => crate::tf!(" ({count} • 1 target)", " ({count} • 1 cible)"),
                    marked => crate::tf!(
                        " ({count} • {marked} targets)",
                        " ({count} • {marked} cibles)"
                    ),
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
        // Three different situations, each with the key that gets out of it.
        let k = |action| keymap.hint(action);
        let text = if m.all.is_empty() && !m.error.is_empty() && !m.error.starts_with("Warning") {
            crate::tf!(
                "The SSH configuration could not be read. {}: see the error • {}: reload",
                "La configuration SSH n'a pas pu être lue. {} : voir l'erreur • {} : recharger",
                k(PaletteAction::Messages),
                k(PaletteAction::Reload)
            )
        } else if m.all.is_empty() {
            crate::tf!(
                "No host in {}. {}: add a host",
                "Aucun hôte dans {}. {} : ajouter un hôte",
                m.batch.root.display(),
                k(PaletteAction::Add)
            )
        } else if m.favorites_only {
            crate::tf!(
                "No favorite matches the filters. {}: Favorites only, to show every host again",
                "Aucun favori ne correspond aux filtres. {} : Favoris seuls, pour revoir tous les hôtes",
                k(PaletteAction::Palette)
            )
        } else {
            crate::tf!(
                "No host matches the filters. {}: clear the search • {}: Clear all filters",
                "Aucun hôte ne correspond aux filtres. {} : effacer la recherche • {} : Effacer tous les filtres",
                k(PaletteAction::ClearSearch),
                k(PaletteAction::Palette)
            )
        };
        f.render_widget(
            Paragraph::new(text)
                .style(Style::default().fg(GRAY))
                .wrap(Wrap { trim: true }),
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
        crate::tf!(
            "{total} host{}",
            "{total} hôte{}",
            if total > 1 { "s" } else { "" }
        )
    } else {
        crate::tf!(
            "{shown} host{} of {total}",
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
        filters.push(crate::t!("Favorites only", "Favoris seuls").to_owned());
    }
    if m.show_hidden {
        filters.push(crate::tf!(
            "Hidden shown ({})",
            "Masqués visibles ({})",
            keymap.hint(A::ToggleHidden)
        ));
    }
    // The view in use leads the criteria; a context that moved away says so.
    if let Some((name, modified)) = m.view_state() {
        filters.insert(
            0,
            if modified {
                crate::tf!("View: {name} (modified)", "Vue : {name} (modifiée)")
            } else {
                crate::tf!("View: {name}", "Vue : {name}")
            },
        );
    }
    let file = m
        .source_filter
        .as_ref()
        .map(|path| crate::tf!("File: {}", "Fichier : {}", path.display()));
    if !m.batch.marks.is_empty() && compact {
        filters.push(crate::tf!(
            "Batch: {} target(s)",
            "Lot : {} cible(s)",
            m.batch.marks.len()
        ));
    }
    if let (Some(path), true) = (&m.source_filter, compact) {
        // One row only: the file scope leads the criteria, with its clear key.
        filters.insert(
            0,
            crate::tf!(
                "File ({}/F2): {}",
                "Fichier ({}/F2) : {}",
                keymap.hint(A::ClearSourceFilter),
                path.display()
            ),
        );
    }
    let grouping = groups::modes()
        .iter()
        .find(|(mode, _)| *mode == m.preferences.group_mode && *mode != GroupMode::Flat)
        .map(|(_, label)| crate::tf!("Groups: {label}", "Groupes : {label}"));
    let mut lines = vec![];
    let count = host_count(m);
    if with_query || !filters.is_empty() || grouping.is_some() {
        let mut left = if filters.is_empty() {
            crate::tf!("{} Search", "{} Rechercher", keymap.hint(A::Search))
        } else {
            crate::tf!("Filters: {}", "Filtres : {}", filters.join(" • "))
        };
        if let Some(grouping) = &grouping {
            left.push_str(" • ");
            left.push_str(grouping);
        }
        if query {
            let hint = crate::tf!(
                "  ({}: edit • {}: clear)",
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
        let plural = if marks > 1 { "s" } else { "" };
        let mut text = crate::tf!(
            "Batch: {marks} marked target{plural}",
            "Lot : {marks} cible{plural} marquée{plural}"
        );
        if outside > 0 {
            text.push_str(&crate::tf!(
                ", {outside} outside the filter",
                ", dont {outside} hors du filtre"
            ));
        }
        let hint = crate::tf!(
            " • {}: view / prepare • F2: clear all",
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
        let hint = crate::tf!(
            "  ({}/F2: all files)",
            "  ({}/F2 : tous les fichiers)",
            keymap.hint(A::ClearSourceFilter)
        );
        let text = if file.width() + hint.width() <= width {
            format!("{file}{hint}")
        } else if let Some(path) = &m.source_filter {
            // Keep the distinguishing tail of a long path.
            let label = crate::t!("File: ", "Fichier : ");
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
        crate::tf!("Error: {}", "Erreur : {}", m.error)
    };
    let style = Style::default().fg(if warning { Color::Yellow } else { Color::Red });
    let width = usize::from(width);
    if text.width() <= width {
        return Line::styled(text, style);
    }
    let more = crate::tf!(
        " ({}: full text)",
        " ({} : texte complet)",
        keymap.hint(PaletteAction::Messages)
    );
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
    let border = block(crate::t!("Messages", "Messages"), true);
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
                format!(
                    "[{}] ",
                    match level {
                        "Error" => crate::t!("Error", "Erreur"),
                        "Warning" => crate::t!("Warning", "Avertissement"),
                        _ => "Info",
                    }
                ),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(""),
        ]));
        lines.extend(text.lines().map(|line| Line::from(line.to_owned())));
        lines.push(Line::from(""));
    }
    if lines.is_empty() {
        lines.push(Line::from(crate::t!("No message.", "Aucun message.")));
    }
    m.page = usize::from(parts[0].height).max(1);
    details::render_rows(f, parts[0], lines, &mut m.scroll);
    f.render_widget(
        Paragraph::new(crate::t!(
            "↑↓/PgUp/PgDn: scroll • Esc, q or Enter: back",
            "↑↓/PgUp/PgDn : défiler • Échap, q ou Entrée : retour"
        ))
        .style(Style::default().fg(GRAY)),
        parts[1],
    );
}

/// What the screen under the help is for: shown first, before the shortcuts.
fn screen_guide(view: Option<&View>) -> Vec<&'static str> {
    match view {
        Some(View::Info(_)) => vec![
            crate::t!("This screen: a host sheet. Read its sections; Tab or ←/→ picks a named action", "Cet écran : la fiche d'un hôte. Lisez ses sections ; Tab ou ←/→ choisit une action nommée"),
            crate::t!("(Connect, Edit, Note, Copy, Close) and Enter runs it. Enter alone does nothing.", "(Connexion, Modifier, Note, Copier, Fermer) et Entrée la lance. Entrée seule ne fait rien."),
        ],
        Some(View::Host(_)) => vec![
            crate::t!("This screen: the host form. Tab/↑↓ walk fields, section headers and buttons;", "Cet écran : le formulaire d'hôte. Tab/↑↓ parcourent champs, titres de section et boutons ;"),
            crate::t!("Enter moves to the next field or activates; Ctrl+S previews, nothing is written before.", "Entrée passe au champ suivant ou active ; Ctrl+S prévisualise, rien n'est écrit avant."),
        ],
        Some(View::Forward(_)) => vec![
            crate::t!("This screen: a tunnel. Choose its type with ←/→, read the route in words,", "Cet écran : un tunnel. Choisissez son type avec ←/→, lisez le trajet en toutes lettres,"),
            crate::t!("then Tab to Launch or Save profile. Enter never saves or launches from a field.", "puis Tab jusqu'à Lancer ou Enregistrer le profil. Entrée n'enregistre ni ne lance depuis un champ."),
        ],
        Some(View::Note(_)) => vec![
            crate::t!("This screen: a personal note, kept in the ssho data and never in the SSH file.", "Cet écran : une note personnelle, conservée dans les données de ssho, jamais dans le fichier SSH."),
            crate::t!("Enter adds a line; Tab reaches Save and Cancel; Ctrl+S saves.", "Entrée ajoute une ligne ; Tab atteint Enregistrer et Annuler ; Ctrl+S enregistre."),
        ],
        _ => vec![
            crate::t!("This screen: the host list. Type / to search, ↑↓ to choose, Enter to connect.", "Cet écran : la liste des hôtes. / pour chercher, ↑↓ pour choisir, Entrée pour se connecter."),
            crate::t!("F2 lists every action with its shortcut; Esc goes back one level and never quits.", "F2 liste toutes les actions avec leur raccourci ; Échap revient d'un niveau et ne quitte jamais."),
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
                // The labels are the ones the sheet writes, in its language.
                let label = line.spans[0].content.as_ref();
                let is = |name: &str| label.strip_suffix(": ") == Some(name);
                use details::label as l;
                field = if is(l::host()) || is(l::hostname()) || is(l::note()) {
                    Some(search::Field::Any)
                } else if is(l::user()) {
                    Some(search::Field::User)
                } else if is(l::tags()) {
                    Some(search::Field::Tag)
                } else if is(l::source()) {
                    Some(search::Field::File)
                } else {
                    None
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
        let suffix = crate::tf!(" +{others} alias", " +{others} alias");
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
        signs.push(crate::t!("[x] batch target", "[x] cible de lot"));
    }
    if m.visible
        .iter()
        .any(|&index| m.is_favorite(&m.all[index].name))
    {
        signs.push(crate::t!("* favorite", "* favori"));
    }
    if m.visible
        .iter()
        .any(|&index| !m.note(&m.all[index].name).is_empty())
    {
        signs.push(crate::t!("≡ personal note", "≡ note personnelle"));
    }
    if m.preferences.group_mode != GroupMode::Flat {
        signs.push(crate::t!(
            "▼ ▶ group open / collapsed",
            "▼ ▶ groupe ouvert / replié"
        ));
    }
    if m.preferences.group_aliases {
        signs.push(crate::t!(
            "a→b active alias of a block",
            "a→b alias actif d'un bloc"
        ));
    }
    if signs.is_empty() {
        return None;
    }
    signs.push(crate::t!(
        "+ online  ! offline  ~ in progress  ? unknown",
        "+ en ligne  ! hors ligne  ~ en cours  ? inconnu"
    ));
    Some(Line::from(ellipsize(
        &crate::tf!("Legend: {}", "Légende : {}", signs.join(" · ")),
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
            crate::tf!(
                "{} blocks • {}/{} aliases",
                "{} blocs • {}/{} alias",
                m.visible.len(),
                m.matching.len(),
                m.all.len()
            )
        } else {
            crate::tf!(
                "Hosts: {}/{}",
                "Hôtes : {}/{}",
                m.visible.len(),
                m.all.len()
            )
        };
        let groups = crate::tf!("{} groups", "{} groupes", m.group_projection.group_count);
        let rows = crate::tf!(
            "Selected: {position}/{} • Rows: {first}-{last}",
            "Sélection : {position}/{} • Lignes : {first}-{last}",
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
                Line::from(crate::tf!(
                    "{} blocks • {groups}",
                    "{} blocs • {groups}",
                    m.visible.len()
                )),
                Line::from(crate::tf!(
                    "{}/{} aliases",
                    "{}/{} alias",
                    m.matching.len(),
                    m.all.len()
                )),
            ]
        } else {
            vec![Line::from(hosts), Line::from(groups)]
        };
    }
    if !m.preferences.group_aliases {
        return list_status(width, m.visible.len(), m.all.len(), position, first, last);
    }
    let blocks = crate::tf!("{} blocks", "{} blocs", m.visible.len());
    let aliases = crate::tf!(
        "{}/{} aliases",
        "{}/{} alias",
        m.matching.len(),
        m.all.len()
    );
    let counts = format!("{blocks} • {aliases}");
    let full = crate::tf!(
        "{counts} • Selected: {position}/{} • Rows: {first}-{last}",
        "{counts} • Sélection : {position}/{} • Lignes : {first}-{last}",
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
        crate::tf!(
            "Hosts: {counts} • Selected: {position}/{count} • Rows: {range}",
            "Hôtes : {counts} • Sélection : {position}/{count} • Lignes : {range}"
        ),
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
    let enter = if group {
        crate::t!("open/close group", "déplier")
    } else {
        crate::t!("connect", "connexion")
    };
    let (palette, reload, connect) = (k(A::Palette), k(A::Reload), k(A::Connect));
    let (info, help, quit) = (k(A::Info), k(A::Help), k(A::Quit));
    let (favorite, add, edit, delete) = (k(A::Favorite), k(A::Add), k(A::Edit), k(A::Delete));
    let (moving, tunnel, test) = (k(A::Move), k(A::Tunnel), k(A::Test));
    let (file, all, method) = (
        k(A::SourceFilter),
        k(A::ClearSourceFilter),
        k(A::Availability),
    );
    // French rows are worded to fit the same widths as the English ones.
    let full = if search_focus {
        vec![crate::tf!(
            "{palette}: actions • {reload}: reload • ↑/↓: select • {connect}: {enter} • Tab/Esc: table, filter kept",
            "{palette} : actions • {reload} : recharger • ↑/↓ : choisir • {connect} : {enter} • Tab/Échap : tableau, filtre conservé"
        )]
    } else {
        vec![
            crate::tf!(
                "{palette}: actions • {reload}: reload • ↑/↓: navigate • {connect}: {enter} • Tab: search • {info}: info • {help}: help • {quit}: quit",
                "{palette} : actions • {reload} : relire • ↑/↓ : choisir • {connect} : {enter} • Tab : chercher • {info} : fiche • {help} : aide • {quit} : quitter"
            ),
            crate::tf!(
                "{favorite}: favorite • {add}: add • {edit}: edit • {delete}: del • {moving}: move • {tunnel}: tunnel • {test}: test • {file}/{all}: file/all",
                "{favorite} : favori • {add} : ajouter • {edit} : modifier • {delete} : suppr. • {moving} : déplacer • {tunnel} : tunnel • {test} : tester • {file}/{all} : fichier/tous"
            ),
        ]
    };
    let medium = if search_focus {
        vec![crate::tf!(
            "{palette}: actions • ↑↓: select • {connect}: {enter} • Tab/Esc: table",
            "{palette} : actions • ↑↓ : choisir • {connect} : {enter} • Tab/Échap : tableau"
        )]
    } else {
        vec![
            crate::tf!(
                "{palette}: actions • ↑↓: select • {connect}: {enter} • Tab: search • {quit}: quit",
                "{palette} : actions • ↑↓ : choisir • {connect} : {enter} • Tab : filtrer • {quit} : quitter"
            ),
            crate::tf!(
                "{file}/{all}: file/all • {add}/{edit}/{delete}/{moving}: manage • {tunnel}: tunnel • {test}: test • {method}: method • {help}: help",
                "{file}/{all} : fichier/tous • {add}/{edit}/{delete}/{moving} : gérer • {tunnel} : tunnel • {test} : tester • {help} : aide"
            ),
        ]
    };
    let compact = vec![crate::tf!(
        "{palette}: actions • ↑↓ • {connect}",
        "{palette} : actions • ↑↓ • {connect}"
    )];
    // The width picks the richest legend, then a legend whose rows would be cut
    // gives way to the next one: a longer language never loses the end of a row.
    let first = if width >= 105 {
        0
    } else if width >= 65 {
        1
    } else {
        2
    };
    let fits = |line: &String| line.width() <= usize::from(width);
    let mut lines = Vec::new();
    for variant in [full, medium, compact].into_iter().skip(first) {
        // A legend is kept when its first row fits; a later row that would be
        // cut is left out rather than shown without its end.
        let whole = variant.first().is_some_and(fits);
        lines = variant;
        if whole {
            lines.retain(fits);
            break;
        }
    }
    // The two-step prefix is announced wherever a row still has room for it.
    if let Some(prefix) = keymap.prefix() {
        let hint = crate::tf!(" • {prefix}: quick", " • {prefix} : rapide");
        let room = |line: &&mut String| line.width() + hint.width() <= usize::from(width);
        if width >= 65 {
            if let Some(line) = lines.iter_mut().rev().find(room) {
                line.push_str(&hint);
            }
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
    let title = crate::tf!(
        "Quick actions — {}",
        "Actions rapides — {}",
        host.as_ref().map_or(
            crate::t!("no host selected", "aucun hôte sélectionné"),
            |h| &h.name
        )
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
        Paragraph::new(notice.map_or(String::new(), |reason| {
            crate::tf!("Unavailable: {reason}", "Indisponible : {reason}")
        }))
        .style(Style::default().fg(Color::Yellow)),
        parts[1],
    );
    f.render_widget(
        Paragraph::new(crate::t!(
            "Letter: run • Esc: cancel",
            "Lettre : lancer • Échap : annuler"
        ))
        .style(Style::default().fg(GRAY)),
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
        crate::tf!("Hosts > Edit {}", "Hôtes > Modifier {}", original.name)
    } else if let Some(source) = form.copy_source() {
        crate::tf!(
            "Hosts > Duplicate {} — enter a NEW name",
            "Hôtes > Dupliquer {} — saisir un NOUVEAU nom",
            source.name
        )
    } else {
        crate::t!("Hosts > Add host", "Hôtes > Ajouter un hôte").to_owned()
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
                &crate::tf!(
                    "{} field(s) to fix — {}",
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
        crate::t!("Alias *", "Alias *"),
        crate::t!("Address *", "Adresse *"),
        crate::t!("User", "Utilisateur"),
        "Port",
        crate::t!("SSH key", "Clé SSH"),
        "ProxyJump",
        "ProxyCommand",
        crate::t!("SSH options", "Options SSH"),
        "Tags",
        crate::t!("Remote command", "Commande distante"),
        "RequestTTY",
    ];
    for stop in form.stops() {
        let focused = stop == current;
        if focused {
            focus_y = lines.len();
        }
        match stop {
            Stop::Control(Control::File) => {
                let button = nav::button(crate::t!("Change", "Changer"), focused);
                // A path may use the whole row: its button follows it directly.
                let room = usize::from(parts[1].width)
                    .saturating_sub(marker(focused).width() + 10 + button.width() + 1);
                lines.push(Line::from(vec![
                    Span::styled(
                        crate::tf!("{}File: ", "{}Fichier : ", marker(focused)),
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
                        ellipsize(
                            &crate::tf!("  File: {}", "  Fichier : {}", form.target.display()),
                            width,
                        ),
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
                            nav::button(
                                crate::t!("Choose", "Choisir"),
                                current == Stop::Control(Control::KeyPicker),
                            )
                        }),
                    ),
                    FormField::Name(0) if form.original.is_some() => (
                        &form.names[0],
                        crate::t!("Alias *", "Alias *").to_owned(),
                        None,
                    ),
                    FormField::Name(i) => (
                        &form.names[i],
                        crate::tf!(
                            "  Alias {}",
                            "  Alias {}",
                            i + 1 + usize::from(form.original.is_none())
                        ),
                        Some(nav::button(
                            crate::t!("Remove", "Retirer"),
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
                            crate::tf!(
                                "  also: {identity} (read-only, kept)",
                                "  aussi : {identity} (lecture seule, conservée)"
                            ),
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
                                line.spans.iter().any(|span| {
                                    span.content.contains(crate::t!("Choose", "Choisir"))
                                })
                            })
                            .unwrap_or(focus_y);
                    }
                }
            }
            Stop::Control(Control::Section(section)) => {
                let open = form.is_open(section);
                let summary = match section {
                    Section::Aliases => {
                        crate::tf!(
                            "Additional aliases ({})",
                            "Alias supplémentaires ({})",
                            form.extra_aliases().len()
                        )
                    }
                    Section::Advanced => crate::tf!(
                        "Advanced options ({} set)",
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
                            crate::t!("Tags and note", "Tags et note").to_owned()
                        } else {
                            crate::tf!(
                                "Tags and note — inherited: {inherited}",
                                "Tags et note — hérités : {inherited}"
                            )
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
                    nav::button(crate::t!("+ Add an alias", "+ Ajouter un alias"), focused),
                ]));
                lines.push(Line::styled(
                    ellipsize(
                        crate::t!("    Aliases share this block; favorites, notes and connections target one exact alias.", "    Les alias partagent ce bloc ; favoris, notes et connexions visent un alias précis."),
                        width,
                    ),
                    Style::default().fg(GRAY),
                ));
            }
            Stop::Control(Control::Note) => {
                let preview = match note.as_deref() {
                    Some("") | None => crate::t!("(empty)", "(vide)").to_owned(),
                    Some(text) => text.to_owned(),
                };
                let button = nav::button(crate::t!("Edit", "Modifier"), focused);
                let head = format!(
                    "{}  {:<16}",
                    marker(focused),
                    crate::t!("Personal note", "Note personnelle")
                );
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
                    nav::button(
                        crate::t!("Preview the changes", "Prévisualiser les modifications"),
                        focused,
                    ),
                    Span::raw("   "),
                    nav::button(
                        crate::t!("Cancel", "Annuler"),
                        current == Stop::Control(Control::Cancel),
                    ),
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
            crate::t!(
                "Enter: choose another file; the draft is kept",
                "Entrée : choisir un autre fichier ; le brouillon est conservé"
            )
        }
        Stop::Control(Control::KeyPicker) => {
            crate::t!(
                "Enter or F9: choose an existing key; only this field is filled",
                "Entrée ou F9 : choisir une clé existante ; seul ce champ est rempli"
            )
        }
        Stop::Control(Control::Section(_)) => crate::t!(
            "Enter or Space: unfold / fold",
            "Entrée ou Espace : déplier / replier"
        ),
        Stop::Control(Control::AddAlias) => crate::t!(
            "Enter: add an alias to this block (Alt+A)",
            "Entrée : ajouter un alias à ce bloc (Alt+A)"
        ),
        Stop::Control(Control::RemoveAlias(_)) => crate::t!(
            "Enter: remove this alias from the block (Alt+D)",
            "Entrée : retirer cet alias du bloc (Alt+D)"
        ),
        Stop::Control(Control::Note) => {
            crate::t!(
                "Enter: edit the note, kept in the ssho data, outside the SSH file",
                "Entrée : éditer la note, conservée dans les données de ssho, hors du fichier SSH"
            )
        }
        Stop::Control(Control::Preview) => {
            crate::t!(
                "Enter: show the preview; nothing is written before \"Save\"",
                "Entrée : afficher l'aperçu ; rien n'est écrit avant « Enregistrer »"
            )
        }
        Stop::Control(Control::Cancel) => {
            crate::t!("Enter: close this form", "Entrée : fermer ce formulaire")
        }
    };
    let mut footer = vec![];
    if !form.error.is_empty() {
        footer.push(Line::styled(
            crate::tf!("Error: {}", "Erreur : {}", form.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::styled(hint, Style::default().fg(GRAY)));
    footer.push(Line::styled(
        crate::t!("Tab/↓ next • Shift+Tab/↑ previous • Enter: next field / activate • Ctrl+S: preview • Esc: cancel • * required", "Tab/↓ suivant • Maj+Tab/↑ précédent • Entrée : champ suivant / activer • Ctrl+S : prévisualiser • Échap : annuler • * requis"),
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
        block(
            crate::t!(
                "Port Forwarding Setup",
                "Configuration du transfert de port"
            ),
            true,
        )
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
        Paragraph::new(crate::tf!(
            "Host: {} • {} • {} (Ctrl+T)",
            "Hôte : {} • {} • {} (Ctrl+T)",
            form.host,
            form.mode.label(),
            if form.dirty() {
                crate::t!("Unsaved changes", "Modifications non enregistrées")
            } else if form.original_name.is_some() {
                crate::t!("Profile saved", "Profil enregistré")
            } else {
                crate::t!("Draft", "Brouillon")
            }
        ))
        .style(accent()),
        parts[0],
    );
    let mut lines = vec![];
    let mut focus_y = field_lines(
        &mut lines,
        crate::t!(
            "Profile name (required to save only):",
            "Nom du profil (requis pour enregistrer seulement) :"
        )
        .into(),
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
        crate::t!("Forward Type:", "Type de transfert :"),
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
            0 => crate::t!(
                "Local forwarding: ssh -L [bind_address:]local_port:remote_host:remote_port",
                "Transfert local : ssh -L [adresse:]port_local:hôte_distant:port_distant"
            ),
            1 => crate::t!(
                "Remote forwarding: ssh -R [bind_address:]remote_port:local_host:local_port",
                "Transfert distant : ssh -R [adresse:]port_distant:hôte_local:port_local"
            ),
            _ => crate::t!(
                "Dynamic forwarding (SOCKS proxy): ssh -D [bind_address:]port",
                "Transfert dynamique (proxy SOCKS) : ssh -D [adresse:]port"
            ),
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
            crate::t!(
                "Local Port (listens on this machine):",
                "Port local (écoute sur ce poste) :"
            ),
            crate::t!(
                "Remote Host (destination, seen from the server):",
                "Hôte distant (destination, vue du serveur) :"
            ),
            crate::t!("Remote Port (destination):", "Port distant (destination) :"),
            crate::t!(
                "Bind Address (listening address, optional):",
                "Adresse d'écoute (facultative) :"
            ),
        ],
        1 => [
            "",
            crate::t!(
                "Remote Port (listens on the SSH server):",
                "Port distant (écoute sur le serveur SSH) :"
            ),
            crate::t!(
                "Local Host (destination, seen from this machine):",
                "Hôte local (destination, vue de ce poste) :"
            ),
            crate::t!("Local Port (destination):", "Port local (destination) :"),
            crate::t!(
                "Bind Address (listening address, optional):",
                "Adresse d'écoute (facultative) :"
            ),
        ],
        _ => [
            "",
            crate::t!(
                "SOCKS Port (listens on this machine):",
                "Port SOCKS (écoute sur ce poste) :"
            ),
            "",
            "",
            crate::t!(
                "Bind Address (listening address, optional):",
                "Adresse d'écoute (facultative) :"
            ),
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
        crate::t!("Launch mode:", "Mode de lancement :"),
        if form.focus == F::MODE {
            accent()
        } else {
            Style::default()
        },
    ));
    lines.push(Line::from(format!(
        "{}{}   {}",
        if form.focus == F::MODE { "> " } else { "  " },
        choice(
            crate::t!("Shell session", "Session shell"),
            form.mode == LaunchMode::Shell
        ),
        choice(
            crate::t!("Tunnel only", "Tunnel seul"),
            form.mode == LaunchMode::TunnelOnly
        ),
    )));
    lines.push(Line::styled(
        ellipsize(
            if form.mode == LaunchMode::Shell {
                crate::t!("Opens a shell with the forwarding; the tunnel ends when you leave that shell.", "Ouvre un shell avec le transfert ; le tunnel s'arrête quand vous quittez ce shell.")
            } else {
                crate::t!("Runs in the background, owned by ssho: follow or stop it in F7; it stops when ssho quits.", "Tourne en arrière-plan, possédé par ssho : suivi et arrêt dans F7 ; il s'arrête quand ssho quitte.")
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
        nav::button(crate::t!("Launch", "Lancer"), form.focus == F::LAUNCH),
        Span::raw("  "),
        nav::button(
            crate::t!("Save profile", "Enregistrer le profil"),
            form.focus == F::SAVE,
        ),
        Span::raw("  "),
        nav::button(crate::t!("Cancel", "Annuler"), form.focus == F::CANCEL),
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
            crate::tf!("Error: {}", "Erreur : {}", form.error),
            Style::default().fg(Color::Red),
        ));
    }
    footer.push(Line::styled(
        crate::t!(
            "Tab/↓: next field • Shift+Tab/↑: previous field • ←/→: type, mode or button",
            "Tab/↓ : champ suivant • Maj+Tab/↑ : précédent • ←/→ : type, mode ou bouton"
        ),
        Style::default().fg(GRAY),
    ));
    footer.push(Line::styled(
        crate::t!(
            "Ctrl+S: save profile • Ctrl+L: launch • Ctrl+T: mode • Esc: cancel",
            "Ctrl+S : enregistrer le profil • Ctrl+L : lancer • Ctrl+T : mode • Échap : annuler"
        ),
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
        label: crate::t!("All files", "Tous les fichiers").into(),
        aside: String::new(),
        detail: vec![
            crate::t!("No filter by file.", "Aucun filtre par fichier.").into(),
            crate::tf!(
                "{} host(s) in the configuration",
                "{} hôte(s) dans la configuration",
                m.all.len()
            ),
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
            title: crate::t!("Filter by source file", "Filtrer par fichier source").into(),
            subject: crate::t!(
                "Limit the list to the hosts declared in one file",
                "Limiter la liste aux hôtes déclarés dans un fichier"
            )
            .into(),
            items,
            selected: *selected,
            error: "",
            apply: crate::t!("apply", "appliquer"),
            query: Some(&m.selector_query),
            empty: crate::t!(
                "No SSH configuration file.",
                "Aucun fichier de configuration SSH."
            ),
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
                    crate::tf!("File: {}", "Fichier : {}", host.source_file.display()),
                    crate::t!("Connection, favorite and note target this exact alias; the SSH block is shared.", "Connexion, favori et note visent cet alias précis ; le bloc SSH est partagé.")
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
            title: crate::t!(
                "Choose an alias — exact connection target",
                "Choisir un alias — cible exacte de connexion"
            )
            .into(),
            subject: crate::tf!(
                "Block of {primary}: {} aliases",
                "Bloc de {primary} : {} alias",
                hosts.len()
            ),
            items,
            selected: *selected,
            error: "",
            apply: crate::t!("choose", "choisir"),
            query: Some(&m.selector_query),
            empty: crate::t!("This block has no alias.", "Ce bloc n'a aucun alias."),
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
            crate::t!(
                "Change the destination file",
                "Changer le fichier de destination"
            )
            .to_owned(),
            crate::t!(
                "The draft is kept; only its target file changes",
                "Le brouillon est conservé ; seul son fichier cible change"
            )
            .to_owned(),
        )
    } else if let Some(host) = host {
        (
            crate::tf!(
                "Hosts > Move {} — destination file",
                "Hôtes > Déplacer {} — fichier de destination",
                host.name
            ),
            crate::tf!("Source: {}", "Source : {}", host.source_file.display()),
        )
    } else if let Some(source) = duplicate.as_deref() {
        (
            crate::tf!(
                "Hosts > Duplicate {} — destination file",
                "Hôtes > Dupliquer {} — fichier de destination",
                source.name
            ),
            crate::t!(
                "The file of the original is a possible destination too",
                "Le fichier de l'original est aussi une destination possible"
            )
            .to_owned(),
        )
    } else {
        (
            crate::t!(
                "Hosts > Add host — destination file",
                "Hôtes > Ajouter un hôte — fichier de destination"
            )
            .to_owned(),
            crate::t!(
                "SSH file in which the new block will be written",
                "Fichier SSH dans lequel le nouveau bloc sera écrit"
            )
            .to_owned(),
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
            apply: crate::t!("choose", "choisir"),
            query: Some(&m.selector_query),
            empty: crate::t!(
                "No SSH configuration file.",
                "Aucun fichier de configuration SSH."
            ),
        },
    );
}
fn info(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Info(h) = &m.view else {
        return;
    };
    let area = panel(area, 96, 34);
    let title = crate::tf!(
        "SSH Host Information: {}",
        "Fiche de l'hôte SSH : {}",
        h.name
    );
    let border = block(&title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    // Under the content: the named actions, the keys, then what the last key did.
    let notice: Vec<Line> = if let Some((label, value)) = &m.copied {
        // Each on its own row: the value can be selected by hand if the
        // terminal ignored the clipboard request.
        vec![
            Line::styled(
                crate::tf!(
                    "Copy requested from the terminal (OSC 52): {label}",
                    "Copie demandée au terminal (OSC 52) : {label}"
                ),
                Style::default().fg(Color::Green),
            ),
            Line::styled(
                crate::t!(
                    "If pasting gives nothing, select the value below:",
                    "Si le collage reste vide, sélectionnez la valeur ci-dessous :"
                ),
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
    for (index, label) in details::sheet_actions().into_iter().enumerate() {
        buttons.push(nav::button(label, m.sheet_focus == Some(index)));
        buttons.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(buttons),
            Line::styled(
                crate::t!("↑↓/PgUp/PgDn: scroll • Tab/←→: actions • F5: reload • F2: other actions • Esc: back", "↑↓/PgUp/PgDn défiler • Tab/←→ : actions • F5 : recharger • F2 : autres actions • Échap : retour"),
                Style::default().fg(GRAY),
            ),
        ]),
        parts[1],
    );
    f.render_widget(Paragraph::new(notice).wrap(Wrap { trim: false }), parts[2]);
    if !m.error.is_empty() {
        f.render_widget(
            Paragraph::new(crate::tf!("Error: {}", "Erreur : {}", m.error))
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
            title: crate::tf!("Copy a value — {host}", "Copier une valeur — {host}"),
            subject: crate::t!(
                "A single value is copied; nothing is exported",
                "Une seule valeur est copiée ; rien n'est exporté"
            )
            .into(),
            items,
            selected: *selected,
            error: "",
            apply: crate::t!("copy", "copier"),
            query: None,
            empty: crate::t!("No value to copy.", "Aucune valeur à copier."),
        },
    );
}
fn help(f: &mut Frame, m: &mut Model, area: Rect) {
    // Wide enough for the longest line of either language in each column.
    let area = panel(area, 126, 48);
    let border = block(crate::t!("SSHO - Commands", "SSHO - Commandes"), true);
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
                        crate::t!("F2 only", "F2 seul").to_owned()
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
            crate::t!("Search", "Recherche"),
            crate::t!(
                "Words/phrases and fields combine with AND/ET",
                "Mots, phrases et champs se combinent par AND/ET"
            ),
            "tag:work AND user:deploy",
            "tag:work ET is:online",
            "file:\"Team Hosts\" is:favorite",
            crate::t!(
                "Tab/Shift+Tab switch search / table",
                "Tab/Maj+Tab : recherche / tableau"
            ),
            crate::t!(
                "Esc    leave search, filter kept / back one level",
                "Échap  quitter la saisie, filtre conservé / retour"
            ),
            crate::t!(
                "Backspace (table) or Ctrl+U (input) clears the search",
                "Retour arrière (tableau) ou Ctrl+U (saisie) : effacer"
            ),
        ]
        .map(String::from),
    );
    let mut right = section(&[C::Tools, C::Application]);
    if let Some(prefix) = keymap.prefix() {
        right.pop();
        right.push(crate::tf!(
            "{prefix:<9} Quick actions (then a letter)",
            "{prefix:<9} Actions rapides (puis une lettre)"
        ));
        right.push(String::new());
    }
    right.extend(
        [
            crate::t!("Signs of the list", "Signes de la liste"),
            crate::t!(
                "[x] batch target  * favorite  ≡ note  ▼ ▶ group",
                "[x] cible de lot  * favori  ≡ note  ▼ ▶ groupe"
            ),
            crate::t!(
                "+ online  ! offline  ~ in progress  ? unknown",
                "+ en ligne  ! hors ligne  ~ en cours  ? inconnu"
            ),
            crate::t!(
                "a→b active alias of a block  +N alias: other aliases",
                "a→b alias actif d'un bloc  +N alias : autres alias du bloc"
            ),
            "",
            crate::t!("Navigation", "Navigation"),
            crate::t!(
                "↑↓     select (Ctrl+P/N in search)",
                "↑↓     choisir (Ctrl+P/N dans la recherche)"
            ),
            crate::t!(
                "Group Enter/Space: expand/collapse; ←/→ parent/open",
                "Groupe Entrée/Espace : déplier ; ←/→ parent/ouvrir"
            ),
            crate::t!(
                "SSH exit returns here with its result",
                "La fin de SSH revient ici avec son résultat"
            ),
            crate::t!(
                "Alt+↑↓ scroll details (Ctrl+PgUp/PgDn; width >=110)",
                "Alt+↑↓ défiler les détails (Ctrl+PgUp/PgDn ; largeur >=110)"
            ),
            "",
            crate::t!("Forms and reviews", "Formulaires et aperçus"),
            crate::t!(
                "Ctrl+S preview/save • Ctrl+L launch tunnel/batch review",
                "Ctrl+S aperçu/enregistrer • Ctrl+L lancer tunnel / vérifier lot"
            ),
            crate::t!(
                "Host form: Tab/↑↓ fields, sections, buttons; F9 keys",
                "Formulaire d'hôte : Tab/↑↓ champs, sections, boutons ; F9 clés"
            ),
            crate::t!(
                "Enter: next field or activate; never submits a field",
                "Entrée : champ suivant ou activer ; ne soumet jamais"
            ),
            crate::t!(
                "Alt+A/D add/remove an additional alias",
                "Alt+A/D ajouter/retirer un alias supplémentaire"
            ),
            crate::t!(
                "Ctrl+T forwarding: shell session / tunnel only",
                "Ctrl+T transfert : session shell / tunnel seul"
            ),
            crate::t!(
                "Ctrl+A/E start/end of input",
                "Ctrl+A/E début/fin de saisie"
            ),
            crate::t!(
                "Review: Tab to the button, Ctrl+S confirms, Esc returns",
                "Aperçu : Tab jusqu'au bouton, Ctrl+S confirme, Échap revient"
            ),
            crate::t!(
                "Modified draft: Tab chooses, Enter confirms, Esc resumes",
                "Brouillon modifié : Tab choisit, Entrée valide, Échap reprend"
            ),
            crate::t!(
                "Esc/Ctrl+C: back one level; quitting is asked from the list",
                "Échap/Ctrl+C : retour d'un niveau ; quitter se demande à la liste"
            ),
            crate::t!(
                "Columns: Space toggle, Shift+↑↓ order, r reset",
                "Colonnes : Espace cocher, Maj+↑↓ ordre, r réinitialiser"
            ),
            crate::t!(
                "Batch results: s cancels the batch",
                "Résultats d'un lot : s annule le lot"
            ),
            crate::t!(
                "Keys: F4 full details/error; ↑↓ PgUp/Dn Home/End scroll",
                "Clés : F4 détails/erreur ; ↑↓ PgUp/Dn Début/Fin défiler"
            ),
            crate::t!(
                "ICMP local: no bastion / SSH auth",
                "ICMP local : ni bastion ni authentification SSH"
            ),
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
            vec![crate::t!(
                "No line of the help matches.",
                "Aucune ligne de l'aide ne correspond."
            )
            .to_owned()]
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
            crate::tf!(
                "File: {} • {}/F2: clear",
                "Fichier : {} • {}/F2 : effacer",
                path.display(),
                keymap.hint(PaletteAction::ClearSourceFilter)
            ),
        );
    }
    if !keymap.conflicts.is_empty() {
        text.push(
            crate::t!(
                "Shortcuts refused (key_bindings in config.json):",
                "Raccourcis refusés (key_bindings de config.json) :"
            )
            .into(),
        );
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
        text.push(
            crate::t!(
                "Configuration warnings (FileTags ignored):",
                "Avertissements de configuration (FileTags ignorés) :"
            )
            .into(),
        );
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
            crate::tf!(
                "Search: {}{} • Enter: keep • Esc: clear",
                "Recherche : {}{} • Entrée : garder • Échap : effacer",
                m.selector_query.value,
                if m.help_search { "_" } else { "" }
            )
        } else {
            crate::tf!(
                "/: search • ↑↓/PgUp/PgDn: scroll • Esc, {}, q or Enter: close",
                "/ : rechercher • ↑↓/PgUp/PgDn : défiler • Échap, {}, q ou Entrée : fermer",
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
    let border = block(crate::t!("Personal note", "Note personnelle"), true);
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
        Span::styled(crate::tf!("Host: {}", "Hôte : {}", editor.host), accent()),
        Span::raw("  "),
        if editor.dirty() {
            Span::styled(
                crate::t!("● Modified", "● Modifiée"),
                Style::default().fg(Color::Yellow),
            )
        } else {
            Span::styled(crate::t!("Saved", "Enregistrée"), Style::default().fg(GRAY))
        },
    ])];
    if inner.height >= 10 {
        if let Some(source) = source {
            header.push(Line::styled(
                ellipsize(
                    &crate::tf!(
                        "SSH file of the host: {source}",
                        "Fichier SSH de l'hôte : {source}"
                    ),
                    usize::from(inner.width),
                ),
                Style::default().fg(GRAY),
            ));
        }
        header.push(Line::styled(
            ellipsize(
                &crate::tf!("ssho data: {data}", "Donnée de ssho : {data}"),
                usize::from(inner.width),
            ),
            Style::default().fg(GRAY),
        ));
        header.push(Line::styled(
            ellipsize(
                crate::t!(
                    "Never written to the SSH file; not to be confused with a tag.",
                    "Jamais écrite dans le fichier SSH ; à ne pas confondre avec un tag."
                ),
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
            crate::tf!("Error: {}", "Erreur : {}", editor.error),
            Style::default().fg(Color::Red),
        ));
    }
    if footer_height > 1 {
        footer.push(Line::from(vec![
            nav::button(crate::t!("Save", "Enregistrer"), editor.button == Some(0)),
            Span::raw("  "),
            nav::button(crate::t!("Cancel", "Annuler"), editor.button == Some(1)),
        ]));
    }
    footer.push(Line::styled(
        crate::t!("Enter: newline • Tab: buttons • Ctrl+S: save • Esc/Ctrl+C: cancel", "Entrée : nouvelle ligne • Tab : boutons • Ctrl+S : enregistrer • Échap/Ctrl+C : annuler"),
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
        Paragraph::new(crate::tf!("Host: {}", "Hôte : {}", list.host)).block(block(
            crate::t!("Tunnel profiles", "Profils de tunnels"),
            true,
        )),
        parts[0],
    );
    if let Some(name) = &list.delete {
        f.render_widget(
            Paragraph::new(crate::tf!(
                "Delete profile '{name}'?\nEnter: confirm • Esc: cancel",
                "Supprimer le profil « {name} » ?\nEntrée : confirmer • Échap : annuler"
            ))
            .block(block(
                crate::t!("Delete tunnel profile", "Supprimer un profil de tunnel"),
                true,
            )),
            parts[1],
        );
    } else {
        let profiles = m.tunnel_profiles.get(&list.host);
        if profiles.is_none_or(BTreeMap::is_empty) {
            f.render_widget(
                Paragraph::new(crate::t!(
                    "No profiles. Press a to create one; f resumes last forwarding.",
                    "Aucun profil. a pour en créer un ; f reprend le dernier transfert."
                )),
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
                .header(
                    Row::new([
                        crate::t!("Name", "Nom"),
                        "Type",
                        crate::t!("Mode", "Mode"),
                        crate::t!("Forwarding", "Transfert"),
                    ])
                    .style(accent()),
                )
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
        profiles::actions().len() - 1
    } else {
        list.button
    };
    let mut buttons = vec![];
    for (index, label) in profiles::actions().into_iter().enumerate() {
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
                crate::t!("↑↓: profile • ←/→: action • Enter: run it • f: last forwarding draft • Esc: back", "↑↓ : profil • ←/→ : action • Entrée : la lancer • f : dernier transfert • Échap : retour"),
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
    let items = groups::modes()
        .iter()
        .map(|(mode, label)| select::Item {
            label: (*label).to_owned(),
            aside: String::new(),
            detail: vec![match mode {
                GroupMode::Flat => crate::t!("Every host in a single sorted list.", "Tous les hôtes dans une seule liste triée."),
                GroupMode::Files => crate::t!("One collapsible group per configuration file.", "Un groupe repliable par fichier de configuration."),
                GroupMode::FileTags => {
                    crate::t!("By file, then by main tag: first own tag, else first inherited tag, else \"No tag\".", "Par fichier, puis par tag principal : premier tag propre, sinon premier tag hérité, sinon « Sans tag ».")
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
            title: crate::t!(
                "Groups — presentation of the list",
                "Groupes — présentation de la liste"
            )
            .into(),
            subject: crate::t!(
                "Saved preference; collapsed groups stay those of the session",
                "Préférence enregistrée ; les replis restent ceux de la session"
            )
            .into(),
            items,
            selected: *selected,
            error: &m.error,
            apply: crate::t!("save", "enregistrer"),
            query: None,
            empty: "",
        },
    );
}

pub(super) fn group_content(key: &groups::Key, count: usize, aliases: bool) -> Vec<Line<'static>> {
    let (path, tag) = match key {
        groups::Key::File(path) => (path, None),
        groups::Key::Tag(path, tag) => (
            path,
            Some(tag.as_deref().unwrap_or(crate::t!("No tag", "Sans tag"))),
        ),
    };
    let mut lines = vec![
        Line::from(crate::t!("Source:", "Source :")),
        Line::from(path.display().to_string()),
    ];
    if let Some(tag) = tag {
        lines.push(Line::from(crate::tf!(
            "Primary tag: {tag}",
            "Tag principal : {tag}"
        )));
    }
    // Why these hosts are together, in the terms of the grouping mode.
    lines.push(Line::from(match tag {
        None => crate::t!("Criterion: hosts declared in this configuration file.", "Critère : hôtes déclarés dans ce fichier de configuration."),
        Some(_) => {
            crate::t!("Criterion: hosts of this file whose main tag is this one (first own tag, else first inherited tag, else \"No tag\").", "Critère : hôtes de ce fichier dont le tag principal est celui-ci (premier tag propre, sinon premier tag hérité, sinon « Sans tag »).")
        }
    }));
    lines.extend([
        Line::from(crate::tf!("{count} matching {}", "{count} {} correspondant(s)", if aliases { crate::t!("blocks", "blocs") } else { crate::t!("hosts", "hôtes") })),
        Line::from(crate::t!("In list: Enter/Space toggles group; Left closes or selects parent; Right opens or selects child.", "Dans la liste : Entrée/Espace déplie le groupe ; Gauche replie ou va au parent ; Droite ouvre ou va à l'enfant.")),
        Line::from(crate::t!("Select a host for SSH and host actions.", "Sélectionnez un hôte pour SSH et les actions d'hôte.")),
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
    f.render_widget(
        Paragraph::new(crate::t!("Group info", "Fiche de groupe")).style(accent()),
        parts[0],
    );
    m.page = usize::from(parts[1].height).max(1);
    details::render_rows(f, parts[1], lines, &mut m.scroll);
    f.render_widget(
        Paragraph::new(crate::t!(
            "↑↓: scroll • Esc: back",
            "↑↓ : défiler • Échap : retour"
        )),
        parts[2],
    );
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
                    crate::t!("Checks that an SSH server answers: direct handshake, or a check through the configured proxy.", "Vérifie qu'un serveur SSH répond : poignée de main directe, ou contrôle via le proxy configuré.").to_owned(),
                    crate::t!("It does not prove authentication.", "Ne prouve pas l'authentification.").to_owned(),
                ],
                AvailabilityMethod::Icmp => vec![
                    crate::t!("Ping from this machine: network availability only, without bastion or SSH authentication.", "Ping depuis ce poste : disponibilité réseau seulement, sans bastion ni authentification SSH.").to_owned(),
                    crate::t!("\"Unknown\" state when the ping command is missing or refused.", "État « inconnu » si la commande ping manque ou est refusée.").to_owned(),
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
            title: crate::t!("Availability method", "Méthode de disponibilité").into(),
            subject: crate::t!(
                "What the state shown in the list measures",
                "Ce que mesure l'état affiché dans la liste"
            )
            .into(),
            items,
            selected,
            error: &m.error,
            apply: crate::t!("save", "enregistrer"),
            query: None,
            empty: "",
        },
    );
}

/// Each language is named in itself, and the screen says what it is in both,
/// so the way back is readable whatever is currently displayed.
fn language(f: &mut Frame, m: &Model, area: Rect) {
    let View::Language { selected } = m.view else {
        return;
    };
    let items = crate::i18n::Language::ALL
        .into_iter()
        .map(|language| select::Item {
            label: language.name().to_owned(),
            aside: String::new(),
            detail: vec![match language {
                crate::i18n::Language::En => "The whole interface is displayed in English.",
                crate::i18n::Language::Fr => "Toute l'interface est affichée en français.",
            }
            .to_owned()],
            current: language == m.preferences.language,
            search: String::new(),
        })
        .collect();
    select::draw(
        f,
        inset(area),
        select::Spec {
            // Both languages on purpose: the screen must be readable from either.
            title: "Language / Langue de l'interface".into(),
            subject: crate::t!(
                "Saved in config.json (\"language\"); applied immediately",
                "Enregistrée dans config.json (« language ») ; appliquée aussitôt"
            )
            .into(),
            items,
            selected,
            error: &m.error,
            apply: crate::t!("save", "enregistrer"),
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
        block(crate::t!("Columns", "Colonnes"), true)
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
        .unwrap_or_else(|| crate::t!("group / no host", "groupe / aucun hôte").into());
    f.render_widget(
        Paragraph::new(ellipsize(
            &crate::tf!("Columns • {target}", "Colonnes • {target}"),
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
            crate::t!("fixed", "fixe")
        } else if hidden {
            crate::t!("hidden: width", "masquée : largeur")
        } else if active {
            crate::t!("shown", "affichée")
        } else {
            crate::t!("off", "désactivée")
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
    for (index, label) in columns::actions().into_iter().enumerate() {
        buttons.push(nav::button(label, editor.button == Some(index)));
        buttons.push(Span::raw(" "));
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(ellipsize(
                &crate::tf!(
                    "Shown at this width: {}",
                    "Affichées à cette largeur : {}",
                    names(visible)
                ),
                usize::from(parts[3].width),
            )),
            Line::styled(
                ellipsize(
                    &if waiting.is_empty() {
                        crate::t!(
                            "Every active column fits.",
                            "Toutes les colonnes actives tiennent."
                        )
                        .to_owned()
                    } else {
                        crate::tf!(
                            "Active but short of room: {}",
                            "Actives mais sans place : {}",
                            names(waiting)
                        )
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
            crate::t!("↑↓ select • Space toggle • Tab: actions\nShift+↑↓ move • r reset\nEnter save • Esc cancel", "↑↓ choisir • Espace cocher • Tab : actions\nMaj+↑↓ déplacer • r réinitialiser\nEntrée enregistrer • Échap annuler"),
        ),
        parts[4],
    );
}

/// Preferences: sections of options with their value, the effect of the focused
/// one, then the buttons. The draft is only written by Apply.
fn preferences(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Preferences(editor) = &m.view else {
        return;
    };
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let border = block(crate::t!("Preferences", "Préférences"), true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let all = prefs::rows();
    let focused = all[editor.row.min(all.len() - 1)];
    let width = usize::from(inner.width).max(1);
    // A value that the row may cut (a path, the launcher) is repeated in full.
    let mut detail: Vec<String> = Vec::new();
    if !focused.editable() {
        detail.push(focused.value(m, editor));
    }
    detail.extend(focused.detail().iter().map(|line| (*line).to_owned()));
    let detail_rows: usize = detail
        .iter()
        .map(|line| select::wrapped_rows(line, width))
        .sum();
    let detail_height = (detail_rows as u16).min((inner.height / 3).max(1));
    let footer = crate::t!(
        "↑↓: option • Space/←→: change • Enter: change or open • Tab: buttons • Ctrl+S: apply • Esc: cancel",
        "↑↓ : option • Espace/←→ : changer • Entrée : changer ou ouvrir • Tab : boutons • Ctrl+S : appliquer • Échap : annuler"
    );
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(detail_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    let label_width = all.iter().map(|row| row.label().width()).max().unwrap_or(0) + 2;
    let mut lines = Vec::new();
    let mut focus_y = 0;
    for (title, rows) in prefs::sections() {
        lines.push(Line::styled(title, accent()));
        for row in rows {
            let on_row = *row == focused && editor.button.is_none();
            if *row == focused {
                focus_y = lines.len();
            }
            let label = format!(
                "{} {:<label_width$}",
                if on_row { ">" } else { " " },
                row.label()
            );
            let mut spans = vec![
                Span::styled(label, if on_row { accent() } else { Style::default() }),
                Span::styled(
                    // A long path keeps its end, where the file name is; the
                    // focused row repeats it in full under the list.
                    group_label(&row.value(m, editor), width.saturating_sub(label_width + 2)),
                    if row.editable() {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(GRAY)
                    },
                ),
            ];
            if editor.changed(*row) {
                spans.push(Span::styled(
                    crate::t!("  ● modified", "  ● modifié"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            lines.push(Line::from(spans));
        }
    }
    let mut scroll = editor.scroll;
    // The title of a section stays above its first option.
    scroll_focus(
        &mut scroll,
        focus_y.saturating_sub(1),
        parts[0].height,
        lines.len(),
        true,
    );
    f.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), parts[0]);
    f.render_widget(
        Paragraph::new(
            detail
                .into_iter()
                .map(Line::from)
                .collect::<Vec<Line<'_>>>(),
        )
        .style(Style::default().fg(GRAY))
        .wrap(Wrap { trim: true }),
        parts[1],
    );
    let mut buttons = Vec::new();
    for (index, label) in prefs::buttons().into_iter().enumerate() {
        if index > 0 {
            buttons.push(Span::raw("  "));
        }
        buttons.push(nav::button(label, editor.button == Some(index)));
    }
    f.render_widget(Paragraph::new(Line::from(buttons)), parts[2]);
    let state = if !editor.notice.is_empty() {
        Span::styled(editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else if editor.dirty() {
        Span::styled(
            crate::t!(
                "● Unsaved changes — nothing is written before Apply",
                "● Modifications non enregistrées — rien n'est écrit avant Appliquer"
            ),
            Style::default().fg(Color::Yellow),
        )
    } else {
        Span::styled(
            crate::t!("No change", "Aucune modification"),
            Style::default().fg(GRAY),
        )
    };
    f.render_widget(Paragraph::new(Line::from(state)), parts[3]);
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[4],
    );
    if let View::Preferences(editor) = &mut m.view {
        editor.scroll = scroll;
    }
}

/// Shortcut editor: one row per action with its keys and where they come from.
fn shortcuts(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Shortcuts(editor) = &m.view else {
        return;
    };
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let border = block(
        crate::t!("Preferences > Shortcuts", "Préférences > Raccourcis"),
        true,
    );
    let inner = border.inner(area);
    f.render_widget(border, area);
    let footer = if editor.capture {
        crate::t!(
            "Waiting for a key — Esc: keep the current one",
            "En attente d'une touche — Échap : garder l'actuelle"
        )
    } else {
        crate::t!(
            "↑↓: action • Tab/←→: buttons • Enter: run the button • Ctrl+S: done • Esc: cancel",
            "↑↓ : action • Tab/←→ : boutons • Entrée : lancer le bouton • Ctrl+S : terminer • Échap : annuler"
        )
    };
    let footer_height = select::wrapped_rows(footer, usize::from(inner.width).max(1)).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    let keymap = Keymap::new(&editor.bindings);
    let actions = prefs::shortcut_actions();
    let selected = editor.selected.min(actions.len() - 1);
    let label_width = actions
        .iter()
        .map(|action| action.label().width())
        .max()
        .unwrap_or(0)
        + 2;
    let key_width = 22;
    f.render_widget(
        Paragraph::new(format!(
            "  {:<label_width$}{:<key_width$}{}",
            "Action",
            crate::t!("Keys", "Touches"),
            crate::t!("Origin", "Origine")
        ))
        .style(accent()),
        parts[0],
    );
    let height = usize::from(parts[1].height).max(1);
    let mut offset = editor.offset.min(actions.len().saturating_sub(height));
    if selected < offset {
        offset = selected;
    } else if selected >= offset + height {
        offset = selected + 1 - height;
    }
    let lines: Vec<Line<'_>> = actions
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(index, action)| {
            let keys = keymap.keys(*action);
            let keys = if keys.is_empty() {
                "—".to_owned()
            } else {
                keys.join(", ")
            };
            let current = index == selected;
            let text = format!(
                "{} {:<label_width$}{:<key_width$}{}",
                if current { "▶" } else { " " },
                action.label(),
                keys,
                editor.state(*action)
            );
            Line::styled(
                text,
                if current && editor.button.is_none() {
                    Style::default().fg(Color::Black).bg(CYAN)
                } else if current {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), parts[1]);
    let notice = if editor.notice.is_empty() {
        Span::styled(
            crate::tf!(
                "{}/{} — Enter: press a new key for the selected action",
                "{}/{} — Entrée : saisir une nouvelle touche pour l'action sélectionnée",
                selected + 1,
                actions.len()
            ),
            Style::default().fg(GRAY),
        )
    } else {
        Span::styled(
            editor.notice.clone(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };
    f.render_widget(
        Paragraph::new(Line::from(notice)).wrap(Wrap { trim: true }),
        parts[2],
    );
    let mut buttons = Vec::new();
    for (index, label) in prefs::shortcut_buttons().into_iter().enumerate() {
        if index > 0 {
            buttons.push(Span::raw("  "));
        }
        buttons.push(nav::button(label, editor.button == Some(index)));
    }
    f.render_widget(Paragraph::new(Line::from(buttons)), parts[3]);
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[4],
    );
    if let View::Shortcuts(editor) = &mut m.view {
        editor.offset = offset;
    }
}

/// Saved views: the view in use, the list, the criteria of the selected one,
/// then the named actions. Naming and deleting are steps of this same screen.
fn saved_views(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Views(editor) = &m.view else {
        return;
    };
    let whole = area;
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let border = block(crate::t!("Saved views", "Vues enregistrées"), true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let width = usize::from(inner.width).max(1);
    let naming = matches!(editor.mode, views::Mode::SaveAs | views::Mode::Rename);
    let selected = m.views.iter().nth(editor.selected);
    let detail: Vec<String> = match (naming, editor.mode, selected) {
        // A new view is made of the current context: show exactly what is kept.
        (true, views::Mode::SaveAs, _) => views::summary(&m.current_view()),
        (_, _, Some((_, view))) => views::summary(view),
        _ => Vec::new(),
    };
    let detail_rows: usize = detail
        .iter()
        .map(|line| select::wrapped_rows(line, width))
        .sum();
    let footer = if naming {
        crate::t!(
            "Enter: to the Save button • Tab: name and buttons • Ctrl+S: save • Esc: cancel",
            "Entrée : vers le bouton Enregistrer • Tab : nom et boutons • Ctrl+S : enregistrer • Échap : annuler"
        )
    } else {
        crate::t!(
            "↑↓: view • Tab/←→: action • Enter: run the action • Esc: back",
            "↑↓ : vue • Tab/←→ : action • Entrée : lancer l'action • Échap : retour"
        )
    };
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    // A refused action (missing source file...) is shown in full, not cut.
    let (notice, notice_style) = if m.error.is_empty() {
        (editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else {
        (
            crate::tf!("Error: {}", "Erreur : {}", m.error),
            Style::default().fg(Color::Red),
        )
    };
    let notice_height = select::wrapped_rows(&notice, width).clamp(1, 4) as u16;
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length((detail_rows as u16).min((inner.height / 3).max(1))),
        Constraint::Length(2),
        Constraint::Length(notice_height),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    let in_use = match m.view_state() {
        Some((name, true)) => crate::tf!(
            "View in use: {name} (modified — Update stores the current list)",
            "Vue en cours : {name} (modifiée — Mettre à jour enregistre la liste actuelle)"
        ),
        Some((name, false)) => crate::tf!("View in use: {name}", "Vue en cours : {name}"),
        None => crate::t!("No view in use", "Aucune vue en cours").to_owned(),
    };
    f.render_widget(Paragraph::new(in_use).style(accent()), parts[0]);
    let lines: Vec<Line<'_>> = if m.views.is_empty() {
        vec![Line::styled(
            crate::t!(
                "No saved view. \"Save current as…\" keeps the search, the filters, the sort, the groups and the columns of the list under a name.",
                "Aucune vue enregistrée. « Enregistrer sous… » garde sous un nom la recherche, les filtres, le tri, les groupes et les colonnes de la liste."
            ),
            Style::default().fg(GRAY),
        )]
    } else {
        let height = usize::from(parts[1].height).max(1);
        let offset = (editor.selected + 1).saturating_sub(height);
        m.views
            .keys()
            .enumerate()
            .skip(offset)
            .take(height)
            .map(|(index, name)| {
                let current = index == editor.selected;
                let used = m.active_view.as_deref() == Some(name.as_str());
                Line::styled(
                    format!(
                        "{} {name}{}",
                        if current { "▶" } else { " " },
                        if used {
                            crate::t!("  ✓ in use", "  ✓ en cours")
                        } else {
                            ""
                        }
                    ),
                    if current {
                        Style::default().fg(Color::Black).bg(CYAN)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect()
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), parts[1]);
    f.render_widget(
        Paragraph::new(
            detail
                .into_iter()
                .map(Line::from)
                .collect::<Vec<Line<'_>>>(),
        )
        .style(Style::default().fg(GRAY))
        .wrap(Wrap { trim: true }),
        parts[2],
    );
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(parts[3]);
    if naming {
        let label = if editor.mode == views::Mode::SaveAs {
            crate::t!("Name of the new view: ", "Nom de la nouvelle vue : ")
        } else {
            crate::t!("New name: ", "Nouveau nom : ")
        };
        let prefix = label.width() as u16 + 2;
        let room = usize::from(rows[0].width.saturating_sub(prefix)).max(1);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{} {label}", if editor.name_focus == 0 { ">" } else { " " }),
                    if editor.name_focus == 0 {
                        accent()
                    } else {
                        Style::default()
                    },
                ),
                input_span(&editor.name, editor.name_focus == 0, room),
            ])),
            rows[0],
        );
        if editor.name_focus == 0 {
            input_cursor(f, &editor.name, rows[0], prefix, 0);
        }
        f.render_widget(
            Paragraph::new(Line::from(vec![
                nav::button(crate::t!("Save", "Enregistrer"), editor.name_focus == 1),
                Span::raw("  "),
                nav::button(crate::t!("Cancel", "Annuler"), editor.name_focus == 2),
            ])),
            rows[1],
        );
    } else {
        // Two rows of three, so that no button is cut at 80 columns.
        for (row, chunk) in views::buttons().chunks(3).enumerate() {
            let mut spans = Vec::new();
            for (index, label) in chunk.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(nav::button(label, editor.button == row * 3 + index));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), rows[row]);
        }
    }
    f.render_widget(
        Paragraph::new(notice)
            .style(notice_style)
            .wrap(Wrap { trim: true }),
        parts[4],
    );
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[5],
    );
    if editor.mode == views::Mode::Delete {
        if let Some((name, _)) = selected {
            m.confirm.draw(
                f,
                whole,
                crate::t!("Delete this view?", "Supprimer cette vue ?"),
                vec![
                    Line::from(name.clone()),
                    Line::from(crate::t!(
                        "Only the saved view is removed; the list and the SSH files are unchanged.",
                        "Seule la vue enregistrée est retirée ; la liste et les fichiers SSH sont inchangés."
                    )),
                ],
                crate::t!("Go back", "Revenir"),
                crate::t!("Delete the view", "Supprimer la vue"),
            );
        }
    }
}

/// Command catalogue: list and named actions, then the form of one command or
/// the values of its parameters. Each step says what Enter does not do.
fn saved_commands(f: &mut Frame, m: &mut Model, area: Rect) {
    let View::Commands(editor) = &m.view else {
        return;
    };
    let whole = area;
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let title = match editor.mode {
        commands::Mode::Browse => crate::t!("Saved commands", "Commandes enregistrées").to_owned(),
        commands::Mode::Form => match &editor.original {
            Some(name) => crate::tf!(
                "Saved commands > Edit {name}",
                "Commandes enregistrées > Modifier {name}"
            ),
            None => crate::t!(
                "Saved commands > New command",
                "Commandes enregistrées > Nouvelle commande"
            )
            .to_owned(),
        },
        commands::Mode::Params => crate::t!(
            "Saved commands > Parameters",
            "Commandes enregistrées > Paramètres"
        )
        .to_owned(),
    };
    let border = block(&title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let width = usize::from(inner.width).max(1);
    let chosen = m.commands.iter().nth(editor.selected);
    let (notice, notice_style) = if m.error.is_empty() {
        (editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else {
        (
            crate::tf!("Error: {}", "Erreur : {}", m.error),
            Style::default().fg(Color::Red),
        )
    };
    let notice_height = select::wrapped_rows(&notice, width).clamp(1, 3) as u16;
    let footer = match editor.mode {
        commands::Mode::Browse => crate::t!(
            "↑↓: command • Tab/←→: action • Enter: run the action • Esc: back",
            "↑↓ : commande • Tab/←→ : action • Entrée : lancer l'action • Échap : retour"
        ),
        commands::Mode::Form => crate::t!(
            "Tab/↑↓: field • Enter: next field • ←/→: scope or button • Ctrl+S: save • Esc: cancel",
            "Tab/↑↓ : champ • Entrée : champ suivant • ←/→ : portée ou bouton • Ctrl+S : enregistrer • Échap : annuler"
        ),
        commands::Mode::Params => crate::t!(
            "Tab/↑↓: value • Enter: next • ←/→: choice or button • Ctrl+L: continue to the review • Esc: back",
            "Tab/↑↓ : valeur • Entrée : suivant • ←/→ : choix ou bouton • Ctrl+L : passer à la vérification • Échap : retour"
        ),
    };
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(notice_height),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(notice)
            .style(notice_style)
            .wrap(Wrap { trim: true }),
        parts[1],
    );
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[2],
    );
    let body = parts[0];
    let row = |index: usize| Rect::new(body.x, body.y + index as u16, body.width, 1);
    let fits = |index: usize| index < usize::from(body.height);
    let label_style = |focused: bool| {
        if focused {
            accent()
        } else {
            Style::default()
        }
    };
    match editor.mode {
        commands::Mode::Browse => {
            let host = m.selected_host().map_or_else(
                || crate::t!("no host selected", "aucun hôte sélectionné").to_owned(),
                |host| host.name,
            );
            let context = crate::tf!(
                "Selected host: {host} • Batch: {} marked target(s)",
                "Hôte sélectionné : {host} • Lot : {} cible(s) marquée(s)",
                m.batch.marks.len()
            );
            let detail: Vec<String> = chosen.map_or_else(Vec::new, |(_, command)| {
                vec![
                    crate::tf!("Command: {}", "Commande : {}", command.template),
                    crate::tf!(
                        "Meant for: {}",
                        "Prévue pour : {}",
                        commands::scope_label(command.scope)
                    ),
                    commands::parameters_summary(&command.template).unwrap_or_else(|e| e),
                ]
            });
            let detail_rows: usize = detail
                .iter()
                .map(|line| select::wrapped_rows(line, width))
                .sum();
            let zones = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length((detail_rows as u16).min((body.height / 2).max(1))),
                Constraint::Length(2),
            ])
            .split(body);
            f.render_widget(Paragraph::new(context).style(accent()), zones[0]);
            let lines: Vec<Line<'_>> = if m.commands.is_empty() {
                vec![Line::styled(
                    crate::t!(
                        "No saved command. \"New\" keeps a command under a name, with the parameters it asks at each use. Using it only fills the batch preparation: you still review the targets and the command before anything runs.",
                        "Aucune commande enregistrée. « Nouvelle » garde une commande sous un nom, avec les paramètres qu'elle demande à chaque utilisation. L'utiliser remplit seulement la préparation du lot : vous vérifiez encore les cibles et la commande avant toute exécution."
                    ),
                    Style::default().fg(GRAY),
                )]
            } else {
                let height = usize::from(zones[1].height).max(1);
                let offset = (editor.selected + 1).saturating_sub(height);
                m.commands
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .take(height)
                    .map(|(index, (name, command))| {
                        let current = index == editor.selected;
                        let description = if command.description.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", command.description)
                        };
                        Line::styled(
                            format!("{} {name}{description}", if current { "▶" } else { " " }),
                            if current {
                                Style::default().fg(Color::Black).bg(CYAN)
                            } else {
                                Style::default()
                            },
                        )
                    })
                    .collect()
            };
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), zones[1]);
            f.render_widget(
                Paragraph::new(
                    detail
                        .into_iter()
                        .map(Line::from)
                        .collect::<Vec<Line<'_>>>(),
                )
                .style(Style::default().fg(GRAY))
                .wrap(Wrap { trim: true }),
                zones[2],
            );
            let rows =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(zones[3]);
            // Two rows of three, so that no button is cut at 80 columns.
            for (line, chunk) in commands::buttons().chunks(3).enumerate() {
                let mut spans = Vec::new();
                for (index, label) in chunk.iter().enumerate() {
                    if index > 0 {
                        spans.push(Span::raw("  "));
                    }
                    spans.push(nav::button(label, editor.button == line * 3 + index));
                }
                f.render_widget(Paragraph::new(Line::from(spans)), rows[line]);
            }
        }
        commands::Mode::Form => {
            let labels = [
                crate::t!("Name *", "Nom *"),
                crate::t!("Description", "Description"),
                crate::t!("Meant for", "Prévue pour"),
                crate::t!("Command *", "Commande *"),
            ];
            let label_width = labels.iter().map(|l| l.width()).max().unwrap_or(0) + 2;
            let prefix = label_width as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            for (stop, label) in labels.iter().enumerate() {
                if !fits(stop) {
                    break;
                }
                let focused = editor.focus == stop;
                let head = Span::styled(
                    format!("{} {label:<label_width$}", if focused { ">" } else { " " }),
                    label_style(focused),
                );
                let value = if stop == commands::SCOPE {
                    Span::raw(format!(
                        "‹ {} ›{}",
                        commands::scope_label(editor.scope),
                        if focused { "  ←/→" } else { "" }
                    ))
                } else {
                    let input = &editor.inputs[if stop == commands::TEMPLATE { 2 } else { stop }];
                    input_span(input, focused, room)
                };
                f.render_widget(Paragraph::new(Line::from(vec![head, value])), row(stop));
                if focused && stop != commands::SCOPE {
                    let input = &editor.inputs[if stop == commands::TEMPLATE { 2 } else { stop }];
                    input_cursor(f, input, row(stop), prefix, 0);
                }
            }
            let (summary, style) = match commands::parameters_summary(&editor.inputs[2].value) {
                Ok(text) => (text, Style::default()),
                Err(reason) if editor.inputs[2].value.trim().is_empty() => {
                    (reason, Style::default().fg(GRAY))
                }
                Err(reason) => (reason, Style::default().fg(Color::Red)),
            };
            let help = [
                crate::t!(
                    "Parameters: {name} text, {name=default}, {n:number=10}, {c:choice=a|b}; {{ and }} are literal braces.",
                    "Paramètres : {nom} texte, {nom=défaut}, {n:number=10}, {c:choice=a|b} ; {{ et }} sont des accolades littérales."
                ),
                crate::t!(
                    "Each value is inserted as one quoted word: do not put quotes around a parameter.",
                    "Chaque valeur est insérée comme un seul mot protégé : ne mettez pas de guillemets autour d'un paramètre."
                ),
                crate::t!(
                    "Never write a password or a secret here: commands are stored in clear text.",
                    "N'écrivez jamais de mot de passe ni de secret ici : les commandes sont enregistrées en clair."
                ),
            ];
            let mut lines = vec![Line::styled(summary, style)];
            lines.extend(
                help.iter()
                    .map(|line| Line::styled(*line, Style::default().fg(GRAY))),
            );
            let text_rows: usize = lines
                .iter()
                .map(|line| select::wrapped_rows(&line.to_string(), width))
                .sum();
            let top = 5u16;
            let available = body.height.saturating_sub(top + 2);
            let text_height = (text_rows as u16).min(available);
            if body.height > top {
                f.render_widget(
                    Paragraph::new(lines).wrap(Wrap { trim: true }),
                    Rect::new(body.x, body.y + top, body.width, text_height),
                );
            }
            let buttons_y = (top + text_height + 1).min(body.height.saturating_sub(1));
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    nav::button(
                        crate::t!("Save", "Enregistrer"),
                        editor.focus == commands::SAVE,
                    ),
                    Span::raw("  "),
                    nav::button(
                        crate::t!("Cancel", "Annuler"),
                        editor.focus == commands::CANCEL,
                    ),
                ])),
                Rect::new(body.x, body.y + buttons_y, body.width, 1),
            );
        }
        commands::Mode::Params => {
            let Some((name, command)) = chosen else {
                return;
            };
            let target = if editor.batch {
                crate::tf!(
                    "the batch ({} marked target(s))",
                    "le lot ({} cible(s) marquée(s))",
                    m.batch.marks.len()
                )
            } else {
                m.selected_host().map_or_else(String::new, |host| host.name)
            };
            f.render_widget(
                Paragraph::new(crate::tf!(
                    "{name} — for {target}",
                    "{name} — pour {target}"
                ))
                .style(accent()),
                row(0),
            );
            let label_width = editor
                .params
                .iter()
                .map(|p| p.name.width() + commands::kind_label(&p.kind).width() + 3)
                .max()
                .unwrap_or(0)
                .min(width / 2)
                + 2;
            let prefix = label_width as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            let count = editor.params.len();
            for (index, (parameter, input)) in editor.params.iter().zip(&editor.values).enumerate()
            {
                let y = index + 2;
                if !fits(y) {
                    break;
                }
                let focused = editor.param_focus == index;
                let label = format!(
                    "{} ({})",
                    parameter.name,
                    commands::kind_label(&parameter.kind)
                );
                let head = Span::styled(
                    format!(
                        "{} {label:<label_width$.label_width$}",
                        if focused { ">" } else { " " }
                    ),
                    label_style(focused),
                );
                let choice = matches!(parameter.kind, crate::storage::commands::Kind::Choice(_));
                let value = if choice {
                    Span::raw(format!(
                        "‹ {} ›{}",
                        input.value,
                        if focused { "  ←/→" } else { "" }
                    ))
                } else {
                    input_span(input, focused, room)
                };
                f.render_widget(Paragraph::new(Line::from(vec![head, value])), row(y));
                if focused && !choice {
                    input_cursor(f, input, row(y), prefix, 0);
                }
            }
            let (result, style) = match editor.rendered(&command.template) {
                Ok(text) => (
                    crate::tf!("Command to review: {text}", "Commande à vérifier : {text}"),
                    Style::default(),
                ),
                Err(reason) => (reason, Style::default().fg(Color::Yellow)),
            };
            let result_y = (count + 3) as u16;
            let result_rows = (select::wrapped_rows(&result, width) as u16).min(4);
            if body.height > result_y + 1 {
                let height = result_rows.min(body.height - result_y - 1);
                f.render_widget(
                    Paragraph::new(result)
                        .style(style)
                        .wrap(Wrap { trim: true }),
                    Rect::new(body.x, body.y + result_y, body.width, height),
                );
            }
            let buttons_y = (result_y + result_rows + 1).min(body.height.saturating_sub(1));
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    nav::button(
                        crate::t!("Continue to the review", "Passer à la vérification"),
                        editor.param_focus == count,
                    ),
                    Span::raw("  "),
                    nav::button(crate::t!("Back", "Retour"), editor.param_focus == count + 1),
                ])),
                Rect::new(body.x, body.y + buttons_y, body.width, 1),
            );
        }
    }
    match editor.pending {
        Some(commands::Pending::Delete) => {
            if let Some((name, _)) = chosen {
                m.confirm.draw(
                    f,
                    whole,
                    crate::t!("Delete this command?", "Supprimer cette commande ?"),
                    vec![
                        Line::from(name.clone()),
                        Line::from(crate::t!(
                            "Only the saved command is removed; nothing is run and no host changes.",
                            "Seule la commande enregistrée est retirée ; rien n'est exécuté et aucun hôte ne change."
                        )),
                    ],
                    crate::t!("Go back", "Revenir"),
                    crate::t!("Delete the command", "Supprimer la commande"),
                );
            }
        }
        Some(commands::Pending::Discard) => {
            m.confirm.draw(
                f,
                whole,
                crate::t!("Unsaved changes", "Modifications non enregistrées"),
                vec![Line::from(crate::t!(
                    "Discard the changes of this command?",
                    "Abandonner les modifications de cette commande ?"
                ))],
                crate::t!("Keep editing", "Continuer l'édition"),
                crate::t!("Discard", "Abandonner"),
            );
        }
        None => {}
    }
}

/// Export / import: the scope of an export, or what an import would change,
/// is always on screen before the button that writes.
fn data_transfer(f: &mut Frame, m: &mut Model, area: Rect) {
    use crate::storage::transfer::{Policy, Section};
    let View::Transfer(editor) = &m.view else {
        return;
    };
    let whole = area;
    let area = nav::bounded(inset(area), FORM_WIDTH as u16 + 2);
    let title = match editor.mode {
        transfer::Mode::Home => {
            crate::t!("Export / import data", "Exporter / importer les données")
        }
        transfer::Mode::Export => crate::t!(
            "Export / import data > Export",
            "Exporter / importer les données > Exporter"
        ),
        transfer::Mode::ImportPath | transfer::Mode::Preview => crate::t!(
            "Export / import data > Import",
            "Exporter / importer les données > Importer"
        ),
    };
    let border = block(title, true);
    let inner = border.inner(area);
    f.render_widget(border, area);
    let width = usize::from(inner.width).max(1);
    let (notice, notice_style) = if m.error.is_empty() {
        (editor.notice.clone(), Style::default().fg(Color::Yellow))
    } else {
        (
            crate::tf!("Error: {}", "Erreur : {}", m.error),
            Style::default().fg(Color::Red),
        )
    };
    let notice_height = select::wrapped_rows(&notice, width).clamp(1, 4) as u16;
    let footer = match editor.mode {
        transfer::Mode::Home => crate::t!(
            "Tab/←→: choose • Enter: open • Esc: back",
            "Tab/←→ : choisir • Entrée : ouvrir • Échap : retour"
        ),
        transfer::Mode::Export => crate::t!(
            "Tab/↑↓: move • Space: include or not • Enter: next • Ctrl+S: export • Esc: back",
            "Tab/↑↓ : se déplacer • Espace : inclure ou non • Entrée : suivant • Ctrl+S : exporter • Échap : retour"
        ),
        transfer::Mode::ImportPath => crate::t!(
            "Enter: to the Read button • Ctrl+L: read the file • Esc: back — reading changes nothing",
            "Entrée : vers le bouton Lire • Ctrl+L : lire le fichier • Échap : retour — lire ne modifie rien"
        ),
        transfer::Mode::Preview => crate::t!(
            "Tab/↑↓: move • Space: include or not • ←/→: change the choice • Enter: next • Esc: back",
            "Tab/↑↓ : se déplacer • Espace : inclure ou non • ←/→ : changer le choix • Entrée : suivant • Échap : retour"
        ),
    };
    let footer_height = select::wrapped_rows(footer, width).min(2) as u16;
    let parts = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(notice_height),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    f.render_widget(
        Paragraph::new(notice)
            .style(notice_style)
            .wrap(Wrap { trim: true }),
        parts[1],
    );
    f.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(GRAY))
            .wrap(Wrap { trim: true }),
        parts[2],
    );
    let body = parts[0];
    let gray = Style::default().fg(GRAY);
    let mark = |focused: bool| if focused { ">" } else { " " };
    let style = |focused: bool| {
        if focused {
            accent()
        } else {
            Style::default()
        }
    };
    let never = crate::t!(
        "Never included: private keys, passwords, SSH files, connection history, backups, SSH launcher.",
        "Jamais inclus : clés privées, mots de passe, fichiers SSH, historique des connexions, sauvegardes, lanceur SSH."
    );
    let private = crate::t!(
        "Notes and commands may hold private information: the file is created readable by you only.",
        "Notes et commandes peuvent contenir des informations privées : le fichier est créé lisible par vous seul."
    );
    let mut lines: Vec<Line<'_>> = Vec::new();
    // Row of the line that owns the caret, with the width of its label.
    let mut caret: Option<(usize, u16, &Input)> = None;
    match editor.mode {
        transfer::Mode::Home => {
            for text in [
                crate::tf!(
                    "Configuration in use: {}",
                    "Configuration en cours : {}",
                    m.batch.root.display()
                ),
                crate::t!(
                    "An export writes, to a file you name, what you choose among: favorites and notes, tunnel profiles and saved views of this configuration, saved commands, portable preferences.",
                    "Un export écrit, dans un fichier que vous nommez, ce que vous choisissez parmi : favoris et notes, profils de tunnels et vues enregistrées de cette configuration, commandes enregistrées, préférences portables."
                )
                .to_owned(),
                never.to_owned(),
                crate::t!(
                    "An import reads such a file into this configuration. It first shows what is new, identical or different; nothing changes before you confirm, and the current files are copied aside first.",
                    "Un import lit un tel fichier dans cette configuration. Il montre d'abord ce qui est nouveau, identique ou différent ; rien ne change avant votre confirmation, et les fichiers actuels sont d'abord copiés à part."
                )
                .to_owned(),
            ] {
                lines.push(Line::from(text));
                lines.push(Line::from(""));
            }
            let mut spans = Vec::new();
            for (index, label) in transfer::home_buttons().into_iter().enumerate() {
                if index > 0 {
                    spans.push(Span::raw("  "));
                }
                spans.push(nav::button(label, editor.focus == index));
            }
            lines.push(Line::from(spans));
        }
        transfer::Mode::Export => {
            lines.push(Line::styled(
                crate::tf!(
                    "What the file will hold, from {}:",
                    "Ce que le fichier contiendra, depuis {} :",
                    m.batch.root.display()
                ),
                accent(),
            ));
            for (index, section) in Section::ALL.into_iter().enumerate() {
                let focused = editor.focus == index;
                let count = editor.counts.get(&section).copied().unwrap_or(0);
                lines.push(Line::styled(
                    format!(
                        "{} [{}] {:<26} {}",
                        mark(focused),
                        if editor.chosen[index] { "x" } else { " " },
                        transfer::section_label(section),
                        if section == Section::Preferences {
                            crate::t!(
                                "language, display, list, keyboard",
                                "langue, affichage, liste, clavier"
                            )
                            .to_owned()
                        } else {
                            crate::tf!("{count} entries", "{count} entrées")
                        }
                    ),
                    style(focused),
                ));
            }
            lines.push(Line::from(""));
            lines.push(Line::styled(private, gray));
            lines.push(Line::styled(never, gray));
            lines.push(Line::from(""));
            let label = crate::t!("File to create: ", "Fichier à créer : ");
            let focused = editor.focus == 5;
            let prefix = label.width() as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            if focused {
                caret = Some((lines.len(), prefix, &editor.path));
            }
            lines.push(Line::from(vec![
                Span::styled(format!("{} {label}", mark(focused)), style(focused)),
                input_span(&editor.path, focused, room),
            ]));
            lines.push(Line::styled(
                crate::t!(
                    "  An existing file is never replaced. ~/ is your home directory.",
                    "  Un fichier existant n'est jamais remplacé. ~/ est votre dossier personnel."
                ),
                gray,
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                nav::button(crate::t!("Export", "Exporter"), editor.focus == 6),
                Span::raw("  "),
                nav::button(crate::t!("Back", "Retour"), editor.focus == 7),
            ]));
        }
        transfer::Mode::ImportPath => {
            lines.push(Line::from(crate::tf!(
                "The file is imported into the configuration in use: {}",
                "Le fichier est importé dans la configuration en cours : {}",
                m.batch.root.display()
            )));
            lines.push(Line::from(""));
            let label = crate::t!("Export file to read: ", "Fichier d'export à lire : ");
            let focused = editor.focus == 0;
            let prefix = label.width() as u16 + 2;
            let room = usize::from(body.width.saturating_sub(prefix)).max(1);
            if focused {
                caret = Some((lines.len(), prefix, &editor.import_path));
            }
            lines.push(Line::from(vec![
                Span::styled(format!("{} {label}", mark(focused)), style(focused)),
                input_span(&editor.import_path, focused, room),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                nav::button(
                    crate::t!("Read the file", "Lire le fichier"),
                    editor.focus == 1,
                ),
                Span::raw("  "),
                nav::button(crate::t!("Back", "Retour"), editor.focus == 2),
            ]));
        }
        transfer::Mode::Preview => {
            if let Some(export) = &editor.export {
                lines.push(Line::styled(
                    crate::tf!(
                        "From: {} ({}, {})",
                        "Depuis : {} ({}, {})",
                        export.configuration,
                        export.application,
                        export.exported_at
                    ),
                    gray,
                ));
                lines.push(Line::styled(
                    crate::tf!("Into: {}", "Vers : {}", m.batch.root.display()),
                    accent(),
                ));
            }
            let listed = transfer::sections(editor);
            for (stop, section) in listed.iter().enumerate() {
                let Some(plan) = editor.plan.get(section) else {
                    continue;
                };
                let index = Section::ALL.iter().position(|s| s == section).unwrap_or(0);
                let focused = editor.focus == stop;
                let mut counts = vec![
                    crate::tf!("{} new", "{} nouvelles", plan.new.len()),
                    crate::tf!("{} identical", "{} identiques", plan.same),
                    crate::tf!("{} different", "{} différentes", plan.conflicts.len()),
                ];
                if !plan.unknown.is_empty() {
                    counts.push(crate::tf!(
                        "{} for an alias unknown here",
                        "{} pour un alias inconnu ici",
                        plan.unknown.len()
                    ));
                }
                if plan.local_only > 0 && *section != Section::Preferences {
                    counts.push(crate::tf!(
                        "{} only here",
                        "{} seulement ici",
                        plan.local_only
                    ));
                }
                lines.push(Line::styled(
                    format!(
                        "{} [{}] {:<24} {}",
                        mark(focused),
                        if editor.include[index] { "x" } else { " " },
                        transfer::section_label(*section),
                        counts.join(" • ")
                    ),
                    style(focused),
                ));
            }
            let missing: Vec<&str> = editor
                .plan
                .values()
                .flat_map(|plan| plan.missing_sources.iter().map(String::as_str))
                .collect();
            if !missing.is_empty() {
                lines.push(Line::styled(
                    crate::tf!(
                        "  View(s) whose source file is not in this configuration: {} — applying them will ask for an update.",
                        "  Vue(s) dont le fichier source n'est pas dans cette configuration : {} — leur application demandera une mise à jour.",
                        missing.join(", ")
                    ),
                    Style::default().fg(Color::Yellow),
                ));
            }
            lines.push(Line::from(""));
            let policy_stop = listed.len();
            let focused = editor.focus == policy_stop;
            lines.push(Line::styled(
                format!(
                    "{} {}‹ {} ›{}",
                    mark(focused),
                    crate::t!(
                        "Entries present on both sides: ",
                        "Entrées présentes des deux côtés : "
                    ),
                    transfer::policy_label(editor.policy),
                    if focused { "  ←/→" } else { "" }
                ),
                style(focused),
            ));
            lines.push(Line::styled(
                format!("  {}", transfer::policy_detail(editor.policy)),
                if editor.policy == Policy::ReplaceSections {
                    Style::default().fg(Color::Yellow)
                } else {
                    gray
                },
            ));
            let mut next = policy_stop + 1;
            if transfer::has_unknown(editor) {
                let focused = editor.focus == next;
                let names: Vec<&str> = editor
                    .plan
                    .values()
                    .flat_map(|plan| plan.unknown.iter().map(String::as_str))
                    .collect();
                lines.push(Line::styled(
                    format!(
                        "{} {}‹ {} ›{}",
                        mark(focused),
                        crate::t!("Aliases unknown here: ", "Alias inconnus ici : "),
                        if editor.keep_unknown {
                            crate::t!("import them anyway", "les importer quand même")
                        } else {
                            crate::t!("leave them out", "les laisser de côté")
                        },
                        if focused { "  ←/→" } else { "" }
                    ),
                    style(focused),
                ));
                lines.push(Line::styled(
                    crate::tf!(
                        "  {} — an entry is attached to an alias of this configuration, never to a name alone.",
                        "  {} — une entrée est rattachée à un alias de cette configuration, jamais à un nom seul.",
                        names.join(", ")
                    ),
                    gray,
                ));
                next += 1;
            }
            lines.push(Line::from(""));
            let (added, replaced, removed, skipped) = transfer::totals(editor);
            lines.push(Line::from(crate::tf!(
                "This import would add {added}, replace {replaced}, remove {removed} and leave out {skipped}.",
                "Cet import ajouterait {added} entrées, en remplacerait {replaced}, en supprimerait {removed} et en laisserait {skipped} de côté."
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                nav::button(crate::t!("Import", "Importer"), editor.focus == next),
                Span::raw("  "),
                nav::button(crate::t!("Back", "Retour"), editor.focus == next + 1),
            ]));
        }
    }
    // Wrapped rows before the caret line decide where the caret really is.
    let caret = caret.map(|(index, prefix, input)| {
        let above: usize = lines[..index]
            .iter()
            .map(|line| select::wrapped_rows(&line.to_string(), width))
            .sum();
        (above, prefix, input)
    });
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
    if let Some((row, prefix, input)) = caret {
        input_cursor(f, input, body, prefix, row);
    }
    if editor.pending {
        let (added, replaced, removed, skipped) = transfer::totals(editor);
        let root = m
            .batch
            .history_root
            .as_ref()
            .map_or_else(String::new, |root| {
                root.join("data-backups").display().to_string()
            });
        m.confirm.draw(
            f,
            whole,
            crate::t!("Import these data?", "Importer ces données ?"),
            vec![
                Line::from(crate::tf!(
                    "{added} added, {replaced} replaced, {removed} removed, {skipped} left out.",
                    "{added} ajoutées, {replaced} remplacées, {removed} supprimées, {skipped} laissées de côté."
                )),
                Line::from(crate::tf!(
                    "The current files are first copied to {root}",
                    "Les fichiers actuels sont d'abord copiés dans {root}"
                )),
            ],
            crate::t!("Go back", "Revenir"),
            crate::t!("Import", "Importer"),
        );
    }
}
