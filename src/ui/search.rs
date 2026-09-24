//! TUI-only query syntax and Unicode-safe match ranges. CLI Host::matches is unchanged.
use crate::{
    runtime::{effective::EffectiveConfig, PingStatus},
    types::Host,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    Any,
    Tag,
    User,
    File,
    State,
}

#[derive(Debug, Default)]
pub(super) struct Query(Vec<(Field, String)>);

impl Query {
    pub(super) fn parse(input: &str) -> Result<Self, String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        let mut started = false;
        let mut had_quote = false;
        for c in input.chars() {
            if c == '"' {
                quoted = !quoted;
                had_quote = true;
                started = true;
            } else if c.is_whitespace() && !quoted {
                if started {
                    words.push((std::mem::take(&mut word), had_quote));
                }
                started = false;
                had_quote = false;
            } else {
                word.push(c);
                started = true;
            }
        }
        if quoted {
            return Err("Unclosed quote: add a closing double quote (\")".into());
        }
        if started {
            words.push((word, had_quote));
        }
        let mut terms = Vec::new();
        let mut conjunction = false;
        for (word, had_quote) in words {
            let word = word.to_lowercase();
            if !had_quote && matches!(word.as_str(), "and" | "et") {
                if terms.is_empty() || conjunction {
                    return Err(
                        "Misplaced conjunction AND/ET: put a search term on both sides".into(),
                    );
                }
                conjunction = true;
                continue;
            }
            conjunction = false;
            let (field, value) = match word.split_once(':') {
                Some(("tag", value)) => (Field::Tag, value),
                Some(("user", value)) => (Field::User, value),
                Some(("file", value)) => (Field::File, value),
                Some(("is", value)) => (Field::State, value),
                _ => (Field::Any, word.as_str()),
            };
            if value.is_empty() {
                return Err(
                    "Missing search value: enter text after the field or inside quotes".into(),
                );
            }
            if field == Field::State
                && !["online", "offline", "unknown", "connecting", "favorite"].contains(&value)
            {
                return Err(
                    "Invalid is: value; use online, offline, unknown, connecting or favorite"
                        .into(),
                );
            }
            terms.push((field, value.to_owned()));
        }
        if conjunction {
            return Err("Trailing conjunction AND/ET: add a search term after it".into());
        }
        Ok(Self(terms))
    }

    pub(super) fn matches(
        &self,
        host: &Host,
        effective: Option<&EffectiveConfig>,
        note: &str,
        favorite: bool,
        status: PingStatus,
    ) -> bool {
        let user = effective.map(|e| e.user.as_str()).unwrap_or(&host.user);
        let hostname = effective
            .map(|e| e.hostname.as_str())
            .unwrap_or(&host.hostname);
        self.0.iter().all(|(field, value)| {
            let contains = |text: &str| text.to_lowercase().contains(value);
            match field {
                Field::Any => {
                    [&host.name, hostname, user, note].into_iter().any(contains)
                        || host.tags.iter().any(|s| contains(s))
                }
                Field::Tag => host.tags.iter().any(|s| contains(s)),
                Field::User => contains(user),
                Field::File => contains(&host.source_file.to_string_lossy()),
                Field::State => match value.as_str() {
                    "favorite" => favorite,
                    "online" => status == PingStatus::Online,
                    "offline" => status == PingStatus::Offline,
                    "connecting" => status == PingStatus::Connecting,
                    _ => status == PingStatus::Unknown,
                },
            }
        })
    }

    /// Map folded byte matches back to complete original graphemes (İ, accents, emoji).
    pub(super) fn ranges(&self, text: &str, field: Field) -> Vec<Range<usize>> {
        // Fold the whole string so contextual lowercase (Greek final sigma) matches
        // the matcher. Context changes spelling, but not the grapheme byte lengths.
        let folded = text.to_lowercase();
        let mut offset = 0;
        let mapping: Vec<_> = text
            .grapheme_indices(true)
            .map(|(start, grapheme)| {
                let from = offset;
                offset += grapheme.to_lowercase().len();
                (from..offset, start..start + grapheme.len())
            })
            .collect();
        let mut ranges = Vec::new();
        for (_, needle) in self
            .0
            .iter()
            .filter(|(f, _)| *f != Field::State && (*f == Field::Any || *f == field))
        {
            // Inspect every character boundary so overlapping textual matches also highlight.
            for (start, _) in folded.char_indices() {
                if folded[start..].starts_with(needle) {
                    let end = start + needle.len();
                    let mut matched = mapping
                        .iter()
                        .filter(|(fold, _)| fold.start < end && fold.end > start);
                    if let Some((_, first)) = matched.next() {
                        let last = matched
                            .next_back()
                            .map(|(_, original)| original.end)
                            .unwrap_or(first.end);
                        ranges.push(first.start..last);
                    }
                }
            }
        }
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in ranges {
            if let Some(last) = merged.last_mut().filter(|last| last.end >= range.start) {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        merged
    }
}
