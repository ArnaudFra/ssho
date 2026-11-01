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
            crate::t!(
                "Details hidden (<110 columns) • Tab: table, then i: info • F4: disable",
                "Détails masqués (<110 colonnes) • Tab : tableau, puis i : fiche • F4 : désactiver"
            )
        } else {
            crate::t!(
                "Details hidden (<110 columns) • i: info • F4: disable",
                "Détails masqués (<110 colonnes) • i : fiche • F4 : désactiver"
            )
        }
    } else if search_focus {
        crate::t!("Tab: table • i: info", "Tab : tableau • i : fiche")
    } else {
        crate::t!("i: info • F4: disable", "i : fiche • F4 : désactiver")
    }
}

/// Shared by the pane and full Info view. Declared Host remains the edit target.
pub(super) fn content(
    declared: &Host,
    diagnostic: Option<&PingResult>,
    effective: Option<&std::result::Result<runtime::effective::EffectiveConfig, String>>,
    history: Option<&ConnectionInfo>,
    note: &str,
) -> Vec<Line<'static>> {
    let projected = effective
        .and_then(|result| result.as_ref().ok())
        .map(|value| value.host(declared));
    let host = projected.as_ref().unwrap_or(declared);
    let resolution = match effective {
        None => crate::t!(
            "Pending (showing declared values)",
            "En attente (valeurs déclarées affichées)"
        )
        .into(),
        Some(Ok(_)) => crate::t!(
            "Resolved via ssh -G (effective values)",
            "Résolue par ssh -G (valeurs effectives)"
        )
        .into(),
        Some(Err(error)) => crate::tf!(
            "Error: {error} (showing declared values)",
            "Erreur : {error} (valeurs déclarées affichées)"
        ),
    };
    let optional = |value: &str| {
        if value.is_empty() {
            crate::t!("Not set", "Non défini").to_string()
        } else {
            value.to_string()
        }
    };
    let mut identities = Vec::new();
    if !host.identity.is_empty() {
        identities.push(host.identity.clone());
    }
    identities.extend(host.extra_identities.iter().cloned());
    let scope = diagnostic
        .map(|result| result.method.scope())
        .unwrap_or(crate::t!("Not checked", "Non vérifié"));
    let diagnostic = diagnostic
        .map(|result| {
            let status = format!("{} • {}", result.method.label(), result.status.label());
            let latency = crate::tf!(
                "diagnostic {} ms",
                "diagnostic {} ms",
                result.duration.as_millis()
            );
            result
                .error
                .as_ref()
                .map(|error| {
                    crate::tf!(
                        "{status} ({latency})\nCause: {error}",
                        "{status} ({latency})\nCause : {error}"
                    )
                })
                .unwrap_or_else(|| format!("{status} ({latency})"))
        })
        .unwrap_or_else(|| crate::t!("? Unknown (not checked)", "? Inconnu (non vérifié)").into());
    // Grouped by what the value tells: who the host is, what OpenSSH resolves,
    // what the file declares, what was measured and what was attempted. An
    // attempt, an exit code 0, an availability state and an authentication are
    // different facts and keep their own lines.
    let summary = vec![
        (label::host(), host.name.clone()),
        (label::hostname(), optional(&host.hostname)),
        (label::user(), optional(&host.user)),
        ("Port", optional(&host.port)),
        (
            crate::t!("Aliases", "Alias"),
            optional(&host.aliases.join(", ")),
        ),
        (label::tags(), optional(&host.display_tags())),
        (label::note(), optional(note)),
    ];
    let resolved = vec![
        ("Configuration", resolution),
        (
            crate::t!("Identity File", "Fichier d'identité"),
            optional(&identities.join("\n")),
        ),
        ("ProxyJump", optional(&host.proxy_jump)),
        ("ProxyCommand", optional(&host.proxy_command)),
        ("RemoteCommand", optional(&host.remote_command)),
        ("RequestTTY", optional(&host.request_tty)),
    ];
    let declaration = vec![
        (
            label::source(),
            format!("{}:{}", host.source_file.display(), host.line_number),
        ),
        (
            crate::t!("Declared target", "Cible déclarée"),
            format!(
                "hostname={} user={} port={}",
                optional(&declared.hostname),
                optional(&declared.user),
                optional(&declared.port)
            ),
        ),
        (
            crate::t!("Declared SSH Options", "Options SSH déclarées"),
            optional(&declared.options),
        ),
    ];
    let availability = vec![
        (crate::t!("Diagnostic", "Diagnostic"), diagnostic),
        (
            crate::t!("Availability scope", "Portée de la disponibilité"),
            scope.into(),
        ),
    ];
    let mut attempts = vec![];
    if let Some(history) = history {
        attempts.push((
            crate::t!("Last attempt", "Dernière tentative"),
            crate::tf!(
                "{} ({} attempts)",
                "{} ({} tentatives)",
                history.last_connect,
                history.connect_count
            ),
        ));
        if let Some(result) = &history.last_result {
            attempts.push((
                crate::t!("Process result", "Résultat du processus"),
                format!(
                    "{}: {}",
                    result.completed_at,
                    result
                        .exit_code
                        .map(|code| crate::tf!("SSH exit {code}", "SSH a fini avec le code {code}"))
                        .unwrap_or_else(|| result
                            .launch_error
                            .clone()
                            .unwrap_or_else(|| crate::t!("unknown", "inconnu").into()))
                ),
            ));
        }
        if let Some(time) = history.last_successful_completion {
            attempts.push((
                crate::t!("Last exit 0 completion", "Dernière fin avec code 0"),
                time.to_string(),
            ));
        }
    } else {
        attempts.push((
            crate::t!("Last attempt", "Dernière tentative"),
            crate::t!("no recorded connection", "aucune connexion enregistrée").into(),
        ));
    }
    let title = |text: &str| {
        // Two spans: a title is never a continuation of the field above it.
        Line::from(vec![
            Span::styled(
                format!("── {text} "),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::raw(""),
        ])
    };
    [
        (crate::t!("Summary", "Résumé"), summary),
        (
            crate::t!("Effective configuration", "Configuration effective"),
            resolved,
        ),
        (
            crate::t!("Declaration and source", "Déclaration et source"),
            declaration,
        ),
        (
            crate::t!("Measured availability", "Disponibilité mesurée"),
            availability,
        ),
        (
            crate::t!("Connection history", "Historique des connexions"),
            attempts,
        ),
    ]
    .into_iter()
    .flat_map(|(section, fields)| {
        std::iter::once(title(section)).chain(fields.into_iter().flat_map(|(label, value)| {
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
        }))
    })
    .collect()
}

