//! One selector for every small choice: destination file, source filter, exact
//! alias, grouping and availability method. Same keys, same layout, a check mark
//! on the current value and a cursor that is distinct from it.
use super::actions::fold;
use super::forms::Input;
use super::nav;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;

const CYAN: Color = Color::Rgb(0, 173, 216);

pub(super) struct Item {
    pub label: String,
    /// Secondary column, such as `user@address` next to an alias.
    pub aside: String,
    /// Shown under the list for the highlighted item: full path, explanation.
    pub detail: Vec<String>,
    /// The value in effect before this selector was opened.
    pub current: bool,
    /// Text the search matches, beyond the label (a full path, for instance).
    pub search: String,
}

pub(super) struct Spec<'a> {
    pub title: String,
    /// What is being chosen, and for what.
    pub subject: String,
    pub items: Vec<Item>,
    pub selected: usize,
    pub error: &'a str,
    /// Verb of the Enter key: "appliquer", "enregistrer", "choisir".
    pub apply: &'static str,
    /// `Some` when the list can be narrowed by typing.
    pub query: Option<&'a Input>,
    pub empty: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Stay,
    /// Leave without changing the value in effect.
    Cancel,
    Apply,
}

fn matches(text: &str, query: &str) -> bool {
    let text = fold(text);
    fold(query)
        .split_whitespace()
        .all(|word| text.contains(word))
}

/// Indices of the items a query keeps, in display order.
pub(super) fn visible(texts: &[String], query: &str) -> Vec<usize> {
    (0..texts.len())
        .filter(|&index| matches(&texts[index], query))
        .collect()
}

/// Shared keys. The cursor moves among the items the query keeps and wraps at
/// both ends; nothing is applied before Enter, so Esc always keeps the value.
/// With a query the letters search; without one `j`/`k`/`q` keep their old roles.
pub(super) fn key(
    selected: &mut usize,
    texts: &[String],
    query: Option<&mut Input>,
    k: KeyEvent,
) -> Outcome {
    if nav::cancels(k) {
        return Outcome::Cancel;
    }
    let filter = query.as_ref().map_or("", |input| input.value.as_str());
    let shown = visible(texts, filter);
    let position = shown.iter().position(|index| index == selected);
    let step = |delta: isize| match position {
        Some(position) => shown.get(nav::wrap(position, shown.len(), delta)).copied(),
        None => shown.first().copied(),
    };
    let editing = query.as_ref().is_some_and(|input| !input.value.is_empty());
    let plain = !k
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    match k.code {
        KeyCode::Up => *selected = step(-1).unwrap_or(*selected),
        KeyCode::Down => *selected = step(1).unwrap_or(*selected),
        KeyCode::Home if !editing => *selected = shown.first().copied().unwrap_or(*selected),
        KeyCode::End if !editing => *selected = shown.last().copied().unwrap_or(*selected),
        KeyCode::Enter => {
            if position.is_some() {
                return Outcome::Apply;
            }
        }
        KeyCode::Tab | KeyCode::BackTab => {}
        _ => match query {
            Some(input) => {
                let before = input.value.clone();
                input.key(k);
                if before != input.value {
                    // Narrowing only moves the cursor; the value in effect keeps
                    // its check mark and is untouched until Enter.
                    let shown = visible(texts, &input.value);
                    if !shown.contains(selected) {
                        *selected = shown.first().copied().unwrap_or(*selected);
                    }
                }
            }
            None if plain => match k.code {
                KeyCode::Char('k') => *selected = step(-1).unwrap_or(*selected),
                KeyCode::Char('j') => *selected = step(1).unwrap_or(*selected),
                KeyCode::Char('q') => return Outcome::Cancel,
                _ => {}
            },
            None => {}
        },
    }
    Outcome::Stay
}

