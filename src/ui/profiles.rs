//! Profile list interaction and storage commit boundaries.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct List {
    pub host: String,
    pub selected: usize,
    pub delete: Option<String>,
    pub error: String,
    /// Focused action under the list; see `ACTIONS`.
    pub button: usize,
}
/// Named actions on the selected profile, then the one that needs none.
pub(super) fn actions() -> [&'static str; 5] {
    [
        crate::t!("Open", "Ouvrir"),
        crate::t!("Launch", "Lancer"),
        crate::t!("Duplicate", "Dupliquer"),
        crate::t!("Delete", "Supprimer"),
        crate::t!("New", "Nouveau"),
    ]
}
impl List {
    pub fn new(host: String) -> Self {
        Self {
            host,
            selected: 0,
            delete: None,
            error: String::new(),
            button: 0,
        }
    }
}
impl Model {
    pub(super) fn profile_key(&mut self, k: KeyEvent) -> Action {
        let View::Profiles(list) = &mut self.view else {
            return Action::None;
        };
        if let Some(name) = &list.delete {
            if k.code == KeyCode::Enter {
                return Action::DeleteProfile(list.host.clone(), name.clone());
            }
            if nav::cancels(k) {
                list.delete = None;
            }
            return Action::None;
        }
        let profiles = self.tunnel_profiles.get(&list.host);
        let len = profiles.map_or(0, BTreeMap::len);
        let selected = profiles.and_then(|p| p.iter().nth(list.selected));
        // Without any profile only "New" can act.
        if len == 0 {
            list.button = actions().len() - 1;
        }
        // Enter runs the focused action: each one is also a direct key.
        let k = match (k.code, list.button) {
            (KeyCode::Enter, 1) => KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
            (KeyCode::Enter, 2) => KeyEvent::new(KeyCode::Char('D'), KeyModifiers::NONE),
            (KeyCode::Enter, 3) => KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
            (KeyCode::Enter, 4) => KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
            _ => k,
        };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            _ if nav::cancels(k) => self.back(),
            KeyCode::Right | KeyCode::Tab => {
                list.button = nav::wrap(list.button, actions().len(), 1);
            }
            KeyCode::Left | KeyCode::BackTab => {
                list.button = nav::wrap(list.button, actions().len(), -1);
            }
            KeyCode::Char('D') => {
                if let Some((name, forward)) = selected {
                    // A copy is a new unsaved draft: the original is untouched.
                    let mut form = ForwardForm::new(list.host.clone(), Some(forward.clone()));
                    form.mode = self
                        .tunnel_modes
                        .get(&list.host)
                        .and_then(|m| m.get(name))
                        .copied()
                        .unwrap_or_default();
                    form.inputs[5].set(&format!("{name}-copy"));
                    form.focus = 5;
                    self.open_view(View::Forward(form));
                }
            }
            KeyCode::Char('q') => self.back(),
            KeyCode::Up | KeyCode::Char('k') => list.selected = nav::wrap(list.selected, len, -1),
            KeyCode::Down | KeyCode::Char('j') => list.selected = nav::wrap(list.selected, len, 1),
            KeyCode::Home => list.selected = 0,
            KeyCode::End => list.selected = len.saturating_sub(1),
            KeyCode::Char('a') => {
                let mut form = ForwardForm::new(list.host.clone(), None);
                form.focus = 5;
                self.open_view(View::Forward(form));
            }
            KeyCode::Char('f') => {
                let saved = self
                    .history
                    .get(&list.host)
                    .and_then(|c| c.port_forwarding.clone());
                let form = ForwardForm::new(list.host.clone(), saved);
                self.open_view(View::Forward(form));
            }
            KeyCode::Enter | KeyCode::Char('e' | 'r') => {
                if let Some((name, forward)) = selected {
                    let mut form =
                        ForwardForm::named(list.host.clone(), name.clone(), forward.clone());
                    form.mode = self
                        .tunnel_modes
                        .get(&list.host)
                        .and_then(|m| m.get(name))
                        .copied()
                        .unwrap_or_default();
                    form.mark_saved();
                    self.open_view(View::Forward(form));
                }
            }
            KeyCode::Char('l') if ctrl => {
                if let Some((name, forward)) = selected {
                    if self.tunnel_modes.get(&list.host).and_then(|m| m.get(name))
                        == Some(&LaunchMode::TunnelOnly)
                    {
                        return Action::StartTunnel(
                            list.host.clone(),
                            Some(name.clone()),
                            forward.clone(),
                        );
                    }
                    return Action::Connect(list.host.clone(), Some(forward.clone()));
                }
            }
            KeyCode::Char('d') => list.delete = selected.map(|(name, _)| name.clone()),
            _ => {}
        }
        Action::None
    }
}
pub(super) fn apply(m: &mut Model, store: &mut TunnelProfiles, action: &Action) -> Result<()> {
    match action {
        Action::SaveProfile {
            host,
            original,
            name,
            forward,
            mode,
        } => {
            store.save_with_mode(host, original.as_deref(), name, forward, *mode)?;
            if let View::Forward(form) = &mut m.view {
                form.mark_saved();
            }
            // Select a renamed/new profile when returning to the parent list.
            for (parent, _) in &mut m.parents {
                if let View::Profiles(list) = parent {
                    list.selected = store
                        .hosts()
                        .get(host)
                        .and_then(|p| p.keys().position(|n| n == name))
                        .unwrap_or(0);
                }
            }
        }
        Action::DeleteProfile(host, name) => {
            store.delete(host, name)?;
            if let View::Profiles(list) = &mut m.view {
                list.delete = None;
                list.selected = list.selected.min(
                    store
                        .hosts()
                        .get(host)
                        .map_or(0, BTreeMap::len)
                        .saturating_sub(1),
                );
                list.error.clear();
            }
        }
        _ => return Ok(()),
    }
    m.tunnel_profiles = store.hosts().clone();
    m.tunnel_modes = store.modes().clone();
    m.error.clear();
    Ok(())
}

/// Both stores load before publishing any new scope/browser state.
pub(super) fn reload(
    inspector: &runtime::effective::Inspector,
    m: &mut Model,
    config: &Path,
    root: &Path,
    metadata: &mut Metadata,
    profiles: &mut TunnelProfiles,
) -> Result<Receiver<runtime::effective::Inspection>> {
    let next_metadata = Metadata::load(root, config)?;
    let next_profiles = TunnelProfiles::load(root, config)?;
    let receiver = reload_hosts(inspector, m, config)?;
    *metadata = next_metadata;
    *profiles = next_profiles;
    m.metadata = metadata.hosts().clone();
    m.tunnel_profiles = profiles.hosts().clone();
    m.tunnel_modes = profiles.modes().clone();
    m.filter(false);
    Ok(receiver)
}

/// SSH has committed. Refresh its saved identity even if either app store fails.
pub(super) fn complete_saved(
    m: &mut Model,
    all: Vec<Host>,
    action: &Action,
    metadata: &mut Metadata,
    profiles: &mut TunnelProfiles,
) {
    let result = if let Action::Save(Some(old), new, _) = action {
        profiles.rename_hosts(old, new)
    } else {
        Ok(())
    };
    m.tunnel_profiles = profiles.hosts().clone();
    m.tunnel_modes = profiles.modes().clone();
    complete_saved_mutation(m, all, action, metadata);
    if let Err(error) = result {
        if !m.error.is_empty() {
            m.error.push_str(" • ");
        }
        m.error.push_str(&crate::tf!(
            "SSH saved; tunnel profile rename failed: {error:#}",
            "SSH enregistré ; échec du renommage du profil de tunnel : {error:#}"
        ));
    }
}
