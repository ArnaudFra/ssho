//! Profile list interaction and storage commit boundaries.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct List {
    pub host: String,
    pub selected: usize,
    pub delete: Option<String>,
    pub error: String,
}
impl List {
    pub fn new(host: String) -> Self {
        Self {
            host,
            selected: 0,
            delete: None,
            error: String::new(),
        }
    }
}
impl Model {
    pub(super) fn profile_key(&mut self, k: KeyEvent) -> Action {
        let View::Profiles(list) = &mut self.view else {
            return Action::None;
        };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if let Some(name) = &list.delete {
            if k.code == KeyCode::Enter {
                return Action::DeleteProfile(list.host.clone(), name.clone());
            }
            if k.code == KeyCode::Esc || (ctrl && k.code == KeyCode::Char('c')) {
                list.delete = None;
            }
            return Action::None;
        }
        let profiles = self.tunnel_profiles.get(&list.host);
        let len = profiles.map_or(0, BTreeMap::len);
        let selected = profiles.and_then(|p| p.iter().nth(list.selected));
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => self.back(),
            KeyCode::Char('c') if ctrl => self.back(),
            KeyCode::Up | KeyCode::Char('k') => list.selected = list.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                list.selected = (list.selected + 1).min(len.saturating_sub(1))
            }
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
                    let form = ForwardForm::named(list.host.clone(), name.clone(), forward.clone());
                    self.open_view(View::Forward(form));
                }
            }
            KeyCode::Char('l') if ctrl => {
                if let Some((_, forward)) = selected {
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
        } => {
            store.save(host, original.as_deref(), name, forward)?;
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
    complete_saved_mutation(m, all, action, metadata);
    if let Err(error) = result {
        if !m.error.is_empty() {
            m.error.push_str(" • ");
        }
        m.error.push_str(&format!(
            "SSH saved; tunnel profile rename failed: {error:#}"
        ));
    }
}