/// Shortest tail that tells each path from the others: the file name, then as
/// many parent directories as two files of the same name need.
pub(super) fn short_paths(paths: &[PathBuf]) -> Vec<String> {
    let parts: Vec<Vec<String>> = paths
        .iter()
        .map(|path| {
            path.components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .collect();
    let tail = |index: usize, depth: usize| {
        let all = &parts[index];
        let start = all.len().saturating_sub(depth);
        (start, all[start..].join("/"))
    };
    (0..paths.len())
        .map(|index| {
            let mut depth = 1;
            loop {
                let (start, label) = tail(index, depth);
                let unique = (0..paths.len()).all(|other| {
                    other == index || paths[other] == paths[index] || tail(other, depth).1 != label
                });
                if unique || start == 0 {
                    break if start == 0 {
                        paths[index].display().to_string()
                    } else {
                        format!("…/{label}")
                    };
                }
                depth += 1;
            }
        })
        .collect()
}

/// Full paths while they fit the width; otherwise the shortest distinguishing
/// tails, the full path of the highlighted file being in the detail zone.
pub(super) fn path_labels(paths: &[PathBuf], width: u16) -> Vec<String> {
    let full: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    // Room for the cursor, the check mark and the "(actuel)" note.
    if full
        .iter()
        .all(|path| path.width() + 14 <= usize::from(width))
    {
        full
    } else {
        short_paths(paths)
    }
}

/// Number of hosts declared in a file, worded for the detail zone.
pub(super) fn host_count(hosts: usize) -> String {
    match hosts {
        0 => "aucun hôte : fichier vide ou sans bloc Host".to_owned(),
        1 => "1 hôte dans ce fichier".to_owned(),
        hosts => format!("{hosts} hôtes dans ce fichier"),
    }
}

pub(super) fn file_detail(path: &Path, hosts: usize) -> Vec<String> {
    vec![path.display().to_string(), host_count(hosts)]
}

/// Rows a line takes once wrapped at word boundaries, as the paragraph draws it.
pub(super) fn wrapped_rows(line: &str, width: usize) -> usize {
    let mut rows = 1;
    let mut used = 0;
    for word in line.split_whitespace() {
        let size = word.width();
        if used > 0 && used + 1 + size > width {
            rows += 1;
            used = 0;
        }
        // A word longer than the row is cut across several rows.
        rows += (used + usize::from(used > 0) + size).saturating_sub(1) / width;
        used = (used + usize::from(used > 0) + size - 1) % width + 1;
    }
    rows
}

pub(super) fn draw(f: &mut Frame, area: Rect, spec: Spec) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let gray = Style::default().fg(Color::DarkGray);
    let filter = spec.query.map_or("", |input| input.value.as_str());
    let texts: Vec<String> = spec
        .items
        .iter()
        .map(|item| format!("{} {} {}", item.label, item.aside, item.search))
        .collect();
    let shown = visible(&texts, filter);
    let errors: Vec<Line> = spec
        .error
        .lines()
        .map(|line| Line::styled(line.to_owned(), Style::default().fg(Color::Red)))
        .collect();
    // Short terminals keep the title, the list, the keys and a whole error;
    // the optional rows only take what is left.
    let width = usize::from(area.width).max(1);
    let error_rows: usize = spec
        .error
        .lines()
        .map(|line| wrapped_rows(line, width))
        .sum();
    let mut spare = usize::from(area.height).saturating_sub(3);
    let mut heights = [0u16; 4];
    let wanted = [error_rows, usize::from(spec.query.is_some()), 1, 3];
    for (index, wanted) in wanted.into_iter().enumerate() {
        // The detail zone, last, needs a list that still shows several choices.
        if index == 3 && (area.height < 10 || spare < 5) {
            continue;
        }
        let granted = wanted.min(spare);
        spare -= granted;
        heights[index] = granted as u16;
    }
    let [error_height, search_height, subject_height, detail_height] = heights;
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(subject_height),
        Constraint::Length(search_height),
        Constraint::Min(1),
        Constraint::Length(detail_height),
        Constraint::Length(error_height),
        Constraint::Length(u16::from(area.height >= 3)),
    ])
    .split(area);
    f.render_widget(
        Paragraph::new(spec.title).style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        parts[0],
    );
    f.render_widget(Paragraph::new(spec.subject).style(gray), parts[1]);
    if let Some(query) = spec.query {
        let prompt = "Rechercher : ";
        let prompt_width = prompt.width() as u16;
        let (text, cursor) = query.viewport(
            true,
            usize::from(parts[2].width.saturating_sub(prompt_width)),
        );
        let hint = if query.value.is_empty() {
            "taper pour réduire la liste".to_owned()
        } else {
            text
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw(prompt),
                Span::styled(
                    hint,
                    if query.value.is_empty() {
                        gray
                    } else {
                        Style::default()
                    },
                ),
            ])),
            parts[2],
        );
        if let (Some(column), true) = (cursor, parts[2].height > 0) {
            if prompt_width + column < parts[2].width {
                f.set_cursor_position((parts[2].x + prompt_width + column, parts[2].y));
            }
        }
    }
    let capacity = usize::from(parts[3].height).max(1);
    let position = shown.iter().position(|index| *index == spec.selected);
    let offset = position.map_or(0, |position| position.saturating_sub(capacity - 1));
    let width = usize::from(parts[3].width);
    let label_width = shown
        .iter()
        .map(|&index| spec.items[index].label.width())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = shown
        .iter()
        .skip(offset)
        .take(capacity)
        .map(|&index| {
            let item = &spec.items[index];
            let selected = index == spec.selected;
            // Cursor and current value are two different marks.
            let mut text = format!(
                "{}{} {:<label_width$}",
                if selected { "▶" } else { " " },
                if item.current { "✓" } else { " " },
                item.label
            );
            if !item.aside.is_empty() {
                text.push_str("  ");
                text.push_str(&item.aside);
            }
            if item.current && text.width() + 10 <= width {
                text.push_str("  (actuel)");
            }
            Line::styled(
                text,
                if selected {
                    Style::default().bg(CYAN).fg(Color::Black)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    if shown.is_empty() {
        f.render_widget(
            Paragraph::new(if spec.items.is_empty() {
                spec.empty
            } else {
                "Aucun choix ne correspond à la recherche."
            })
            .style(gray),
            parts[3],
        );
    } else {
        f.render_widget(Paragraph::new(lines), parts[3]);
    }
    let detail: Vec<Line> = position
        .map(|_| &spec.items[spec.selected])
        .map(|item| {
            item.detail
                .iter()
                .map(|line| Line::from(line.clone()))
                .collect()
        })
        .unwrap_or_default();
    f.render_widget(Paragraph::new(detail).wrap(Wrap { trim: false }), parts[4]);
    f.render_widget(Paragraph::new(errors).wrap(Wrap { trim: false }), parts[5]);
    let keys = format!(
        "↑↓ choisir (boucle) • Entrée : {} • Échap : annuler, rien ne change{}",
        spec.apply,
        if spec.query.is_some() {
            " • lettres : rechercher"
        } else {
            ""
        }
    );
    f.render_widget(
        Paragraph::new(if keys.width() <= width {
            keys
        } else {
            format!("↑↓ • Entrée : {} • Échap", spec.apply)
        })
        .style(gray),
        parts[6],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_of(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn same_named_files_stay_distinguishable_with_the_shortest_tail() {
        let paths: Vec<PathBuf> = [
            "/etc/ssh/config",
            "/home/a/work/hosts",
            "/home/a/home/hosts",
            "/x/y/z",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(
            short_paths(&paths),
            ["…/config", "…/work/hosts", "…/home/hosts", "…/z"]
        );
        assert_eq!(short_paths(&[PathBuf::from("config")]), ["config"]);
        // Identical paths cannot be told apart by any tail: the full path is kept.
        let twice = vec![PathBuf::from("/a/b"), PathBuf::from("/a/b")];
        assert_eq!(short_paths(&twice), ["…/b", "…/b"]);
    }

    #[test]
    fn cursor_wraps_and_filtering_never_applies_anything() {
        let texts: Vec<String> = ["alpha", "beta", "gamma"].map(String::from).to_vec();
        let mut selected = 0;
        assert_eq!(
            key(&mut selected, &texts, None, key_of(KeyCode::Up)),
            Outcome::Stay
        );
        assert_eq!(selected, 2);
        key(&mut selected, &texts, None, key_of(KeyCode::Down));
        assert_eq!(selected, 0);
        key(&mut selected, &texts, None, key_of(KeyCode::Char('j')));
        assert_eq!(selected, 1);
        assert_eq!(
            key(&mut selected, &texts, None, key_of(KeyCode::Char('q'))),
            Outcome::Cancel
        );
        // With a search, letters are text: the cursor follows the matches.
        let mut query = Input::default();
        for c in "GÂM".chars() {
            assert_eq!(
                key(
                    &mut selected,
                    &texts,
                    Some(&mut query),
                    key_of(KeyCode::Char(c))
                ),
                Outcome::Stay
            );
        }
        assert_eq!((query.value.as_str(), selected), ("GÂM", 2));
        assert_eq!(visible(&texts, &query.value), [2]);
        key(
            &mut selected,
            &texts,
            Some(&mut query),
            key_of(KeyCode::Char('q')),
        );
        assert_eq!(query.value, "GÂMq", "q is text while searching");
        assert_eq!(
            key(
                &mut selected,
                &texts,
                Some(&mut query),
                key_of(KeyCode::Enter)
            ),
            Outcome::Stay,
            "nothing matches: Enter applies nothing"
        );
        key(
            &mut selected,
            &texts,
            Some(&mut query),
            key_of(KeyCode::Backspace),
        );
        assert_eq!(
            key(
                &mut selected,
                &texts,
                Some(&mut query),
                key_of(KeyCode::Enter)
            ),
            Outcome::Apply
        );
        assert_eq!(
            key(
                &mut selected,
                &texts,
                Some(&mut query),
                key_of(KeyCode::Esc)
            ),
            Outcome::Cancel
        );
    }
}
