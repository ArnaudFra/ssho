//! Key assistant navigation; native operations leave this draft on screen on error.
use super::*;
use runtime::keys::{Entry, PublicKey, Scanner};
#[derive(Default)]
pub(super) struct State {
    pub entries: Vec<Entry>,
    pub scanner: Scanner,
    session: Vec<PathBuf>,
    refresh_pending: Option<(PathBuf, Vec<Host>)>,
}
#[derive(Clone, Debug)]
pub(super) enum Mode {
    List,
    Generate {
        path: Input,
        comment: Input,
        focus: usize,
    },
    Deploy {
        public: PublicKey,
        private: PathBuf,
        alias: String,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub picker: bool,
    pub target: Option<Host>,
    pub selected: usize,
    pub mode: Mode,
    details: bool,
    details_scroll: usize,
    notice: String,
    preferred: Option<PathBuf>,
}
pub(super) fn open_view(m: &mut Model, picker: bool) {
    let target = if picker { None } else { m.selected_host() };
    m.open_view(View::Keys(Editor {
        picker,
        target,
        selected: 0,
        mode: Mode::List,
        details: false,
        details_scroll: 0,
        notice: String::new(),
        preferred: None,
    }));
    m.error.clear();
}
impl State {
    fn refresh(&mut self, home: PathBuf, hosts: Vec<Host>) {
        if self.scanner.busy() {
            // One coalesced request; generation during a scan must not lose its candidate.
            self.refresh_pending = Some((home, hosts));
        } else {
            self.scanner.start(home, hosts, self.session.clone());
        }
    }
    fn remember_generated(&mut self, generated: PathBuf) {
        self.session.retain(|path| path != &generated);
        if self.session.len() == runtime::keys::INVENTORY_LIMIT {
            self.session.remove(0);
        }
        self.session.push(generated);
    }
}
pub(super) fn refresh(m: &mut Model) {
    if let Some(home) = std::env::var_os("HOME") {
        m.keys.refresh(PathBuf::from(home), m.all.clone());
    } else {
        m.error = "HOME unavailable; cannot inventory ~/.ssh".into();
    }
}
pub(super) fn input(editor: &mut Editor) -> Option<&mut Input> {
    if editor.details {
        return None;
    }
    if let Mode::Generate {
        path,
        comment,
        focus,
    } = &mut editor.mode
    {
        Some(if *focus == 0 { path } else { comment })
    } else {
        None
    }
}
pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::Keys(editor) = &mut m.view else {
        return Action::None;
    };
    if k.code == KeyCode::F(4) {
        editor.details = !editor.details;
        editor.details_scroll = 0;
        return Action::None;
    }
    if editor.details {
        match k.code {
            _ if nav::cancels(k) => editor.details = false,
            KeyCode::Down => editor.details_scroll = editor.details_scroll.saturating_add(1),
            KeyCode::Up => editor.details_scroll = editor.details_scroll.saturating_sub(1),
            KeyCode::PageDown => {
                editor.details_scroll = editor.details_scroll.saturating_add(m.page)
            }
            KeyCode::PageUp => editor.details_scroll = editor.details_scroll.saturating_sub(m.page),
            KeyCode::Home => editor.details_scroll = 0,
            KeyCode::End => editor.details_scroll = usize::MAX,
            _ => {}
        }
        return Action::None;
    }
    if nav::cancels(k) {
        if matches!(editor.mode, Mode::List) {
            m.back();
        } else {
            editor.mode = Mode::List;
        }
        m.error.clear();
        return Action::None;
    }
    let previous_selection = editor.selected;
    match &mut editor.mode {
        Mode::Generate { focus, .. } => match k.code {
            KeyCode::Tab | KeyCode::BackTab => *focus = 1 - *focus,
            KeyCode::Enter => return Action::GenerateKey,
            _ => {
                if let Some(input) = input(editor) {
                    input.key(k);
                }
            }
        },
        Mode::Deploy { .. } => match k.code {
            KeyCode::Enter => return Action::DeployKey,
            KeyCode::Down => m.scroll = m.scroll.saturating_add(1),
            KeyCode::Up => m.scroll = m.scroll.saturating_sub(1),
            KeyCode::PageDown => m.scroll = m.scroll.saturating_add(m.page),
            KeyCode::PageUp => m.scroll = m.scroll.saturating_sub(m.page),
            KeyCode::Home => m.scroll = 0,
            _ => {}
        },
        Mode::List => match k.code {
            KeyCode::Up | KeyCode::Char('k') => {
                editor.selected = if editor.selected == usize::MAX {
                    m.keys.entries.len().saturating_sub(1)
                } else {
                    editor.selected.saturating_sub(1)
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                editor.selected = if editor.selected == usize::MAX {
                    0
                } else {
                    editor
                        .selected
                        .saturating_add(1)
                        .min(m.keys.entries.len().saturating_sub(1))
                }
            }
            KeyCode::PageUp => {
                editor.selected = if editor.selected == usize::MAX {
                    0
                } else {
                    editor.selected.saturating_sub(m.page)
                }
            }
            KeyCode::PageDown => {
                editor.selected = editor
                    .selected
                    .saturating_add(m.page)
                    .min(m.keys.entries.len().saturating_sub(1))
            }
            KeyCode::Home => editor.selected = 0,
            KeyCode::End => editor.selected = m.keys.entries.len().saturating_sub(1),
            KeyCode::F(5) => return Action::RefreshKeys,
            KeyCode::Char('n') => {
                editor.mode = Mode::Generate {
                    path: Input::default(),
                    comment: Input::default(),
                    focus: 0,
                }
            }
            KeyCode::Char('a') => return Action::AddKey,
            KeyCode::Char('t') if !editor.picker => return Action::AttachKey,
            KeyCode::Char('d') if !editor.picker => {
                if let (Some(entry), Some(target)) =
                    (m.keys.entries.get(editor.selected), &editor.target)
                {
                    if let Some(public) = &entry.public {
                        m.scroll = 0;
                        editor.mode = Mode::Deploy {
                            public: public.clone(),
                            private: entry.private.clone(),
                            alias: target.name.clone(),
                        };
                    } else {
                        m.error =
                            "No reviewed public fingerprint; refresh or fix the public file".into();
                    }
                } else {
                    m.error = "Open Keys with an exact host alias selected to deploy".into();
                }
            }
            KeyCode::Enter if editor.picker => {
                if let Some(entry) = m.keys.entries.get(editor.selected) {
                    if !entry.private.is_absolute() {
                        m.error = "Unresolved identity cannot fill the draft".into();
                        return Action::None;
                    }
                    let value = entry.private.to_string_lossy().into_owned();
                    m.back();
                    if let View::Host(form) = &mut m.view {
                        form.inputs[4].set(&value);
                    }
                    return Action::None;
                }
            }
            _ => {}
        },
    }
    if editor.selected != previous_selection {
        editor.notice.clear();
        editor.preferred = None;
    }
    Action::None
}
pub(super) fn attach(m: &mut Model, root: &Path) -> Result<()> {
    let View::Keys(editor) = &m.view else {
        anyhow::bail!("key assistant is closed")
    };
    let host = editor
        .target
        .as_ref()
        .context("Open Keys with an exact host alias selected to attach")?;
    let entry = m
        .keys
        .entries
        .get(editor.selected)
        .context("select a key")?;
    anyhow::ensure!(entry.private.is_absolute(), "identity path is unresolved");
    let next = runtime::keys::attach(host, &entry.private);
    let action = Action::Save(Some(host.clone()), Box::new(next), host.source_file.clone());
    changes::open(m, action, root)
}
use anyhow::Context;
/// Called only after the batch foreground guard and terminal suspension.
pub(super) fn native(m: &mut Model, action: &Action, root: &Path) -> Result<()> {
    m.batch.ensure_foreground_idle()?;
    let View::Keys(editor) = &m.view else {
        anyhow::bail!("key assistant is closed")
    };
    match action {
        Action::GenerateKey => {
            let Mode::Generate { path, comment, .. } = &editor.mode else {
                anyhow::bail!("generation form is closed")
            };
            let generated = PathBuf::from(&path.value);
            runtime::keys::generate(
                Path::new(&path.value),
                &comment.value,
                Path::new("ssh-keygen"),
            )?;
            m.keys.remember_generated(generated.clone());
            if let View::Keys(editor) = &mut m.view {
                editor.preferred = Some(generated);
                editor.selected = usize::MAX;
                editor.notice = "Generated key awaiting inventory; F5 refresh / F4 details".into();
            }
        }
        Action::AddKey => {
            let entry = m
                .keys
                .entries
                .get(editor.selected)
                .context("select a private key")?;
            runtime::keys::agent_add(&entry.private)?;
        }
        Action::DeployKey => {
            let Mode::Deploy {
                public,
                private,
                alias,
            } = &editor.mode
            else {
                anyhow::bail!("deployment has not been reviewed")
            };
            // Exact declared alias, frozen in review, remains present before launch.
            config::find_host(root, alias)?;
            runtime::keys::Deployment::prepare(public, private, root, alias)?
                .run(Path::new("ssh-copy-id"))?;
        }
        _ => anyhow::bail!("not a native key operation"),
    }
    if let View::Keys(editor) = &mut m.view {
        editor.mode = Mode::List;
    }
    m.error = "Native key operation completed".into();
    refresh(m);
    Ok(())
}
fn selected_details(entry: Option<&Entry>) -> String {
    entry
        .map(|entry| {
            format!(
                "Private: {}\nFingerprint: {}\nUses: {}\nDiagnostic: {}",
                entry.private.display(),
                entry
                    .public
                    .as_ref()
                    .map(|p| p.fingerprint.as_str())
                    .unwrap_or("unavailable"),
                entry.usages.join(", "),
                entry.diagnostic
            )
        })
        .unwrap_or_else(|| "No selected key; use ↑/↓ to choose.".into())
}
const LIST_HELP: &str = "↑/↓ PgUp/PgDn Home/End: select key\nEnter: choose in picker • n: generate • a: agent\nt: attach (review; keep other identities)\nd: deploy (review public key + exact alias)\nF5: refresh • F4: full details/error • Esc: back";

pub(super) fn draw(f: &mut ratatui::Frame, m: &mut Model, area: ratatui::layout::Rect) {
    use ratatui::{
        layout::{Constraint, Layout},
        style::{Color, Style},
        text::Line,
        widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    };
    let View::Keys(editor) = &mut m.view else {
        return;
    };
    let entry = m.keys.entries.get(editor.selected);
    if editor.details {
        let parts = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        f.render_widget(Paragraph::new("Keys: full details / error"), parts[0]);
        m.page = parts[1].height.max(1) as usize;
        let mode = match &editor.mode {
            Mode::Generate { path, comment, .. } => format!("Generation path: {}\nComment: {}\nTab: next field • Enter: generate in native terminal", path.value, comment.value),
            Mode::Deploy { public, private, alias } => format!("Reviewed public: {}\nPrivate: {}\nFingerprint: {}\nExact alias: {alias}\nRoot: {}", public.path.display(), private.display(), public.fingerprint, m.batch.root.display()),
            Mode::List => LIST_HELP.into(),
        };
        let text = format!(
            "{}\n{}\n{}\n{mode}",
            m.error,
            editor.notice,
            selected_details(entry)
        );
        details::render_rows(
            f,
            parts[1],
            text.lines()
                .map(|line| Line::from(line.to_owned()))
                .collect(),
            &mut editor.details_scroll,
        );
        f.render_widget(Paragraph::new("↑↓ PgUp/Dn Home/End • Esc"), parts[2]);
        return;
    }
    let compact = area.height < 12 || area.width < 60;
    let parts = Layout::vertical([
        Constraint::Length(if compact { 1 } else { 3 }),
        Constraint::Min(1),
        Constraint::Length(if compact { 2 } else { 4 }),
    ])
    .split(area);
    let target = editor
        .target
        .as_ref()
        .map(|h| h.name.as_str())
        .unwrap_or("none");
    f.render_widget(
        Paragraph::new(format!(
            "Keys — {} • target: {target}\nPublic files only • declared usages • {}",
            if editor.picker {
                "IdentityFile picker"
            } else {
                "OpenSSH assistant"
            },
            if m.keys.scanner.busy() {
                "Scanning (bounded)…"
            } else {
                "F5 refresh"
            }
        )),
        parts[0],
    );
    let help = match &mut editor.mode {
        Mode::List => {
            let item_height = if compact { 1 } else { 3 };
            m.page = (parts[1].height as usize / item_height).max(1);
            let items = m
                .keys
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let name = entry
                        .private
                        .file_name()
                        .unwrap_or(entry.private.as_os_str())
                        .to_string_lossy();
                    let label = format!("{} {}", index + 1, name);
                    // Keep the distinguishing filename tail visible; F4 exposes every cell.
                    let tail = Input::new(&label).viewport(true, parts[1].width as usize).0;
                    let mut lines = vec![Line::from(if compact {
                        tail
                    } else {
                        entry.private.display().to_string()
                    })];
                    if !compact {
                        lines.push(Line::from(
                            entry
                                .public
                                .as_ref()
                                .map(|p| p.fingerprint.clone())
                                .unwrap_or_else(|| entry.diagnostic.clone()),
                        ));
                        lines.push(Line::from(format!(
                            "Uses: {} • {}",
                            entry.usages.join(", "),
                            entry.diagnostic
                        )));
                    }
                    ListItem::new(lines)
                })
                .collect::<Vec<_>>();
            let selected = (editor.selected < m.keys.entries.len()).then_some(editor.selected);
            f.render_stateful_widget(
                List::new(items).highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black)),
                parts[1],
                &mut ListState::default().with_selected(selected),
            );
            if compact {
                "↑↓ select • F4 details/error"
            } else {
                "↑↓ select • n generate • a agent • t attach • d deploy\nEnter choose (picker) • F4 details/error • F5 refresh • Esc back"
            }
        }
        Mode::Generate {
            path,
            comment,
            focus,
        } => {
            let rows = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(parts[1]);
            for (index, (label, input)) in
                [("Absolute private key path", path), ("Comment", comment)]
                    .into_iter()
                    .enumerate()
            {
                // At heights below the supported eight rows, render the active field first.
                let row = if parts[1].height < 4 {
                    if index == *focus {
                        parts[1]
                    } else {
                        ratatui::layout::Rect::default()
                    }
                } else {
                    rows[index]
                };
                let (text, cursor) = input.viewport(*focus == index, row.width as usize);
                f.render_widget(
                    Paragraph::new(text).block(Block::default().borders(Borders::TOP).title(label)),
                    row,
                );
                if let Some(x) = cursor {
                    if row.height > 1 && row.width > 0 {
                        f.set_cursor_position((row.x + x, row.y + 1));
                    }
                }
            }
            f.render_widget(Paragraph::new("Ed25519 • native passphrase prompt in terminal\nExisting private/.pub paths are never overwritten.").wrap(Wrap {trim:false}), rows[2]);
            if compact {
                "Tab field • F4 details/error"
            } else {
                "Tab next field • Enter generate in native terminal • Esc back\nF4 full details/error"
            }
        }
        Mode::Deploy {
            public,
            private,
            alias,
        } => {
            m.page = parts[1].height.max(1) as usize;
            let text = format!("Confirm deployment\nPublic: {}\nPrivate sibling for duplicate check: {}\nFingerprint: {}\nExact alias: {alias}\nRoot config: {}\nNative ssh-copy-id; password and host-key prompts stay native.\nPublic snapshot revalidated before launch; duplicate checking retained.", public.path.display(), private.display(), public.fingerprint, m.batch.root.display());
            details::render_rows(
                f,
                parts[1],
                text.lines()
                    .map(|line| Line::from(line.to_owned()))
                    .collect(),
                &mut m.scroll,
            );
            if compact {
                "↑↓ scroll • F4 details/error"
            } else {
                "↑/↓ PgUp/PgDn scroll • Enter deploy • Esc cancel\nF4 full details/error"
            }
        }
    };
    let status = if !editor.notice.is_empty() {
        &editor.notice
    } else if !m.error.is_empty() {
        &m.error
    } else {
        match editor.mode {
            Mode::List => "F5 refresh • Esc back",
            Mode::Generate { .. } => "Enter generate • Esc back",
            Mode::Deploy { .. } => "Enter deploy • Esc cancel",
        }
    };
    // A clipped status always has a visible F4 route to its complete wrapped text.
    f.render_widget(Paragraph::new(format!("{help}\n{status}")), parts[2]);
}

