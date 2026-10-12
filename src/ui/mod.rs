//! Native terminal interface, adapted from SSHM, copyright Guillaume Archambault (MIT).
mod actions;
mod active;
mod aliases;
#[cfg(test)]
mod aliases_tests;
mod batch;
#[cfg(test)]
mod batch_tests;
mod changes;
mod details;
mod forms;
mod groups;
#[cfg(test)]
mod groups_tests;
mod keys;
#[cfg(test)]
mod keys_tests;
mod notes;
mod profiles;
mod render;
mod search;
mod source_filter;
#[cfg(test)]
mod tests;
use crate::{
    config,
    runtime::{self, PingResult, PingStatus},
    storage::{
        metadata::{HostMetadata, Metadata},
        tunnels::{HostModes, HostProfiles, LaunchMode, TunnelProfiles},
        AppConfig, AppPaths, AvailabilityMethod, GroupMode, History,
    },
    types::{ConnectionInfo, Forward, Host},
    version::{self, UpdateInfo},
};
use actions::PaletteAction;
use anyhow::Result;
use chrono::Utc;
use crossterm::{
    cursor,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
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
    Availability {
        selected: usize,
    },
    List,
    Keys(keys::Editor),
    Batch(batch::Editor),
    BatchResults {
        selected: usize,
        stderr: bool,
        offset: usize,
    },
    Changes(changes::Review),
    Backups {
        entries: Vec<config::backups::Backup>,
        selected: usize,
    },
    Actions {
        selected: usize,
    },
    Host(Box<HostForm>),
    Info(Host),
    GroupInfo {
        key: groups::Key,
        count: usize,
    },
    Note(notes::Editor),
    Help,
    Forward(ForwardForm),
    Profiles(profiles::List),
    Active {
        selected: usize,
    },
    Aliases {
        hosts: Vec<Host>,
        selected: usize,
    },
    GroupMode {
        selected: usize,
    },
    SourceFiles {
        files: Vec<PathBuf>,
        selected: usize,
    },
    Files {
        files: Vec<PathBuf>,
        selected: usize,
        host: Option<Host>,
        seed: String,
        duplicate: Option<Box<Host>>,
    },
}
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Availability(AvailabilityMethod),
    None,
    OpenKeys(bool),
    RefreshKeys,
    GenerateKey,
    AddKey,
    AttachKey,
    DeployKey,
    ConfirmChanges,
    Backups,
    RestoreBackup(usize),
    Quit,
    Connect(String, Option<Forward>),
    StartTunnel(String, Option<String>, Forward),
    StopTunnel(u64),
    StartBatch,
    CancelBatch,
    Add(String),
    Move(Host),
    Duplicate(Host),
    SaveCopy(Box<Host>, PathBuf),
    Save(Option<Host>, Box<Host>, PathBuf),
    Delete(Host),
    MoveTo(Host, PathBuf),
    Ping,
    Reload,
    SourceFilter,
    ToggleAliases,
    GroupMode(GroupMode),
    Favorite(Host),
    SaveNote(String, String),
    SaveProfile {
        host: String,
        original: Option<String>,
        name: String,
        forward: Forward,
        mode: LaunchMode,
    },
    DeleteProfile(String, String),
}
struct PendingPreferred {
    host: Host,
    query: String,
    // Compatibility-only snapshot across repeated saves, never a resolved UI value.
    provisional: Option<runtime::effective::EffectiveConfig>,
}
struct Model {
    keys: keys::State,
    batch: batch::State,
    all: Vec<Host>,
    visible: Vec<usize>,
    // Unsorted raw search membership, independent of projected display rows.
    matching: Vec<usize>,
    alias_projection: aliases::Projection,
    rows: Vec<groups::Row>,
    group_projection: groups::Projection,
    selected: usize,
    offset: usize,
    query: Input,
    search: std::result::Result<search::Query, String>,
    // Keep the saved identity visible until its fresh effective values settle the query.
    pending_preferred: Option<PendingPreferred>,
    search_focus: bool,
    show_hidden: bool,
    source_filter: Option<PathBuf>,
    config_warnings: Vec<config::ConfigWarning>,
    recent: bool,
    favorites_only: bool,
    metadata: BTreeMap<String, HostMetadata>,
    tunnel_profiles: HostProfiles,
    tunnel_modes: HostModes,
    tunnels: Vec<runtime::tunnels::Record>,
    quit_tunnels: bool,
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
            keys: keys::State::default(),
            batch: batch::State::default(),
            all,
            visible: vec![],
            matching: vec![],
            alias_projection: aliases::Projection::default(),
            rows: Vec::new(),
            group_projection: groups::Projection::default(),
            selected: 0,
            offset: 0,
            query: Input {
                placeholder: "Search words, tag:, user:, file:, is:...".into(),
                limit: 1024,
                ..Input::default()
            },
            search: Ok(search::Query::default()),
            pending_preferred: None,
            search_focus,
            show_hidden: false,
            source_filter: None,
            config_warnings: vec![],
            recent: false,
            favorites_only: false,
            metadata: BTreeMap::new(),
            tunnel_profiles: BTreeMap::new(),
            tunnel_modes: BTreeMap::new(),
            tunnels: vec![],
            quit_tunnels: false,
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
        m.batch.cache_sources(&m.all);
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
        match self.rows.get(self.selected) {
            Some(groups::Row::Host(index)) => self.all.get(*index).cloned(),
            _ => None,
        }
    }
    fn filter(&mut self, reset: bool) {
        let identity = if reset { None } else { self.selected_host() };
        let fallback = if reset { 0 } else { self.selected };
        if reset {
            self.offset = 0;
            self.pending_preferred = None;
        }
        self.refilter(identity.as_ref(), fallback);
        if reset {
            self.selected = self
                .rows
                .iter()
                .position(|row| matches!(row, groups::Row::Host(_)))
                .unwrap_or(0);
        }
    }
    fn matching_indices(&self) -> Vec<usize> {
        self.all
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                self.source_filter
                    .as_ref()
                    .is_none_or(|path| path == &h.source_file)
                    && (self.show_hidden || !h.hidden())
                    && self.matches_search(h)
                    && (!self.favorites_only || self.is_favorite(&h.name))
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn matches_search(&self, host: &Host) -> bool {
        if self.pending_preferred.as_ref().is_some_and(|pending| {
            pending.host.name == host.name
                && pending.host.source_file == host.source_file
                && pending.query == self.query.value
        }) {
            return true;
        }
        self.search.as_ref().is_ok_and(|query| {
            query.matches(
                host,
                self.effective.get(&host.name).and_then(|r| r.as_ref().ok()),
                self.note(&host.name),
                self.is_favorite(&host.name),
                self.statuses
                    .get(&host.name)
                    .map(|r| r.status)
                    .unwrap_or_default(),
            )
        })
    }
    fn refilter(&mut self, identity: Option<&Host>, fallback: usize) {
        let group = self.selected_group();
        self.search = search::Query::parse(&self.query.value);
        self.matching = self.matching_indices();
        self.visible = if self.preferences.group_aliases {
            self.alias_projection
                .project(&self.all, &self.matching, identity)
        } else {
            self.matching.clone()
        };
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
        self.rebuild_rows(identity, group, fallback);
    }
    /// Replace the backing collection only after capturing the old selection's identity.
    /// Prefer saved hosts within the exact source scope; retain existing text-search behavior.
    fn replace_hosts(&mut self, all: Vec<Host>, preferred: Option<Host>) {
        self.batch.cache_sources(&all);
        let identity = preferred.clone().or_else(|| self.selected_host());
        let fallback = self.selected;
        let pending = self.pending_preferred.take();
        // Old resolved values are only a provisional compatibility input. Even explicit
        // edits may be masked by earlier Host/Match/global values; await fresh ssh -G.
        let provisional = preferred.as_ref().and_then(|host| {
            let previous = self.all.iter().find(|old| old.name == host.name)?;
            let effective = self
                .effective
                .get(&host.name)
                .and_then(|result| result.as_ref().ok())
                .cloned()
                .or_else(|| {
                    pending
                        .as_ref()
                        .filter(|pending| {
                            pending.query == self.query.value
                                && pending.host.name == previous.name
                                && pending.host.source_file == previous.source_file
                        })
                        .and_then(|pending| pending.provisional.clone())
                })?;
            Some(effective)
        });
        // Never present the previous resolution as the saved declaration's cache.
        let changed: Vec<_> = all
            .iter()
            .filter(|host| !self.all.contains(host))
            .map(|host| host.name.clone())
            .collect();
        for name in changed {
            self.effective.remove(&name);
            self.statuses.remove(&name);
        }
        self.search = search::Query::parse(&self.query.value);
        if let Some(host) = preferred.as_ref().filter(|host| {
            self.source_filter
                .as_ref()
                .is_none_or(|path| path == &host.source_file)
        }) {
            let compatible = self.search.as_ref().is_ok_and(|query| {
                // Either the saved declaration or the previous native projection may
                // win. Only the new resolution can decide which values actually apply.
                [provisional.as_ref(), None].into_iter().any(|effective| {
                    query.matches(
                        host,
                        effective,
                        self.note(&host.name),
                        self.is_favorite(&host.name),
                        self.statuses
                            .get(&host.name)
                            .map(|result| result.status)
                            .unwrap_or_default(),
                    )
                })
            });
            if !compatible {
                self.query.set("");
            } else if !self.query.value.is_empty() {
                self.pending_preferred = Some(PendingPreferred {
                    host: host.clone(),
                    query: self.query.value.clone(),
                    provisional,
                });
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
    /// Diagnostics may change membership, but must not silently apply new history sorting.
    fn refresh_search_membership(&mut self) {
        let next = self.matching_indices();
        if next != self.matching {
            let was_empty = self.rows.is_empty();
            self.filter(false);
            if was_empty {
                // First async results have no prior identity. Keep Enter targeting
                // the first displayed host without opening deliberately closed groups.
                if let Some(index) = self
                    .rows
                    .iter()
                    .position(|row| matches!(row, groups::Row::Host(_)))
                {
                    self.selected = index;
                }
            }
        }
    }
    fn apply_inspection(&mut self, result: runtime::effective::Inspection) {
        if result
            .ping
            .as_ref()
            .is_some_and(|ping| ping.method != self.preferences.availability_method)
        {
            return;
        }
        if let Some(ping) = result.ping {
            self.statuses.insert(result.name.clone(), ping);
        }
        let settles_preferred = self
            .pending_preferred
            .as_ref()
            .is_some_and(|pending| pending.host.name == result.name);
        self.effective.insert(result.name, result.effective);
        if settles_preferred {
            let pending = self.pending_preferred.take().unwrap();
            // Typed queries cancel this preference in filter(true). Fresh failures
            // use the normal declared fallback; neither path retains stale data.
            if pending.query == self.query.value && !self.matches_search(&pending.host) {
                self.query.set("");
                self.filter(false);
                return;
            }
        }
        self.refresh_search_membership();
    }
    fn complete_mutation(&mut self, all: Vec<Host>, action: &Action) {
        // Visibility (hidden/search/favorites) must never decide whether a file is empty.
        if let Some(path) = &self.source_filter {
            let removed_from_source = match action {
                Action::Delete(host) => &host.source_file == path,
                Action::MoveTo(host, target) => &host.source_file == path && target != path,
                _ => false,
            };
            if removed_from_source && !all.iter().any(|host| &host.source_file == path) {
                self.source_filter = None;
            }
        }
        let preferred_identity = match action {
            Action::Save(_, host, _) | Action::SaveCopy(host, _) => {
                Some((&host.name, &host.source_file))
            }
            Action::MoveTo(host, target) => Some((&host.name, target)),
            _ => None,
        };
        // The reloaded declaration owns destination inheritance and saved metadata.
        // Keep the acted-on alias identity, but never reuse its pre-save FileTags.
        let preferred = preferred_identity.and_then(|(name, source)| {
            all.iter()
                .find(|host| &host.name == name && &host.source_file == source)
                .cloned()
        });
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
            .min(self.rows.len().saturating_sub(1));
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
    fn has_unsaved_context(&self) -> bool {
        std::iter::once(&self.view)
            .chain(self.parents.iter().map(|(view, _)| view))
            .any(|view| match view {
                View::Host(form) => form.dirty(),
                View::Forward(form) => form.dirty(),
                View::Note(editor) => editor.dirty(),
                View::Changes(_) => true,
                View::Keys(keys::Editor {
                    mode: keys::Mode::Generate { path, comment, .. },
                    ..
                }) => !path.value.is_empty() || !comment.value.is_empty(),
                _ => false,
            })
    }
    fn quit_action(&mut self) -> Action {
        if (self.has_unsaved_context() || self.batch.manager.has_active()) && !self.quit_tunnels {
            self.quit_tunnels = true;
            Action::None
        } else {
            Action::Quit
        }
    }
    fn quit_key(&self, k: KeyEvent) -> bool {
        self.preferences
            .key_bindings
            .should_quit_on_key(&key_name(k))
    }
    /// Pasted data only edits a focused input; it never enters command dispatch.
    fn paste(&mut self, text: &str) {
        if self.discard || self.delete.is_some() || self.quit_tunnels {
            return;
        }
        let multiline = matches!(self.view, View::Note(_));
        let input = match &mut self.view {
            View::List if self.search_focus => &mut self.query,
            View::Note(editor) => &mut editor.input,
            View::Keys(editor) => {
                let Some(input) = keys::input(editor) else {
                    return;
                };
                input
            }
            View::Batch(editor) => {
                let Some(input) = editor.input() else { return };
                input
            }
            View::Host(form) => {
                if form.name_focus {
                    &mut form.names[form.focus]
                } else {
                    &mut form.inputs[form.focus]
                }
            }
            View::Forward(form) if form.focus != 0 => &mut form.inputs[form.focus],
            _ => return,
        };
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        for c in normalized.chars() {
            let c = if c.is_whitespace() {
                if multiline && c == '\n' {
                    '\n'
                } else {
                    ' '
                }
            } else if c.is_control() {
                continue;
            } else {
                c
            };
            input.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        if matches!(self.view, View::List) {
            self.filter(true);
        }
    }
    fn key(&mut self, k: KeyEvent) -> Action {
        if matches!(k.kind, KeyEventKind::Release) {
            return Action::None;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if self.quit_tunnels {
            if k.code == KeyCode::Enter {
                return Action::Quit;
            }
            if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
                self.quit_tunnels = false;
            }
            return Action::None;
        }
        if self.discard {
            if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
                self.discard = false;
            } else if k.code == KeyCode::Enter {
                self.discard = false;
                self.back();
            }
            return Action::None;
        }
        if k.code == KeyCode::F(7)
            && self.delete.is_none()
            && !matches!(&self.view, View::Profiles(list) if list.delete.is_some())
        {
            if matches!(self.view, View::Active { .. }) {
                self.back();
            } else {
                self.open_view(View::Active { selected: 0 });
            }
            return Action::None;
        }
        if k.code == KeyCode::F(8) && self.delete.is_none() {
            if matches!(self.view, View::Backups { .. }) {
                self.back();
                return Action::None;
            }
            return Action::Backups;
        }
        if k.code == KeyCode::F(9) && matches!(self.view, View::Host(_)) {
            return Action::OpenKeys(true);
        }
        match &mut self.view {
            View::Keys(_) => return keys::key(self, k),
            View::Batch(_) | View::BatchResults { .. } => return batch::key(self, k),
            View::Changes(_) | View::Backups { .. } => return changes::key(self, k),
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
                            if f.is_duplicate() {
                                Action::SaveCopy(Box::new(host), f.target.clone())
                            } else {
                                Action::Save(f.original.clone(), Box::new(host), f.target.clone())
                            }
                        }
                        Err(e) => {
                            f.error = e.to_string();
                            Action::None
                        }
                    },
                    _ => Action::None,
                }
            }
            View::Aliases { .. } => return aliases::key(self, k),
            View::Availability { selected } => {
                match k.code {
                    KeyCode::Esc => self.back(),
                    KeyCode::Char('c') if ctrl => self.back(),
                    KeyCode::Up | KeyCode::Home => *selected = 0,
                    KeyCode::Down | KeyCode::End => *selected = 1,
                    KeyCode::Enter => {
                        return Action::Availability(if *selected == 0 {
                            AvailabilityMethod::Ssh
                        } else {
                            AvailabilityMethod::Icmp
                        })
                    }
                    _ => {}
                }
                return Action::None;
            }
            View::GroupMode { .. } => return groups::key(self, k),
            View::Profiles(_) => return self.profile_key(k),
            View::Active { .. } => return self.active_key(k),
            View::Forward(f) => {
                if ctrl && k.code == KeyCode::Char('l') {
                    return if f.validate() {
                        match f.forward() {
                            Ok(forward) => {
                                if f.mode == LaunchMode::TunnelOnly {
                                    Action::StartTunnel(
                                        f.host.clone(),
                                        (!f.inputs[5].value.is_empty())
                                            .then(|| f.inputs[5].value.clone()),
                                        forward,
                                    )
                                } else {
                                    Action::Connect(f.host.clone(), Some(forward))
                                }
                            }
                            Err(error) => {
                                f.error = error.to_string();
                                Action::None
                            }
                        }
                    } else {
                        Action::None
                    };
                }
                return match f.key(k) {
                    FormEvent::Cancel => {
                        if f.dirty() {
                            self.discard = true;
                        } else {
                            self.back();
                        }
                        Action::None
                    }
                    FormEvent::Submit if !f.validate_save() => Action::None,
                    FormEvent::Submit => match f.forward() {
                        Ok(forward) => Action::SaveProfile {
                            host: f.host.clone(),
                            original: f.original_name.clone(),
                            name: f.inputs[5].value.clone(),
                            forward,
                            mode: f.mode,
                        },
                        Err(e) => {
                            f.error = e.to_string();
                            Action::None
                        }
                    },
                    _ => Action::None,
                };
            }
            View::GroupInfo { .. } => {
                if matches!(k.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter)
                    || (ctrl && k.code == KeyCode::Char('c'))
                {
                    self.back();
                } else {
                    self.scroll_key(k);
                }
                return Action::None;
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
            View::SourceFiles { .. } => return source_filter::key(self, k),
            View::Files {
                files,
                selected,
                host,
                seed,
                duplicate,
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
                            let form = if let Some(source) = duplicate {
                                HostForm::duplicate((**source).clone(), target.clone())
                            } else {
                                HostForm::add(seed.clone(), target.clone())
                            };
                            let view = View::Host(Box::new(form));
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
        if k.code == KeyCode::F(3) {
            return self.activate_action(PaletteAction::ChooseAlias);
        }
        if k.code == KeyCode::F(5) {
            return Action::Reload;
        }
        if k.code == KeyCode::F(6) {
            return self.activate_action(PaletteAction::Profiles);
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
                self.group_navigation(KeyCode::Enter);
            }
            KeyCode::Char('/') if !self.search_focus => self.search_focus = true,
            KeyCode::Char('f') if ctrl && !self.search_focus => self.search_focus = true,
            KeyCode::Char('K') if !self.search_focus => return Action::OpenKeys(false),
            _ if self.search_focus => {
                let old = self.query.value.clone();
                self.query.key(k);
                if old != self.query.value {
                    self.filter(true);
                }
            }
            KeyCode::Char('P') => self.open_availability(),
            KeyCode::Char('x') => batch::mark(self),
            KeyCode::Char('B') => batch::open(self),
            KeyCode::Char('A') => return Action::ToggleAliases,
            KeyCode::Char('g') => groups::open(self),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => self.group_navigation(k.code),
            KeyCode::Char('c') => return Action::SourceFilter,
            KeyCode::Char('C') => {
                self.source_filter = None;
                self.filter(false);
            }
            KeyCode::Char('j') => self.navigate(1),
            KeyCode::Char('k') => self.navigate(-1),
            KeyCode::PageDown => self.navigate(self.page as isize),
            KeyCode::PageUp => self.navigate(-(self.page as isize)),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.rows.len().saturating_sub(1),
            KeyCode::Char('*') => return self.activate_action(PaletteAction::Favorite),
            KeyCode::Char('N') => return self.activate_action(PaletteAction::Note),
            KeyCode::Char('D') => return self.activate_action(PaletteAction::Duplicate),
            KeyCode::Char('a') => return self.activate_action(PaletteAction::Add),
            KeyCode::Char('e') => return self.activate_action(PaletteAction::Edit),
            KeyCode::Char('i') => {
                if let Some(h) = self.selected_host() {
                    self.open_view(View::Info(h));
                } else if let Some(groups::Row::Group { key, count, .. }) =
                    self.rows.get(self.selected)
                {
                    self.open_view(View::GroupInfo {
                        key: key.clone(),
                        count: *count,
                    });
                }
            }
            KeyCode::Char('m') => {
                if let Some(h) = self.selected_host() {
                    return Action::Move(h);
                }
            }
            KeyCode::Char('d') => {
                if let Some(host) = self.selected_host() {
                    return Action::Delete(host);
                }
            }
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
    fn open_availability(&mut self) {
        self.open_view(View::Availability {
            selected: usize::from(self.preferences.availability_method == AvailabilityMethod::Icmp),
        });
    }
    fn activate_action(&mut self, action: PaletteAction) -> Action {
        if !action.enabled(self.selected_host().is_some()) {
            return Action::None;
        }
        match action {
            PaletteAction::BatchMark => batch::mark(self),
            PaletteAction::BatchSelectVisible => batch::select_visible(self),
            PaletteAction::BatchClear => self.batch.marks.clear(),
            PaletteAction::BatchOpen => batch::open(self),
            PaletteAction::BatchResults => batch::show_results(self),
            PaletteAction::ToggleAliases => return Action::ToggleAliases,
            PaletteAction::Availability => self.open_availability(),
            PaletteAction::GroupMode => groups::open(self),
            PaletteAction::ChooseAlias => aliases::open(self),
            PaletteAction::SourceFilter => return Action::SourceFilter,
            PaletteAction::ClearSourceFilter => {
                self.source_filter = None;
                self.filter(false);
            }
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
            PaletteAction::Keys => return Action::OpenKeys(false),
            PaletteAction::Add => return Action::Add(String::new()),
            PaletteAction::Duplicate => {
                if let Some(host) = self.selected_host() {
                    return Action::Duplicate(host);
                }
            }
            PaletteAction::Edit => {
                if let Some(host) = self.selected_host() {
                    self.open_view(View::Host(Box::new(HostForm::edit(host))));
                }
            }
            PaletteAction::Backups => return Action::Backups,
            PaletteAction::Active => self.open_view(View::Active { selected: 0 }),
            PaletteAction::Profiles => {
                if let Some(host) = self.selected_host() {
                    self.open_view(View::Profiles(profiles::List::new(host.name)));
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
            EnableBracketedPaste,
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
                DisableBracketedPaste,
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
    start_availability_inspection(inspector, m, config, probe)
}

/// Changing methods keeps the last resolved values while their replacements
/// arrive, so effective-value searches and concrete selection do not disappear.
fn start_availability_inspection(
    inspector: &runtime::effective::Inspector,
    m: &mut Model,
    config: &Path,
    probe: bool,
) -> Receiver<runtime::effective::Inspection> {
    m.statuses.clear();
    let hosts = m.all.clone();
    for h in &hosts {
        m.statuses.insert(
            h.name.clone(),
            PingResult {
                method: m.preferences.availability_method,
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
    m.refresh_search_membership();
    inspector.inspect_with_method(
        hosts,
        config.to_path_buf(),
        probe,
        m.preferences.availability_method,
    )
}
/// Load completely before changing the browser or discarding its diagnostic generation.
fn reload_hosts(
    inspector: &runtime::effective::Inspector,
    m: &mut Model,
    config: &Path,
) -> Result<Receiver<runtime::effective::Inspection>> {
    let report = config::load_report_checked(config)?;
    let info = match &m.view {
        View::Info(host) => Some(host.clone()),
        _ => None,
    };
    m.replace_hosts(report.hosts, None);
    m.config_warnings = report.warnings;
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
            duplicate: None,
        });
    }
    Ok(())
}
/// Duplication needs the source alias's effective connection target, never add defaults.
/// Resolution is performed by the existing worker pool; pending/errors leave navigation live.
fn select_duplicate_files(m: &mut Model, config: &Path, mut source: Host) -> Result<()> {
    let effective = match m.effective.get(&source.name) {
        Some(Ok(effective)) => effective,
        Some(Err(error)) => anyhow::bail!(
            "Cannot duplicate: source ssh -G failed: {error}. F5 reloads; retry D afterwards."
        ),
        None => anyhow::bail!(
            "Cannot duplicate yet: source ssh -G is pending. Retry D when resolution completes."
        ),
    };
    source.hostname.clone_from(&effective.hostname);
    source.user.clone_from(&effective.user);
    source.port.clone_from(&effective.port);
    let files = config::config_files(config)?;
    m.open_view(View::Files {
        files,
        selected: 0,
        host: None,
        seed: String::new(),
        duplicate: Some(Box::new(source)),
    });
    m.error.clear();
    Ok(())
}
/// Recheck the full Include graph immediately before the targeted add operation.
#[cfg(test)]
fn save_copy(config: &Path, host: &Host, target: &Path, app_dir: &Path) -> Result<()> {
    config::add_host_preserving_target(host, target, app_dir, config)
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
    let report = config::load_report(config)?;
    let mut metadata = Metadata::load(&paths.root, config)?;
    let mut profiles = TunnelProfiles::load(&paths.root, config)?;
    let standalone = !matches!(initial, InitialView::List);
    let mut m = Model::new(
        report.hosts,
        preferences,
        history.connections.clone(),
        force_search,
    );
    m.metadata = metadata.hosts().clone();
    m.batch.root = config.to_path_buf();
    m.batch.history_root = Some(paths.root.clone());
    m.config_warnings = report.warnings;
    m.tunnel_profiles = profiles.hosts().clone();
    m.tunnel_modes = profiles.modes().clone();
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
    let mut tunnels = active::Session::default();
    let mut guard = TerminalGuard::enter()?;
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    loop {
        keys::poll(&mut m);
        match m.batch.poll() {
            Ok(Some(history)) => m.history = history.connections,
            Ok(None) => {}
            Err(error) => m.error = format!("Warning: {error:#}"),
        }
        tunnels.poll(&mut m, paths);
        while let Ok(result) = inspection.try_recv() {
            m.apply_inspection(result);
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
                m.paste(&text);
                Action::None
            }
            _ => Action::None,
        };
        if standalone && matches!(m.view, View::List) && matches!(action, Action::None) {
            return Ok(0);
        }
        let mut completed_action = action.clone();
        let operation: Result<bool> = match action {
            Action::Favorite(_) | Action::SaveNote(..) => {
                apply_metadata_action(&mut m, &mut metadata, &action).map(|_| false)
            }
            Action::SaveProfile { .. } | Action::DeleteProfile(..) => {
                profiles::apply(&mut m, &mut profiles, &action).map(|_| false)
            }
            Action::ConfirmChanges => changes::apply(&mut m, &paths.root).map(|action| {
                completed_action = action;
                true
            }),
            Action::Backups => changes::open_backups(&mut m, &paths.root).map(|_| false),
            Action::RestoreBackup(index) => changes::open_restore(&mut m, index).map(|_| false),
            Action::StartBatch => batch::start(&mut m).map(|_| false),
            Action::CancelBatch => {
                m.batch.manager.cancel();
                Ok(false)
            }
            Action::OpenKeys(picker) => {
                keys::open_view(&mut m, picker);
                keys::refresh(&mut m);
                Ok(false)
            }
            Action::RefreshKeys => {
                keys::refresh(&mut m);
                Ok(false)
            }
            Action::AttachKey => keys::attach(&mut m, config).map(|_| false),
            Action::GenerateKey | Action::AddKey | Action::DeployKey
                if m.batch.manager.has_active() =>
            {
                m.batch.ensure_foreground_idle().map(|_| false)
            }
            Action::GenerateKey | Action::AddKey | Action::DeployKey => {
                m.batch.ensure_foreground_idle()?;
                #[cfg(unix)]
                let signals = SshSignals::enter()?;
                guard.restore();
                let result = keys::native(&mut m, &action, config);
                guard = TerminalGuard::enter()?;
                #[cfg(unix)]
                drop(signals);
                terminal.clear()?;
                result.map(|_| false)
            }
            Action::None => Ok(false),
            Action::Quit => {
                if (tunnels.manager.has_active()
                    || m.batch.manager.has_active()
                    || m.has_unsaved_context())
                    && !m.quit_tunnels
                {
                    m.quit_tunnels = true;
                    Ok(false)
                } else {
                    m.batch.manager.cancel();
                    if let Err(error) = m.batch.poll() {
                        m.error = format!("Warning: {error:#}");
                    }
                    tunnels.manager.stop_all();
                    tunnels.poll(&mut m, paths);
                    guard.restore();
                    if !m.error.is_empty() {
                        eprintln!("{}", m.error);
                    }
                    return Ok(0);
                }
            }
            Action::StartTunnel(name, profile, forward) => tunnels
                .start(&mut m, paths, config, name, profile, forward)
                .map(|_| false),
            Action::StopTunnel(id) => tunnels.manager.stop(id).map(|_| false),
            Action::Connect(_, _) if m.batch.manager.has_active() => {
                m.batch.ensure_foreground_idle().map(|_| false)
            }
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
            Action::Reload => profiles::reload(
                &inspector,
                &mut m,
                config,
                &paths.root,
                &mut metadata,
                &mut profiles,
            )
            .map(|receiver| {
                inspection = receiver;
                false
            }),
            Action::ToggleAliases | Action::Availability(_) | Action::GroupMode(_) => {
                apply_preference_action(
                    &inspector,
                    &mut m,
                    &paths.root,
                    config,
                    &mut inspection,
                    &action,
                )
                .map(|_| false)
            }
            Action::SourceFilter => source_filter::open(&mut m, config).map(|_| false),
            Action::Add(seed) => select_files(&mut m, config, None, seed).map(|_| false),
            Action::Duplicate(host) => select_duplicate_files(&mut m, config, host).map(|_| false),
            Action::Move(host) => {
                select_files(&mut m, config, Some(host), String::new()).map(|_| false)
            }
            Action::SaveCopy(..) | Action::Save(..) | Action::Delete(_) | Action::MoveTo(..) => {
                changes::open(&mut m, action, config).map(|_| false)
            }
        };
        match operation {
            Ok(true) => {
                let report = config::load_report_checked(config)?;
                m.config_warnings = report.warnings;
                profiles::complete_saved(
                    &mut m,
                    report.hosts,
                    &completed_action,
                    &mut metadata,
                    &mut profiles,
                );
                if standalone {
                    anyhow::ensure!(m.error.is_empty(), "{}", m.error);
                    return Ok(0);
                }
                inspection = start_inspection(&inspector, &mut m, config, false);
            }
            Ok(false) => {}
            Err(e) => match &mut m.view {
                View::Host(f) => f.error = e.to_string(),
                View::Forward(f) => f.error = e.to_string(),
                View::Note(editor) => editor.error = format!("{e:#}"),
                View::Profiles(list) => {
                    list.error = format!("{e:#}");
                    list.delete = None;
                }
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

#[cfg(test)]
mod file_tags_tests;

fn save_availability(m: &mut Model, root: &Path, method: AvailabilityMethod) -> Result<()> {
    let preferences = AppConfig::update(root, |config| config.availability_method = method)?;
    m.preferences = preferences;
    m.error.clear();
    m.back();
    Ok(())
}

/// Shared dispatch for every action that adopts a fresh complete AppConfig.
fn apply_preference_action(
    inspector: &runtime::effective::Inspector,
    m: &mut Model,
    root: &Path,
    config: &Path,
    inspection: &mut Receiver<runtime::effective::Inspection>,
    action: &Action,
) -> Result<()> {
    let previous_method = m.preferences.availability_method;
    match action {
        Action::ToggleAliases => aliases::toggle(m, root),
        Action::GroupMode(mode) => groups::save(m, root, *mode),
        Action::Availability(method) => save_availability(m, root, *method),
        _ => unreachable!("only preference adoption actions reach this dispatcher"),
    }?;
    // Alias/group saves also adopt the entire fresh config, including external
    // method edits. Never publish that choice with the old diagnostic generation.
    if previous_method != m.preferences.availability_method
        || matches!(action, Action::Availability(_))
    {
        *inspection = start_availability_inspection(inspector, m, config, true);
    }
    Ok(())
}
