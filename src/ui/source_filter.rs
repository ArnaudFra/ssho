//! Exact source scope, independent of the free-text file: predicate.
use super::*;

pub(super) fn open(m: &mut Model, config: &Path) -> Result<()> {
    let files = config::config_files(config)?;
    let selected = m
        .source_filter
        .as_ref()
        .and_then(|path| files.iter().position(|file| file == path))
        .map_or(0, |index| index + 1);
    m.open_view(View::SourceFiles { files, selected });
    Ok(())
}

pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let View::SourceFiles { files, selected } = &mut m.view else {
        return Action::None;
    };
    match k.code {
        _ if nav::cancels(k) => m.back(),
        KeyCode::Char('q') => m.back(),
        // "All files" is the first entry of this selector.
        KeyCode::Up | KeyCode::Char('k') => *selected = nav::wrap(*selected, files.len() + 1, -1),
        KeyCode::Down | KeyCode::Char('j') => *selected = nav::wrap(*selected, files.len() + 1, 1),
        KeyCode::Home => *selected = 0,
        KeyCode::End => *selected = files.len(),
        KeyCode::Enter => {
            m.source_filter = selected.checked_sub(1).and_then(|i| files.get(i)).cloned();
            // A result from the old scope must not rewrite this scope's search.
            // Expanding to all files (or retaining the same source) keeps the preference.
            if m.pending_preferred.as_ref().is_some_and(|pending| {
                m.source_filter
                    .as_ref()
                    .is_some_and(|path| path != &pending.host.source_file)
            }) {
                m.pending_preferred = None;
            }
            m.back();
            m.filter(false);
        }
        _ => {}
    }
    Action::None
}
