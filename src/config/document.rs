use super::parser::{concrete, quote, split_line, verbatim};
use crate::types::Host;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, HashSet};

pub(super) struct Document {
    pub lines: Vec<String>,
    pub newline: &'static str,
}
#[derive(Clone)]
pub(super) struct Block {
    pub start: usize,
    pub header: usize,
    pub end: usize,
    pub patterns: Vec<String>,
}
impl Document {
    pub fn new(text: &str) -> Self {
        Self {
            lines: text.split_inclusive('\n').map(str::to_owned).collect(),
            newline: if text.contains("\r\n") { "\r\n" } else { "\n" },
        }
    }
    pub fn text(&self) -> String {
        self.lines.concat()
    }
    pub fn locate(&self, host: &Host) -> Result<Block> {
        for (header, text) in self.lines.iter().enumerate() {
            if host.line_number != 0 && host.line_number != header + 1 {
                continue;
            }
            let Some(line) = split_line(text) else {
                continue;
            };
            if !line.keyword.eq_ignore_ascii_case("host")
                || !line
                    .args
                    .iter()
                    .any(|name| name == &host.name && concrete(name))
            {
                continue;
            }
            let mut start = header;
            let mut i = header;
            while i > 0 {
                let prior = self.lines[i - 1].trim();
                if prior.is_empty() || prior.starts_with('#') {
                    if prior.starts_with("# Tags:") {
                        start = i - 1;
                    }
                    i -= 1;
                } else {
                    break;
                }
            }
            let mut end = self
                .lines
                .iter()
                .enumerate()
                .skip(header + 1)
                .find(|(_, text)| {
                    split_line(text).is_some_and(|line| {
                        line.keyword.eq_ignore_ascii_case("host")
                            || line.keyword.eq_ignore_ascii_case("match")
                    })
                })
                .map_or(self.lines.len(), |(i, _)| i);
            // Comments before the next block belong to the untouched suffix.
            while end > header + 1
                && (self.lines[end - 1].trim().is_empty()
                    || self.lines[end - 1].trim().starts_with('#'))
            {
                end -= 1;
            }
            return Ok(Block {
                start,
                header,
                end,
                patterns: line.args,
            });
        }
        bail!(
            "host '{}' not found at {}:{}; reload the configuration",
            host.name,
            host.source_file.display(),
            host.line_number
        )
    }
    pub fn contains(&self, name: &str) -> bool {
        self.lines.iter().any(|text| {
            split_line(text).is_some_and(|line| {
                line.keyword.eq_ignore_ascii_case("host")
                    && line
                        .args
                        .iter()
                        .any(|pattern| concrete(pattern) && pattern == name)
            })
        })
    }
    pub fn append(&mut self, text: &str) {
        if let Some(last) = self.lines.last_mut() {
            if !last.ends_with('\n') {
                last.push_str(self.newline);
            }
            if !last.trim().is_empty() {
                self.lines.push(self.newline.into());
            }
        }
        self.lines.extend(Document::new(text).lines);
    }

    /// Insert at a real scope boundary, retaining the original boundary's comment block.
    /// Global-prefix directives remain ahead of the addition; OpenSSH preflight decides
    /// whether they permit the requested target values.
    pub fn insert_before_first_block(&mut self, text: &str) -> bool {
        let Some(mut start) = self.lines.iter().position(|text| {
            split_line(text).is_some_and(|line| {
                line.keyword.eq_ignore_ascii_case("host")
                    || line.keyword.eq_ignore_ascii_case("match")
            })
        }) else {
            self.append(text);
            return false;
        };
        while start > 0
            && (self.lines[start - 1].trim().is_empty()
                || self.lines[start - 1].trim().starts_with('#'))
        {
            start -= 1;
        }
        let mut added = Document::new(text).lines;
        added.push(self.newline.into());
        self.lines.splice(start..start, added);
        true
    }

    /// Remove one concrete name; aliases and pattern-only declarations survive.
    pub fn remove(&mut self, block: &Block, name: &str) {
        let remaining: Vec<_> = block
            .patterns
            .iter()
            .filter(|pattern| pattern.as_str() != name)
            .cloned()
            .collect();
        if remaining.iter().all(|pattern| pattern.starts_with('!')) {
            // Includes remain in their source document, even when inside this block.
            let keep: Vec<_> = self.lines[block.start..block.end]
                .iter()
                .filter(|text| {
                    split_line(text)
                        .is_some_and(|line| line.keyword.eq_ignore_ascii_case("include"))
                })
                .cloned()
                .collect();
            self.lines.splice(block.start..block.end, keep);
        } else {
            self.lines[block.header] =
                header_preserving_comment(&remaining, &self.lines[block.header], self.newline);
        }
    }

