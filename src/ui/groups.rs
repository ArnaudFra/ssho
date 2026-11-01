//! Header-inclusive display projection over the sorted concrete alias indices.
//! Collapse never changes raw membership or the active-alias projection.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Key {
    File(PathBuf),
    Tag(PathBuf, Option<String>),
}
impl Key {
    fn parent(&self) -> Option<Self> {
        match self {
            Self::Tag(path, _) => Some(Self::File(path.clone())),
            Self::File(_) => None,
        }
    }
    pub(super) fn label(&self) -> String {
        match self {
            Self::File(path) => path.display().to_string(),
            Self::Tag(_, tag) => tag
                .clone()
                .unwrap_or_else(|| crate::t!("No tag", "Sans tag").into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Host(usize),
    Group {
        key: Key,
        depth: usize,
        count: usize,
    },
}

#[derive(Default)]
pub(super) struct Projection {
    // Manual choices survive search; query-local choices expire on query changes.
    closed: BTreeSet<Key>,
    query_closed: BTreeSet<Key>,
    query: String,
    pub(super) group_count: usize,
}
impl Projection {
    fn collapsed_mut(&mut self) -> &mut BTreeSet<Key> {
        if self.query.is_empty() {
            &mut self.closed
        } else {
            &mut self.query_closed
        }
    }
    pub(super) fn collapsed(&self, key: &Key) -> bool {
        if self.query.is_empty() {
            &self.closed
        } else {
            &self.query_closed
        }
        .contains(key)
    }
    pub(super) fn sync_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.into();
            self.query_closed.clear();
        }
    }
    pub(super) fn reveal(&mut self, host: &Host, mode: GroupMode) {
        for key in ancestors(host, mode) {
            self.collapsed_mut().remove(&key);
        }
    }
    pub(super) fn project(&mut self, all: &[Host], visible: &[usize], mode: GroupMode) -> Vec<Row> {
        self.group_count = 0;
        if mode == GroupMode::Flat {
            return visible.iter().copied().map(Row::Host).collect();
        }
        // File/tag order is stable; incoming favorite/recent/name order stays inside each group.
        let mut files: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
        for &index in visible {
            files
                .entry(all[index].source_file.clone())
                .or_default()
                .push(index);
        }
        let mut rows = Vec::new();
        for (path, members) in files {
            let file_key = Key::File(path.clone());
            self.group_count += 1;
            rows.push(Row::Group {
                key: file_key.clone(),
                depth: 0,
                count: members.len(),
            });
            if mode == GroupMode::Files {
                if !self.collapsed(&file_key) {
                    rows.extend(members.into_iter().map(Row::Host));
                }
                continue;
            }
            let mut tags: BTreeMap<Option<String>, Vec<usize>> = BTreeMap::new();
            for index in members {
                tags.entry(primary_tag(&all[index]))
                    .or_default()
                    .push(index);
            }
            self.group_count += tags.len();
            if self.collapsed(&file_key) {
                continue;
            }
            for (tag, members) in tags {
                let key = Key::Tag(path.clone(), tag);
                rows.push(Row::Group {
                    key: key.clone(),
                    depth: 1,
                    count: members.len(),
                });
                if !self.collapsed(&key) {
                    rows.extend(members.into_iter().map(Row::Host));
                }
            }
        }
        rows
    }
}
fn primary_tag(host: &Host) -> Option<String> {
    host.tags
        .first()
        .or_else(|| host.file_tags.first())
        .cloned()
}
pub(super) fn ancestors(host: &Host, mode: GroupMode) -> Vec<Key> {
    match mode {
        GroupMode::Flat => vec![],
        GroupMode::Files => vec![Key::File(host.source_file.clone())],
        GroupMode::FileTags => vec![
            Key::File(host.source_file.clone()),
            Key::Tag(host.source_file.clone(), primary_tag(host)),
        ],
    }
}

impl Model {
    pub(super) fn selected_group(&self) -> Option<Key> {
        match self.rows.get(self.selected) {
            Some(Row::Group { key, .. }) => Some(key.clone()),
            _ => None,
        }
    }
    /// Called after refiltering real hosts. Explicit preferred hosts open only if eligible.
    pub(super) fn rebuild_rows(
        &mut self,
        identity: Option<&Host>,
        group: Option<Key>,
        fallback: usize,
    ) {
        self.group_projection.sync_query(&self.query.value);
        let preferred = identity.and_then(|host| {
            self.visible.iter().copied().find(|&i| {
                self.all[i].name == host.name && self.all[i].source_file == host.source_file
            })
        });
        if let Some(index) = preferred {
            self.group_projection
                .reveal(&self.all[index], self.preferences.group_mode);
        }
        self.rows =
            self.group_projection
                .project(&self.all, &self.visible, self.preferences.group_mode);
        self.selected = preferred
            .and_then(|index| self.rows.iter().position(|row| *row == Row::Host(index)))
            .or_else(|| {
                group.as_ref().and_then(|group| {
                    self.rows
                        .iter()
                        .position(|row| matches!(row, Row::Group { key, .. } if key == group))
                })
            })
            .unwrap_or(fallback)
            .min(self.rows.len().saturating_sub(1));
        self.offset = self.offset.min(self.selected);
    }
    pub(super) fn group_navigation(&mut self, code: KeyCode) {
        let group = self.selected_group();
        match code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(key) = group.clone() {
                    if !self.group_projection.collapsed_mut().remove(&key) {
                        self.group_projection.collapsed_mut().insert(key);
                    }
                    self.rebuild_rows(None, group, self.selected);
                }
            }
            KeyCode::Left => {
                if let Some(key) = group
                    .clone()
                    .filter(|key| !self.group_projection.collapsed(key))
                {
                    self.group_projection.collapsed_mut().insert(key);
                    self.rebuild_rows(None, group, self.selected);
                } else {
                    let parent = group.and_then(|key| key.parent()).or_else(|| {
                        self.selected_host()
                            .and_then(|host| ancestors(&host, self.preferences.group_mode).pop())
                    });
                    if let Some(index) = self.rows.iter().position(
                        |row| matches!(row, Row::Group { key, .. } if Some(key) == parent.as_ref()),
                    ) {
                        self.selected = index;
                    }
                }
            }
            KeyCode::Right => {
                if let Some(key) = group.clone() {
                    if self.group_projection.collapsed_mut().remove(&key) {
                        self.rebuild_rows(None, group, self.selected);
                    } else if self.selected + 1 < self.rows.len() {
                        self.selected += 1;
                    }
                }
            }
            _ => {}
        }
        self.offset = self.offset.min(self.selected);
    }
}