/// Reconcile both visible and suspended editors by path before replacing the scan.
pub(super) fn replace_entries(m: &mut Model, entries: Vec<Entry>) {
    for view in std::iter::once(&mut m.view).chain(m.parents.iter_mut().map(|(view, _)| view)) {
        let View::Keys(editor) = view else { continue };
        let previous = m
            .keys
            .entries
            .get(editor.selected)
            .map(|entry| &entry.private);
        if let Some(index) = editor
            .preferred
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| &entry.private == path))
        {
            editor.selected = index;
            editor.preferred = None;
            editor.notice.clear();
        } else if let Some(previous) = previous {
            if let Some(index) = entries.iter().position(|entry| &entry.private == previous) {
                editor.selected = index;
            } else {
                editor.selected = usize::MAX;
                editor.notice = format!(
                    "Selected key disappeared: {}. Use ↑/↓ to choose a key.",
                    previous.display()
                );
            }
        } else if editor.selected != usize::MAX {
            editor.selected = 0;
        }
        // Mode::Deploy contains its own reviewed public/private/alias snapshot.
    }
    m.keys.entries = entries;
}
pub(super) fn poll(m: &mut Model) {
    if let Some(entries) = m.keys.scanner.poll() {
        replace_entries(m, entries);
        if let Some((home, hosts)) = m.keys.refresh_pending.take() {
            m.keys.refresh(home, hosts);
        }
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test]
    fn generated_candidate_during_busy_scan_survives_coalesced_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = Model::new(vec![], AppConfig::default(), BTreeMap::new(), false);
        open_view(&mut m, true);
        m.keys.refresh(tmp.path().into(), vec![]);
        assert!(m.keys.scanner.busy());
        let generated = tmp.path().join("outside/generated");
        // Native successful generation has already published the pair; retain only its path.
        // A missing sibling must still produce an inventory row, never silently disappear.
        m.keys.remember_generated(generated.clone());
        if let View::Keys(editor) = &mut m.view {
            editor.preferred = Some(generated.clone());
        }
        for _ in 0..100 {
            m.keys.refresh(tmp.path().into(), vec![]);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while m.keys.scanner.busy() {
            poll(&mut m);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(m.keys.refresh_pending.is_none());
        let View::Keys(editor) = &m.view else {
            panic!()
        };
        assert_eq!(m.keys.entries[editor.selected].private, generated);
        assert!(m.keys.entries[editor.selected]
            .diagnostic
            .contains("public file missing"));
        assert!(editor.preferred.is_none());
        assert_eq!(m.keys.session.len(), 1);
    }
}
