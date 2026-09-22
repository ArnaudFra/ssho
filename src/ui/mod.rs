//! Native terminal interface, adapted from SSHM, copyright Guillaume Archambault (MIT).
mod actions;
mod details;
mod forms;
mod notes;
mod render;
#[cfg(test)]
mod tests;
use crate::{
    config,
    runtime::{self, PingResult, PingStatus},
    storage::{
        metadata::{HostMetadata, Metadata},
        AppConfig, AppPaths, History,
    },
    types::{ConnectionInfo, Forward, Host},
    version::{self, UpdateInfo},
};
use actions::PaletteAction;
use anyhow::Result;
use chrono::Utc;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use forms::{FormEvent, ForwardForm, HostForm, Input};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::{Path, PathBuf},
    sync::mpsc::Receiver,
    time::Duration,
};
#[derive(Clone, Debug)]
pub enum InitialView {
    List,
    Add(String),
    Edit(String),
    Move(String),
}
#[derive(Clone, Debug)]
enum View {
    List,
    Actions {
        selected: usize,
    },
    Host(Box<HostForm>),
    Info(Host),
    Note(notes::Editor),
    Help,
    Forward(ForwardForm),
    Files {
        files: Vec<PathBuf>,
        selected: usize,
        host: Option<Host>,
        seed: String,
    },
}
#[derive(Clone, Debug, PartialEq)]
enum Action {
    None,
    Quit,
    Connect(String, Option<Forward>),
    Add(String),
    Move(Host),
    Save(Option<Host>, Box<Host>, PathBuf),
    Delete(Host),
    MoveTo(Host, PathBuf),
    Ping,
    Reload,
    Favorite(Host),
    SaveNote(String, String),
}
struct Model {
    all: Vec<Host>,
    visible: Vec<usize>,
    selected: usize,
    offset: usize,
    query: Input,
    search_focus: bool,
    show_hidden: bool,
    recent: bool,
    favorites_only: bool,
    metadata: BTreeMap<String, HostMetadata>,
    delete: Option<Host>,
    discard: bool,
    view: View,
    parents: Vec<(View, usize)>,
    preferences: AppConfig,
    history: BTreeMap<String, ConnectionInfo>,
    statuses: BTreeMap<String, PingResult>,
    effective: BTreeMap<String, std::result::Result<runtime::effective::EffectiveConfig, String>>,
    error: String,
    connection_result: String,
    update: Option<UpdateInfo>,
    page: usize,
    scroll: usize,
    details: details::State,
}
impl Model {
    fn new(
        all: Vec<Host>,
        preferences: AppConfig,
        history: BTreeMap<String, ConnectionInfo>,
        force_search: bool,
    ) -> Self {
        let search_focus = force_search || preferences.focus_search_on_start;
        let mut m = Self {
            all,
            visible: vec![],
            selected: 0,
            offset: 0,
            query: Input {
                placeholder: "Search hosts or tags...".into(),
                limit: 50,
                ..Input::default()
            },
            search_focus,
            show_hidden: false,
            recent: false,
            favorites_only: false,
            metadata: BTreeMap::new(),
            delete: None,
            discard: false,
            view: View::List,
            parents: vec![],
            preferences,
            history,
            statuses: BTreeMap::new(),
            effective: BTreeMap::new(),
            error: String::new(),
            connection_result: String::new(),
            update: None,
            page: 10,
            scroll: 0,
            details: details::State::default(),
        };
        m.filter(false);
        m
    }
    fn is_favorite(&self, name: &str) -> bool {
        self.metadata.get(name).is_some_and(|entry| entry.favorite)
    }
    fn note(&self, name: &str) -> &str {
        self.metadata
            .get(name)
            .map(|entry| entry.note.as_str())
            .unwrap_or("")
    }
    fn selected_host(&self) -> Option<Host> {
        self.visible
            .get(self.selected)
            .and_then(|&i| self.all.get(i).cloned())
    }
    fn filter(&mut self, reset: bool) {
        let identity = if reset { None } else { self.selected_host() };
        let fallback = if reset { 0 } else { self.selected };
        if reset {
            self.offset = 0;
        }
        self.refilter(identity.as_ref(), fallback);
    }
    fn refilter(&mut self, identity: Option<&Host>, fallback: usize) {
        self.visible = self
            .all
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                (self.show_hidden || !h.hidden())
                    && h.matches(&self.query.value)
                    && (!self.favorites_only || self.is_favorite(&h.name))
            })
            .map(|(i, _)| i)
            .collect();
        let metadata = &self.metadata;
        self.visible.sort_by(|&a, &b| {
            let a = &self.all[a];
            let b = &self.all[b];
            let names = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
            let favorites = metadata
                .get(&b.name)
                .is_some_and(|entry| entry.favorite)
                .cmp(&metadata.get(&a.name).is_some_and(|entry| entry.favorite));
            favorites.then_with(|| {
                if self.recent {
                    self.history
                        .get(&b.name)
                        .map(|h| h.last_connect)
                        .cmp(&self.history.get(&a.name).map(|h| h.last_connect))
                        .then_with(names)
                } else {
                    names()
                }
            })
        });
        self.selected = identity
            .and_then(|host| {
                self.visible.iter().position(|&i| {
                    self.all[i].name == host.name && self.all[i].source_file == host.source_file
                })
            })
            .unwrap_or(fallback)
            .min(self.visible.len().saturating_sub(1));
        self.offset = self.offset.min(self.selected);
    }
    /// Replace the backing collection only after capturing the old selection's identity.
    /// A preferred host (creation/rename/move) is made visible even under an old filter.
    fn replace_hosts(&mut self, all: Vec<Host>, preferred: Option<Host>) {
        let identity = preferred.clone().or_else(|| self.selected_host());
        let fallback = self.selected;
        if let Some(host) = &preferred {
            if !host.matches(&self.query.value) {
                self.query.set("");
            }
            if host.hidden() {
                self.show_hidden = true;
            }
            if self.favorites_only && !self.is_favorite(&host.name) {
                self.favorites_only = false;
            }
        }
        self.all = all;
        self.refilter(identity.as_ref(), fallback);
    }
    fn complete_mutation(&mut self, all: Vec<Host>, action: &Action) {
        let preferred = match action {
            Action::Save(_, host, _) => Some((**host).clone()),
            Action::MoveTo(host, target) => Some(Host {
                source_file: target.clone(),
                ..host.clone()
            }),
            _ => None,
        };
        self.replace_hosts(all, preferred);
        let info_parent = matches!(action, Action::Save(..))
            && matches!(self.parents.last(), Some((View::Info(_), _)));
        if info_parent {
            self.back();
            if let Some(host) = self.selected_host() {
                self.view = View::Info(host);
            }
        } else {
            self.view = View::List;
            self.parents.clear();
            self.scroll = 0;
        }
        self.delete = None;
        self.error.clear();
    }
    /// Refresh metadata without rebuilding the browser's visible order or viewport.
    fn resume_connection(&mut self, name: &str, result: Result<i32>, history: Result<History>) {
        while !self.parents.is_empty() {
            self.back();
        }
        self.view = View::List;
        self.delete = None;
        self.discard = false;
        self.error.clear();
        self.connection_result = match result {
            Ok(code) => format!("SSH exit {code}: {name}"),
            Err(error) => format!("SSH failed: {error:#} ({name})"),
        };
        match history {
            Ok(history) => self.history = history.connections,
            Err(error) => self
                .connection_result
                .push_str(&format!(" • History: {error:#}")),
        }
    }
    fn navigate(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.visible.len().saturating_sub(1));
    }
    fn open_view(&mut self, view: View) {
        let parent = std::mem::replace(&mut self.view, view);
        self.parents.push((parent, self.scroll));
        self.scroll = 0;
    }
    fn back(&mut self) {
        let (view, scroll) = self.parents.pop().unwrap_or((View::List, 0));
        self.view = view;
        self.scroll = scroll;
    }
    fn quit_key(&self, k: KeyEvent) -> bool {
        self.preferences
            .key_bindings
            .should_quit_on_key(&key_name(k))
    }
    fn key(&mut self, k: KeyEvent) -> Action {
        if matches!(k.kind, KeyEventKind::Release) {
            return Action::None;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if self.discard {
            if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
                self.discard = false;
            } else if k.code == KeyCode::Enter {
                self.discard = false;
                self.back();
            }
            return Action::None;
        }
        match &mut self.view {
            View::Note(editor) => {
                return match editor.key(k) {
                    FormEvent::Cancel => {
                        if editor.dirty() {
                            self.discard = true;
                        } else {
                            self.back();
                        }
                        Action::None
                    }
                    FormEvent::Submit => {
                        Action::SaveNote(editor.host.clone(), editor.input.value.clone())
                    }
                    FormEvent::None => Action::None,
                };
            }
            View::Host(f) => {
                return match f.key(k) {
                    FormEvent::Cancel => {
                        if f.dirty() {
                            self.discard = true;
                        } else {
                            self.back();
                        }
                        Action::None
                    }
                    FormEvent::Submit if !f.validate() => Action::None,
                    FormEvent::Submit => match f.host() {
                        Ok(host) => {
                            Action::Save(f.original.clone(), Box::new(host), f.target.clone())
                        }
                        Err(e) => {
                            f.error = e.to_string();
                            Action::None
                        }
                    },
                    _ => Action::None,
                }
            }
            View::Forward(f) => {
                return match f.key(k) {
                    FormEvent::Cancel => {
                        if f.dirty() {
                            self.discard = true;
                        } else {
                            self.back();
                        }
                        Action::None
                    }
                    FormEvent::Submit if !f.validate() => Action::None,
                    FormEvent::Submit => match f.forward() {
                        Ok(forward) => Action::Connect(f.host.clone(), Some(forward)),
                        Err(e) => {
                            f.error = e.to_string();
                            Action::None
                        }
                    },
                    _ => Action::None,
                }
            }
            View::Info(h) => {
                if k.code == KeyCode::F(5) {
                    return Action::Reload;
                }
                if matches!(k.code, KeyCode::Char('e') | KeyCode::Enter) {
                    let view = View::Host(Box::new(HostForm::edit(h.clone())));
                    self.open_view(view);
                    return Action::None;
                }
                if matches!(k.code, KeyCode::Esc | KeyCode::Char('q'))
                    || (ctrl && k.code == KeyCode::Char('c'))
                {
                    self.back();
                } else {
                    self.scroll_key(k);
                }
                return Action::None;
            }
            View::Help => {
                if matches!(
                    k.code,
                    KeyCode::Esc | KeyCode::Char('q' | 'h') | KeyCode::Enter
                ) || (ctrl && k.code == KeyCode::Char('c'))
                {
                    self.back();
                } else {
                    self.scroll_key(k);
                }
                return Action::None;
            }
            View::Files {
                files,
                selected,
                host,
                seed,
            } => {
                match k.code {
                    KeyCode::Esc => self.back(),
                    KeyCode::Char('c') if ctrl => self.back(),
                    KeyCode::Char('q') if host.is_some() => self.back(),
                    KeyCode::Up | KeyCode::Char('k') => *selected = selected.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        *selected = (*selected + 1).min(files.len().saturating_sub(1))
                    }
                    KeyCode::Enter => {
                        if let Some(target) = files.get(*selected) {
                            if let Some(host) = host {
                                return Action::MoveTo(host.clone(), target.clone());
                            }
                            let view =
                                View::Host(Box::new(HostForm::add(seed.clone(), target.clone())));
                            self.open_view(view);
                        }
                    }
                    _ => {}
                }
                return Action::None;
            }
            View::Actions { selected } => {
                match k.code {
                    KeyCode::Up => *selected = selected.saturating_sub(1),
                    KeyCode::Down => *selected = (*selected + 1).min(PaletteAction::ALL.len() - 1),
                    KeyCode::Esc => self.back(),
                    KeyCode::Char('c') if ctrl => self.back(),
                    KeyCode::Enter => {
                        let action = PaletteAction::ALL[*selected];
                        if action.enabled(self.selected_host().is_some()) {
                            self.back();
                            return self.activate_action(action);
                        }
                    }
                    _ => {}
                }
                return Action::None;
            }
            View::List => {}
        }
        if let Some(host) = self.delete.clone() {
            if matches!(k.code, KeyCode::Esc) || (ctrl && k.code == KeyCode::Char('c')) {
                self.delete = None;
            } else if k.code == KeyCode::Enter {
                return Action::Delete(host);
            }
            return Action::None;
        }
        if k.code == KeyCode::F(2) {
            self.open_view(View::Actions { selected: 0 });
            return Action::None;
        }
        if k.code == KeyCode::F(5) {
            return Action::Reload;
        }
        if k.code == KeyCode::F(4) {
            return self.activate_action(PaletteAction::Details);
        }
        if self.details.key(k) {
            return Action::None;
        }
        if k.code == KeyCode::Esc && self.search_focus {
            if self.query.value.is_empty() {
                self.search_focus = false;
            } else {
                self.query.set("");
                self.filter(true);
            }
            return Action::None;
        }
        // Every printable key is text while search is focused, including configured quit letters.
        if (!self.search_focus || !matches!(k.code, KeyCode::Char(_)) || ctrl) && self.quit_key(k) {
            return Action::Quit;
        }
        match k.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.search_focus = !self.search_focus;
            }
            KeyCode::Up => self.navigate(-1),
            KeyCode::Down => self.navigate(1),
            KeyCode::Char('p') if ctrl => self.navigate(-1),
            KeyCode::Char('n') if ctrl => self.navigate(1),
            KeyCode::Enter => {
                if let Some(h) = self.selected_host() {
                    return Action::Connect(h.name, None);
                }
            }
            KeyCode::Char('/') if !self.search_focus => self.search_focus = true,
            KeyCode::Char('f') if ctrl && !self.search_focus => self.search_focus = true,
            _ if self.search_focus => {
                let old = self.query.value.clone();
                self.query.key(k);
                if old != self.query.value {
                    self.filter(true);
                }
            }
            KeyCode::Char('j') => self.navigate(1),
            KeyCode::Char('k') => self.navigate(-1),
            KeyCode::PageDown => self.navigate(self.page as isize),
            KeyCode::PageUp => self.navigate(-(self.page as isize)),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.visible.len().saturating_sub(1),
            KeyCode::Char('*') => return self.activate_action(PaletteAction::Favorite),
            KeyCode::Char('N') => return self.activate_action(PaletteAction::Note),
            KeyCode::Char('a') => return self.activate_action(PaletteAction::Add),
            KeyCode::Char('e') => return self.activate_action(PaletteAction::Edit),
            KeyCode::Char('i') => {
                if let Some(h) = self.selected_host() {
                    self.open_view(View::Info(h));
                }
            }
            KeyCode::Char('m') => {
                if let Some(h) = self.selected_host() {
                    return Action::Move(h);
                }
            }
            KeyCode::Char('d') => self.delete = self.selected_host(),
            KeyCode::Char('f') => return self.activate_action(PaletteAction::Tunnel),
            KeyCode::Char('p') => return self.activate_action(PaletteAction::Test),
            KeyCode::Char('H') => {
                self.show_hidden = !self.show_hidden;
                self.filter(false);
            }
            KeyCode::Char('s') => {
                self.recent = !self.recent;
                self.filter(false);
            }
            KeyCode::Char('n') => {
                self.recent = false;
                self.filter(false);
            }
            KeyCode::Char('r') => {
                self.recent = true;
                self.filter(false);
            }
            KeyCode::Char('h') => return self.activate_action(PaletteAction::Help),
            _ => {}
        }
        Action::None
    }
    fn activate_action(&mut self, action: PaletteAction) -> Action {
        match action {
            PaletteAction::Favorite => {
                if let Some(host) = self.selected_host() {
                    return Action::Favorite(host);
                }
            }
            PaletteAction::Note => {
                if let Some(host) = self.selected_host() {
                    self.open_view(View::Note(notes::Editor::new(
                        host.name.clone(),
                        self.note(&host.name),
                    )));
                }
            }
            PaletteAction::FavoritesOnly => {
                self.favorites_only = !self.favorites_only;
                self.filter(false);
            }
            PaletteAction::Add => return Action::Add(String::new()),
            PaletteAction::Edit => {
                if let Some(host) = self.selected_host() {
                    self.open_view(View::Host(Box::new(HostForm::edit(host))));
                }
            }
            PaletteAction::Tunnel => {
                if let Some(host) = self.selected_host() {
                    let saved = self
                        .history
                        .get(&host.name)
                        .and_then(|c| c.port_forwarding.clone());
                    self.open_view(View::Forward(ForwardForm::new(host.name, saved)));
                }
            }
            PaletteAction::Test => return Action::Ping,
            PaletteAction::Reload => return Action::Reload,
            PaletteAction::Details => self.details.shown = !self.details.shown,
            PaletteAction::Help => {
                self.open_view(View::Help);
            }
        }
        Action::None
    }
    fn scroll_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(self.page),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(self.page),
            KeyCode::Home => self.scroll = 0,
            _ => {}
        }
    }
}
/// SSH writes have already committed: always refresh to the saved host, even if
/// the separate metadata migration fails. Originals remain available for recovery.
fn complete_saved_mutation(
    m: &mut Model,
    all: Vec<Host>,
    action: &Action,
    metadata: &mut Metadata,
) {
    let migration = if let Action::Save(Some(original), replacement, _) = action {
        metadata.rename_hosts(original, replacement)
    } else {
        Ok(())
    };
    m.metadata = metadata.hosts().clone();
    m.complete_mutation(all, action);
    if let Err(error) = migration {
        m.error = format!("SSH saved; metadata rename failed: {error:#}");
    }
}
/// Commit storage first; only successful writes reorder favorites or close the draft.
fn apply_metadata_action(m: &mut Model, metadata: &mut Metadata, action: &Action) -> Result<()> {
    match action {
        Action::Favorite(host) => metadata.set_favorite(&host.name, !m.is_favorite(&host.name))?,
        Action::SaveNote(name, note) => metadata.set_note(name, note)?,
        _ => return Ok(()),
    }
    m.metadata = metadata.hosts().clone();
    m.filter(false);
    if matches!(action, Action::SaveNote(..)) {
        m.back();
    }
    m.error.clear();
    Ok(())
}
fn key_name(k: KeyEvent) -> String {
    let base = match k.code {
        KeyCode::Esc => "esc".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::F(n) => format!("f{n}"),
        _ => format!("{:?}", k.code).to_lowercase(),
    };
    if k.modifiers.contains(KeyModifiers::CONTROL) {
        format!("ctrl+{base}")
    } else if k.modifiers.contains(KeyModifiers::ALT) {
        format!("alt+{base}")
    } else {
        base
    }
}