/// Named actions of the host sheet, in the order Tab walks them.
pub(super) fn sheet_actions() -> [&'static str; 5] {
    [
        crate::t!("Connect", "Connexion"),
        crate::t!("Edit", "Modifier"),
        "Note",
        crate::t!("Copy", "Copier"),
        crate::t!("Close", "Fermer"),
    ]
}

/// Field labels of the sheet, shared with the search highlighting.
pub(super) mod label {
    pub(in crate::ui) fn host() -> &'static str {
        crate::t!("Host", "Hôte")
    }
    pub(in crate::ui) fn hostname() -> &'static str {
        crate::t!("Hostname/IP", "Adresse/IP")
    }
    pub(in crate::ui) fn user() -> &'static str {
        crate::t!("User", "Utilisateur")
    }
    pub(in crate::ui) fn tags() -> &'static str {
        "Tags"
    }
    pub(in crate::ui) fn note() -> &'static str {
        crate::t!("Personal note", "Note personnelle")
    }
    pub(in crate::ui) fn source() -> &'static str {
        crate::t!("Declared source", "Source déclarée")
    }
}

/// Values worth copying from a host sheet, each under the name shown to choose it.
pub(super) fn copyable(
    declared: &Host,
    effective: Option<&std::result::Result<runtime::effective::EffectiveConfig, String>>,
) -> Vec<(String, String)> {
    let projected = effective
        .and_then(|result| result.as_ref().ok())
        .map(|value| value.host(declared));
    let host = projected.as_ref().unwrap_or(declared);
    let mut values = vec![
        ("Alias".to_owned(), declared.name.clone()),
        (
            crate::t!("Command", "Commande").to_owned(),
            format!("ssh {}", declared.name),
        ),
    ];
    if !host.hostname.is_empty() {
        values.push((
            crate::t!("Address", "Adresse").to_owned(),
            host.hostname.clone(),
        ));
        if !host.user.is_empty() {
            values.push((
                crate::t!("User@address", "Utilisateur@adresse").to_owned(),
                format!("{}@{}", host.user, host.hostname),
            ));
        }
    }
    if !declared.source_file.as_os_str().is_empty() {
        values.push((
            crate::t!("Source file", "Fichier source").to_owned(),
            declared.source_file.display().to_string(),
        ));
    }
    values
}

/// OSC 52 asks the terminal to place text in the system clipboard. Support is
/// the terminal's choice, so the value is always shown as well.
pub(super) fn osc52(value: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in value.as_bytes().chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let word = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            encoded.push(if index <= chunk.len() {
                ALPHABET[(word >> shift & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    format!("\x1b]52;c;{encoded}\x07")
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
        .title(crate::t!("Details • F4: hide", "Détails • F4 : masquer"))
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
                m.note(&host.name),
            )
        })
        .unwrap_or_else(|| match m.rows.get(m.selected) {
            Some(groups::Row::Group { key, count, .. }) => {
                render::group_content(key, *count, m.preferences.group_aliases)
            }
            _ => vec![Line::from(crate::t!(
                "No selected host",
                "Aucun hôte sélectionné"
            ))],
        });
    let lines = render::highlight_details(m, lines);
    m.details.page = usize::from(parts[0].height).max(1);
    m.details.max_scroll = render_rows(f, parts[0], lines, &mut m.details.scroll);
    f.render_widget(
        Paragraph::new(crate::t!(
            "Alt+↑/↓: scroll • Ctrl+PgUp/PgDn",
            "Alt+↑/↓ : défiler • Ctrl+PgUp/PgDn"
        ))
        .style(Style::default().fg(Color::DarkGray)),
        parts[1],
    );
}
