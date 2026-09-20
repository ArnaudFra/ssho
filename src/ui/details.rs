//! Parsed host details, pane layout, and scrolling in rendered (wrapped) rows.
use super::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub(super) struct State {
    pub(super) shown: bool,
    scroll: usize,
    page: usize,
    max_scroll: usize,
    visible: bool,
    host: Option<Host>,
}

impl State {
    pub(super) fn key(&mut self, key: KeyEvent) -> bool {
        if !self.shown || !self.visible {
            return false;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        self.scroll = match key.code {
            KeyCode::PageDown if ctrl => self.scroll.saturating_add(self.page).min(self.max_scroll),
            KeyCode::PageUp if ctrl => self.scroll.saturating_sub(self.page),
            KeyCode::Down if alt => self.scroll.saturating_add(1).min(self.max_scroll),
            KeyCode::Up if alt => self.scroll.saturating_sub(1),
            _ => return false,
        };
        true
    }
}

/// The threshold uses the actual viewport, not its optional outer margin.
pub(super) fn split(state: &mut State, viewport_width: u16, area: Rect) -> (Rect, Option<Rect>) {
    state.visible = state.shown && viewport_width >= 110;
    if !state.visible {
        return (area, None);
    }
    let parts =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);
    (parts[0], Some(parts[1]))
}

pub(super) fn narrow_hint(width: u16, search_focus: bool) -> &'static str {
    if width >= 65 {
        if search_focus {
            "Details hidden (<110 columns) • Tab: table, then i: info • F4: disable"
        } else {
            "Details hidden (<110 columns) • i: info • F4: disable"
        }
    } else if search_focus {
        "Tab: table • i: info"
    } else {
        "i: info • F4: disable"
    }
}

/// Shared by the pane and full Info view. Declared Host remains the edit target.
pub(super) fn content(
    declared: &Host,
    diagnostic: Option<&PingResult>,
    effective: Option<&std::result::Result<runtime::effective::EffectiveConfig, String>>,
    history: Option<&ConnectionInfo>,
) -> Vec<Line<'static>> {
    let projected = effective
        .and_then(|result| result.as_ref().ok())
        .map(|value| value.host(declared));
    let host = projected.as_ref().unwrap_or(declared);
    let resolution = match effective {
        None => "Pending (showing declared values)".into(),
        Some(Ok(_)) => "Resolved via ssh -G (effective values)".into(),
        Some(Err(error)) => format!("Error: {error} (showing declared values)"),
    };
    let optional = |value: &str| {
        if value.is_empty() {
            "Not set".to_string()
        } else {
            value.to_string()
        }
    };
    let mut identities = Vec::new();
    if !host.identity.is_empty() {
        identities.push(host.identity.clone());
    }
    identities.extend(host.extra_identities.iter().cloned());
    let diagnostic = diagnostic
        .map(|result| {
            let status = result.status.label();
            let latency = format!("{} ms", result.duration.as_millis());
            result
                .error
                .as_ref()
                .map(|error| format!("{status} ({latency}): {error}"))
                .unwrap_or_else(|| format!("{status} ({latency})"))
        })
        .unwrap_or_else(|| "? Unknown (not checked)".into());
    let mut sections = vec![
        ("Host", host.name.clone()),
        ("Configuration", resolution),
        ("Hostname/IP", optional(&host.hostname)),
        ("User", optional(&host.user)),
        ("Port", optional(&host.port)),
        ("Identity File", optional(&identities.join("\n"))),
        ("ProxyJump", optional(&host.proxy_jump)),
        ("ProxyCommand", optional(&host.proxy_command)),
        (
            "Declared source",
            format!("{}:{}", host.source_file.display(), host.line_number),
        ),
        (
            "Declared target",
            format!(
                "hostname={} user={} port={}",
                optional(&declared.hostname),
                optional(&declared.user),
                optional(&declared.port)
            ),
        ),
        ("Diagnostic", diagnostic),
        ("Declared SSH Options", optional(&declared.options)),
        ("RemoteCommand", optional(&host.remote_command)),
        ("RequestTTY", optional(&host.request_tty)),
        ("Aliases", optional(&host.aliases.join(", "))),
        ("Tags", optional(&host.tags.join(", "))),
    ];
    if let Some(history) = history {
        sections.push((
            "Last attempt",
            format!(
                "{} ({} attempts)",
                history.last_connect, history.connect_count
            ),
        ));
        if let Some(result) = &history.last_result {
            sections.push((
                "Process result",
                format!(
                    "{}: {}",
                    result.completed_at,
                    result
                        .exit_code
                        .map(|code| format!("SSH exit {code}"))
                        .unwrap_or_else(|| result
                            .launch_error
                            .clone()
                            .unwrap_or_else(|| "unknown".into()))
                ),
            ));
        }
        if let Some(time) = history.last_successful_completion {
            sections.push(("Last exit 0 completion", time.to_string()));
        }
    }
    sections
        .into_iter()
        .flat_map(|(label, value)| {
            value
                .split('\n')
                .enumerate()
                .map(|(index, line)| {
                    Line::from(if index == 0 {
                        vec![
                            Span::styled(format!("{label}: "), Style::default().fg(Color::Cyan)),
                            Span::raw(line.to_owned()),
                        ]
                    } else {
                        vec![Span::raw(line.to_owned())]
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Wrap on grapheme boundaries, including unbroken paths and commands. Slicing
/// these visual rows keeps scroll bounds exact and avoids Paragraph's u16 offset.
pub(super) fn wrapped(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![];
    }
    let mut result = Vec::new();
    for line in lines {
        let mut row = Vec::new();
        let mut used = 0;
        for span in line.spans {
            for grapheme in span.styled_graphemes(Style::default()) {
                let columns = grapheme.symbol.width();
                if used + columns > usize::from(width) && !row.is_empty() {
                    result.push(Line::from(std::mem::take(&mut row)));
                    used = 0;
                }
                row.push(Span::styled(grapheme.symbol.to_owned(), grapheme.style));
                used += columns;
            }
        }
        result.push(Line::from(row));
    }
    result
}

pub(super) fn render_rows(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    scroll: &mut usize,
) -> usize {
    let lines = wrapped(lines, area.width);
    let max_scroll = lines.len().saturating_sub(usize::from(area.height));
    *scroll = (*scroll).min(max_scroll);
    let visible = lines
        .into_iter()
        .skip(*scroll)
        .take(usize::from(area.height))
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(visible), area);
    max_scroll
}

pub(super) fn draw(f: &mut Frame, m: &mut Model, area: Rect) {
    let border = Block::default()
        .title("Details • F4: hide")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = border.inner(area);
    f.render_widget(border, area);
    let parts = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(inner);
    let host = m.selected_host();
    if host != m.details.host {
        m.details.scroll = 0;
        m.details.host = host.clone();
    }
    let lines = host
        .as_ref()
        .map(|host| {
            content(
                host,
                m.statuses.get(&host.name),
                m.effective.get(&host.name),
                m.history.get(&host.name),
            )
        })
        .unwrap_or_else(|| vec![Line::from("No selected host")]);
    m.details.page = usize::from(parts[0].height).max(1);
    m.details.max_scroll = render_rows(f, parts[0], lines, &mut m.details.scroll);
    f.render_widget(
        Paragraph::new("Alt+↑/↓: scroll • Ctrl+PgUp/PgDn")
            .style(Style::default().fg(Color::DarkGray)),
        parts[1],
    );
}
