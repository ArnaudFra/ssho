use crate::types::Host;
use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub(super) struct Line {
    pub keyword: String,
    pub args: Vec<String>,
    pub raw: String,
    pub uncommented: String,
}

/// OpenSSH's argv_split rules, including literal backslashes in Windows paths.
pub(super) fn split_line(line: &str) -> Option<Line> {
    let s = line.trim_matches([' ', '\t', '\r', '\n']);
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    let end = s.find([' ', '\t', '=']).unwrap_or(s.len());
    let mut rest = s[end..].trim_start_matches([' ', '\t']);
    if let Some(after) = rest.strip_prefix('=') {
        rest = after.trim_start_matches([' ', '\t']);
    }
    let mut result = Line {
        keyword: s[..end].into(),
        args: vec![],
        raw: rest.into(),
        uncommented: rest.into(),
    };
    let chars: Vec<(usize, char)> = rest.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && matches!(chars[i].1, ' ' | '\t' | '\n' | '\r') {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        if chars[i].1 == '#' {
            result.uncommented = rest[..chars[i].0].trim_end_matches([' ', '\t']).into();
            break;
        }
        let mut arg = String::new();
        let mut quote = None;
        while i < chars.len() {
            let c = chars[i].1;
            if c == '\\' && i + 1 < chars.len() {
                let next = chars[i + 1].1;
                if matches!(next, '\'' | '"' | '\\') || (quote.is_none() && next == ' ') {
                    arg.push(next);
                    i += 2;
                    continue;
                }
            }
            if quote.is_none() && matches!(c, ' ' | '\t' | '\n' | '\r') {
                break;
            }
            if quote.is_none() && matches!(c, '\'' | '"') {
                quote = Some(c);
                i += 1;
                continue;
            }
            if quote == Some(c) {
                quote = None;
                i += 1;
                continue;
            }
            arg.push(c);
            i += 1;
        }
        result.args.push(arg);
    }
    Some(result)
}

/// File-scoped metadata is a comment, never an OpenSSH directive.
pub(super) fn file_tags(text: &str) -> Option<Vec<String>> {
    let comment = text.trim().strip_prefix('#')?.trim_start();
    let (key, values) = comment.split_once(':')?;
    if !key.trim().eq_ignore_ascii_case("filetags") {
        return None;
    }
    Some(
        values
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

pub(super) fn closes_header(text: &str) -> bool {
    split_line(text).is_some_and(|line| {
        matches!(
            line.keyword.to_ascii_lowercase().as_str(),
            "host" | "include" | "match"
        )
    })
}

pub(super) fn verbatim(key: &str) -> bool {
    matches!(
        key,
        "proxycommand" | "remotecommand" | "localcommand" | "knownhostscommand"
    )
}

pub(super) fn concrete(name: &str) -> bool {
    !name.starts_with('!') && !name.contains(['*', '?'])
}

pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    let full = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut cleaned = PathBuf::new();
    for component in full.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            c => cleaned.push(c.as_os_str()),
        }
    }
    Ok(cleaned)
}

pub(super) fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub(super) fn expand_tilde(path: &str) -> PathBuf {
    if let Some(suffix) = path.strip_prefix('~') {
        if let Some(home) = home() {
            return home.join(suffix.trim_start_matches('/'));
        }
    }
    PathBuf::from(path)
}

