//! Ordered table layout and isolated preference draft. Rendering reads fresh preferences.
use super::*;
use crate::storage::default_host_columns;

impl HostColumn {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Hostname => "Hostname",
            Self::User => "User",
            Self::Port => "Port",
            Self::Tags => "Tags",
            Self::LastAttempt => "Last Attempt",
        }
    }
    fn minimum(self) -> u16 {
        match self {
            Self::Name => 12,
            Self::Hostname => 16,
            Self::User | Self::Tags => 10,
            Self::Port => 5,
            Self::LastAttempt => 14,
        }
    }
}
pub(super) const ALL: [HostColumn; 6] = [
    HostColumn::Name,
    HostColumn::Hostname,
    HostColumn::User,
    HostColumn::Port,
    HostColumn::Tags,
    HostColumn::LastAttempt,
];

/// Columns at the end are lowest priority. The default keeps historical percentages.
pub(super) fn layout(columns: &[HostColumn], width: u16) -> Vec<(HostColumn, u16)> {
    if columns == default_host_columns() {
        let percentages: &[u16] = match width {
            0..=36 => &[100],
            37..=64 => &[45, 55],
            65..=94 => &[40, 38, 22],
            _ => &[26, 30, 23, 21],
        };
        let available = width.saturating_sub(percentages.len() as u16 - 1);
        let mut widths: Vec<_> = percentages
            .iter()
            .map(|p| (u32::from(available) * u32::from(*p) / 100) as u16)
            .collect();
        let remainder = available - widths.iter().sum::<u16>();
        *widths.last_mut().unwrap() += remainder;
        return columns.iter().copied().zip(widths).collect();
    }
    let mut count = columns.len();
    while count > 1
        && columns[..count].iter().map(|c| c.minimum()).sum::<u16>() + count as u16 - 1 > width
    {
        count -= 1;
    }
    if count <= 1 {
        return vec![(HostColumn::Name, width)];
    }
    let minimum: u16 = columns[..count].iter().map(|c| c.minimum()).sum();
    let extra = width - minimum - (count as u16 - 1);
    columns[..count]
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                *c,
                c.minimum() + extra / count as u16 + u16::from((i as u16) < extra % count as u16),
            )
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(super) struct Editor {
    pub draft: Vec<HostColumn>,
    pub selected: usize,
}
impl Editor {
    pub(super) fn items(&self) -> Vec<HostColumn> {
        self.draft
            .iter()
            .copied()
            .chain(ALL.into_iter().filter(|c| !self.draft.contains(c)))
            .collect()
    }
}
pub(super) fn open(m: &mut Model) {
    m.open_view(View::Columns(Editor {
        draft: m.preferences.host_columns.clone(),
        selected: 0,
    }));
    m.error.clear();
}
pub(super) fn key(m: &mut Model, key: KeyEvent) -> Action {
    let View::Columns(editor) = &mut m.view else {
        return Action::None;
    };
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Esc => {
            m.back();
            m.error.clear();
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            m.back();
            m.error.clear();
        }
        KeyCode::Up if shift => {
            if editor.selected > 1 && editor.selected < editor.draft.len() {
                editor.draft.swap(editor.selected, editor.selected - 1);
                editor.selected -= 1;
            }
        }
        KeyCode::Down if shift => {
            if editor.selected > 0 && editor.selected + 1 < editor.draft.len() {
                editor.draft.swap(editor.selected, editor.selected + 1);
                editor.selected += 1;
            }
        }
        KeyCode::Up => editor.selected = editor.selected.saturating_sub(1),
        KeyCode::Down => editor.selected = (editor.selected + 1).min(ALL.len() - 1),
        KeyCode::Home => editor.selected = 0,
        KeyCode::End => editor.selected = ALL.len() - 1,
        KeyCode::Char(' ') => {
            let column = editor.items()[editor.selected];
            if column != HostColumn::Name {
                if editor.selected < editor.draft.len() {
                    editor.draft.remove(editor.selected);
                } else {
                    editor.draft.push(column);
                }
                editor.selected = editor.items().iter().position(|c| *c == column).unwrap();
            }
        }
        KeyCode::Char('r') => {
            editor.draft = default_host_columns();
            editor.selected = 0;
        }
        KeyCode::Enter => return Action::Columns(editor.draft.clone()),
        _ => {}
    }
    Action::None
}
pub(super) fn save(m: &mut Model, root: &Path, columns: &[HostColumn]) -> Result<()> {
    let host = m.selected_host();
    let preferences = AppConfig::update(root, |config| config.host_columns = columns.to_vec())?;
    m.preferences = preferences;
    // Other externally changed projection preferences are adopted together.
    m.refilter(host.as_ref(), m.selected);
    m.error.clear();
    m.back();
    Ok(())
}