    pub fn extracted(&self, block: &Block, name: &str) -> String {
        let mut lines = self.lines[block.start..block.end].to_vec();
        let patterns = if block.patterns.iter().filter(|p| concrete(p)).count() > 1 {
            block
                .patterns
                .iter()
                .filter(|pattern| !concrete(pattern) || pattern.as_str() == name)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            block.patterns.clone()
        };
        if patterns != block.patterns {
            lines[block.header - block.start] = header_preserving_comment(
                &patterns,
                &lines[block.header - block.start],
                self.newline,
            );
        }
        // Includes refer to their source directory and must never migrate.
        lines.retain(|text| {
            !split_line(text).is_some_and(|line| line.keyword.eq_ignore_ascii_case("include"))
        });
        let mut result = lines.concat();
        if !result.ends_with('\n') {
            result.push_str(self.newline);
        }
        result
    }

    pub fn replace(&mut self, block: &Block, original: &Host, replacement: &Host) -> Result<()> {
        let detach = replacement.aliases.is_empty()
            && block.patterns.iter().filter(|p| concrete(p)).count() > 1;
        if detach {
            let raw = self.extracted(block, &original.name);
            let mut detached = Document::new(&raw);
            let mut selected = original.clone();
            selected.line_number = 0;
            let detached_block = detached.locate(&selected)?;
            detached.patch(&detached_block, original, replacement);
            self.remove(block, &original.name);
            self.append(&detached.text());
        } else {
            self.patch(block, original, replacement);
        }
        Ok(())
    }

    fn patch(&mut self, block: &Block, original: &Host, replacement: &Host) {
        let mut names = if replacement.aliases.is_empty() {
            vec![replacement.name.clone()]
        } else {
            replacement.aliases.clone()
        };
        // An explicitly edited alias list is authoritative. A form that only
        // changes the selected name leaves the original alias list untouched.
        if replacement.aliases == original.aliases {
            for name in &mut names {
                if name == &original.name {
                    *name = replacement.name.clone();
                }
            }
        }
        let mut seen = HashSet::new();
        names.retain(|name| seen.insert(name.clone()));
        let concrete_names: Vec<_> = block
            .patterns
            .iter()
            .filter(|p| concrete(p))
            .cloned()
            .collect();
        let patterns = if names == concrete_names {
            block.patterns.clone()
        } else {
            names
                .into_iter()
                .chain(block.patterns.iter().filter(|p| !concrete(p)).cloned())
                .collect()
        };
        let mut changes: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (key, before, after, quoted) in [
            ("hostname", &original.hostname, &replacement.hostname, true),
            ("user", &original.user, &replacement.user, true),
            ("port", &original.port, &replacement.port, false),
            (
                "proxyjump",
                &original.proxy_jump,
                &replacement.proxy_jump,
                false,
            ),
            (
                "proxycommand",
                &original.proxy_command,
                &replacement.proxy_command,
                false,
            ),
            (
                "remotecommand",
                &original.remote_command,
                &replacement.remote_command,
                false,
            ),
            (
                "requesttty",
                &original.request_tty,
                &replacement.request_tty,
                false,
            ),
        ] {
            if before != after {
                changes.insert(
                    key,
                    if after.is_empty() {
                        vec![]
                    } else {
                        vec![format!(
                            "    {}{}{}{}",
                            display_key(key),
                            if key == "proxycommand" { "=" } else { " " },
                            if quoted { quote(after) } else { after.clone() },
                            self.newline
                        )]
                    },
                );
            }
        }
        if original.identity != replacement.identity
            || original.extra_identities != replacement.extra_identities
        {
            changes.insert(
                "identityfile",
                std::iter::once(&replacement.identity)
                    .chain(&replacement.extra_identities)
                    .filter(|s| !s.is_empty())
                    .map(|identity| format!("    IdentityFile {}{}", quote(identity), self.newline))
                    .collect(),
            );
        }
        let options_changed = original.options != replacement.options;
        let mut written = HashSet::new();
        let mut rewritten = Vec::new();
        if original.tags != replacement.tags && !replacement.tags.is_empty() {
            rewritten.push(format!(
                "# Tags: {}{}",
                replacement.tags.join(", "),
                self.newline
            ));
        }
        for text in &self.lines[block.start..block.header] {
            if original.tags != replacement.tags && text.trim().starts_with("# Tags:") {
                continue;
            }
            rewritten.push(text.clone());
        }
        rewritten.push(if patterns == block.patterns {
            self.lines[block.header].clone()
        } else {
            header_preserving_comment(&patterns, &self.lines[block.header], self.newline)
        });
        for text in &self.lines[block.header + 1..block.end] {
            let Some(line) = split_line(text) else {
                rewritten.push(text.clone());
                continue;
            };
            let key = line.keyword.to_ascii_lowercase();
            if let Some(values) = changes.get(key.as_str()) {
                let values = if written.insert(key.clone()) {
                    values.clone()
                } else {
                    vec![]
                };
                emit_changed(&mut rewritten, values, text, self.newline);
            } else if options_changed && !known(&key) {
                let values = if written.insert("options".into()) {
                    option_lines(&replacement.options, self.newline)
                } else {
                    vec![]
                };
                emit_changed(&mut rewritten, values, text, self.newline);
            } else {
                rewritten.push(text.clone());
            }
        }
        for (key, lines) in changes {
            if !written.contains(key) {
                rewritten.extend(lines);
            }
        }
        if options_changed && !written.contains("options") {
            rewritten.extend(option_lines(&replacement.options, self.newline));
        }
        // A final original line without a newline must not merge with inserted lines.
        for i in 0..rewritten.len().saturating_sub(1) {
            if !rewritten[i].ends_with('\n') {
                rewritten[i].push_str(self.newline);
            }
        }
        self.lines.splice(block.start..block.end, rewritten);
    }
}