/// Preserve glob matching while surfacing I/O failures on every traversed prefix.
/// The glob crate reports wildcard read_dir failures, but drops metadata errors on
/// literal components. Checking literal children of each matched prefix covers both.
fn checked_include_paths(pattern: &Path) -> Result<Vec<PathBuf>> {
    let mut prefixes: Vec<_> = pattern.ancestors().collect();
    prefixes.reverse();
    let mut paths = Vec::<PathBuf>::new();
    for prefix in prefixes {
        for parent in &paths {
            // A wildcard can match a symlink whose target cannot be traversed.
            // glob silently treats that metadata failure as a non-directory.
            check_include_metadata(parent)?;
            if let Some(component) = prefix.components().next_back() {
                let name = component.as_os_str();
                if !name.to_string_lossy().contains(['*', '?', '[']) {
                    check_include_metadata(&parent.join(name))?;
                }
            }
        }
        // Keep the legacy treatment of invalid patterns as unmatched Includes.
        let Ok(matches) = glob::glob(&prefix.to_string_lossy()) else {
            return Ok(vec![]);
        };
        paths = matches
            .map(|entry| {
                entry.map_err(|error| {
                    let path = error.path().to_path_buf();
                    anyhow::Error::new(error.into_error())
                        .context(format!("expanding SSH Include {}", path.display()))
                })
            })
            .collect::<Result<_>>()?;
    }
    Ok(paths)
}

fn check_include_metadata(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        }
        Err(error) => {
            Err(error).with_context(|| format!("expanding SSH Include {}", path.display()))
        }
    }
}

pub(super) struct Parsed {
    pub hosts: Vec<Host>,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<super::ConfigWarning>,
    pub has_file_tags: bool,
    seen: HashSet<PathBuf>,
    strict: bool,
}
impl Parsed {
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_mode(path, false)
    }

    pub fn load_checked(path: &Path) -> Result<Self> {
        Self::load_mode(path, true)
    }

    fn load_mode(path: &Path, strict: bool) -> Result<Self> {
        let mut parsed = Self {
            hosts: vec![],
            files: vec![],
            warnings: vec![],
            has_file_tags: false,
            seen: HashSet::new(),
            strict,
        };
        parsed.visit(path, true)?;
        Ok(parsed)
    }

    fn append(&mut self, current: &mut Option<Host>) {
        if let Some(host) = current.take() {
            for name in &host.aliases {
                let mut alias = host.clone();
                alias.name = name.clone();
                self.hosts.push(alias);
            }
        }
    }

    fn visit(&mut self, path: &Path, required: bool) -> Result<()> {
        let path = absolute(path)?;
        let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !self.seen.insert(identity) {
            return Ok(());
        }
        self.files.push(path.clone());
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && (!self.strict || !required) => {
                return Ok(());
            }
            Err(e) => {
                return Err(e).with_context(|| format!("reading SSH config {}", path.display()))
            }
        };
        let mut current = None;
        let mut pending_tags = Vec::new();
        let mut inherited = Vec::new();
        let mut header = true;
        for (index, text) in content.lines().enumerate() {
            if let Some(tags) = file_tags(text) {
                self.has_file_tags = true;
                if header {
                    for tag in tags {
                        if !inherited.contains(&tag) {
                            inherited.push(tag);
                        }
                    }
                } else {
                    self.warnings.push(super::ConfigWarning {
                        path: path.clone(),
                        line: index + 1,
                        message: "FileTags ignored after Host, Include or Match".into(),
                    });
                }
                continue;
            }
            if closes_header(text) {
                header = false;
            }

            if let Some(tags) = text.trim().strip_prefix("# Tags:") {
                pending_tags.extend(
                    tags.split(',')
                        .map(str::trim)
                        .filter(|tag| !tag.is_empty())
                        .map(str::to_owned),
                );
                continue;
            }
            let Some(line) = split_line(text) else {
                continue;
            };
            if line.args.is_empty() {
                continue;
            }
            let key = line.keyword.to_ascii_lowercase();
            match key.as_str() {
                "include" => {
                    for pattern in &line.args {
                        let pattern = expand_tilde(pattern);
                        let pattern = if pattern.is_absolute() {
                            pattern
                        } else {
                            path.parent().unwrap_or(Path::new(".")).join(pattern)
                        };
                        let mut paths: Vec<_> = if self.strict {
                            checked_include_paths(&pattern)?
                        } else if let Ok(paths) = glob::glob(&pattern.to_string_lossy()) {
                            paths.filter_map(Result::ok).collect()
                        } else {
                            vec![]
                        };
                        paths.sort();
                        for included in paths {
                            if included.is_dir() || excluded(&included) {
                                continue;
                            }
                            let loaded = self.visit(&included, false);
                            if self.strict {
                                loaded?;
                            }
                            // The legacy Go-compatible reader ignores included-file errors.
                        }
                    }
                }
                "match" => {
                    self.append(&mut current);
                    pending_tags.clear();
                }
                "host" => {
                    self.append(&mut current);
                    let aliases: Vec<String> = line
                        .args
                        .into_iter()
                        .filter(|name| concrete(name))
                        .collect();
                    if let Some(name) = aliases.first() {
                        current = Some(Host {
                            name: name.clone(),
                            aliases,
                            port: "22".into(),
                            tags: std::mem::take(&mut pending_tags),
                            file_tags: inherited.clone(),
                            source_file: path.clone(),
                            line_number: index + 1,
                            ..Host::default()
                        });
                    } else {
                        pending_tags.clear();
                    }
                }
                _ => {
                    if let Some(host) = current.as_mut() {
                        let value = line.args.join(" ");
                        let raw = if verbatim(&key) {
                            line.raw
                        } else {
                            line.uncommented
                        };
                        match key.as_str() {
                            "hostname" => host.hostname = value,
                            "user" => host.user = value,
                            "port" => host.port = value,
                            "identityfile" => {
                                if host.identity.is_empty() {
                                    host.identity = value
                                } else {
                                    host.extra_identities.push(value)
                                }
                            }
                            "proxyjump" => host.proxy_jump = raw,
                            "proxycommand" => host.proxy_command = raw,
                            "remotecommand" => host.remote_command = raw,
                            "requesttty" => host.request_tty = value,
                            _ => {
                                if !host.options.is_empty() {
                                    host.options.push('\n');
                                }
                                host.options.push_str(&line.keyword);
                                host.options.push(' ');
                                host.options.push_str(&raw);
                            }
                        }
                    }
                }
            }
        }
        self.append(&mut current);
        Ok(())
    }
}

