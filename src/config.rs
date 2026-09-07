//! SSH documents and safely backed-up mutations, adapted from SSHM (MIT).
//! The list follows SSHM's display rules rather than OpenSSH's effective lookup:
//! scalar directives use their last value, wildcards are omitted, Match bodies
//! are skipped, and each concrete alias produces a separate list entry.
use crate::types::Host;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

mod document;
mod files;
mod parser;
use document::Document;
use files::{Snapshot, WRITE_LOCK};
use parser::{quote, split_line, Parsed};

pub fn load_hosts(path: &Path) -> Result<Vec<Host>> {
    Ok(Parsed::load(path)?.hosts)
}

pub fn find_host(path: &Path, name: &str) -> Result<Host> {
    load_hosts(path)?
        .into_iter()
        .find(|h| h.name == name)
        .ok_or_else(|| anyhow::anyhow!("host '{name}' not found in any configuration file"))
}

/// Main file first, then included files in first-encounter, sorted-glob order.
pub fn config_files(path: &Path) -> Result<Vec<PathBuf>> {
    Ok(Parsed::load(path)?.files)
}

pub fn add_host(host: &Host, target: &Path, app_dir: &Path) -> Result<()> {
    safe_fields(host)?;
    let _lock = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("SSH configuration lock poisoned"))?;
    let snapshot = Snapshot::read(target)?;
    let mut document = Document::new(snapshot.text()?);
    check_add(&document, host)?;
    document.append(&document::serialize(host, document.newline));
    let staged = snapshot.stage(document.text().as_bytes())?;
    snapshot.backup(app_dir)?;
    snapshot.commit(staged)
}

/// Nonempty aliases edit the whole declaration; an empty list detaches this
/// alias into its own block. Pattern/negation entries are retained internally.
pub fn update_host(original: &Host, replacement: &Host, app_dir: &Path) -> Result<()> {
    safe_fields(replacement)?;
    let _lock = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("SSH configuration lock poisoned"))?;
    let snapshot = Snapshot::read(&original.source_file)?;
    let mut document = Document::new(snapshot.text()?);
    let block = document.locate(original)?;
    document.replace(&block, original, replacement)?;
    let output = document.text();
    if output.as_bytes() == snapshot.bytes.as_deref().unwrap_or_default() {
        return Ok(());
    }
    let staged = snapshot.stage(output.as_bytes())?;
    snapshot.backup(app_dir)?;
    snapshot.commit(staged)
}

pub fn delete_host(host: &Host, app_dir: &Path) -> Result<()> {
    let _lock = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("SSH configuration lock poisoned"))?;
    let snapshot = Snapshot::read(&host.source_file)?;
    let mut document = Document::new(snapshot.text()?);
    let block = document.locate(host)?;
    document.remove(&block, &host.name);
    let staged = snapshot.stage(document.text().as_bytes())?;
    snapshot.backup(app_dir)?;
    snapshot.commit(staged)
}

pub fn move_host(host: &Host, target: &Path, app_dir: &Path) -> Result<()> {
    move_with_hook(host, target, app_dir, || {})
}

