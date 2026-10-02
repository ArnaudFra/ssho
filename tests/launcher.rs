#![cfg(unix)]
use serde_json::{json, Value};
use ssho::storage::AppConfig;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};

// Serialize executable creation against this test binary's concurrent forks.
static FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    dir: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}
impl Fixture {
    fn new() -> Self {
        let guard = FIXTURES.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".ssh")).unwrap();
        fs::create_dir_all(dir.path().join(".config/ssho")).unwrap();
        fs::create_dir_all(dir.path().join("bin")).unwrap();
        fs::write(
            dir.path().join(".ssh/config"),
            "Host prod\n HostName prod.invalid\n ProxyCommand /bin/true\n",
        )
        .unwrap();
        let f = Self { dir, _guard: guard };
        f.script("bin/ssh", "printf '%s\\0' \"$@\" >> \"$SSHM_NATIVE_ARGS\"\nif [ \"$1\" = -G ]; then exec /usr/bin/ssh \"$@\"; fi\nexit 0\n");
        f
    }
    fn root(&self) -> PathBuf {
        self.dir.path().join(".config/ssho")
    }
    fn preferences(&self, value: Value) {
        fs::write(self.root().join("config.json"), value.to_string()).unwrap();
    }
    fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env("HOME", self.dir.path())
            .env("USERPROFILE", self.dir.path())
            .env("XDG_CONFIG_HOME", self.dir.path().join(".config"))
            .env("APPDATA", self.dir.path().join(".config"))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.dir.path().join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("SSHM_NATIVE_ARGS", self.dir.path().join("native-args"))
            .env("SSHM_WRAPPER_ARGS", self.dir.path().join("wrapper-args"));
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_ssho"))
            .args(args)
            .output()
            .unwrap()
    }
    fn history(&self) -> Value {
        serde_json::from_slice(&fs::read(self.root().join("sshm_history.json")).unwrap()).unwrap()
    }
}

#[test]
fn launcher_defaults_and_save_preserve_unknown_preferences() {
    let f = Fixture::new();
    for document in [json!({}), json!({"ssh_command":null}), Value::Null] {
        f.preferences(document);
        let config = AppConfig::load(&f.root()).unwrap();
        assert_eq!(
            serde_json::to_value(config).unwrap()["ssh_command"],
            json!(["ssh"])
        );
    }
    f.preferences(
        json!({"ssh_command":["/a b/wrapper", "", "$(literal);*"],"future":{"keep":true}}),
    );
    let config = AppConfig::load(&f.root()).unwrap();
    config.save(&f.root()).unwrap();
    let saved: Value =
        serde_json::from_slice(&fs::read(f.root().join("config.json")).unwrap()).unwrap();
    assert_eq!(
        saved["ssh_command"],
        json!(["/a b/wrapper", "", "$(literal);*"])
    );
    assert_eq!(saved["future"]["keep"], true);
}

#[test]
fn launcher_invalid_in_memory_prefix_cannot_overwrite_preferences() {
    let f = Fixture::new();
    f.preferences(json!({"ssh_command":["ssh"],"future":42}));
    let before = fs::read(f.root().join("config.json")).unwrap();
    for command in [
        vec![],
        vec![" ".into()],
        vec!["ssh".into(), "bad\0argument".into()],
    ] {
        let mut config = AppConfig::load(&f.root()).unwrap();
        config.ssh_command = command;
        assert!(config.save(&f.root()).is_err());
        assert_eq!(fs::read(f.root().join("config.json")).unwrap(), before);
    }
}

