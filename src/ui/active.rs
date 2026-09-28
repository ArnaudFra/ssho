//! The active list uses owned manager IDs and immutable launch snapshots, never profile lookups.
use super::*;
use runtime::tunnels::{Manager, Snapshot, State};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Session {
    pub manager: Manager,
    completed: BTreeSet<u64>,
    root: Option<PathBuf>,
}
impl Session {
    pub fn start(
        &mut self,
        m: &mut Model,
        paths: &AppPaths,
        config: &Path,
        host: String,
        profile: Option<String>,
        forward: Forward,
    ) -> Result<()> {
        self.root = Some(paths.root.clone());
        if let Err(e) = History::load(&paths.root).and_then(|mut h| h.record(&host, Some(&forward)))
        {
            m.error = format!("Warning: could not record tunnel attempt: {e:#}");
        }
        let result = self.manager.start(Snapshot {
            config: config.into(),
            host,
            profile,
            forward,
        });
        self.poll(m, paths);
        if result.is_ok() {
            // The form remains in the navigation stack; launching never saves its edits.
            m.open_view(View::Active {
                selected: self.manager.records().len().saturating_sub(1),
            });
        }
        result.map(|_| ())
    }
    fn record_completions(&mut self, root: &Path) -> Vec<String> {
        let mut warnings = vec![];
        for r in self.manager.records() {
            if r.state != State::Active && self.completed.insert(r.id) {
                let result = History::load(root).and_then(|mut h| {
                    h.complete(
                        &r.snapshot.host,
                        r.exit_code,
                        (r.exit_code.is_none()).then(|| r.diagnostic.clone()),
                    )
                });
                if let Err(e) = result {
                    warnings.push(format!("Warning: could not record tunnel result: {e:#}"));
                }
            }
        }
        warnings
    }
    pub fn poll(&mut self, m: &mut Model, paths: &AppPaths) {
        self.manager.poll();
        for warning in self.record_completions(&paths.root) {
            m.error = warning;
        }
        let changed = m.tunnels.len() != self.manager.records().len()
            || m.tunnels
                .iter()
                .zip(self.manager.records())
                .any(|(old, new)| old.state != new.state);
        m.tunnels = self.manager.records().to_vec();
        if changed {
            match History::load(&paths.root) {
                Ok(history) => m.history = history.connections,
                Err(e) => m.error = format!("Warning: could not reload tunnel history: {e:#}"),
            }
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.manager.stop_all();
        if let Some(root) = self.root.clone() {
            // On terminal/error unwinding there is no usable UI; preserve the truthful result.
            for warning in self.record_completions(&root) {
                eprintln!("{warning}");
            }
        }
    }
}
impl Model {
    pub(super) fn active_key(&mut self, k: KeyEvent) -> Action {
        let View::Active { selected } = &mut self.view else {
            return Action::None;
        };
        match k.code {
            KeyCode::Esc | KeyCode::F(7) => self.back(),
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                return Action::Quit
            }
            KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                *selected = (*selected + 1).min(self.tunnels.len().saturating_sub(1))
            }
            KeyCode::Char('s') => {
                if let Some(record) = self
                    .tunnels
                    .get(*selected)
                    .filter(|r| r.state == State::Active)
                {
                    return Action::StopTunnel(record.id);
                }
            }
            _ => {}
        }
        Action::None
    }
}
pub(super) fn draw(f: &mut ratatui::Frame, m: &Model, area: ratatui::layout::Rect) {
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Color, Style},
        widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap},
    };
    let View::Active { selected } = m.view else {
        return;
    };
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(8),
        Constraint::Length(3),
    ])
    .split(area);
    f.render_widget(Paragraph::new("Owned tunnels • Active = process running; forwarding readiness unverified\nEsc: return • s: stop selected • q: stop + quit confirmation"), areas[0]);
    let rows = m.tunnels.iter().map(|r| {
        Row::new(vec![
            r.id.to_string(),
            r.snapshot.host.clone(),
            r.snapshot
                .profile
                .clone()
                .unwrap_or_else(|| "(draft)".into()),
            r.snapshot.forward.ssh_args().unwrap_or_default().join(" "),
            match r.state {
                State::Active => "Active",
                State::Completed => "Completed",
                State::Failed => "Error",
                State::Stopped => "Stopped",
            }
            .to_owned(),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
            Constraint::Length(12),
        ],
    )
    .header(Row::new([
        "ID",
        "Host",
        "Profile snapshot",
        "Ports",
        "State",
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Owned tunnels"),
    )
    .row_highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan));
    f.render_stateful_widget(
        table,
        areas[1],
        &mut TableState::default().with_selected(Some(selected)),
    );
    let detail = m.tunnels.get(selected).map(|r| format!("Config at launch: {}\nExit: {}\n{}", r.snapshot.config.display(), r.exit_code.map_or_else(|| "pending".into(), |n| n.to_string()), r.diagnostic)).unwrap_or_else(|| "No owned tunnels. Choose Tunnel only (Ctrl+T) in a forwarding form, then Ctrl+L to start.".into());
    f.render_widget(Paragraph::new(detail).wrap(Wrap { trim: false }), areas[2]);
    f.render_widget(
        Paragraph::new(m.error.as_str())
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false }),
        areas[3],
    );
}