fn move_with_hook(
    host: &Host,
    target: &Path,
    app_dir: &Path,
    before_source: impl FnOnce(),
) -> Result<()> {
    let _lock = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("SSH configuration lock poisoned"))?;
    let source = Snapshot::read(&host.source_file)?;
    let target = Snapshot::read(target)?;
    if source.path == target.path {
        bail!("host '{}' is already in the target config file", host.name);
    }
    let mut source_doc = Document::new(source.text()?);
    let mut target_doc = Document::new(target.text()?);
    if target_doc.contains(&host.name) {
        bail!("host '{}' already exists in target config", host.name);
    }
    let block = source_doc.locate(host)?;
    let raw = source_doc.extracted(&block, &host.name);
    source_doc.remove(&block, &host.name);
    target_doc.append(&raw);
    let source_text = source_doc.text();
    let target_text = target_doc.text();
    // Prepare both edits, recovery data and rollback before publishing either.
    let source_staged = source.stage(source_text.as_bytes())?;
    let target_staged = target.stage(target_text.as_bytes())?;
    let rollback = target
        .bytes
        .as_ref()
        .map(|bytes| target.stage(bytes))
        .transpose()?;
    source.backup(app_dir)?;
    target.backup(app_dir)?;
    source.unchanged()?;
    target.commit(target_staged)?;
    before_source();
    if let Err(error) = source.commit(source_staged) {
        return match target.restore(rollback, target_text.as_bytes()) {
            Ok(()) => Err(error).context("move failed; target restored and host retained in source"),
            Err(rollback_error) => Err(error).context(format!("move failed; rollback also failed: {rollback_error:#}; recovery copies remain in {}", app_dir.join("backups").display())),
        };
    }
    Ok(())
}

fn check_add(document: &Document, host: &Host) -> Result<()> {
    for name in std::iter::once(&host.name).chain(&host.aliases) {
        if document.contains(name) {
            bail!("host '{name}' already exists");
        }
    }
    Ok(())
}

// The standalone validator is used by forms. Document writes also handle legacy
// quoted names and unresolved/missing identity files that SSHM can already read.
fn safe_fields(host: &Host) -> Result<()> {
    if host.name.is_empty() {
        bail!("host name is required");
    }
    for value in [
        &host.name,
        &host.hostname,
        &host.user,
        &host.port,
        &host.identity,
        &host.proxy_jump,
        &host.proxy_command,
        &host.remote_command,
        &host.request_tty,
    ]
    .into_iter()
    .chain(&host.aliases)
    .chain(&host.extra_identities)
    .chain(&host.tags)
    {
        if value.contains(['\n', '\r', '\0']) {
            bail!("host fields cannot contain newline or NUL characters");
        }
    }
    if host.options.contains('\0') {
        bail!("SSH options cannot contain NUL characters");
    }
    Ok(())
}

pub fn validate_host(host: &Host) -> Result<()> {
    safe_fields(host)?;
    if host.name.trim().is_empty() {
        bail!("host name is required");
    }
    if host.name.len() > 50 || host.name.contains([' ', '\t', '\n', '\r', '#']) {
        bail!("invalid host name: cannot contain spaces or special characters");
    }
    if host.hostname.trim().is_empty() {
        bail!("hostname/IP is required");
    }
    if !valid_hostname(&host.hostname) && host.hostname.parse::<std::net::IpAddr>().is_err() {
        bail!("invalid hostname or IP address format");
    }
    if !host.port.is_empty() && !host.port.parse::<u16>().is_ok_and(|port| port > 0) {
        bail!("port must be between 1 and 65535");
    }
    for identity in std::iter::once(&host.identity)
        .chain(&host.extra_identities)
        .filter(|s| !s.is_empty())
    {
        if !valid_identity(identity) {
            bail!("identity file does not exist: {identity}");
        }
    }
    Ok(())
}
fn token(value: &str) -> bool {
    value
        .as_bytes()
        .windows(2)
        .any(|pair| pair[0] == b'%' && b"hprunCdiklLT".contains(&pair[1]))
}
fn valid_hostname(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 253
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains(' ')
    {
        return false;
    }
    if token(value) {
        return true;
    }
    value.split('.').all(|label| {
        let bytes = label.as_bytes();
        !bytes.is_empty()
            && bytes.len() <= 63
            && bytes[0].is_ascii_alphanumeric()
            && bytes[bytes.len() - 1].is_ascii_alphanumeric()
            && bytes
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || *c == b'-')
    })
}
fn valid_identity(value: &str) -> bool {
    if token(value) {
        return true;
    }
    let regex = regex::Regex::new(r"\$\{([^}]+)\}|\$([A-Za-z_][A-Za-z0-9_]*)")
        .expect("static environment pattern");
    let mut undefined = false;
    let expanded = regex.replace_all(value, |capture: &regex::Captures<'_>| {
        let name = capture
            .get(1)
            .or_else(|| capture.get(2))
            .expect("environment name")
            .as_str();
        match std::env::var(name) {
            Ok(value) => value,
            Err(_) => {
                undefined = true;
                capture[0].to_owned()
            }
        }
    });
    if undefined {
        return true;
    }
    let expanded = if expanded.starts_with("~/") {
        parser::expand_tilde(&expanded)
    } else {
        PathBuf::from(expanded.as_ref())
    };
    expanded.exists()
}