fn excluded(path: &Path) -> bool {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if filename.starts_with('.') || filename == "readme" {
        return true;
    }
    let extensions = [
        "backup", "txt", "md", "rst", "doc", "docx", "pdf", "log", "tmp", "bak", "old", "orig",
        "json", "xml", "yaml", "yml", "toml", "sh", "bash", "zsh", "fish", "ps1", "bat", "cmd",
        "py", "pl", "rb", "js", "php", "go", "c", "cpp", "jpg", "jpeg", "png", "gif", "bmp", "svg",
        "zip", "tar", "gz", "bz2", "xz",
    ];
    if path
        .extension()
        .is_some_and(|ext| extensions.contains(&ext.to_string_lossy().to_lowercase().as_str()))
    {
        return true;
    }
    use std::io::Read;
    let mut buffer = [0; 2048];
    if let Ok(mut file) = fs::File::open(path) {
        if let Ok(count) = file.read(&mut buffer) {
            let content = String::from_utf8_lossy(&buffer[..count]).to_lowercase();
            return [
                "<!doctype",
                "<html>",
                "<xml>",
                "<?xml",
                "#!/bin/",
                "#!/usr/bin/",
                "# readme",
                "# documentation",
                "# license",
                "package main",
                "function ",
                "class ",
                "def ",
                "import ",
                "require ",
                "#include",
            ]
            .iter()
            .any(|indicator| content.contains(indicator));
        }
    }
    false
}

pub(super) fn quote(value: &str) -> String {
    let chars: Vec<_> = value.chars().collect();
    let needs = value.contains([' ', '\t', '"', '\''])
        || value.starts_with(['#', '='])
        || value.contains("\\\\");
    if !needs {
        return value.into();
    }
    let mut result = String::from("\"");
    for (i, c) in chars.iter().enumerate() {
        if *c == '"'
            || (*c == '\\' && (i + 1 == chars.len() || matches!(chars[i + 1], '"' | '\'' | '\\')))
        {
            result.push('\\');
        }
        result.push(*c);
    }
    result.push('"');
    result
}