fn inline_comment(text: &str) -> Option<String> {
    let line = split_line(text)?;
    if verbatim(&line.keyword.to_ascii_lowercase()) || line.raw == line.uncommented {
        return None;
    }
    Some(line.raw[line.uncommented.len()..].to_owned())
}
fn header_preserving_comment(patterns: &[String], original: &str, newline: &str) -> String {
    let mut result = header(patterns, "");
    if let Some(comment) = inline_comment(original) {
        result.push_str(&comment);
    }
    result.push_str(newline);
    result
}
fn emit_changed(output: &mut Vec<String>, mut values: Vec<String>, original: &str, newline: &str) {
    if let Some(comment) = inline_comment(original) {
        if let Some(first) = values.first_mut() {
            let end = first.trim_end_matches(['\r', '\n']).len();
            first.insert_str(end, &comment);
        } else {
            output.push(format!("    {}{}", comment.trim_start(), newline));
        }
    }
    output.extend(values);
}

fn known(key: &str) -> bool {
    matches!(
        key,
        "hostname"
            | "user"
            | "port"
            | "identityfile"
            | "proxyjump"
            | "proxycommand"
            | "remotecommand"
            | "requesttty"
            | "host"
            | "match"
            | "include"
    )
}
fn display_key(key: &str) -> &str {
    match key {
        "hostname" => "HostName",
        "user" => "User",
        "port" => "Port",
        "proxyjump" => "ProxyJump",
        "proxycommand" => "ProxyCommand",
        "remotecommand" => "RemoteCommand",
        "requesttty" => "RequestTTY",
        _ => key,
    }
}
fn option_lines(options: &str, newline: &str) -> Vec<String> {
    options
        .lines()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| format!("    {text}{newline}"))
        .collect()
}
fn header(patterns: &[String], newline: &str) -> String {
    format!(
        "Host {}{}",
        patterns
            .iter()
            .map(|name| quote(name))
            .collect::<Vec<_>>()
            .join(" "),
        newline
    )
}

pub(super) fn serialize(host: &Host, newline: &str) -> String {
    serialize_with_port(host, newline, false)
}

pub(super) fn serialize_copy(host: &Host, newline: &str) -> String {
    serialize_with_port(host, newline, true)
}

fn serialize_with_port(host: &Host, newline: &str, explicit_default_port: bool) -> String {
    let mut lines = Vec::new();
    if !host.tags.is_empty() {
        lines.push(format!("# Tags: {}{}", host.tags.join(", "), newline));
    }
    let names = if host.aliases.is_empty() {
        vec![host.name.clone()]
    } else {
        host.aliases.clone()
    };
    lines.push(header(&names, newline));
    lines.push(format!("    HostName {}{}", quote(&host.hostname), newline));
    if !host.user.is_empty() {
        lines.push(format!("    User {}{}", quote(&host.user), newline));
    }
    if !host.port.is_empty() && (host.port != "22" || explicit_default_port) {
        lines.push(format!("    Port {}{}", host.port, newline));
    }
    for identity in std::iter::once(&host.identity)
        .chain(&host.extra_identities)
        .filter(|s| !s.is_empty())
    {
        lines.push(format!("    IdentityFile {}{}", quote(identity), newline));
    }
    for (key, value, separator) in [
        ("ProxyJump", &host.proxy_jump, " "),
        ("ProxyCommand", &host.proxy_command, "="),
        ("RemoteCommand", &host.remote_command, " "),
        ("RequestTTY", &host.request_tty, " "),
    ] {
        if !value.is_empty() {
            lines.push(format!("    {key}{separator}{value}{newline}"));
        }
    }
    lines.extend(option_lines(&host.options, newline));
    lines.concat()
}
