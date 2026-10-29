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
    /// What is about to be written, in words, above the exact bytes.
    summary: Vec<String>,
    /// Name of the confirming button: Save, Delete, Move, Copy or Restore.
    verb: &'static str,
    /// 0 the diff being read, 1 the confirming button, 2 Back.
    focus: usize,
    notice: bool,
}
impl Review {
    const CONFIRM: usize = 1;
    const BACK: usize = 2;
}
/// Operation, block, files and what happens to personal data.
fn summarize(action: &Action) -> (&'static str, Vec<String>) {
    let data = "Favorite, note and tunnel profiles are ssho data: this write does not change them.";
    match action {
        Action::Save(Some(original), host, _) => {
            let mut lines = vec![
                format!("Operation: EDIT host '{}'", original.name),
                format!("File: {}", original.source_file.display()),
            ];
            let before = if original.aliases.is_empty() {
                vec![original.name.clone()]
            } else {
                original.aliases.clone()
            };
            let after = if host.aliases.is_empty() {
                vec![host.name.clone()]
            } else {
                host.aliases.clone()
            };
            lines.push(if before == after {
                data.to_owned()
            } else {
                format!(
                    "Aliases: {} → {}. Favorite, note and tunnel profiles follow a renamed alias.",
                    before.join(" "),
                    after.join(" ")
                )
            });
            ("Save", lines)
        }
        Action::Save(None, host, target) => (
            "Save",
            vec![
                format!("Operation: ADD host '{}'", host.name),
                format!("File: {}", target.display()),
                "A new host starts without favorite or note.".to_owned(),
            ],
        ),
        Action::SaveCopy(host, target) => (
            "Copy",
            vec![
                format!("Operation: COPY to new host '{}'", host.name),
                format!("File: {}", target.display()),
                "The original is unchanged; its favorite and note are not copied.".to_owned(),
            ],
        ),
        Action::Delete(host) => (
            "Delete",
            vec![
                format!("Operation: DELETE host '{}'", host.name),
                format!("File: {}", host.source_file.display()),
                "Its favorite, note and tunnel profiles stay in the ssho data.".to_owned(),
            ],
        ),
        Action::MoveTo(host, target) => (
            "Move",
            vec![
                format!("Operation: MOVE host '{}'", host.name),
                format!(
                    "From: {} → To: {}",
                    host.source_file.display(),
                    target.display()
                ),
                data.to_owned(),
            ],
        ),
        _ => ("Restore", vec![]),
    }
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
    let (verb, mut summary) = summarize(&action);
    if summary.is_empty() {
        summary = vec![
            format!("Operation: RESTORE a backup — {title}"),
            "The current file is backed up first; notes, favorites and profiles are not restored."
                .to_owned(),
        ];
    }
    m.error.clear();
    m.open_view(View::Changes(Review {
        plan: Arc::new(plan),
        action,
        text,
        title,
        rows: 1,
        summary,
        verb,
        focus: 0,
        notice: false,
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
                View::Keys(_)
                    | View::Host(_)
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
    let texts = m.view.selector_texts().unwrap_or_default();
    if k.code == KeyCode::Esc
        || (k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c'))
    {
        m.back();
        m.error.clear();
        return Action::None;
    }
    match &mut m.view {
        View::Changes(review) => match k.code {
            // Reading the diff writes nothing: Enter only acts on a button, and
            // Ctrl+S is the explicit confirmation shown in the footer.
            KeyCode::Char('s') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                return Action::ConfirmChanges
            }
            KeyCode::Tab => review.focus = (review.focus + 1) % 3,
            KeyCode::BackTab => review.focus = (review.focus + 2) % 3,
            KeyCode::Left | KeyCode::Right if review.focus != 0 => {
                review.focus = Review::CONFIRM + Review::BACK - review.focus;
            }
            KeyCode::Enter if review.focus == Review::CONFIRM => return Action::ConfirmChanges,
            KeyCode::Enter if review.focus == Review::BACK => {
                m.back();
                m.error.clear();
            }
            KeyCode::Enter => review.notice = true,
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
        // Typing narrows the list by date, file or kind of change; Enter only
        // opens the restoration preview, which writes nothing by itself.
        View::Backups { entries, selected } => {
            if select::key(selected, &texts, Some(&mut m.selector_query), k)
                == select::Outcome::Apply
                && !entries.is_empty()
            {
                return Action::RestoreBackup(*selected);
            }
        }
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
    let summary = match &m.view {
        View::Changes(review) if area.height >= 12 => review.summary.clone(),
        _ => vec![],
    };
    let layout = Layout::vertical([
        Constraint::Length(summary.len() as u16),
        Constraint::Min(1),
        Constraint::Length(4),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(
            summary
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    Line::styled(
                        line.clone(),
                        if index == 0 {
                            Style::default().add_modifier(ratatui::style::Modifier::BOLD)
                        } else {
                            Style::default()
                        },
                    )
                })
                .collect::<Vec<_>>(),
        ),
        layout[0],
    );
    let parts = [layout[1], layout[2]];
    m.page = parts[0].height.saturating_sub(2).max(1) as usize;
    let mut buttons = None;
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
            buttons = Some((review.verb, review.focus, review.notice));
            "Tab: choose • Enter: confirm the choice • Ctrl+S: confirm exact changes • Esc: return unchanged • ↑↓/PgUp/PgDn: scroll"
        }
        View::Backups { entries, selected } => {
            // Entries wrap to variable heights; anchor the selected destination
            // at the top so it remains visible even in a very short terminal.
            let texts: Vec<String> = entries
                .iter()
                .map(|entry| entry.description.clone())
                .collect();
            let shown = select::visible(&texts, &m.selector_query.value);
            let lines: Vec<_> = if entries.is_empty() {
                vec![Line::from("No backups yet")]
            } else if shown.is_empty() {
                vec![Line::from("No backup matches the search")]
            } else {
                entries
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| shown.contains(i) && *i >= *selected)
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
                    .block(Block::default().borders(Borders::ALL).title(
                        if m.selector_query.value.is_empty() {
                            "Backups — explicit recorded destinations — type to search".to_owned()
                        } else {
                            format!(
                                "Backups — search: {} ({} of {})",
                                m.selector_query.value,
                                shown.len(),
                                entries.len()
                            )
                        },
                    ))
                    .wrap(Wrap { trim: false }),
                parts[0],
            );
            "Enter: preview restoration • type: search by date or file • Esc/F8: return • unknown provenance: restore disabled"
        }
        _ => return,
    };
    let mut lines = vec![];
    if let Some((verb, focus, notice)) = buttons {
        lines.push(Line::from(vec![
            nav::button(verb, focus == Review::CONFIRM),
            ratatui::text::Span::raw("  "),
            nav::button("Back", focus == Review::BACK),
            ratatui::text::Span::raw(if focus == 0 {
                if notice {
                    "  Nothing written: Tab reaches the buttons, Ctrl+S confirms."
                } else {
                    "  (reading the diff)"
                }
            } else {
                ""
            }),
        ]));
    }
    lines.push(Line::from(footer));
    lines.extend(m.error.lines().map(|line| Line::from(line.to_owned())));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), parts[1]);
}
