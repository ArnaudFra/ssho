use super::*;
use std::{fs, path::Path};
fn fixture() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn public(path: &Path) {
    fs::write(path, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIF1fG2i8gd9jrxzvSE+ovUwKq1qlw9m6tGJh+OK0jkRO fixture\n").unwrap();
}
#[test]
fn inventory_public_fingerprints_usages_unresolved_and_missing() {
    let tmp = fixture();
    let ssh = tmp.path().join(".ssh");
    fs::create_dir(&ssh).unwrap();
    let private = ssh.join("key with spaces");
    fs::write(&private, "PRIVATE MUST NOT BE READ").unwrap();
    public(&public_path(&private));
    let missing = ssh.join("missing");
    fs::write(&missing, "PRIVATE").unwrap();
    let hosts = vec![crate::types::Host {
        name: "secondary".into(),
        identity: private.display().to_string(),
        extra_identities: vec![missing.display().to_string(), "%d/key".into()],
        ..Default::default()
    }];
    let entries = inventory(
        tmp.path(),
        &hosts,
        Path::new("ssh-keygen"),
        &std::sync::atomic::AtomicBool::new(false),
    );
    let key = entries.iter().find(|k| k.private == private).unwrap();
    assert!(key.public.as_ref().unwrap().fingerprint.contains("SHA256:"));
    assert_eq!(key.usages, ["secondary"]);
    assert!(entries
        .iter()
        .any(|k| k.diagnostic.contains("public file missing")));
    assert!(entries.iter().any(|k| k.diagnostic.contains("unresolved")));
}
#[test]
fn publish_never_clobbers_either_destination() {
    let tmp = fixture();
    let staged = tmp.path().join("stage");
    fs::write(&staged, "secret fixture").unwrap();
    public(&public_path(&staged));
    let dest = tmp.path().join("dest");
    public(&public_path(&dest));
    assert!(publish_pair(&staged, &dest).is_err());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "secret fixture");
    assert!(public_path(&dest).exists());
    fs::remove_file(public_path(&dest)).unwrap();
    fs::write(&dest, "original").unwrap();
    assert!(publish_pair(&staged, &dest).is_err());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "original");
    fs::remove_file(&dest).unwrap();
    publish_pair(&staged, &dest).unwrap();
    assert!(dest.exists());
    assert!(public_path(&dest).exists());
}
#[test]
fn deploy_revalidates_public_and_rejects_alias_injection() {
    let tmp = fixture();
    let private = tmp.path().join("hostile ' $(touch injected) key");
    fs::write(&private, "fixture").unwrap();
    public(&public_path(&private));
    let reviewed = review_public(&public_path(&private), Path::new("ssh-keygen")).unwrap();
    assert!(Deployment::prepare(&reviewed, &private, Path::new("config"), "-bad").is_err());
    let deployment = Deployment::prepare(
        &reviewed,
        &private,
        Path::new("config with spaces"),
        "secondary",
    )
    .unwrap();
    assert_eq!(deployment.arguments()[2], "-F");
    assert_eq!(deployment.arguments().last().unwrap(), "secondary");
    assert!(!deployment.arguments()[1].contains('\''));
    assert!(!deployment.arguments()[1].contains('$'));
    assert!(!deployment.arguments().contains(&"-f".into()));
    fs::write(&reviewed.path, "changed").unwrap();
    assert!(Deployment::prepare(&reviewed, &private, Path::new("config"), "secondary").is_err());
}
#[test]
fn attach_preserves_alias_tags_and_existing_identities() {
    let host = crate::types::Host {
        name: "secondary".into(),
        aliases: vec!["primary".into(), "secondary".into()],
        identity: "old".into(),
        extra_identities: vec!["other".into()],
        tags: vec!["prod".into()],
        ..Default::default()
    };
    let next = attach(&host, Path::new("/key with spaces"));
    assert_eq!(next.identity, "/key with spaces");
    assert_eq!(next.extra_identities, ["old", "other"]);
    assert_eq!(next.aliases, host.aliases);
    assert_eq!(next.tags, host.tags);
    assert_eq!(next.name, "secondary");
}
#[test]
fn inventory_strips_only_last_public_suffix() {
    let tmp = fixture();
    fs::create_dir(tmp.path().join(".ssh")).unwrap();
    let private = tmp.path().join(".ssh/key.pub");
    public(&public_path(&private));
    let entries = inventory(
        tmp.path(),
        &[],
        Path::new("ssh-keygen"),
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(entries
        .iter()
        .any(|k| k.private == private && k.public.is_some()));
}
#[cfg(unix)]
fn script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    // A concurrent test can fork while fs::write holds this executable open;
    // its pre-exec child then briefly retains that writer despite CLOEXEC and
    // causes ETXTBSY here. Write in a child so the test process never owns the
    // executable's writer descriptor, and wait for that writer to exit.
    let status = std::process::Command::new("/bin/sh")
        .args(["-c", "printf '%s' \"$1\" > \"$2\"", "fixture-writer"])
        .arg(format!("#!/bin/sh\n{body}\n"))
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success(), "fixture writer failed: {status}");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path
}
#[cfg(unix)]
#[test]
fn fingerprint_missing_failure_timeout_and_output_bound() {
    let tmp = fixture();
    let file = tmp.path().join("key.pub");
    public(&file);
    assert!(review_public(&file, &tmp.path().join("absent"))
        .unwrap_err()
        .to_string()
        .contains("ssh-keygen"));
    let fail = script(tmp.path(), "fail", "echo native-failure >&2; exit 9");
    assert!(review_public(&file, &fail)
        .unwrap_err()
        .to_string()
        .contains("native-failure"));
    let slow = script(tmp.path(), "slow", "sleep 60");
    let start = std::time::Instant::now();
    assert!(format!("{:#}", review_public(&file, &slow).unwrap_err()).contains("timed out"));
    assert!(start.elapsed() < std::time::Duration::from_secs(4));
    let loud = script(tmp.path(), "loud", "yes long-diagnostic >&2");
    let start = std::time::Instant::now();
    assert!(review_public(&file, &loud).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(4));
}
#[cfg(unix)]
#[test]
fn generation_native_arguments_cleanup_and_no_clobber_race() {
    let tmp = fixture();
    let destination = tmp.path().join("key with spaces");
    let tool = script(
        tmp.path(),
        "keygen",
        r#"test "$1" = '-t' && test "$2" = ed25519 && test "$3" = '-f' && test "$5" = '-C' && test "$6" = 'my comment' && test "$#" = 6 || exit 22
printf 'fixture private' > "$4"
printf 'ssh-ed25519 fixture' > "$4.pub""#,
    );
    generate(&destination, "my comment", &tool).unwrap();
    assert!(destination.exists());
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let fail = script(
        tmp.path(),
        "failgen",
        "printf 'partial fixture' > \"$4\"; exit 8",
    );
    assert!(generate(&tmp.path().join("failkey"), "comment", &fail).is_err());
    assert!(!fs::read_dir(tmp.path())
        .unwrap()
        .flatten()
        .any(|p| p.file_name().to_string_lossy().starts_with(".sshm-key-")));
}
#[cfg(unix)]
#[test]
fn generation_race_keeps_published_private_and_reports_manual_recovery() {
    let tmp = fixture();
    let destination = tmp.path().join("raced");
    let tool = script(
        tmp.path(),
        "race",
        r#"printf 'fixture private' > "$4"
printf 'ssh-ed25519 fixture' > "$4.pub"
printf 'occupied by racer' > "$(dirname "$(dirname "$4")")/raced.pub""#,
    );
    let error = format!(
        "{:#}",
        generate(&destination, "comment", &tool).unwrap_err()
    );
    assert_eq!(
        fs::read_to_string(&destination).unwrap_or_else(|read_error| panic!(
            "private destination missing: {read_error}; generation returned: {error}"
        )),
        "fixture private"
    );
    assert!(error.contains(&destination.display().to_string()));
    assert!(error.contains(&public_path(&destination).display().to_string()));
    assert!(error.contains("left untouched"));
    assert!(error.contains("Inspect both paths"));
    assert!(error.contains("ssh-keygen -y"));
    assert!(error.contains("File exists"));
    assert_eq!(
        fs::read_to_string(public_path(&destination)).unwrap(),
        "occupied by racer"
    );
    assert!(!fs::read_dir(tmp.path())
        .unwrap()
        .flatten()
        .any(|p| p.file_name().to_string_lossy().starts_with(".sshm-key-")));
}
#[cfg(unix)]
#[test]
fn inventory_never_opens_private_fifo_and_rejects_public_symlink() {
    use std::os::unix::ffi::OsStrExt;
    let tmp = fixture();
    fs::create_dir(tmp.path().join(".ssh")).unwrap();
    let private = tmp.path().join(".ssh/id_fifo");
    let name = std::ffi::CString::new(private.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    public(&public_path(&private));
    let hosts = vec![crate::types::Host {
        name: "fixture".into(),
        identity: private.to_string_lossy().into_owned(),
        ..Default::default()
    }];
    let entries = inventory(
        tmp.path(),
        &hosts,
        Path::new("ssh-keygen"),
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(entries[0].public.is_some());
    let symlink = tmp.path().join("symlink.pub");
    std::os::unix::fs::symlink(&private, &symlink).unwrap();
    assert!(review_public(&symlink, Path::new("ssh-keygen")).is_err());
}
#[test]
fn scanner_repeated_refresh_is_single_flight() {
    let tmp = fixture();
    fs::create_dir(tmp.path().join(".ssh")).unwrap();
    public(&tmp.path().join(".ssh/id_fixture.pub"));
    let mut scanner = Scanner::default();
    scanner.start(tmp.path().into(), vec![], vec![]);
    assert!(scanner.busy());
    for _ in 0..100 {
        scanner.start(tmp.path().join("ignored"), vec![], vec![]);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        if let Some(entries) = scanner.poll() {
            assert_eq!(entries.len(), 1);
            assert!(!scanner.busy());
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn review_rejects_multiple_public_keys_not_all_shown_in_fingerprint() {
    let tmp = fixture();
    let path = tmp.path().join("multiple.pub");
    public(&path);
    let bytes = fs::read(&path).unwrap();
    let mut doubled = bytes.clone();
    doubled.extend(bytes);
    fs::write(&path, doubled).unwrap();
    assert!(review_public(&path, Path::new("ssh-keygen")).is_err());
}
#[cfg(unix)]
#[test]
fn native_copy_id_safe_snapshot_keeps_duplicate_check_for_hostile_filename() {
    let native = Path::new("/usr/bin/ssh-copy-id");
    if !native.exists() {
        return;
    }
    let tmp = fixture();
    fs::create_dir(tmp.path().join(".ssh")).unwrap();
    let private = tmp.path().join("evil$(touch INJECTED)' key");
    fs::write(&private, "fake private fixture never read by app").unwrap();
    public(&public_path(&private));
    let reviewed = review_public(&public_path(&private), Path::new("ssh-keygen")).unwrap();
    let root = tmp.path().join("config ' with spaces");
    fs::write(&root, "Host fixture\n HostName invalid.test\n").unwrap();
    let deployment = Deployment::prepare(&reviewed, &private, &root, "fixture").unwrap();
    // This ssh stub accepts the duplicate-check call without opening its private
    // argument. ssh-copy-id must then skip installation, never contacting a host.
    script(
        tmp.path(),
        "ssh",
        "printf '%s\\n' \"$@\" >> \"$HOME/ssh-arguments\"; exit 0",
    );
    let output = std::process::Command::new(native)
        .args(deployment.arguments())
        .env("HOME", tmp.path())
        .env("PATH", format!("{}:/usr/bin:/bin", tmp.path().display()))
        .env_remove("SSH_AUTH_SOCK")
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fs::read_to_string(tmp.path().join("ssh-arguments")).unwrap();
    assert!(args.contains("IdentitiesOnly=yes"));
    assert!(args.contains("fixture"));
    assert!(!tmp.path().join("INJECTED").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exist"));
}

#[test]
fn publication_failure_staging_cleanup_preserves_replacement_destination() {
    let tmp = fixture();
    let stage = tempfile::tempdir_in(tmp.path()).unwrap();
    let staged = stage.path().join("key");
    fs::write(&staged, "generated fixture").unwrap();
    public(&public_path(&staged));
    let destination = tmp.path().join("published key");
    fs::write(public_path(&destination), "existing public fixture").unwrap();
    assert!(publish_pair(&staged, &destination).is_err());
    // Second-file failure must not remove the published pathname. The caller
    // owns only staging cleanup, which is still pending at this boundary.
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "generated fixture"
    );
    let replacement = tmp.path().join("other writer");
    fs::write(&replacement, "replacement private fixture").unwrap();
    fs::rename(&replacement, &destination).unwrap();
    drop(stage);
    assert_eq!(
        fs::read_to_string(&destination).unwrap(),
        "replacement private fixture"
    );
    assert_eq!(
        fs::read_to_string(public_path(&destination)).unwrap(),
        "existing public fixture"
    );
}

#[test]
fn session_candidates_share_cap_and_missing_public_diagnostics() {
    let tmp = fixture();
    let session: Vec<_> = (0..300)
        .map(|n| tmp.path().join(format!("outside/key-{n:03}")))
        .collect();
    let entries = inventory_with_candidates(
        tmp.path(),
        &[],
        &session,
        Path::new("unused-tool"),
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert_eq!(entries.len(), INVENTORY_LIMIT);
    assert!(entries.iter().any(|entry| entry.private == session[299]));
    assert!(!entries.iter().any(|entry| entry.private == session[0]));
    assert!(entries
        .iter()
        .all(|entry| entry.diagnostic.contains("public file missing")));
    assert!(entries.last().unwrap().diagnostic.contains("capped at 256"));
}