/// Each mode with its name in the interface language.
pub(super) fn modes() -> [(GroupMode, &'static str); 3] {
    [
        (
            GroupMode::Flat,
            crate::t!("Flat — one list", "Flat — liste plate"),
        ),
        (GroupMode::Files, crate::t!("Files", "Files — fichiers")),
        (
            GroupMode::FileTags,
            crate::t!(
                "File / tags — files, then main tag",
                "File / tags — fichiers et tag principal"
            ),
        ),
    ]
}
pub(super) fn open(m: &mut Model) {
    let selected = modes()
        .iter()
        .position(|(mode, _)| *mode == m.preferences.group_mode)
        .unwrap_or(0);
    m.open_view(View::GroupMode { selected });
}
pub(super) fn key(m: &mut Model, key: KeyEvent) -> Action {
    let View::GroupMode { selected } = &mut m.view else {
        return Action::None;
    };
    let texts: Vec<String> = modes()
        .iter()
        .map(|(_, label)| (*label).to_owned())
        .collect();
    match select::key(selected, &texts, None, key) {
        select::Outcome::Cancel => m.back(),
        select::Outcome::Apply => return Action::GroupMode(modes()[*selected].0),
        select::Outcome::Stay => {}
    }
    Action::None
}
pub(super) fn save(m: &mut Model, root: &Path, mode: GroupMode) -> Result<()> {
    // Capture the old concrete selection before adopting fresh settings (including alias grouping).
    let host = m.selected_host();
    let preferences = AppConfig::update(root, |config| config.group_mode = mode)?;
    m.preferences = preferences;
    m.refilter(host.as_ref(), m.selected);
    m.error.clear();
    m.back();
    Ok(())
}