/// Owns the terminal modes so all error and unwind paths restore the shell.
struct TerminalGuard {
    active: bool,
}
impl TerminalGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut guard = Self { active: true };
        if let Err(e) = execute!(
            io::stdout(),
            EnterAlternateScreen,
            cursor::SetCursorStyle::BlinkingBlock,
            cursor::Hide
        ) {
            guard.restore();
            return Err(e.into());
        }
        Ok(guard)
    }
    fn restore(&mut self) {
        if self.active {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(
                io::stdout(),
                LeaveAlternateScreen,
                cursor::SetCursorStyle::DefaultUserShape,
                cursor::Show
            );
            self.active = false;
        }
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}
// A caught handler is reset to the default by exec, unlike SIG_IGN. Thus SSH
// still receives terminal interrupts while its waiting UI parent survives them.
#[cfg(unix)]
struct SshSignals(Vec<(libc::c_int, libc::sigaction)>);
#[cfg(unix)]
impl SshSignals {
    fn enter() -> Result<Self> {
        extern "C" fn interrupted(_: libc::c_int) {}
        let mut guard = Self(Vec::new());
        for signal in [libc::SIGINT, libc::SIGQUIT] {
            // SAFETY: sigaction structures are initialized; the handler does no work,
            // and each previous disposition is owned/restored by this scoped guard.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                let mut previous: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = interrupted as *const () as usize;
                action.sa_flags = libc::SA_RESTART;
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(signal, &action, &mut previous) != 0 {
                    return Err(io::Error::last_os_error().into());
                }
                guard.0.push((signal, previous));
            }
        }
        Ok(guard)
    }
}
#[cfg(unix)]
impl Drop for SshSignals {
    fn drop(&mut self) {
        for (signal, action) in self.0.iter().rev() {
            // SAFETY: restore the valid dispositions saved by enter.
            unsafe { libc::sigaction(*signal, action, std::ptr::null_mut()) };
        }
    }
}
fn start_inspection(
    inspector: &runtime::effective::Inspector,
    m: &mut Model,
    config: &Path,
    probe: bool,
) -> Receiver<runtime::effective::Inspection> {
    m.effective.clear();
    m.statuses.clear();
    let hosts = m.all.clone();
    for h in &hosts {
        m.statuses.insert(
            h.name.clone(),
            PingResult {
                name: h.name.clone(),
                status: if probe {
                    PingStatus::Connecting
                } else {
                    PingStatus::Unknown
                },
                duration: Duration::ZERO,
                error: None,
            },
        );
    }
    inspector.inspect(hosts, config.to_path_buf(), probe)
}
/// Load completely before changing the browser or discarding its diagnostic generation.
fn reload_hosts(
    inspector: &runtime::effective::Inspector,
    m: &mut Model,
    config: &Path,
) -> Result<Receiver<runtime::effective::Inspection>> {
    let hosts = config::load_hosts_checked(config)?;
    let info = match &m.view {
        View::Info(host) => Some(host.clone()),
        _ => None,
    };
    m.replace_hosts(hosts, None);
    if let Some(info) = info {
        if let Some(host) = m
            .all
            .iter()
            .find(|host| host.name == info.name && host.source_file == info.source_file)
        {
            m.view = View::Info(host.clone());
        } else {
            m.view = View::List;
            m.parents.clear();
            m.scroll = 0;
        }
    }
    m.error.clear();
    Ok(start_inspection(inspector, m, config, false))
}
fn select_files(m: &mut Model, config: &Path, host: Option<Host>, seed: String) -> Result<()> {
    let mut files = config::config_files(config)?;
    if let Some(h) = &host {
        files.retain(|f| f != &h.source_file);
        if files.is_empty() {
            anyhow::bail!("no includes found in SSH config file - move operation requires multiple config files");
        }
    }
    if host.is_none() && files.len() <= 1 {
        m.open_view(View::Host(Box::new(HostForm::add(
            seed,
            files.pop().unwrap_or_else(|| config.to_path_buf()),
        ))));
    } else {
        m.open_view(View::Files {
            files,
            selected: 0,
            host,
            seed,
        });
    }
    Ok(())
}
/// Runs the interactive browser or a standalone CLI form. SSH takes over the restored terminal.
pub fn run(
    paths: &AppPaths,
    config: &Path,
    force_search: bool,
    no_update_check: bool,
    initial: InitialView,
) -> Result<i32> {
    let preferences = AppConfig::load(&paths.root)?;
    let history = History::load(&paths.root)?;
    let hosts = config::load_hosts(config)?;
    let mut metadata = Metadata::load(&paths.root, config)?;
    let standalone = !matches!(initial, InitialView::List);
    let mut m = Model::new(
        hosts,
        preferences,
        history.connections.clone(),
        force_search,
    );
    m.metadata = metadata.hosts().clone();
    m.filter(false);
    match initial {
        InitialView::List => {}
        InitialView::Add(seed) => select_files(&mut m, config, None, seed)?,
        InitialView::Edit(name) => {
            m.view = View::Host(Box::new(HostForm::edit(config::find_host(config, &name)?)))
        }
        InitialView::Move(name) => select_files(
            &mut m,
            config,
            Some(config::find_host(config, &name)?),
            String::new(),
        )?,
    }
    let probe = !standalone && m.preferences.ping_on_start;
    let inspector = runtime::effective::Inspector::default();
    let mut inspection = start_inspection(&inspector, &mut m, config, probe);
    let updates = if !standalone && m.preferences.check_for_updates && !no_update_check {
        Some(version::check_updates())
    } else {
        None
    };
    let mut guard = TerminalGuard::enter()?;
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    loop {
        while let Ok(result) = inspection.try_recv() {
            if let Some(ping) = result.ping {
                m.statuses.insert(result.name.clone(), ping);
            }
            m.effective.insert(result.name, result.effective);
        }
        if let Some(rx) = &updates {
            while let Ok(update) = rx.try_recv() {
                m.update = Some(update);
            }
        }
        terminal.draw(|f| render::draw(f, &mut m))?;
        if !event::poll(Duration::from_millis(80))? {
            continue;
        }
        let action = match event::read()? {
            Event::Key(k) => m.key(k),
            Event::Paste(text) => {
                for c in text.chars().filter(|c| !c.is_control()) {
                    m.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
                }
                Action::None
            }
            _ => Action::None,
        };
        if standalone && matches!(m.view, View::List) && matches!(action, Action::None) {
            return Ok(0);
        }
        let completed_action = action.clone();
        let operation: Result<bool> = match action {
            Action::Favorite(_) | Action::SaveNote(..) => {
                apply_metadata_action(&mut m, &mut metadata, &action).map(|_| false)
            }
            Action::None => Ok(false),
            Action::Quit => return Ok(0),
            Action::Connect(name, forward) => {
                #[cfg(unix)]
                let signals = SshSignals::enter()?;
                guard.restore();
                let result = runtime::connect(paths, config, &name, false, &[], forward.as_ref());
                guard = TerminalGuard::enter()?;
                #[cfg(unix)]
                drop(signals);
                // The alternate screen is new: invalidate ratatui's previous frame.
                terminal.clear()?;
                m.resume_connection(&name, result, History::load(&paths.root));
                Ok(false)
            }
            Action::Ping => {
                inspection = start_inspection(&inspector, &mut m, config, true);
                Ok(false)
            }
            Action::Reload => reload_hosts(&inspector, &mut m, config).map(|receiver| {
                inspection = receiver;
                false
            }),
            Action::Add(seed) => select_files(&mut m, config, None, seed).map(|_| false),
            Action::Move(host) => {
                select_files(&mut m, config, Some(host), String::new()).map(|_| false)
            }
            Action::Save(original, host, target) => match original {
                Some(original) => config::update_host(&original, &host, &paths.root),
                None => config::add_host(&host, &target, &paths.root),
            }
            .map(|_| true),
            Action::Delete(host) => config::delete_host(&host, &paths.root).map(|_| true),
            Action::MoveTo(host, target) => {
                config::move_host(&host, &target, &paths.root).map(|_| true)
            }
        };
        match operation {
            Ok(true) => {
                if standalone {
                    if let Action::Save(Some(original), replacement, _) = &completed_action {
                        metadata
                            .rename_hosts(original, replacement)
                            .map_err(|error| {
                                anyhow::anyhow!("SSH saved; metadata rename failed: {error:#}")
                            })?;
                    }
                    return Ok(0);
                }
                complete_saved_mutation(
                    &mut m,
                    config::load_hosts(config)?,
                    &completed_action,
                    &mut metadata,
                );
                inspection = start_inspection(&inspector, &mut m, config, false);
            }
            Ok(false) => {}
            Err(e) => match &mut m.view {
                View::Host(f) => f.error = e.to_string(),
                View::Forward(f) => f.error = e.to_string(),
                View::Note(editor) => editor.error = format!("{e:#}"),
                _ => {
                    m.error = format!("{e:#}");
                    m.delete = None;
                }
            },
        }
    }
}
fn time_ago(time: chrono::DateTime<Utc>) -> String {
    let seconds = (Utc::now() - time).num_seconds().max(1);
    let (n, unit) = if seconds < 60 {
        (seconds, "second")
    } else if seconds < 3600 {
        (seconds / 60, "minute")
    } else if seconds < 86400 {
        (seconds / 3600, "hour")
    } else if seconds < 604800 {
        (seconds / 86400, "day")
    } else if seconds < 2592000 {
        (seconds / 604800, "week")
    } else if seconds < 31536000 {
        (seconds / 2592000, "month")
    } else {
        (seconds / 31536000, "year")
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}