#[test]
fn launcher_invalid_preferences_fail_before_launch_or_history() {
    let f = Fixture::new();
    for invalid in [
        json!([]),
        json!([""]),
        json!([" \t\n"]),
        json!(["ssh\0bad"]),
        json!(["ssh", "bad\0argument"]),
        json!("ssh -v"),
        json!(["ssh", 42]),
        json!({}),
    ] {
        f.preferences(json!({"ssh_command":invalid}));
        let out = f.run(&["prod", "true"]);
        assert_eq!(out.status.code(), Some(1), "accepted {invalid}: {out:?}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains("ssh_command"), "{invalid}: {error}");
        assert!(
            error.contains("array")
                || error.contains("tableau")
                || error.contains("argument")
                || error.contains("executable"),
            "actionable error required: {error}"
        );
        assert!(!f.dir.path().join("native-args").exists());
        assert!(!f.root().join("sshm_history.json").exists());
    }
}

#[test]
fn launcher_literal_prefix_remote_argv_streams_status_and_history() {
    let f = Fixture::new();
    let wrapper = f.script("wrapper space ;$", "printf '%s\\0' \"$@\" > \"$SSHM_WRAPPER_ARGS\"\nprintf 'stdout exact\\n'\nprintf 'stderr exact\\n' >&2\nexit 37\n");
    let literal = format!(
        "$(touch {}) ; * \" quoted",
        f.dir.path().join("injected").display()
    );
    f.preferences(json!({"ssh_command":[wrapper, "prefix space", literal, ""]}));
    let out = f.run(&[
        "-t",
        "prod",
        "printf",
        "a b",
        ";",
        "$(echo nope)",
        "--config",
        "remote",
    ]);
    assert_eq!(out.status.code(), Some(37), "{out:?}");
    assert_eq!(out.stdout, b"stdout exact\n");
    assert_eq!(out.stderr, b"stderr exact\n");
    let bytes = fs::read(f.dir.path().join("wrapper-args")).unwrap();
    let args: Vec<&[u8]> = bytes.split(|b| *b == 0).collect();
    let config = f.dir.path().join(".ssh/config");
    assert_eq!(
        args,
        vec![
            b"prefix space".as_slice(),
            literal.as_bytes(),
            b"",
            b"-F",
            config.as_os_str().as_encoded_bytes(),
            b"-t",
            b"prod",
            b"printf",
            b"a b",
            b";",
            b"$(echo nope)",
            b"--config",
            b"remote",
            b""
        ]
    );
    assert!(!f.dir.path().join("injected").exists());
    assert!(!f.dir.path().join("native-args").exists());
    assert_eq!(
        f.history()["connections"]["prod"]["last_result"]["exit_code"],
        37
    );
}

#[test]
fn launcher_direct_connection_and_signal_status() {
    let f = Fixture::new();
    let wrapper = f.script(
        "signal wrapper",
        "printf 'direct stdout\\n'\nprintf 'direct stderr\\n' >&2\nkill -TERM $$\n",
    );
    f.preferences(json!({"ssh_command":[wrapper]}));
    let out = f.run(&["prod"]);
    assert_eq!(out.status.code(), Some(143), "{out:?}");
    assert_eq!(out.stdout, b"direct stdout\n");
    assert_eq!(out.stderr, b"Connecting to prod...\ndirect stderr\n");
    assert_eq!(
        f.history()["connections"]["prod"]["last_result"]["exit_code"],
        143
    );
}

#[test]
fn launcher_missing_executable_reports_configured_program_and_records_error() {
    let f = Fixture::new();
    let missing = f.dir.path().join("missing wrapper");
    f.preferences(json!({"ssh_command":[missing]}));
    let out = f.run(&["prod", "true"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("missing wrapper") && error.contains("ssh_command"),
        "{error}"
    );
    let history = f.history();
    assert!(history["connections"]["prod"]["last_result"]["exit_code"].is_null());
    assert!(
        history["connections"]["prod"]["last_result"]["launch_error"]
            .as_str()
            .unwrap()
            .contains("missing wrapper")
    );
}

#[test]
fn launcher_native_operations_ignore_configured_wrapper() {
    let f = Fixture::new();
    let wrapper = f.script(
        "forbidden wrapper",
        "printf 'WRAPPER_MUST_NOT_RUN' > \"$SSHM_WRAPPER_ARGS\"\nexit 37\n",
    );
    f.preferences(json!({"ssh_command":[wrapper]}));
    let output = f
        .command(std::env::current_exe().unwrap())
        .args(["--exact", "launcher_native_operations_child", "--nocapture"])
        .env("SSHM_LAUNCHER_CHILD", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!f.dir.path().join("wrapper-args").exists());
    let args = fs::read(f.dir.path().join("native-args")).unwrap();
    let args = String::from_utf8(args).unwrap();
    for flag in [
        "-D\0",
        "-N\0",
        "-G\0",
        "BatchMode=yes\0",
        "exit\0",
        "copied\0",
    ] {
        assert!(
            args.contains(flag),
            "native operation absent: {flag:?}: {args:?}"
        );
    }
}

#[test]
fn launcher_native_operations_child() {
    if std::env::var_os("SSHM_LAUNCHER_CHILD").is_none() {
        return;
    }
    use ssho::{
        runtime::{
            self,
            effective::Inspector,
            tunnels::{Manager, Snapshot},
        },
        storage::AppPaths,
        types::{Forward, Host},
    };
    use std::time::{Duration, Instant};
    let paths = AppPaths::discover().unwrap();
    let forward = Forward {
        kind: "dynamic".into(),
        local_port: "1080".into(),
        ..Forward::default()
    };
    assert_eq!(
        runtime::connect(
            &paths,
            &paths.ssh_config,
            "prod",
            false,
            &[],
            Some(&forward)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        runtime::connect(
            &paths,
            &paths.ssh_config,
            "prod",
            true,
            &["true".into()],
            Some(&forward)
        )
        .unwrap(),
        0
    );
    let mut manager = Manager::default();
    manager
        .start(Snapshot {
            config: paths.ssh_config.clone(),
            host: "prod".into(),
            profile: None,
            forward,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.has_active() && Instant::now() < deadline {
        manager.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!manager.has_active());
    let inspector = Inspector::default();
    let results = inspector.inspect(
        vec![Host {
            name: "prod".into(),
            ..Host::default()
        }],
        paths.ssh_config.clone(),
        true,
    );
    assert!(results
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .effective
        .is_ok());
    assert_eq!(
        results
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .ping
            .unwrap()
            .status,
        runtime::PingStatus::Online
    );
    ssho::config::preview::prepare_copy(
        &Host {
            name: "copied".into(),
            hostname: "copy.invalid".into(),
            ..Host::default()
        },
        &paths.ssh_config,
        &paths.ssh_config,
    )
    .unwrap();
}
