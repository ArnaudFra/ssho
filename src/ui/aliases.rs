//! UI-only declaration projection. Every output index still addresses a concrete raw alias.
use super::*;

/// Only trustworthy declaration provenance may join rows. Membership is part of
/// the key so inconsistent/synthetic declarations cannot borrow another block's aliases.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BlockId {
    source: PathBuf,
    line: usize,
    aliases: Vec<String>,
}

pub(super) fn block_id(host: &Host) -> Option<BlockId> {
    (host.line_number > 0
        && !host.source_file.as_os_str().is_empty()
        && host.aliases.contains(&host.name))
    .then(|| BlockId {
        source: host.source_file.clone(),
        line: host.line_number,
        aliases: host.aliases.clone(),
    })
}

/// Remembers the active alias of each block while it remains eligible. The
/// selected identity has priority, including after F5 moves declaration lines.
#[derive(Default)]
pub(super) struct Projection {
    active: BTreeMap<BlockId, String>,
}
impl Projection {
    pub(super) fn project(
        &mut self,
        all: &[Host],
        matching: &[usize],
        identity: Option<&Host>,
    ) -> Vec<usize> {
        let mut blocks: BTreeMap<BlockId, Vec<usize>> = BTreeMap::new();
        let mut rows = Vec::new();
        for &index in matching {
            if let Some(id) = block_id(&all[index]) {
                blocks.entry(id).or_default().push(index);
            } else {
                rows.push(index);
            }
        }
        let mut next = BTreeMap::new();
        for (id, members) in blocks {
            let selected = identity.and_then(|host| {
                members
                    .iter()
                    .copied()
                    .find(|&i| all[i].name == host.name && all[i].source_file == host.source_file)
            });
            let remembered = self
                .active
                .get(&id)
                .and_then(|name| members.iter().copied().find(|&i| &all[i].name == name));
            let primary = members
                .iter()
                .copied()
                .find(|&i| all[i].aliases.first() == Some(&all[i].name));
            let active = selected.or(remembered).or(primary).unwrap_or(members[0]);
            next.insert(id, all[active].name.clone());
            rows.push(active);
        }
        self.active = next;
        rows
    }
}

pub(super) fn primary(host: &Host) -> &str {
    if block_id(host).is_some() {
        host.aliases
            .first()
            .map(String::as_str)
            .unwrap_or(&host.name)
    } else {
        &host.name
    }
}

pub(super) fn open(m: &mut Model) {
    let Some(host) = m.selected_host() else {
        return;
    };
    let id = block_id(&host);
    let mut hosts: Vec<_> = m
        .all
        .iter()
        .filter(|candidate| id.is_some() && block_id(candidate) == id)
        .cloned()
        .collect();
    if hosts.is_empty() {
        hosts.push(host.clone());
    }
    hosts.sort_by_key(|candidate| host.aliases.iter().position(|name| name == &candidate.name));
    let selected = hosts
        .iter()
        .position(|candidate| candidate.name == host.name)
        .unwrap_or(0);
    m.open_view(View::Aliases { hosts, selected });
}

pub(super) fn key(m: &mut Model, key: KeyEvent) -> Action {
    let texts = m.view.selector_texts().unwrap_or_default();
    let View::Aliases { hosts, selected } = &mut m.view else {
        return Action::None;
    };
    match select::key(selected, &texts, Some(&mut m.selector_query), key) {
        select::Outcome::Cancel => m.back(),
        select::Outcome::Stay => {}
        select::Outcome::Apply => {
            let Some(host) = hosts.get(*selected).cloned() else {
                return Action::None;
            };
            m.back();
            // Explicit selection supersedes a previous mutation's pending native result.
            // No inspection starts here, so do not create a new pending preference.
            m.pending_preferred = None;
            if !m.matches_search(&host) {
                m.query.set("");
            }
            if host.hidden() {
                m.show_hidden = true;
            }
            if !m.is_favorite(&host.name) {
                m.favorites_only = false;
            }
            m.refilter(Some(&host), m.selected);
        }
    }
    Action::None
}

pub(super) fn toggle(m: &mut Model, root: &Path) -> Result<()> {
    let identity = m.selected_host();
    let enabled = !m.preferences.group_aliases;
    // Fresh read protects settings edited externally since the browser was opened.
    let preferences = AppConfig::update(root, |config| config.group_aliases = enabled)?;
    m.preferences = preferences;
    m.refilter(identity.as_ref(), m.selected);
    m.error.clear();
    Ok(())
}
