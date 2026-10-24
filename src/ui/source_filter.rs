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

/// Entry 0 is "all files"; entry `n` is `files[n - 1]`.
pub(super) fn key(m: &mut Model, k: KeyEvent) -> Action {
    let texts = m.view.selector_texts().unwrap_or_default();
    let View::SourceFiles { files, selected } = &mut m.view else {
        return Action::None;
    };
    match select::key(selected, &texts, Some(&mut m.selector_query), k) {
        select::Outcome::Cancel => m.back(),
        select::Outcome::Stay => {}
        select::Outcome::Apply => {
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
    }
    Action::None
}
