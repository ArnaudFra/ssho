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
use files::{ConfigMirror, Snapshot, WRITE_LOCK};
use parser::{quote, split_line, Parsed};

pub fn load_hosts(path: &Path) -> Result<Vec<Host>> {
    Ok(Parsed::load(path)?.hosts)
}

/// Reload a complete browser snapshot, rejecting root/read/Include traversal errors.
/// Includes matching no files remain optional, for literal names as well as globs.
pub fn load_hosts_checked(path: &Path) -> Result<Vec<Host>> {
    Ok(Parsed::load_checked(path)?.hosts)
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
    publish_add(&snapshot, &document, app_dir)
}

fn publish_add(snapshot: &Snapshot, document: &Document, app_dir: &Path) -> Result<()> {
    let staged = snapshot.stage(document.text().as_bytes())?;
    snapshot.backup(app_dir)?;
    snapshot.commit(staged)
}

/// Add a copy at a scope boundary only after native OpenSSH confirms its requested
/// core connection target. Validation uses private mirrors, before backups/writes.
pub fn add_host_preserving_target(
    host: &Host,
    target: &Path,
    app_dir: &Path,
    root: &Path,
) -> Result<()> {
    safe_fields(host)?;
    let _lock = WRITE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("SSH configuration lock poisoned"))?;
    let mut mirror = ConfigMirror::new(root)?;
    anyhow::ensure!(
        !load_hosts_checked(mirror.root())?
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&host.name)),
        "host '{}' already exists in SSH configuration (including Includes and hidden hosts)",
        host.name
    );
    let snapshot = mirror.snapshot(target)?;
    let mut document = Document::new(snapshot.text()?);
    check_add(&document, host)?;
    let boundary =
        document.insert_before_first_block(&document::serialize_copy(host, document.newline));
    anyhow::ensure!(boundary || snapshot.path == std::fs::canonicalize(root)?, "destination Include has no Host/Match boundary; choose the root file or another destination and retry");
    let target = snapshot.path.clone();
    mirror.replace(&target, &document.text())?;
    let effective = crate::runtime::effective::resolve(mirror.root(), &host.name)
        .map_err(|error| anyhow::anyhow!("cannot preflight copy destination; choose another destination or fix SSH configuration and retry: {error:#}"))?;
    anyhow::ensure!(
        effective.hostname == host.hostname
            && (host.user.is_empty() || effective.user == host.user)
            && (host.port.is_empty() || effective.port == host.port),
        "destination precedence changes the requested copy target (resolved {}@{}:{}); choose an earlier destination, usually the root file, or adjust global/Include defaults and retry",
        effective.user, effective.hostname, effective.port
    );
    mirror.unchanged()?;
    publish_add(mirror.snapshot(&target)?, &document, app_dir)
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

#[cfg(test)]
mod checked_load_tests {
    use super::*;

    #[test]
    fn checked_load_rejects_read_errors_without_changing_legacy_loaders() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let include = temp.path().join("included");
        std::fs::write(
            &root,
            "Include included\nHost root-host\n HostName 127.0.0.1\n",
        )
        .unwrap();
        std::fs::write(&include, [0xff]).unwrap();
        assert_eq!(load_hosts(&root).unwrap()[0].name, "root-host");
        assert_eq!(
            config_files(&root).unwrap(),
            [root.clone(), include.clone()]
        );
        let error = load_hosts_checked(&root).unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::InvalidData
        );
        std::fs::remove_file(&include).unwrap();
        assert_eq!(load_hosts_checked(&root).unwrap()[0].name, "root-host");
        std::fs::remove_file(&root).unwrap();
        assert!(load_hosts(&root).unwrap().is_empty());
        assert_eq!(config_files(&root).unwrap(), std::slice::from_ref(&root));
        let error = load_hosts_checked(&root).unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn checked_load_shares_alias_recursive_include_cycle_and_exclusion_semantics() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let parts = temp.path().join("parts");
        std::fs::create_dir_all(parts.join("nested")).unwrap();
        std::fs::write(
            &root,
            "Include parts/**/* missing.conf absent/*\nHost root-host\n HostName 127.0.0.1\n",
        )
        .unwrap();
        std::fs::write(
            parts.join("nested/hosts.conf"),
            "Include ../../config\nHost first second\n HostName 127.0.0.2\n",
        )
        .unwrap();
        std::fs::write(
            parts.join("ignored.conf.bak"),
            "Host ignored\n HostName 127.0.0.3\n",
        )
        .unwrap();
        let hosts = load_hosts_checked(&root).unwrap();
        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["first", "second", "root-host"]
        );
        assert_eq!(hosts, load_hosts(&root).unwrap());
    }
}

