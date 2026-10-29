//! Explicit target snapshots, preparation, confirmation and native batch results.
use super::*;
use runtime::batch::{Event as BatchEvent, Manager, Options, State as RunState, Target};

#[derive(Default)]
pub(super) struct State {
    pub root: PathBuf,
    pub history_root: Option<PathBuf>,
    pub marks: Vec<Target>,
    sources: BTreeMap<PathBuf, PathBuf>,
    pub manager: Manager,
    pub command: String,
}
impl State {
    pub fn ensure_foreground_idle(&self) -> Result<()> {
        anyhow::ensure!(
            !self.manager.has_active(),
            "Batch active: wait or cancel in B before launching a foreground tool"
        );
        Ok(())
    }
    /// Resolve each distinct access spelling at load/reload, never during drawing.
    pub fn cache_sources(&mut self, hosts: &[Host]) {
        self.sources.clear();
        for host in hosts {
            self.sources
                .entry(host.source_file.clone())
                .or_insert_with(|| {
                    host.source_file
                        .canonicalize()
                        .unwrap_or_else(|_| host.source_file.clone())
                });
        }
    }
    pub fn marked(&self, host: &Host) -> bool {
        let source = self
            .sources
            .get(&host.source_file)
            .unwrap_or(&host.source_file);
        self.marks
            .iter()
            .any(|t| t.alias == host.name && &t.source == source)
    }
    fn freeze(&self, host: &Host) -> Result<Target> {
        Target::freeze(&self.root, host)
    }
    fn add(&mut self, target: Target) {
        if !self
            .marks
            .iter()
            .any(|t| t.alias == target.alias && t.root == target.root)
        {
            self.marks.push(target);
        }
    }
    /// All history writes, including shutdown completions, stay on the owner/UI thread.
    pub fn poll(&mut self) -> Result<Option<History>> {
        let events = self.manager.poll();
        let Some(root) = &self.history_root else {
            return Ok(None);
        };
        if events.is_empty() {
            return Ok(None);
        }
        let mut history = History::load(root)?;
        let mut errors = vec![];
        for event in events {
            let result = match event {
                BatchEvent::Started { index, at } => {
                    history.record_at(&self.manager.records()[index].target.alias, None, at)
                }
                BatchEvent::Finished { index, at } => {
                    let r = &self.manager.records()[index];
                    if r.started_at.is_none() {
                        continue;
                    }
                    history.complete_at(
                        &r.target.alias,
                        r.exit_code,
                        (!r.diagnostic.is_empty()).then(|| r.diagnostic.clone()),
                        at,
                    )
                }
            };
            if let Err(error) = result {
                errors.push(format!("{error:#}"));
            }
        }
        anyhow::ensure!(errors.is_empty(), "batch history: {}", errors.join("; "));
        Ok(Some(history))
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.manager.cancel();
        if let Err(error) = self.poll() {
            eprintln!("Warning: {error:#}");
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub targets: Vec<Target>,
    pub command: Input,
    parallel: Input,
    timeout: Input,
    focus: usize,
    selected: usize,
    pub confirm: bool,
    scroll: usize,
    /// Focused button of the review step or of the "no target" page: 0 is the
    /// safe one (go back), 1 continues.
    button: usize,
}
impl Editor {
    fn new(targets: Vec<Target>) -> Self {
        Self {
            targets,
            command: Input {
                limit: 16384,
                ..Input::default()
            },
            parallel: Input::new("4"),
            timeout: Input::new("30"),
            focus: 0,
            selected: 0,
            confirm: false,
            scroll: 0,
            button: 0,
        }
    }
    /// Stops of the preparation step: three inputs, the target list, two buttons.
    const TARGETS: usize = 3;
    const REVIEW: usize = 4;
    const BACK: usize = 5;
    pub fn input(&mut self) -> Option<&mut Input> {
        if self.confirm || self.targets.is_empty() {
            return None;
        }
        match self.focus {
            0 => Some(&mut self.command),
            1 => Some(&mut self.parallel),
            2 => Some(&mut self.timeout),
            _ => None,
        }
    }
    fn options(&self) -> Result<Options> {
        let options = Options {
            parallel: self
                .parallel
                .value
                .parse()
                .map_err(|_| anyhow::anyhow!("parallel connections must be 1–16"))?,
            timeout: Duration::from_secs(
                self.timeout
                    .value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("timeout must be 1–3600 seconds"))?,
            ),
        };
        options.validate()?;
        anyhow::ensure!(
            !self.targets.is_empty(),
            "mark at least one target first (x or F2)"
        );
        anyhow::ensure!(
            !self.command.value.trim().is_empty(),
            "enter a remote command"
        );
        Ok(options)
    }
}
pub(super) fn mark(m: &mut Model) {
    if let Some(host) = m.selected_host() {
        match m.batch.freeze(&host) {
            Ok(target) => {
                if let Some(i) = m
                    .batch
                    .marks
                    .iter()
                    .position(|t| t.root == target.root && t.alias == target.alias)
                {
                    m.batch.marks.remove(i);
                } else {
                    m.batch.add(target);
                }
                m.error.clear();
            }
            Err(e) => m.error = format!("{e:#}"),
        }
    }
}
pub(super) fn select_visible(m: &mut Model) {
    // visible is the active-alias projection, independent of collapsed header rows.
    let targets = m
        .visible
        .iter()
        .map(|i| m.batch.freeze(&m.all[*i]))
        .collect::<Result<Vec<_>>>();
    match targets {
        Ok(targets) => {
            for target in targets {
                m.batch.add(target);
            }
            m.error.clear();
        }
        Err(e) => m.error = format!("{e:#}"),
    }
}
pub(super) fn open(m: &mut Model) {
    if m.batch.manager.has_active() {
        results(m);
    } else {
        m.open_view(View::Batch(Editor::new(m.batch.marks.clone())));
    }
}
fn results(m: &mut Model) {
    m.open_view(View::BatchResults {
        selected: 0,
        stderr: false,
        offset: 0,
    });
}
pub(super) fn start(m: &mut Model) -> Result<()> {
    let View::Batch(editor) = &m.view else {
        anyhow::bail!("batch preparation is absent")
    };
    anyhow::ensure!(editor.confirm, "explicit confirmation required");
    let options = editor.options()?;
    m.batch.manager.start(
        editor.targets.clone(),
        editor.command.value.clone(),
        options,
    )?;
    m.batch.command = editor.command.value.clone();
    // Keep the completed results reachable from F2/B while preparing the next batch.
    m.back();
    results(m);
    m.error.clear();
    Ok(())
}
pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match &mut m.view {
        View::Batch(editor) if editor.targets.is_empty() => {
            // Step 1 without any target: go back, or explicitly take the hosts
            // the filter shows. The filter is never turned into targets on its own.
            if nav::cancels(k) {
                m.back();
                return Action::None;
            }
            match k.code {
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    editor.button = 1 - editor.button;
                }
                KeyCode::Enter if editor.button == 0 => m.back(),
                KeyCode::Enter => {
                    select_visible(m);
                    if m.batch.marks.is_empty() {
                        if m.error.is_empty() {
                            m.error = "No host is shown by the current filter".into();
                        }
                    } else {
                        m.view = View::Batch(Editor::new(m.batch.marks.clone()));
                    }
                }
                _ => {}
            }
        }
        View::Batch(editor) => {
            if nav::cancels(k) {
                if editor.confirm {
                    editor.confirm = false;
                    editor.scroll = 0;
                } else {
                    m.back();
                }
                return Action::None;
            }
            let last = editor.targets.len().saturating_sub(1);
            if editor.confirm {
                // Step 3: reading the review launches nothing. Only the Launch
                // button, or the explicit Ctrl+L, starts the batch.
                match k.code {
                    KeyCode::Char('l') if ctrl => return Action::StartBatch,
                    KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                        editor.button = 1 - editor.button;
                    }
                    KeyCode::Enter if editor.button == 1 => return Action::StartBatch,
                    KeyCode::Enter => {
                        editor.confirm = false;
                        editor.scroll = 0;
                    }
                    KeyCode::Down => editor.selected = (editor.selected + 1).min(last),
                    KeyCode::Up => editor.selected = editor.selected.saturating_sub(1),
                    KeyCode::PageDown => editor.scroll = editor.scroll.saturating_add(m.page),
                    KeyCode::PageUp => editor.scroll = editor.scroll.saturating_sub(m.page),
                    _ => {}
                }
                return Action::None;
            }
            let review = |editor: &mut Editor, error: &mut String| match editor.options() {
                Ok(_) => {
                    editor.confirm = true;
                    editor.button = 0;
                    editor.scroll = 0;
                    error.clear();
                }
                Err(e) => *error = e.to_string(),
            };
            match k.code {
                KeyCode::Char('l') if ctrl => review(editor, &mut m.error),
                KeyCode::Tab => editor.focus = (editor.focus + 1) % 6,
                KeyCode::BackTab => editor.focus = (editor.focus + 5) % 6,
                // Arrows act in the focused control: they only move in the
                // target list when that list has the focus.
                KeyCode::Up if editor.focus == Editor::TARGETS => {
                    editor.selected = editor.selected.saturating_sub(1)
                }
                KeyCode::Down if editor.focus == Editor::TARGETS => {
                    editor.selected = (editor.selected + 1).min(last)
                }
                KeyCode::Up => editor.focus = editor.focus.saturating_sub(1),
                KeyCode::Down => editor.focus = (editor.focus + 1).min(Editor::BACK),
                KeyCode::Left | KeyCode::Right if editor.focus >= Editor::REVIEW => {
                    editor.focus = Editor::REVIEW + Editor::BACK - editor.focus;
                }
                KeyCode::PageDown => editor.scroll = editor.scroll.saturating_add(m.page),
                KeyCode::PageUp => editor.scroll = editor.scroll.saturating_sub(m.page),
                KeyCode::Enter | KeyCode::Char(' ') if editor.focus == Editor::REVIEW => {
                    review(editor, &mut m.error)
                }
                KeyCode::Enter | KeyCode::Char(' ') if editor.focus == Editor::BACK => m.back(),
                // Enter walks the inputs and stops on Review, which launches nothing.
                KeyCode::Enter if editor.focus < Editor::TARGETS => {
                    editor.focus = if editor.focus == 2 {
                        Editor::REVIEW
                    } else {
                        editor.focus + 1
                    };
                }
                KeyCode::Enter => {}
                _ => {
                    if let Some(input) = editor.input() {
                        input.key(k);
                    }
                }
            }
        }
        View::BatchResults {
            selected,
            stderr,
            offset,
        } => match k.code {
            // Returning keeps the batch running; only `s` cancels it.
            _ if nav::cancels(k) => m.back(),
            KeyCode::Char('q') => m.back(),
            KeyCode::Char('s') => return Action::CancelBatch,
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                *offset = 0;
            }
            KeyCode::Down => {
                *selected = (*selected + 1).min(m.batch.manager.records().len().saturating_sub(1));
                *offset = 0;
            }
            KeyCode::Tab => {
                *stderr = !*stderr;
                *offset = 0;
            }
            KeyCode::PageDown => *offset = offset.saturating_add(m.page),
            KeyCode::PageUp => *offset = offset.saturating_sub(m.page),
            KeyCode::Home => *offset = 0,
            _ => {}
        },
        _ => {}
    }
    Action::None
}
pub(super) fn show_results(m: &mut Model) {
    results(m);
}
pub(super) fn draw(f: &mut ratatui::Frame, m: &mut Model, area: ratatui::layout::Rect) {
    use ratatui::{
        layout::{Constraint, Layout, Rect},
        style::{Color, Style},
        text::Line,
        widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap},
    };
    let border = |title: String| Block::default().borders(Borders::ALL).title(title);
    let selected_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Rgb(0, 173, 216));
    // Compact terminals retain the command, at least one target, and the action footer.
    let height = area.height;
    let footer_height = if height >= 12 { 2 } else { 1 };
    let areas = Layout::vertical([
        Constraint::Length(if height >= 12 { 4 } else { 2 }),
        Constraint::Length(if height >= 18 { 7 } else { 3 }),
        Constraint::Min(1),
        Constraint::Length(footer_height),
        Constraint::Length(u16::from(!m.error.is_empty())),
    ])
    .split(area);
    m.page = usize::from(areas[2].height.max(1));
    match &mut m.view {
        View::Batch(editor) => {
            let title = if editor.confirm {
                "Confirm batch"
            } else {
                "Batch"
            };
            // Where the journey stands: Targets → Command → Review → Results.
            let step = if editor.targets.is_empty() {
                0
            } else if editor.confirm {
                2
            } else {
                1
            };
            let steps = ["Targets", "Command", "Review", "Results"]
                .iter()
                .enumerate()
                .map(|(index, name)| match index.cmp(&step) {
                    std::cmp::Ordering::Less => format!("{name} ✓"),
                    std::cmp::Ordering::Equal => format!("[{name}]"),
                    std::cmp::Ordering::Greater => (*name).to_owned(),
                })
                .collect::<Vec<_>>()
                .join(" → ");
            let heading = format!("{title}: {} targets • {steps}", editor.targets.len());
            if editor.targets.is_empty() {
                // Step 1 with nothing marked: say why, and name the two ways out.
                f.render_widget(Paragraph::new(heading), areas[0]);
                let body = Rect {
                    y: areas[1].y,
                    height: areas[1].height + areas[2].height,
                    ..areas[1]
                };
                let hosts = m.visible.len();
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from("No targets selected."),
                        Line::from(
                            "A batch only runs on the aliases you marked: the filter is never turned into targets on its own.",
                        ),
                        Line::from("Back in the list, x marks the selected alias."),
                        Line::from(""),
                        Line::from(vec![
                            nav::button("Back to the list", editor.button == 0),
                            ratatui::text::Span::raw("  "),
                            nav::button(
                                &format!("Add the {hosts} host(s) shown by the filter"),
                                editor.button == 1,
                            ),
                        ]),
                    ])
                    .wrap(Wrap { trim: false }),
                    body,
                );
                f.render_widget(
                    Paragraph::new("Tab/←→: choose • Enter: confirm • Esc: return"),
                    areas[3],
                );
            } else {
                let focus = editor.focus;
                let (label, input) = match focus {
                    1 if !editor.confirm => ("Parallel: ", &editor.parallel),
                    2 if !editor.confirm => ("Timeout: ", &editor.timeout),
                    _ => ("Cmd: ", &editor.command),
                };
                let input_width = usize::from(areas[0].width)
                    .saturating_sub(label.len())
                    .max(1);
                let (value, cursor) = input.viewport(!editor.confirm && focus < 3, input_width);
                let buttons = if editor.confirm {
                    Line::from(vec![
                        nav::button("Back to edit", editor.button == 0),
                        ratatui::text::Span::raw("  "),
                        nav::button(
                            &format!("Launch on {} target(s)", editor.targets.len()),
                            editor.button == 1,
                        ),
                    ])
                } else {
                    Line::from(vec![
                        nav::button("Review", focus == Editor::REVIEW),
                        ratatui::text::Span::raw("  "),
                        nav::button("Back", focus == Editor::BACK),
                    ])
                };
                f.render_widget(
                    Paragraph::new(vec![
                        Line::from(heading),
                        Line::from(format!("{label}{value}")),
                        Line::from(format!(
                            "Parallel: {} (1–16) • Timeout: {}s (1–3600)",
                            editor.parallel.value, editor.timeout.value
                        )),
                        buttons,
                    ]),
                    areas[0],
                );
                if !editor.confirm
                    && focus < 3
                    && areas[0].height > 1
                    && usize::from(areas[0].width) > label.len()
                {
                    f.set_cursor_position((
                        areas[0].x + label.len() as u16 + cursor.unwrap_or(0),
                        areas[0].y + 1,
                    ));
                }
                let rows = editor.targets.iter().map(|t| Row::new([t.alias.clone()]));
                // The list says when the arrows belong to it.
                let listing = editor.confirm || focus == Editor::TARGETS;
                f.render_stateful_widget(
                    Table::new(rows, [Constraint::Min(1)])
                        .block(border(if listing {
                            "● Frozen targets — ↑↓ select".into()
                        } else {
                            "Frozen targets".into()
                        }))
                        .row_highlight_style(selected_style),
                    areas[1],
                    &mut TableState::default().with_selected(Some(editor.selected)),
                );
                let text=editor.targets.get(editor.selected).map(|t|format!("Alias: {}\nSource: {}\nSSH root: {}\nCommand: {}\nParallel: {} • Timeout: {}s\nNative SSH, no prompts. Stopping local SSH cannot promise to stop detached remote commands.",t.alias,t.source.display(),t.root.display(),editor.command.value,editor.parallel.value,editor.timeout.value)).unwrap_or_default();
                details::render_rows(
                    f,
                    areas[2],
                    text.lines()
                        .map(|line| Line::from(line.to_owned()))
                        .collect(),
                    &mut editor.scroll,
                );
                let footer = if editor.confirm {
                    "Tab/←→: choose • Enter: confirm the choice • Ctrl+L: launch • Esc: edit\n↑↓: review targets • PgUp/PgDn: details"
                } else {
                    "Tab/↑↓: next control • Enter: next field, then Review • Ctrl+L: review • Esc: return\nPgUp/PgDn: full command/details"
                };
                f.render_widget(Paragraph::new(footer), areas[3]);
            }
        }
        View::BatchResults {
            selected,
            stderr,
            offset,
        } => {
            let records = m.batch.manager.records();
            let count = |state| records.iter().filter(|r| r.state == state).count();
            f.render_widget(Paragraph::new(format!("Batch results: {} targets\nOK {} • failed {} • running {} • pending {}\nTimed out {} • cancelled {}\nCommand: {}",records.len(),count(RunState::Succeeded),count(RunState::Failed),count(RunState::Running),count(RunState::Pending),count(RunState::TimedOut),count(RunState::Cancelled),m.batch.command)),areas[0]);
            let rows = records.iter().map(|r| {
                Row::new([
                    r.target.alias.clone(),
                    format!(
                        "{} ({})",
                        r.state.label(),
                        r.exit_code.map_or("—".into(), |n| n.to_string())
                    ),
                ])
            });
            f.render_stateful_widget(
                Table::new(
                    rows,
                    [Constraint::Percentage(45), Constraint::Percentage(55)],
                )
                .block(border("Host / state (exit)".into()))
                .row_highlight_style(selected_style),
                areas[1],
                &mut TableState::default().with_selected(Some(*selected)),
            );
            let text =
                records
                    .get(*selected)
                    .map(|r| {
                        let output = if *stderr { &r.stderr } else { &r.stdout };
                        format!(
                        "{}  {}\n{} — {} • {} bytes{}\nSource: {}\nRoot: {}\nCommand: {}\n{}\n{}",
                        // Two named tabs; the open one carries the marker.
                        if *stderr { "[  Output ]" } else { "[▶ Output ]" },
                        if *stderr { "[▶ Errors ]" } else { "[  Errors ]" },
                        r.target.alias,
                        if *stderr { "stderr" } else { "stdout" },
                        output.bytes.len(),
                        if output.truncated {
                            " [TRUNCATED at 256 KiB]"
                        } else {
                            ""
                        },
                        r.target.source.display(),
                        r.target.root.display(),
                        m.batch.command,
                        r.diagnostic,
                        output.text()
                    )
                    })
                    .unwrap_or_else(|| "No batch results yet".into());
            details::render_rows(
                f,
                areas[2],
                text.lines()
                    .map(|line| Line::from(line.to_owned()))
                    .collect(),
                offset,
            );
            f.render_widget(Paragraph::new("s: cancel batch • Tab: Output / Errors • Esc/q: return, batch keeps running\n↑↓: host • PgUp/PgDn: output"),areas[3]);
        }
        _ => {}
    }
    f.render_widget(
        Paragraph::new(m.error.as_str())
            .style(Style::default().fg(Color::Red))
            .wrap(Wrap { trim: false }),
        areas[4],
    );
}
