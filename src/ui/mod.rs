//! Native terminal interface, adapted from SSHM, copyright Guillaume Archambault (MIT).
mod forms;
mod render;
#[cfg(test)]
mod tests;
use crate::{
    config, runtime,
    storage::{AppConfig, AppPaths, History},
    types::{ConnectionInfo, Forward, Host},
};
use anyhow::Result;
use chrono::Utc;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use forms::{FormEvent, HostForm, Input};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Debug)]
pub enum InitialView {
    List,
    Add(String),
    Edit(String),
}
#[derive(Clone, Debug)]
enum View {
    List,
    Host(Box<HostForm>),
}
#[derive(Clone, Debug, PartialEq)]
enum Action {
    None,
    Quit,
    Connect(String, Option<Forward>),
    Add(String),
    Save(Option<Host>, Box<Host>, PathBuf),
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
    view: View,
    preferences: AppConfig,
    history: BTreeMap<String, ConnectionInfo>,
    error: String,
    page: usize,
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
            view: View::List,
            preferences,
            history,
            error: String::new(),
            page: 10,
        };
        m.filter(false);
        m
    }
    fn selected_host(&self) -> Option<Host> {
        self.visible
            .get(self.selected)
            .map(|&i| self.all[i].clone())
    }
    fn filter(&mut self, reset: bool) {
        self.visible = self
            .all
            .iter()
            .enumerate()
            .filter(|(_, h)| (self.show_hidden || !h.hidden()) && h.matches(&self.query.value))
            .map(|(i, _)| i)
            .collect();
        self.visible.sort_by(|&a, &b| {
            let a = &self.all[a];
            let b = &self.all[b];
            let names = || a.name.to_lowercase().cmp(&b.name.to_lowercase());
            if self.recent {
                self.history
                    .get(&b.name)
                    .map(|h| h.last_connect)
                    .cmp(&self.history.get(&a.name).map(|h| h.last_connect))
                    .then_with(names)
            } else {
                names()
            }
        });
        if reset {
            self.selected = 0;
            self.offset = 0;
        } else {
            self.selected = self.selected.min(self.visible.len().saturating_sub(1));
        }
    }
    fn navigate(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.visible.len().saturating_sub(1));
    }
    fn back(&mut self) {
        self.view = View::List;
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
        match &mut self.view {
            View::Host(f) => {
                return match f.key(k) {
                    FormEvent::Cancel => {
                        self.back();
                        Action::None
                    }
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
            View::List => {}
        }
        if k.code == KeyCode::Esc && self.search_focus && !self.query.value.is_empty() {
            self.query.set("");
            self.filter(true);
            return Action::None;
        }
        // Every printable key is text while search is focused, including configured quit letters.
        if (!self.search_focus || !matches!(k.code, KeyCode::Char(_)) || ctrl) && self.quit_key(k) {
            return Action::Quit;
        }
        match k.code {
            KeyCode::Tab => {
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
            KeyCode::Char('a') => return Action::Add(String::new()),
            KeyCode::Char('e') => {
                if let Some(h) = self.selected_host() {
                    self.view = View::Host(Box::new(HostForm::edit(h)));
                }
            }
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
            _ => {}
        }
        Action::None
    }
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
        if let Err(e) = execute!(io::stdout(), EnterAlternateScreen, cursor::Hide) {
            guard.restore();
            return Err(e.into());
        }
        Ok(guard)
    }
    fn restore(&mut self) {
        if self.active {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
            self.active = false;
        }
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}
/// Runs the interactive browser or a standalone CLI form. SSH takes over the restored terminal.
pub fn run(
    paths: &AppPaths,
    config: &Path,
    force_search: bool,
    initial: InitialView,
) -> Result<i32> {
    let preferences = AppConfig::load(&paths.root)?;
    let history = History::load(&paths.root)?;
    let hosts = config::load_hosts(config)?;
    let standalone = !matches!(initial, InitialView::List);
    let mut m = Model::new(
        hosts,
        preferences,
        history.connections.clone(),
        force_search,
    );
    match initial {
        InitialView::List => {}
        InitialView::Add(seed) => {
            m.view = View::Host(Box::new(HostForm::add(seed, config.to_path_buf())))
        }
        InitialView::Edit(name) => {
            m.view = View::Host(Box::new(HostForm::edit(config::find_host(config, &name)?)))
        }
    }
    let mut guard = TerminalGuard::enter()?;
    let mut terminal: Terminal<CrosstermBackend<Stdout>> =
        Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    loop {
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
        let operation: Result<bool> = match action {
            Action::None => Ok(false),
            Action::Quit => return Ok(0),
            Action::Connect(name, forward) => {
                guard.restore();
                return runtime::connect(paths, config, &name, false, &[], forward.as_ref());
            }
            Action::Add(seed) => {
                m.view = View::Host(Box::new(HostForm::add(seed, config.to_path_buf())));
                Ok(false)
            }
            Action::Save(original, host, target) => match original {
                Some(original) => config::update_host(&original, &host, &paths.root),
                None => config::add_host(&host, &target, &paths.root),
            }
            .map(|_| true),
        };
        match operation {
            Ok(true) => {
                if standalone {
                    return Ok(0);
                }
                m.all = config::load_hosts(config)?;
                m.filter(false);
                m.back();
                m.error.clear();
            }
            Ok(false) => {}
            Err(e) => match &mut m.view {
                View::Host(f) => f.error = e.to_string(),
                _ => {
                    m.error = e.to_string();
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
