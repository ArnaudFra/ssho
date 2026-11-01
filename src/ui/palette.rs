//! F2 palette: every catalogue action compatible with the current screen, found
//! by typing, with its shortcut, its state and the reason when it is unavailable.
use super::actions::{fold, Category};
use super::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};

/// The screen under the palette decides which actions are compatible with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scope {
    List,
    /// A host sheet: actions on that host, nothing that rearranges the list.
    Info,
    /// An open draft is kept as it is: only screens that return to it.
    Draft,
}

impl Scope {
    /// Screens from which F2 opens a contextual palette.
    pub(super) fn of(view: &View) -> Option<Self> {
        match view {
            View::Info(_) => Some(Self::Info),
            View::Host(_) | View::Forward(_) | View::Note(_) => Some(Self::Draft),
            _ => None,
        }
    }

    fn offers(self, action: PaletteAction) -> bool {
        use PaletteAction as A;
        match self {
            Self::List => action != A::Palette,
            Self::Info => matches!(
                action,
                A::Connect
                    | A::Tunnel
                    | A::Profiles
                    | A::Active
                    | A::Edit
                    | A::Duplicate
                    | A::Move
                    | A::Delete
                    | A::Favorite
                    | A::Note
                    | A::Keys
                    | A::Reload
                    | A::Help
            ),
            Self::Draft => matches!(action, A::Active | A::Help),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct State {
    scope: Scope,
    pub(super) query: Input,
    /// Identity of the selected action: it survives filtering and refreshes.
    pub(super) selected: Option<PaletteAction>,
    offset: usize,
    /// Reason shown after Enter on an unavailable action.
    notice: Option<&'static str>,
}

impl State {
    pub(super) fn new(scope: Scope) -> Self {
        let mut state = Self {
            scope,
            query: Input {
                placeholder: crate::t!("name or keyword", "nom ou mot-clé").into(),
                limit: 64,
                ..Input::default()
            },
            selected: None,
            offset: 0,
            notice: None,
        };
        state.selected = state.entries().first().copied();
        state
    }

    /// Displayed actions, in catalogue order. Every typed word must be found in
    /// the label, the category or the keywords, ignoring case and accents.
    pub(super) fn entries(&self) -> Vec<PaletteAction> {
        let query = fold(&self.query.value);
        let words: Vec<_> = query.split_whitespace().collect();
        PaletteAction::CATALOGUE
            .into_iter()
            .filter(|action| self.scope.offers(*action))
            .filter(|action| {
                // Either language finds an action, whatever is displayed.
                let (english, french) = action.labels();
                let text = fold(&format!(
                    "{english} {french} {} {}",
                    action.category().label(),
                    action.keywords()
                ));
                words.iter().all(|word| text.contains(word))
            })
            .collect()
    }

    fn position(&self, entries: &[PaletteAction]) -> usize {
        self.selected
            .and_then(|selected| entries.iter().position(|action| *action == selected))
            .unwrap_or(0)
    }

    fn select(&mut self, entries: &[PaletteAction], index: usize) {
        self.selected = entries.get(index).copied();
        self.notice = None;
    }
}

pub(super) fn open(m: &mut Model, scope: Scope) {
    m.open_view(View::Actions(State::new(scope)));
}

pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let context = m.action_context();
    // The key that opens the palette also closes it, unless it is a character.
    let toggles = !matches!(k.code, KeyCode::Char(_))
        && m.keymap().action(&key_name(k)) == Some(PaletteAction::Palette);
    let page = m.page.max(1) as isize;
    let View::Actions(state) = &mut m.view else {
        return Action::None;
    };
    if nav::cancels(k) || toggles {
        m.back();
        return Action::None;
    }
    let entries = state.entries();
    let position = state.position(&entries);
    let last = entries.len().saturating_sub(1);
    match k.code {
        // The list wraps at both ends over the displayed actions; titles are skipped.
        KeyCode::Up => state.select(&entries, nav::wrap(position, entries.len(), -1)),
        KeyCode::Down => state.select(&entries, nav::wrap(position, entries.len(), 1)),
        KeyCode::Char('p') if ctrl => {
            state.select(&entries, nav::wrap(position, entries.len(), -1))
        }
        KeyCode::Char('n') if ctrl => state.select(&entries, nav::wrap(position, entries.len(), 1)),
        KeyCode::PageUp => state.select(&entries, position.saturating_sub(page as usize)),
        KeyCode::PageDown => state.select(&entries, (position + page as usize).min(last)),
        KeyCode::Home if state.query.value.is_empty() => state.select(&entries, 0),
        KeyCode::End if state.query.value.is_empty() => state.select(&entries, last),
        KeyCode::Enter => {
            // Only Enter runs, and only the selected action: a single match is
            // never executed on its own.
            let Some(action) = state.selected.filter(|action| entries.contains(action)) else {
                return Action::None;
            };
            match action.availability(&context) {
                Ok(()) => {
                    m.back();
                    return m.activate_action(action);
                }
                Err(reason) => state.notice = Some(reason),
            }
        }
        KeyCode::Tab | KeyCode::BackTab => {}
        _ => {
            // Letters search; they never trigger a shortcut from here.
            let before = state.query.value.clone();
            state.query.key(k);
            if before != state.query.value {
                let entries = state.entries();
                if !state
                    .selected
                    .is_some_and(|action| entries.contains(&action))
                {
                    state.selected = entries.first().copied();
                }
                state.offset = 0;
                state.notice = None;
            }
        }
    }
    Action::None
}

impl Model {
    pub(super) fn action_context(&self) -> actions::Context {
        actions::Context {
            host: self.selected_host().is_some(),
            group: self.selected_group().is_some(),
            source_filter: self.source_filter.is_some(),
            query: !self.query.value.is_empty(),
            filters: !self.query.value.is_empty()
                || self.source_filter.is_some()
                || self.favorites_only
                || self.show_hidden,
            marks: self.batch.marks.len(),
            batch_results: !self.batch.manager.records().is_empty(),
            messages: !self.messages().is_empty(),
        }
    }

    /// Current value of a toggle or a count, shown in the State column.
    fn action_state(&self, action: PaletteAction) -> String {
        use PaletteAction as A;
        let on = |active: bool| {
            if active {
                crate::t!("on", "activé")
            } else {
                ""
            }
            .to_owned()
        };
        match action {
            A::FavoritesOnly => on(self.favorites_only),
            A::ToggleHidden => on(self.show_hidden),
            A::ToggleAliases => on(self.preferences.group_aliases),
            A::Details => on(self.details.shown),
            A::SortName => on(!self.recent),
            A::SortRecent => on(self.recent),
            A::Availability => self.preferences.availability_method.label().to_owned(),
            A::GroupMode => groups::modes()
                .iter()
                .find(|(mode, _)| *mode == self.preferences.group_mode)
                .map_or(String::new(), |(_, label)| (*label).to_owned()),
            A::SourceFilter => self
                .source_filter
                .as_ref()
                .and_then(|path| path.file_name())
                .map_or(String::new(), |name| name.to_string_lossy().into_owned()),
            A::Active => {
                let active = self
                    .tunnels
                    .iter()
                    .filter(|record| record.state == runtime::tunnels::State::Active)
                    .count();
                if active == 0 {
                    String::new()
                } else {
                    crate::tf!("{active} active", "{active} actif(s)")
                }
            }
            A::BatchOpen | A::BatchClear => match self.batch.marks.len() {
                0 => String::new(),
                marks => crate::tf!("{marks} target(s)", "{marks} cible(s)"),
            },
            A::BatchResults if self.batch.manager.has_active() => {
                crate::t!("running", "en cours").to_owned()
            }
            _ => String::new(),
        }
    }
}

pub(super) fn draw(f: &mut Frame, m: &mut Model, area: Rect) {
    let context = m.action_context();
    let keymap = m.keymap();
    let host = m.selected_host();
    let View::Actions(state) = &m.view else {
        return;
    };
    let entries = state.entries();
    // Sized for the whole scope, so the panel does not jump while typing and a
    // short contextual palette leaves most of its parent screen visible.
    let offered: Vec<_> = PaletteAction::CATALOGUE
        .into_iter()
        .filter(|action| state.scope.offers(*action))
        .collect();
    let categories = Category::ALL
        .iter()
        .filter(|category| offered.iter().any(|action| action.category() == **category))
        .count();
    let width = area.width.min(80);
    let height = area
        .height
        .min((offered.len() + categories + 7).min(24) as u16);
    let area = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, area);
    let title = match (&host, state.scope) {
        (_, Scope::Draft) => {
            crate::t!("Actions — draft kept", "Actions — brouillon conservé").to_owned()
        }
        (Some(host), _) => crate::tf!("Actions — {}", "Actions — {}", host.name),
        (None, _) => crate::t!(
            "Actions — no host selected",
            "Actions — aucun hôte sélectionné"
        )
        .to_owned(),
    };
    let border = Block::default()
        .title(title)
        .borders(if area.height >= 8 && area.width >= 20 {
            Borders::ALL
        } else {
            Borders::NONE
        })
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(0, 173, 216)));
    let inner = border.inner(area);
    f.render_widget(border, area);
    // On a short terminal the search and the list win over the explanations.
    let roomy = inner.height >= 9;
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(if roomy { 2 } else { 0 }),
        Constraint::Length(u16::from(inner.height >= 4)),
    ])
    .split(inner);
    let prompt = if inner.width >= 50 {
        crate::t!("Search an action: ", "Rechercher une action : ")
    } else {
        "> "
    };
    let prompt_width = prompt.chars().count() as u16;
    let (text, cursor) = state
        .query
        .viewport(true, usize::from(inner.width.saturating_sub(prompt_width)));
    f.render_widget(
        Paragraph::new(Line::from(vec![
            prompt.into(),
            ratatui::text::Span::styled(
                text,
                if state.query.value.is_empty() {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default()
                },
            ),
        ])),
        parts[0],
    );
    if let Some(column) = cursor {
        if prompt_width + column < parts[0].width {
            f.set_cursor_position((parts[0].x + prompt_width + column, parts[0].y));
        }
    }
    // Category titles are rows of the table but never selectable.
    let mut rows = Vec::new();
    let mut selected_row = None;
    let mut titled = false;
    let mut category: Option<Category> = None;
    for action in &entries {
        let category_started = category != Some(action.category());
        if category_started {
            category = Some(action.category());
            rows.push(
                Row::new([Cell::from(action.category().label())]).style(
                    Style::default()
                        .fg(Color::Rgb(0, 173, 216))
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }
        if state.selected == Some(*action) {
            selected_row = Some(rows.len());
            // The first action of a category is shown together with its title.
            titled = category_started;
        }
        let availability = action.availability(&context);
        let status = match availability {
            Ok(()) => m.action_state(*action),
            Err(reason) => reason.to_owned(),
        };
        rows.push(
            Row::new([
                Cell::from(format!("  {}", action.label())),
                Cell::from(keymap.shortcut(*action).unwrap_or_else(|| "—".into())),
                Cell::from(status),
            ])
            .style(if availability.is_ok() {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            }),
        );
    }
    // The label column fits the longest label; the State column takes what is
    // left, and its full text is repeated under the list for the selection.
    let label_width = offered
        .iter()
        .map(|action| action.label().chars().count() as u16 + 2)
        .max()
        .unwrap_or(12);
    let widths = if inner.width >= label_width + 24 {
        [
            Constraint::Length(label_width),
            Constraint::Length(9),
            Constraint::Min(0),
        ]
    } else {
        [
            Constraint::Min(12),
            Constraint::Length(7),
            Constraint::Length(0),
        ]
    };
    let offset = match selected_row {
        Some(row) if titled => state.offset.min(row - 1),
        _ => state.offset,
    };
    let mut table_state = TableState::default()
        .with_selected(selected_row)
        .with_offset(offset);
    if entries.is_empty() {
        f.render_widget(
            Paragraph::new(crate::t!(
                "No action matches. Clear the search to see them all.",
                "Aucune action ne correspond. Effacez la recherche pour tout voir."
            ))
            .style(Style::default().fg(Color::DarkGray)),
            parts[1],
        );
    } else {
        f.render_stateful_widget(
            Table::new(rows, widths)
                .header(
                    Row::new([
                        "  Action",
                        crate::t!("Shortcut", "Raccourci"),
                        crate::t!("State", "État"),
                    ])
                    .style(Style::default().fg(Color::DarkGray)),
                )
                .highlight_symbol("▶")
                .row_highlight_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Rgb(0, 173, 216))
                        .add_modifier(Modifier::BOLD),
                ),
            parts[1],
            &mut table_state,
        );
    }
    let explanation = match state.selected.filter(|action| entries.contains(action)) {
        Some(action) => match action.availability(&context) {
            Err(reason) if state.notice.is_some() => {
                crate::tf!(
                    "Unavailable: {reason}. Nothing was run.",
                    "Indisponible : {reason}. Rien n'a été lancé."
                )
            }
            Err(reason) => crate::tf!("Unavailable: {reason}.", "Indisponible : {reason}."),
            Ok(()) => action.description().to_owned(),
        },
        None => String::new(),
    };
    f.render_widget(
        Paragraph::new(explanation).wrap(ratatui::widgets::Wrap { trim: true }),
        parts[2],
    );
    f.render_widget(
        Paragraph::new(if inner.width >= 66 {
            crate::t!(
                "↑↓ Choose (wraps) • Enter Run • Esc Back • letters: search",
                "↑↓ Choisir (boucle) • Entrée Lancer • Échap Retour • lettres : rechercher"
            )
        } else {
            crate::t!("↑↓ • Enter • Esc", "↑↓ • Entrée • Échap")
        })
        .style(Style::default().fg(Color::DarkGray)),
        parts[3],
    );
    let offset = table_state.offset();
    let rows_visible = usize::from(parts[1].height.saturating_sub(1)).max(1);
    if let View::Actions(state) = &mut m.view {
        state.offset = offset;
    }
    m.page = rows_visible;
}