/// Convert form -o arguments to SSH directives, retaining repeated options.
pub fn options_to_config(options: &str) -> String {
    let options = options.trim();
    if options.is_empty() {
        return String::new();
    }
    let words = split_line(&format!("options {options}"))
        .map(|line| line.args)
        .unwrap_or_default();
    let flag = |word: &str| word == "-o" || (word.starts_with("-o") && word.contains('='));
    if !words.iter().any(|word| flag(word)) {
        // Preserve quoted arguments and internal whitespace; only normalize the
        // keyword separator (Go's old Fields normalization lost quoted spaces).
        return options
            .lines()
            .filter_map(split_line)
            .map(|line| {
                format!(
                    "{} {}",
                    line.keyword,
                    if parser::verbatim(&line.keyword.to_ascii_lowercase()) {
                        line.raw
                    } else {
                        line.uncommented
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    let mut result: Vec<String> = vec![];
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        let option = if word == "-o" && i + 1 < words.len() {
            i += 1;
            words[i].clone()
        } else if flag(word) && word != "-o" {
            word[2..].to_owned()
        } else if let Some(prior) = result.last_mut() {
            prior.push(' ');
            prior.push_str(word);
            i += 1;
            continue;
        } else {
            word.clone()
        };
        result.push(if let Some((key, value)) = option.split_once('=') {
            format!("{key} {value}")
        } else {
            option
        });
        i += 1;
    }
    result.join("\n")
}

pub fn options_to_command(options: &str) -> String {
    let trimmed = options.trim();
    if trimmed.starts_with("-o ") {
        return trimmed.to_owned();
    }
    trimmed
        .lines()
        .filter_map(|text| {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let option = text.split_once([' ', '\t']).map_or_else(
                || text.to_owned(),
                |(key, value)| format!("{key}={}", value.trim_start_matches([' ', '\t'])),
            );
            Some(format!("-o {}", quote(&option)))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn fixture(text: &str) -> (TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        let app = dir.path().join("app");
        fs::write(&path, text).unwrap();
        (dir, path, app)
    }
    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn parser_keeps_aliases_tags_negations_match_and_repeated_options() {
        let (_dir, path, _) = fixture("# Tags: prod, hidden\nHost=web alias !old web*\n HostName first\n HostName=final.example\n Port 2200\n IdentityFile '/a b'\n IdentityFile C:\\Users\\me\\id\n ProxyJump \"jump\" # note\n ProxyCommand ssh -W \"%h:%p\" jump # literal\n RemoteCommand echo '#hello'\n SetEnv FOO=\"a  b\"\n SetEnv BAR=b\nMatch host web\n User skipped\nHost next\n User actual\n");
        let hosts = load_hosts(&path).unwrap();
        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["web", "alias", "next"]
        );
        let h = &hosts[0];
        assert_eq!(h.hostname, "final.example");
        assert_eq!(h.aliases, ["web", "alias"]);
        assert_eq!(h.tags, ["prod", "hidden"]);
        assert_eq!(h.identity, "/a b");
        assert_eq!(h.extra_identities, [r"C:\Users\me\id"]);
        assert_eq!(h.proxy_jump, "\"jump\"");
        assert_eq!(h.proxy_command, "ssh -W \"%h:%p\" jump # literal");
        assert_eq!(h.remote_command, "echo '#hello'");
        assert_eq!(h.options, "SetEnv FOO=\"a  b\"\nSetEnv BAR=b");
        assert_eq!(h.user, "");
        assert_eq!(h.line_number, 2);
        assert_eq!(hosts[2].port, "22");
    }

    #[test]
    fn includes_relative_globs_cycles_and_non_config_exclusion() {
        let (dir, path, _) =
            fixture("Include \"parts/*.conf\" parts/nested\nHost root\n HostName root.example\n");
        let parts = dir.path().join("parts");
        fs::create_dir(&parts).unwrap();
        fs::write(
            parts.join("a.conf"),
            "Include ../config\nHost a\n HostName a.example\n",
        )
        .unwrap();
        fs::write(parts.join("b.conf"), "Host b\n HostName b.example\n").unwrap();
        fs::write(
            parts.join("nested"),
            "Include *\nHost nested\n HostName nested.example\n",
        )
        .unwrap();
        fs::write(parts.join("README.md"), "Host excluded\n").unwrap();
        fs::write(parts.join("bad.backup"), "Host excluded\n").unwrap();
        let hosts = load_hosts(&path).unwrap();
        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "nested", "root"]
        );
        let files = config_files(&path).unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0], path);
        assert_eq!(files[1], parts.join("a.conf"));
    }

    #[test]
    fn options_roundtrip_and_typed_forms() {
        for options in [
            "SetEnv TERM=xterm-256color",
            "SendEnv LANG LC_*",
            "LocalCommand echo \"a  b\"",
            "SetEnv FOO=\"a  b\"\nSendEnv LANG LC_*\nSetEnv BAR=x=y",
            "CertificateFile C:\\My Drive\\key",
        ] {
            assert_eq!(options_to_config(&options_to_command(options)), options);
        }
        for (typed, want) in [
            (
                "-o Compression=yes -o ServerAliveInterval=60",
                "Compression yes\nServerAliveInterval 60",
            ),
            ("-oServerAliveInterval=30", "ServerAliveInterval 30"),
            ("LocalCommand tool -opt", "LocalCommand tool -opt"),
            (
                "Compression yes -o ForwardX11=yes",
                "Compression yes\nForwardX11 yes",
            ),
            ("-o LocalCommand=tool -opt", "LocalCommand tool -opt"),
        ] {
            assert_eq!(options_to_config(typed), want);
        }
    }

    #[test]
    fn updates_exact_duplicate_and_keeps_unrelated_bytes_and_match() {
        let before = "# top\nhost dup\n HostName first.example\n\n# Tags: prod\nHOST=dup\n # local comment\n HostName second.example # trailing\n IdentityFile '/a b'\n IdentityFile /second\n SetEnv FOO=\"a  b\"\n SetEnv BAR=b\n\nMatch Host dup exec \"true\"\n  User conditional\n\n# last comment\nHost tail\n HostName tail.example\n";
        let (_dir, path, app) = fixture(before);
        let original = load_hosts(&path).unwrap()[1].clone();
        let mut replacement = original.clone();
        replacement.hostname = "updated.example".into();
        update_host(&original, &replacement, &app).unwrap();
        let after = read(&path);
        assert!(after.starts_with("# top\nhost dup\n HostName first.example\n"));
        assert!(after.contains("# local comment"));
        assert!(after.contains("IdentityFile '/a b'\n IdentityFile /second"));
        assert!(after.contains(" SetEnv FOO=\"a  b\"\n SetEnv BAR=b\n\nMatch Host dup exec \"true\"\n  User conditional\n\n# last comment\nHost tail\n HostName tail.example\n"));
        let hosts = load_hosts(&path).unwrap();
        assert_eq!(hosts[0].hostname, "first.example");
        assert_eq!(hosts[1].hostname, "updated.example");
        assert_eq!(read(&app.join("backups/config.backup")), before);
        delete_host(&hosts[1], &app).unwrap();
        assert_eq!(
            load_hosts(&path)
                .unwrap()
                .iter()
                .filter(|h| h.name == "dup")
                .count(),
            1
        );
        assert!(read(&path).contains("Match Host dup exec \"true\"\n  User conditional"));
    }

    #[test]
    fn edits_shared_aliases_or_detaches_one_without_losing_patterns() {
        let (_dir, path, app) =
            fixture("# Tags: t\nHost a b !old *legacy\n HostName shared.example\n User shared\n");
        let original = find_host(&path, "a").unwrap();
        let mut replacement = original.clone();
        replacement.hostname = "new.example".into();
        update_host(&original, &replacement, &app).unwrap();
        assert_eq!(find_host(&path, "b").unwrap().hostname, "new.example");
        assert!(read(&path).contains("Host a b !old *legacy"));
        let original = find_host(&path, "a").unwrap();
        replacement = original.clone();
        replacement.aliases.clear();
        replacement.hostname = "detached.example".into();
        update_host(&original, &replacement, &app).unwrap();
        assert_eq!(find_host(&path, "a").unwrap().hostname, "detached.example");
        assert_eq!(find_host(&path, "b").unwrap().hostname, "new.example");
        assert!(read(&path).contains("Host b !old *legacy"));
        let b = find_host(&path, "b").unwrap();
        delete_host(&b, &app).unwrap();
        assert!(find_host(&path, "b").is_err());
        assert!(read(&path).contains("Host !old *legacy"));
    }

    #[test]
    fn adding_and_moving_preserves_keys_quotes_and_source_match() {
        let (dir, source, app) = fixture("# Tags: t\nHost h !old\n HostName h.example\n IdentityFile \"/a  b\"\n IdentityFile C:\\Users\\me\\key\n ProxyCommand ssh -W \"%h:%p\" jump # literal\n SetEnv FOO=\"a  b\"\nMatch host h\n User conditional\n");
        let target = dir.path().join("other");
        let host = find_host(&source, "h").unwrap();
        move_host(&host, &target, &app).unwrap();
        assert!(find_host(&source, "h").is_err());
        assert_eq!(read(&source), "Match host h\n User conditional\n");
        let moved = find_host(&target, "h").unwrap();
        assert_eq!(moved.extra_identities, host.extra_identities);
        assert_eq!(moved.identity, host.identity);
        assert_eq!(moved.options, host.options);
        assert!(read(&target).contains("Host h !old"));
        assert!(!read(&target).contains("Match"));
        let h2 = Host {
            name: "quoted host".into(),
            hostname: "h.example".into(),
            user: "john doe".into(),
            identity: "C:\\My Drive\\".into(),
            ..Host::default()
        };
        add_host(&h2, &target, &app).unwrap();
        let parsed = find_host(&target, "quoted host").unwrap();
        assert_eq!(parsed.user, h2.user);
        assert_eq!(parsed.identity, h2.identity);
        assert!(add_host(&h2, &target, &app).is_err());
    }

    #[test]
    fn failed_move_leaves_source_and_target_unchanged() {
        let (dir, source, app) = fixture("Host h\n HostName h.example\n");
        let before = read(&source);
        let target = dir.path().join("directory");
        fs::create_dir(&target).unwrap();
        assert!(move_host(&find_host(&source, "h").unwrap(), &target, &app).is_err());
        assert_eq!(read(&source), before);
        assert!(target.is_dir());
        let other = dir.path().join("other");
        fs::write(&other, "Host h\n HostName conflict.example\n").unwrap();
        assert!(move_host(&find_host(&source, "h").unwrap(), &other, &app).is_err());
        assert_eq!(read(&source), before);
        assert_eq!(read(&other), "Host h\n HostName conflict.example\n");
    }

    #[test]
    fn validation_matches_reference_hostname_port_and_identity_rules() {
        let (_, _, _) = fixture("");
        let mut h = Host {
            name: "h".into(),
            hostname: "db.example".into(),
            ..Host::default()
        };
        validate_host(&h).unwrap();
        for hostname in ["::1", "127.0.0.1", "%h.internal"] {
            h.hostname = hostname.into();
            validate_host(&h).unwrap();
        }
        h.hostname = "db.example".into();
        h.port = "65536".into();
        assert!(validate_host(&h).is_err());
        h.port.clear();
        h.identity = "/missing-key-ssho-test".into();
        assert!(validate_host(&h).is_err());
        h.identity = "%d/.ssh/id".into();
        validate_host(&h).unwrap();
        h.name = "bad host".into();
        assert!(validate_host(&h).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn writes_preserve_permissions_and_reject_redirected_backups() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let (dir, path, app) = fixture("Host h\n HostName h.example\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let h = find_host(&path, "h").unwrap();
        let mut replacement = h.clone();
        replacement.hostname = "changed.example".into();
        update_host(&h, &replacement, &app).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(
            fs::metadata(app.join("backups/config.backup"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let legacy = dir.path().join("legacy");
        fs::create_dir(&legacy).unwrap();
        fs::remove_dir_all(app.join("backups")).unwrap();
        symlink(&legacy, app.join("backups")).unwrap();
        let before = read(&path);
        assert!(delete_host(&find_host(&path, "h").unwrap(), &app).is_err());
        assert_eq!(read(&path), before);
        assert_eq!(fs::read_dir(&legacy).unwrap().count(), 0);
    }

    #[test]
    fn tokenizer_follows_openssh_quoting_escaping_and_leniency() {
        for (line, expected) in [
            (r#"IdentityFile /a/id"#, "/a/id"),
            (r#"IdentityFile "/a  b/id""#, "/a  b/id"),
            (r#"IdentityFile '/a b/id'"#, "/a b/id"),
            (r#"IdentityFile /a\ b/id"#, "/a b/id"),
            (r#"IdentityFile C:\Users\me\id"#, r"C:\Users\me\id"),
            (r#"IdentityFile "C:\Users\me\id""#, r"C:\Users\me\id"),
            (r#"IdentityFile C:\\x\\id"#, r"C:\x\id"),
            (r#"IdentityFile "C:\\x\\id""#, r"C:\x\id"),
            (r#"IdentityFile "/a\"b""#, "/a\"b"),
            (r#"IdentityFile '/a\ b'"#, r"/a\ b"),
            (r#"IdentityFile '/a\\b'"#, r"/a\b"),
            (r#"IdentityFile '/a\'b'"#, "/a'b"),
            (r#"IdentityFile "/a\ b""#, r"/a\ b"),
            (r#"IdentityFile "/a"b"#, "/ab"),
            ("IdentityFile \"/a\tb\"", "/a\tb"),
            (r#"IdentityFile /a/id # old key"#, "/a/id"),
            (r#"IdentityFile /a/id#x"#, "/a/id#x"),
            (r#"IdentityFile "/a #b""#, "/a #b"),
            (r#"IdentityFile=/a/id"#, "/a/id"),
            (r#"IdentityFile = /a/id"#, "/a/id"),
            ("\tIdentityFile\t/a/id", "/a/id"),
            (r#"IdentityFile "/a b"#, "/a b"),
        ] {
            assert_eq!(split_line(line).unwrap().args, [expected], "{line}");
        }
        let parsed = split_line(r#"ProxyCommand ssh -W "%h:%p" bastion # c"#).unwrap();
        assert_eq!(parsed.raw, r#"ssh -W "%h:%p" bastion # c"#);
        assert_eq!(parsed.uncommented, r#"ssh -W "%h:%p" bastion"#);
        assert_eq!(
            split_line(r#"Host a "b c" 'd'"#).unwrap().args,
            ["a", "b c", "d"]
        );
        for line in ["", "  ", "\t", "# comment", " # Tags: prod"] {
            assert!(split_line(line).is_none());
        }
    }

    #[test]
    fn quote_writer_is_symmetric_with_parser_and_installed_ssh() {
        use std::process::Command;
        let ssh_available = Command::new("ssh").arg("-V").output().is_ok();
        let (dir, path, app) = fixture("");
        for value in [
            r"/a b/id",
            r"/a  b/id",
            "/a\tb/id",
            r#"/a"b/id"#,
            "/a'b/id",
            r"/a\b/id",
            r#"/a\\b\"c\'d\ e"#,
            "C:\\my dir\\",
            "#a/id",
            "=a/id",
            r"C:\Users\me\id",
            r"C:\My Drive\key",
        ] {
            fs::write(&path, "").unwrap();
            let host = Host {
                name: "h".into(),
                hostname: "h.example".into(),
                identity: value.into(),
                user: "john doe".into(),
                ..Host::default()
            };
            add_host(&host, &path, &app).unwrap();
            let parsed = find_host(&path, "h").unwrap();
            assert_eq!(parsed.identity, value);
            assert_eq!(parsed.user, "john doe");
            if ssh_available {
                let out = Command::new("ssh")
                    .args(["-F"])
                    .arg(&path)
                    .args(["-G", "h"])
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let stdout = String::from_utf8(out.stdout).unwrap();
                assert!(
                    stdout
                        .lines()
                        .any(|line| line == format!("identityfile {value}")),
                    "{value:?}: {stdout}"
                );
                assert!(stdout.lines().any(|line| line == "user john doe"));
            }
        }
        assert!(dir.path().exists());
    }

    #[test]
    fn failed_second_move_write_rolls_back_existing_and_new_targets() {
        for existing in [false, true] {
            let (dir, source, app) = fixture("Host h\n HostName h.example\n");
            let target = dir.path().join("target");
            let before_target = "# target comment\nHost other\n HostName other.example\n";
            if existing {
                fs::write(&target, before_target).unwrap();
            }
            let h = find_host(&source, "h").unwrap();
            let external = "Host h\n HostName externally-changed.example\n";
            let error = move_with_hook(&h, &target, &app, || fs::write(&source, external).unwrap())
                .unwrap_err();
            assert!(format!("{error:#}").contains("target restored"));
            assert_eq!(read(&source), external);
            if existing {
                assert_eq!(read(&target), before_target);
            } else {
                assert!(!target.exists());
            }
            assert_eq!(
                read(&app.join("backups/config.backup")),
                "Host h\n HostName h.example\n"
            );
        }
    }

    #[test]
    fn arbitrary_block_spellings_and_duplicate_line_selection_survive_edits() {
        for header in [
            "host foo",
            "Host\tfoo",
            "Host=foo",
            "Host \"foo\"",
            "HOST  foo # main",
        ] {
            for prefix in ["", "# Tags: t\n"] {
                let (_dir, path, app) = fixture(&format!("{prefix}{header}\n HostName first.example\n\n{prefix}{header}\n HostName second.example\n"));
                let original = load_hosts(&path).unwrap()[1].clone();
                let mut replacement = original.clone();
                replacement.hostname = "updated.example".into();
                update_host(&original, &replacement, &app).unwrap();
                let hosts = load_hosts(&path).unwrap();
                assert_eq!(hosts[0].hostname, "first.example");
                assert_eq!(hosts[1].hostname, "updated.example");
                delete_host(&hosts[1], &app).unwrap();
                let hosts = load_hosts(&path).unwrap();
                assert_eq!(hosts.len(), 1);
                assert_eq!(hosts[0].hostname, "first.example");
            }
        }
    }

    #[test]
    fn editing_alias_names_tags_keys_and_repeated_options() {
        let (_dir, path, app) = fixture("# Tags: old\nHost a b !old *.legacy\n HostName old.example\n # key note\n IdentityFile old\n IdentityFile extra\n SetEnv FOO=x\n SetEnv BAR=y\n User first\n User last\n");
        let original = find_host(&path, "a").unwrap();
        let mut replacement = original.clone();
        replacement.name = "renamed".into();
        replacement.aliases = vec!["renamed".into(), "c".into()];
        replacement.tags = vec!["new".into(), "work".into()];
        replacement.identity = "main key".into();
        replacement.extra_identities = vec!["extra key".into(), "third".into()];
        replacement.options = "SetEnv A=\"a  b\"\nSetEnv B=x=y".into();
        replacement.user.clear();
        update_host(&original, &replacement, &app).unwrap();
        let text = read(&path);
        assert!(text.contains("Host renamed c !old *.legacy"));
        assert!(text.contains("# key note"));
        assert!(!text.contains("# Tags: old"));
        assert!(!text.contains("User "));
        let hosts = load_hosts(&path).unwrap();
        assert_eq!(hosts.len(), 2);
        for h in hosts {
            assert_eq!(h.identity, "main key");
            assert_eq!(h.extra_identities, ["extra key", "third"]);
            assert_eq!(h.tags, ["new", "work"]);
            assert_eq!(h.options, replacement.options);
            assert_eq!(h.user, "");
        }
    }

    #[test]
    fn no_op_save_preserves_comments_order_spacing_and_crlf_bytes() {
        let before = "# top\r\n# Tags: t\r\nHOST=a b !x\r\n\tHostName = example.org # inline\r\n\r\n  User = me\r\n\tSetEnv FOO=\"a  b\"\r\nMatch host a\r\n User conditional\r\n";
        let (_dir, path, app) = fixture(before);
        let original = find_host(&path, "a").unwrap();
        update_host(&original, &original, &app).unwrap();
        assert_eq!(read(&path), before);
        let mut replacement = original.clone();
        replacement.hostname = "new.example".into();
        update_host(&original, &replacement, &app).unwrap();
        let after = read(&path);
        assert!(after.contains("\r\n    HostName new.example # inline\r\n\r\n  User = me\r\n\tSetEnv FOO=\"a  b\"\r\nMatch host a\r\n User conditional\r\n"));
    }

    #[cfg(unix)]
    #[test]
    fn include_symlink_cycle_and_backup_root_redirects_are_safe() {
        use std::os::unix::fs::symlink;
        let (dir, path, app) = fixture("Include link\nHost h\n HostName h.example\n");
        symlink(&path, dir.path().join("link")).unwrap();
        assert_eq!(load_hosts(&path).unwrap().len(), 1);
        let legacy = dir.path().join("legacy");
        fs::create_dir(&legacy).unwrap();
        symlink(&legacy, &app).unwrap();
        assert!(delete_host(&find_host(&path, "h").unwrap(), &app).is_err());
        assert_eq!(fs::read_dir(&legacy).unwrap().count(), 0);
        assert!(find_host(&path, "h").is_ok());
    }

    #[test]
    fn editing_values_and_aliases_keeps_inline_comments() {
        let (_dir, path, app) = fixture("Host a b !old # aliases\n HostName old.example  # endpoint\n User first # first user\n User last # last user\n SetEnv FOO=x # env note\n");
        let original = find_host(&path, "a").unwrap();
        let mut replacement = original.clone();
        replacement.aliases = vec!["a".into(), "c".into()];
        replacement.hostname = "new.example".into();
        replacement.user.clear();
        replacement.options = "SetEnv BAR=y".into();
        update_host(&original, &replacement, &app).unwrap();
        let text = read(&path);
        for comment in [
            "# aliases",
            "# endpoint",
            "# first user",
            "# last user",
            "# env note",
        ] {
            assert!(text.contains(comment), "lost {comment}: {text}");
        }
        let host = find_host(&path, "c").unwrap();
        assert_eq!(host.hostname, "new.example");
        assert_eq!(host.user, "");
        assert_eq!(host.options, "SetEnv BAR=y");
        delete_host(&find_host(&path, "a").unwrap(), &app).unwrap();
        assert!(read(&path).contains("# aliases"));
    }
}
