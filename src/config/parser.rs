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

pub(super) struct Parsed {
    pub hosts: Vec<Host>,
    pub files: Vec<PathBuf>,
    seen: HashSet<PathBuf>,
}
impl Parsed {
    pub fn load(path: &Path) -> Result<Self> {
        let mut parsed = Self {
            hosts: vec![],
            files: vec![],
            seen: HashSet::new(),
        };
        parsed.visit(path)?;
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

    fn visit(&mut self, path: &Path) -> Result<()> {
        let path = absolute(path)?;
        let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !self.seen.insert(identity) {
            return Ok(());
        }
        self.files.push(path.clone());
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(e).with_context(|| format!("reading SSH config {}", path.display()))
            }
        };
        let mut current = None;
        let mut pending_tags = Vec::new();
        for (index, text) in content.lines().enumerate() {
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
                        if let Ok(paths) = glob::glob(&pattern.to_string_lossy()) {
                            let mut paths: Vec<_> = paths.filter_map(Result::ok).collect();
                            paths.sort();
                            for included in paths {
                                if included.is_dir() || excluded(&included) {
                                    continue;
                                }
                                // The Go reader deliberately ignores errors in included files.
                                let _ = self.visit(&included);
                            }
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