#[cfg(test)]
mod copy_tests {
    use super::*;
    use std::fs;

    #[test]
    fn copy_preflight_preserves_nested_glob_match_and_missing_include_semantics() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let dir = temp.path().join("include folder");
        fs::create_dir(&dir).unwrap();
        let source = dir.join("00-source.conf");
        let target = dir.join("10-target.conf");
        fs::write(
            dir.join(".00-hidden.conf"),
            "Host *\n User excluded-by-native-glob\n Port 2299\n",
        )
        .unwrap();
        fs::write(
            &root,
            format!(
                "Include {}\nHost *\n User fallback\n Port 2207\n",
                quote(&dir.join("*.conf").to_string_lossy())
            ),
        )
        .unwrap();
        fs::write(
            &source,
            "Host source secondary\n HostName source.example\n User specific\n Port 22\n",
        )
        .unwrap();
        fs::write(&target, format!("Include {}\n# Tags: other-tag\n# original comment\nHost other\n HostName other.example\n User other-user\n Port 2244\nMatch host other\n Compression yes\n", quote(&temp.path().join("missing-*.conf").to_string_lossy()))).unwrap();
        let source_before = crate::runtime::effective::resolve(&root, "secondary").unwrap();
        let other_before = crate::runtime::effective::resolve(&root, "other").unwrap();
        let mut copy = find_host(&root, "secondary").unwrap();
        copy.name = "new-copy".into();
        copy.aliases.clear();
        copy.tags = vec!["copy-tag".into()];
        let before_count = load_hosts(&root).unwrap().len();
        add_host_preserving_target(&copy, &target, &temp.path().join("app"), &root).unwrap();
        let actual = crate::runtime::effective::resolve(&root, "new-copy").unwrap();
        assert_eq!(
            (&actual.hostname, &actual.user, &actual.port),
            (
                &source_before.hostname,
                &source_before.user,
                &source_before.port
            )
        );
        assert_eq!(
            crate::runtime::effective::resolve(&root, "secondary").unwrap(),
            source_before
        );
        assert_eq!(
            crate::runtime::effective::resolve(&root, "other").unwrap(),
            other_before
        );
        assert_eq!(find_host(&root, "other").unwrap().tags, ["other-tag"]);
        assert_eq!(load_hosts(&root).unwrap().len(), before_count + 1);
        assert_eq!(find_host(&root, "new-copy").unwrap().aliases, ["new-copy"]);
    }

    #[test]
    fn copy_preflight_unsupported_include_rejects_before_backups_or_writes() {
        for unsupported in ["$UNDEFINED/include", "%d/include", "~nameduser/include"] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("config");
            let text =
                format!("Host original\n HostName original.example\nInclude {unsupported}\n");
            fs::write(&root, &text).unwrap();
            let copy = Host {
                name: "new-copy".into(),
                hostname: "original.example".into(),
                user: "specific".into(),
                port: "22".into(),
                ..Host::default()
            };
            let app = temp.path().join("app");
            let error = add_host_preserving_target(&copy, &root, &app, &root)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("unsupported") && error.contains("retry"),
                "{error}"
            );
            assert_eq!(fs::read_to_string(&root).unwrap(), text);
            assert!(!app.exists());
        }
    }

    #[test]
    fn copy_mirror_detects_changed_files_and_glob_membership_before_publication() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let dir = temp.path().join("parts");
        fs::create_dir(&dir).unwrap();
        let included = dir.join("a.conf");
        fs::write(&root, format!("Include {}\n", dir.join("*.conf").display())).unwrap();
        fs::write(&included, "Host source\n HostName source.example\n").unwrap();
        let mirror = ConfigMirror::new(&root).unwrap();
        assert!(mirror.unchanged().is_ok());
        fs::write(
            &included,
            "Host externally-changed\n HostName external.example\n",
        )
        .unwrap();
        assert!(mirror
            .unchanged()
            .unwrap_err()
            .to_string()
            .contains("changed"));
        let mirror = ConfigMirror::new(&root).unwrap();
        fs::write(
            dir.join("b.conf"),
            "Host new-external\n HostName new.example\n",
        )
        .unwrap();
        assert!(mirror
            .unchanged()
            .unwrap_err()
            .to_string()
            .contains("Include expansion changed"));
    }

    #[cfg(unix)]
    #[test]
    fn copy_native_home_matches_openssh_expansion() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("empty");
        fs::write(&root, "").unwrap();
        let actual = crate::runtime::effective::resolve(&root, "test-alias").unwrap();
        let home = files::native_home().unwrap();
        assert!(actual.directives["userknownhostsfile"][0]
            .starts_with(&home.join(".ssh/known_hosts").to_string_lossy().into_owned()));
    }
    #[cfg(unix)]
    #[test]
    fn copy_preflight_keeps_native_permission_rejection_before_any_publication() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("config");
        let target = temp.path().join("included");
        fs::write(&root, format!("Host source\n HostName source.example\n User specific\n Port 22\nHost new-*\n Include {}\n", target.display())).unwrap();
        fs::write(&target, "Host other\n HostName other.example\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o666)).unwrap();
        let mut copy = find_host(&root, "source").unwrap();
        copy.name = "new-copy".into();
        copy.aliases.clear();
        assert!(crate::runtime::effective::resolve(&root, "source").is_err());
        let root_before = fs::read(&root).unwrap();
        let target_before = fs::read(&target).unwrap();
        let app = temp.path().join("app");
        assert!(
            add_host_preserving_target(&copy, &target, &app, &root).is_err(),
            "mirror must not hide an original Include permission error"
        );
        assert_eq!(fs::read(&root).unwrap(), root_before);
        assert_eq!(fs::read(&target).unwrap(), target_before);
        assert!(!app.exists());
    }
    #[cfg(unix)]
    #[test]
    fn copy_preflight_access_graph_mutations_are_rejected() {
        use std::{
            os::unix::fs::{symlink, PermissionsExt},
            process::Command,
        };
        if std::env::var_os("SSHM_COPY_MUTATION_CHILD").is_some() {
            let root = PathBuf::from(std::env::var_os("SSHM_COPY_ROOT").unwrap());
            let target = PathBuf::from(std::env::var_os("SSHM_COPY_TARGET").unwrap());
            let app = PathBuf::from(std::env::var_os("SSHM_COPY_APP").unwrap());
            let copy = Host {
                name: "new-copy".into(),
                hostname: "source.example".into(),
                user: "specific".into(),
                port: "22".into(),
                ..Host::default()
            };
            let result = add_host_preserving_target(&copy, &target, &app, &root);
            assert!(result.is_err(), "preflight must reject a graph access/metadata mutation instead of publishing: {result:?}");
            assert!(result.unwrap_err().to_string().contains("changed"));
            return;
        }
        for case in [
            "root-link",
            "root-parent",
            "include-link",
            "include-alias",
            "include-parent",
            "target-alias",
            "root-mode",
            "include-mode",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let dir = temp.path();
            let old_dir = dir.join("old");
            let new_dir = dir.join("new");
            let bin = dir.join("bin");
            fs::create_dir(&old_dir).unwrap();
            fs::create_dir(&new_dir).unwrap();
            fs::create_dir(&bin).unwrap();
            let declaration = "Host source secondary\n HostName source.example\n User specific\n Port 22\nHost *\n User fallback\n Port 2207\n";
            let old_file = old_dir.join("config");
            let new_file = new_dir.join("config");
            fs::write(&old_file, declaration).unwrap();
            fs::write(&new_file, "Host *\n User changed-user\n Port 2299\n").unwrap();
            let root_file = dir.join("root");
            let selected = dir.join("selected");
            let root;
            let target;
            let access;
            let replacement;
            let mutation;
            match case {
                "root-link" => {
                    symlink(&old_file, &selected).unwrap();
                    root = selected.clone();
                    target = root.clone();
                    access = selected.clone();
                    replacement = new_file.clone();
                    mutation = "link";
                }
                "root-parent" => {
                    symlink(&old_dir, &selected).unwrap();
                    root = selected.join("config");
                    target = root.clone();
                    access = selected.clone();
                    replacement = new_dir.clone();
                    mutation = "link";
                }
                "include-link" | "include-alias" | "include-parent" => {
                    fs::write(&old_file, "Host never-matching\n HostName never.example\n").unwrap();
                    if case != "include-parent" {
                        symlink(&old_file, &selected).unwrap();
                        replacement = new_file.clone();
                    } else {
                        symlink(&old_dir, &selected).unwrap();
                        replacement = new_dir.clone();
                    }
                    let include = if case != "include-parent" {
                        selected.clone()
                    } else {
                        selected.join("config")
                    };
                    let earlier_alias = if case == "include-alias" {
                        format!("Include {}\n", old_file.display())
                    } else {
                        String::new()
                    };
                    fs::write(
                        &root_file,
                        format!(
                            "{earlier_alias}Include {}\n{declaration}",
                            include.display()
                        ),
                    )
                    .unwrap();
                    root = root_file.clone();
                    target = root.clone();
                    access = selected.clone();
                    mutation = "link";
                }
                "target-alias" => {
                    fs::write(
                        &root_file,
                        format!("Include {}\n{declaration}", old_file.display()),
                    )
                    .unwrap();
                    fs::write(&old_file, "Host never-matching\n HostName never.example\n").unwrap();
                    symlink(&old_file, &selected).unwrap();
                    root = root_file.clone();
                    target = selected.clone();
                    access = selected.clone();
                    replacement = new_file.clone();
                    mutation = "link";
                }
                "root-mode" => {
                    root = old_file.clone();
                    target = root.clone();
                    access = root.clone();
                    replacement = new_file.clone();
                    mutation = "mode";
                }
                "include-mode" => {
                    fs::write(&old_file, "Host never-matching\n HostName never.example\n").unwrap();
                    fs::write(
                        &root_file,
                        format!("Include {}\n{declaration}", old_file.display()),
                    )
                    .unwrap();
                    root = root_file.clone();
                    target = root.clone();
                    access = old_file.clone();
                    replacement = new_file.clone();
                    mutation = "mode";
                }
                _ => unreachable!(),
            }
            assert_eq!(
                crate::runtime::effective::resolve(&root, "secondary")
                    .unwrap()
                    .user,
                "specific"
            );
            let before: Vec<_> = [&old_file, &new_file, &root_file]
                .into_iter()
                .filter(|path| path.exists())
                .map(|path| (path.clone(), fs::read(path).unwrap()))
                .collect();
            let script = bin.join("ssh");
            fs::write(&script, "#!/bin/sh\nset -eu\nif [ \"$SSHM_COPY_MUTATION\" = link ]; then\n /bin/rm -f -- \"$SSHM_COPY_ACCESS\"\n /bin/ln -s -- \"$SSHM_COPY_REPLACEMENT\" \"$SSHM_COPY_ACCESS\"\nelse\n /bin/chmod 666 \"$SSHM_COPY_ACCESS\"\nfi\n/bin/touch \"$SSHM_COPY_WRAPPER_RAN\"\nexec /usr/bin/ssh \"$@\"\n").unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
            let app = dir.join("app");
            let marker = dir.join("native-wrapper-ran");
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "config::copy_tests::copy_preflight_access_graph_mutations_are_rejected",
                    "--nocapture",
                ])
                .env("PATH", &bin)
                .env("SSHM_COPY_MUTATION_CHILD", "1")
                .env("SSHM_COPY_ROOT", &root)
                .env("SSHM_COPY_TARGET", &target)
                .env("SSHM_COPY_APP", &app)
                .env("SSHM_COPY_MUTATION", mutation)
                .env("SSHM_COPY_ACCESS", &access)
                .env("SSHM_COPY_REPLACEMENT", &replacement)
                .env("SSHM_COPY_WRAPPER_RAN", &marker)
                .output()
                .unwrap();
            assert!(
                marker.exists(),
                "{case}: native wrapper must run during preflight"
            );
            assert!(
                output.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if mutation == "link" {
                assert_eq!(
                    fs::read_link(&access).unwrap(),
                    replacement,
                    "{case}: wrapper must complete the retarget"
                );
            } else {
                assert_eq!(
                    fs::metadata(&access).unwrap().permissions().mode() & 0o777,
                    0o666,
                    "{case}: refused copy must retain the external mode change"
                );
            }
            for (path, bytes) in before {
                assert_eq!(
                    fs::read(&path).unwrap(),
                    bytes,
                    "{case}: no publication to {}",
                    path.display()
                );
            }
            assert!(
                !app.exists(),
                "{case}: no backups or app writes on refused preflight"
            );
        }
    }
}
