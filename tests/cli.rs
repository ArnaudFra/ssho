use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

struct Fixture {
    dir: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".ssh")).unwrap();
        fs::write(dir.path().join(".ssh/config"),"# Tags: work\nHost prod alias\n HostName db.example.org\n User deploy\n Port 2222\n\n# Tags: hidden\nHost secret\n HostName internal.example.org\n").unwrap();
        Self { dir }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ssho"))
            .args(args)
            .env("HOME", self.dir.path())
            .env("USERPROFILE", self.dir.path())
            .env("XDG_CONFIG_HOME", self.dir.path().join(".config"))
            .env("APPDATA", self.dir.path().join(".config"))
            .output()
            .unwrap()
    }
    fn config_root(&self) -> std::path::PathBuf {
        self.dir.path().join(".config/ssho")
    }
}

#[test]
fn help_version_and_completion_scripts_do_not_migrate() {
    let f = Fixture::new();
    let legacy = f.dir.path().join(".config/sshm");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("sentinel"), "untouched").unwrap();
    for args in [
        &["--help"][..],
        &["--version"],
        &["info", "--help"],
        &["completion", "bash"],
        &["completion", "zsh"],
        &["completion", "fish"],
        &["completion", "powershell"],
    ] {
        let out = f.run(args);
        assert!(
            out.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!f.config_root().exists());
    }
    let script = f.run(&["completion", "bash"]);
    let path = f.dir.path().join("completion.bash");
    fs::write(&path, script.stdout).unwrap();
    assert!(Command::new("bash")
        .arg("-n")
        .arg(path)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        fs::read_to_string(legacy.join("sentinel")).unwrap(),
        "untouched"
    );
}

#[test]
fn info_search_and_missing_host_are_machine_readable() {
    let f = Fixture::new();
    let out = f.run(&["info", "alias", "--pretty"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["schema"], "sshm.info.v1");
    assert_eq!(json["result"]["target"]["hostname"], "db.example.org");
    assert_eq!(json["result"]["target"]["port"], 2222);
    let out = f.run(&["info", "missing"]);
    assert_eq!(out.status.code(), Some(2));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["error"]["code"], "NOT_FOUND");
    let out = f.run(&["search", "work", "--tags", "--format", "json"]);
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 2);
    let out = f.run(&["search", "--format", "simple"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "prod\nalias\n");
    assert!(!f.config_root().exists());
}

#[test]
fn completion_resolves_custom_config_and_incomplete_subcommands() {
    let f = Fixture::new();
    for args in [
        &["__complete", "pro"][..],
        &["__complete", "edit", "pro"],
        &["__complete", "info", "pro"],
    ] {
        let out = f.run(args);
        assert!(
            out.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8(out.stdout).unwrap(), "prod\n:4\n");
    }
    let out = f.run(&["__complete", "info", "sec"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "secret\n:4\n");
    let custom = f.dir.path().join("custom config");
    fs::write(&custom, "Host custom\n HostName example.org\n").unwrap();
    let out = f.run(&["__complete", "-c", custom.to_str().unwrap(), "cus"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "custom\n:4\n");
    let out = f.run(&["__complete", "prod", ""]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), ":4\n");
}

#[cfg(unix)]
fn fake_ssh(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(dir.join("bin")).unwrap();
    let script = dir.join("bin/ssh");
    fs::write(&script,"#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$SSHM_TEST_ARGS\"\nprintf 'remote-output\\n'\nexit 37\n").unwrap();
    fs::set_permissions(script, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
#[cfg(unix)]
fn direct_ssh_preserves_args_exit_status_and_migrates_once() {
    let f = Fixture::new();
    fake_ssh(f.dir.path());
    let legacy = f.dir.path().join(".config/sshm");
    fs::create_dir_all(legacy.join("backups")).unwrap();
    fs::write(
        legacy.join("config.json"),
        r#"{"ping_on_start":false,"check_for_updates":false}"#,
    )
    .unwrap();
    fs::write(legacy.join("backups/config.backup"), "original backup").unwrap();
    let source_history = r#"{"connections":{"prod":{"host_name":"prod","last_connect":"2025-01-01T10:00:00Z","connect_count":4,"port_forwarding":{"type":"dynamic","local_port":"1080","remote_host":"","remote_port":"","bind_address":"127.0.0.1"}}}}"#;
    fs::write(legacy.join("sshm_history.json"), source_history).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_ssho"))
            .args(["-t", "prod", "ls", "-la", "/a b", "--config", "remote"])
            .env("HOME", f.dir.path())
            .env("XDG_CONFIG_HOME", f.dir.path().join(".config"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    f.dir.path().join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("SSHM_TEST_ARGS", f.dir.path().join("args"))
            .output()
            .unwrap()
    };
    let out = run();
    assert_eq!(
        out.status.code(),
        Some(37),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"remote-output\n");
    let arguments = fs::read_to_string(f.dir.path().join("args")).unwrap();
    assert_eq!(
        arguments.lines().collect::<Vec<_>>(),
        [
            "-F",
            f.dir.path().join(".ssh/config").to_str().unwrap(),
            "-t",
            "prod",
            "ls",
            "-la",
            "/a b",
            "--config",
            "remote"
        ]
    );
    let history: serde_json::Value =
        serde_json::from_slice(&fs::read(f.config_root().join("sshm_history.json")).unwrap())
            .unwrap();
    assert_eq!(history["connections"]["prod"]["connect_count"], 5);
    assert_eq!(
        history["connections"]["prod"]["port_forwarding"]["local_port"],
        "1080"
    );
    assert_eq!(
        fs::read_to_string(legacy.join("sshm_history.json")).unwrap(),
        source_history
    );
    assert_eq!(
        fs::read_to_string(f.config_root().join("backups/config.backup")).unwrap(),
        "original backup"
    );
    fs::write(legacy.join("backups/config.backup"), "new legacy backup").unwrap();
    assert_eq!(run().status.code(), Some(37));
    assert_eq!(
        fs::read_to_string(f.config_root().join("backups/config.backup")).unwrap(),
        "original backup"
    );
}
