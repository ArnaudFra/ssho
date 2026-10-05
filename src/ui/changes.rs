//! Review keeps the original editor on the parent stack until confirmed publication.
use super::*;
use config::preview::{self, PreparedChange};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct Review {
    plan: Arc<PreparedChange>,
    action: Action,
    text: String,
    title: String,
    rows: usize,
}
pub(super) fn open(m: &mut Model, action: Action, root: &Path) -> Result<()> {
    // Native copy preflight may wait on ssh -G; keep batch deadlines and cancellation live.
    if matches!(action, Action::SaveCopy(..)) {
        m.batch.ensure_foreground_idle()?;
    }
    let (plan, title) = match &action {
        Action::Save(Some(original), host, _) => (
            preview::prepare_update(original, host)?,
            "Review SSH changes",
        ),
        Action::Save(None, host, target) => {
            (preview::prepare_add(host, target)?, "Review SSH changes")
        }
        Action::SaveCopy(host, target) => (
            preview::prepare_copy(host, target, root)?,
            "Review SSH copy",
        ),
        Action::Delete(host) => (preview::prepare_delete(host)?, "DELETE SSH HOST — review"),
        Action::MoveTo(host, target) => (preview::prepare_move(host, target)?, "Review SSH move"),
        _ => anyhow::bail!("action does not modify SSH configuration"),
    };
    show(m, action, plan, title.into());
    Ok(())
}
fn show(m: &mut Model, action: Action, plan: PreparedChange, title: String) {
    let text = plan.diff();
    m.error.clear();
    m.open_view(View::Changes(Review {
        plan: Arc::new(plan),
        action,
        text,
        title,
        rows: 1,
    }));
}
pub(super) fn apply(m: &mut Model, app: &Path) -> Result<Action> {
    let View::Changes(review) = &m.view else {
        anyhow::bail!("no prepared changes to confirm");
    };
    review.plan.apply(app)?;
    let action = review.action.clone();
    // Completion depends on the editor and its original parent.
    m.back();
    Ok(action)
}
pub(super) fn open_backups(m: &mut Model, app: &Path) -> Result<()> {
    anyhow::ensure!(
        !std::iter::once(&m.view)
            .chain(m.parents.iter().map(|(view, _)| view))
            .any(|view| matches!(
                view,
                View::Host(_)
                    | View::Forward(_)
                    | View::Note(_)
                    | View::Profiles(_)
                    | View::Changes(_)
                    | View::Files { .. }
            )),
        "Return to the host list before restoring backups; your editor is unchanged"
    );
    let entries = config::backups::list(app)?;
    m.error.clear();
    m.open_view(View::Backups {
        entries,
        selected: 0,
    });
    Ok(())
}
pub(super) fn open_restore(m: &mut Model, index: usize) -> Result<()> {
    let View::Backups { entries, .. } = &m.view else {
        anyhow::bail!("backup list is not open");
    };
    let backup = entries
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("no backup selected"))?;
    let plan = config::backups::prepare_restore(backup)?;
    let title = format!("Review restoration — {}", backup.description);
    show(m, Action::RestoreBackup(index), plan, title);
    Ok(())
}
pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    if k.code == KeyCode::Esc
        || (k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c'))
    {
        m.back();
        m.error.clear();
        return Action::None;
    }
    match &mut m.view {
        View::Changes(review) => match k.code {
            KeyCode::Enter => return Action::ConfirmChanges,
            KeyCode::Up => m.scroll = m.scroll.saturating_sub(1),
            KeyCode::Down => m.scroll = (m.scroll + 1).min(review.rows.saturating_sub(m.page)),
            KeyCode::PageUp => m.scroll = m.scroll.saturating_sub(m.page),
            KeyCode::PageDown => {
                m.scroll = (m.scroll + m.page).min(review.rows.saturating_sub(m.page))
            }
            KeyCode::Home => m.scroll = 0,
            KeyCode::End => m.scroll = review.rows.saturating_sub(m.page),
            _ => {}
        },
        View::Backups { entries, selected } => match k.code {
            KeyCode::Up => *selected = selected.saturating_sub(1),
            KeyCode::Down => *selected = (*selected + 1).min(entries.len().saturating_sub(1)),
            KeyCode::Enter if !entries.is_empty() => return Action::RestoreBackup(*selected),
            _ => {}
        },
        _ => {}
    }
    Action::None
}
pub(super) fn draw(f: &mut ratatui::Frame, m: &mut Model, area: ratatui::layout::Rect) {
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Color, Style},
        text::Line,
        widgets::{Block, Borders, Paragraph, Wrap},
    };
    let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(4)]).split(area);
    m.page = parts[0].height.saturating_sub(2).max(1) as usize;
    let footer = match &mut m.view {
        View::Changes(review) => {
            let width = parts[0].width.saturating_sub(2).max(1) as usize;
            let mut lines = Vec::new();
            for line in review.text.split('\n') {
                let color = if line.starts_with('+') {
                    Color::Green
                } else if line.starts_with('-') {
                    Color::LightRed
                } else {
                    Color::White
                };
                let visible = line.replace('\r', "␍").replace('\t', "→   ");
                let mut chunk = String::new();
                let mut used = 0;
                for c in visible.chars() {
                    let cells = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if used + cells > width && !chunk.is_empty() {
                        lines.push(Line::styled(
                            std::mem::take(&mut chunk),
                            Style::default().fg(color),
                        ));
                        used = 0;
                    }
                    chunk.push(c);
                    used += cells;
                }
                lines.push(Line::styled(chunk, Style::default().fg(color)));
            }
            review.rows = lines.len();
            m.scroll = m.scroll.min(review.rows.saturating_sub(m.page));
            f.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(review.title.as_str()),
                    )
                    .scroll((m.scroll.min(u16::MAX as usize) as u16, 0)),
                parts[0],
            );
            "Enter: confirm exact changes • Esc: return unchanged • ↑↓/PgUp/PgDn: scroll"
        }
        View::Backups { entries, selected } => {
            // Entries wrap to variable heights; anchor the selected destination
            // at the top so it remains visible even in a very short terminal.
            let start = *selected;
            let lines: Vec<_> = if entries.is_empty() {
                vec![Line::from("No backups yet")]
            } else {
                entries
                    .iter()
                    .enumerate()
                    .skip(start)
                    .map(|(i, entry)| {
                        Line::styled(
                            format!(
                                "{} {}",
                                if i == *selected { "›" } else { " " },
                                entry.description
                            ),
                            if i == *selected {
                                Style::default().bg(Color::Cyan).fg(Color::Black)
                            } else {
                                Style::default().fg(Color::White)
                            },
                        )
                    })
                    .collect()
            };
            f.render_widget(
                Paragraph::new(lines)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Backups — explicit recorded destinations"),
                    )
                    .wrap(Wrap { trim: false }),
                parts[0],
            );
            "Enter: preview restoration • Esc/F8: return • unknown provenance: restore disabled"
        }
        _ => return,
    };
    f.render_widget(
        Paragraph::new(format!("{footer}\n{}", m.error)).wrap(Wrap { trim: false }),
        parts[1],
    );
}
