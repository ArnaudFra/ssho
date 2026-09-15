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

fn optional_shell_output(command: &mut Command) -> Option<Output> {
    match command.output() {
        Ok(output) => Some(output),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "Skipping optional shell check: {} was not found on PATH",
                command.get_program().to_string_lossy()
            );
            None
        }
        Err(error) => panic!(
            "failed to execute {}: {error}",
            command.get_program().to_string_lossy()
        ),
    }
}

#[test]
fn optional_shell_check_skips_missing_executable_with_isolated_path() {
    let empty_path = tempfile::tempdir().unwrap();
    assert!(optional_shell_output(
        Command::new("sshm-test-unavailable-shell").env("PATH", empty_path.path())
    )
    .is_none());
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
        assert!(!out.stdout.is_empty(), "{args:?} produced no output");
        assert!(!f.config_root().exists());
    }
    let script = f.run(&["completion", "bash"]);
    let path = f.dir.path().join("completion.bash");
    fs::write(&path, script.stdout).unwrap();
    if let Some(out) = optional_shell_output(Command::new("bash").arg("-n").arg(path)) {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
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
        &["__complete", "move", "pro"],
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

#[test]
fn completion_suggests_contextual_commands_flags_and_values() {
    let f = Fixture::new();
    for (words, expected) in [
        (vec!["ed"], "edit\n:4\n"),
        (vec!["he"], "help\n:4\n"),
        (vec!["co"], "completion\n:4\n"),
        (vec!["help", "ed"], "edit\n:4\n"),
        (vec!["search", "--na"], "--names\n:4\n"),
        (vec!["search", "--ta"], "--tags\n:4\n"),
        (vec!["search", "--fo"], "--format\n:4\n"),
        (vec!["info", "--pr"], "--pretty\n:4\n"),
        (vec!["search", "--format", "j"], "json\n:4\n"),
        (vec!["search", "-f", "s"], "simple\n:4\n"),
        (vec!["search", "--format=j"], "--format=json\n:4\n"),
        (vec!["search", "--format", "json", "--na"], "--names\n:4\n"),
        (vec!["completion", "f"], "fish\n:4\n"),
        (vec!["completion", "fish", ""], ":4\n"),
        (vec!["edit", "prod", ""], ":4\n"),
        (vec!["move", "prod", ""], ":4\n"),
        (vec!["info", "prod", ""], ":4\n"),
        (vec!["prod", ""], ":4\n"),
        (vec!["edit", "sec"], ":4\n"),
        (vec!["move", "sec"], ":4\n"),
        (vec!["sec"], ":4\n"),
        (vec!["info", "sec"], "secret\n:4\n"),
        (vec!["info", "--na"], ":4\n"),
    ] {
        let mut args = vec!["__complete"];
        args.extend(words);
        let out = f.run(&args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{args:?}");
    }
    for flag in [
        "-c",
        "--config",
        "-t",
        "--tty",
        "-s",
        "--search",
        "--no-update-check",
        "-h",
        "--help",
        "-v",
        "--version",
    ] {
        let out = f.run(&["__complete", flag]);
        assert!(
            String::from_utf8(out.stdout)
                .unwrap()
                .lines()
                .any(|line| line == flag),
            "{flag}"
        );
    }
}

#[test]
fn completion_handles_config_paths_spaces_directories_and_short_flags() {
    let f = Fixture::new();
    let paths = f.dir.path().join("config paths");
    fs::create_dir(&paths).unwrap();
    fs::create_dir(paths.join("test directory")).unwrap();
    fs::write(
        paths.join("test config"),
        "Host custom\n HostName example.org\n",
    )
    .unwrap();
    let prefix = paths.join("test");
    let expected = format!(
        "{}\n{}/\n:4\n",
        paths.join("test config").display(),
        paths.join("test directory").display()
    );
    for flag in ["-c", "--config"] {
        let out = f.run(&["__complete", flag, prefix.to_str().unwrap()]);
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{flag}");
    }
    for flag in ["--config=", "-c", "-c="] {
        let partial = format!("{flag}{}", prefix.display());
        let out = f.run(&["__complete", &partial]);
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            format!(
                "{flag}{}\n{flag}{}/\n:4\n",
                paths.join("test config").display(),
                paths.join("test directory").display()
            )
        );
        let config_arg = format!("{flag}{}", paths.join("test config").display());
        let out = f.run(&["__complete", &config_arg, "cus"]);
        assert_eq!(String::from_utf8(out.stdout).unwrap(), "custom\n:4\n");
    }
    let out = f.run(&["__complete", "-c", "~/config paths/test"]);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "~/config paths/test config\n~/config paths/test directory/\n:4\n"
    );
    let out = f.run(&["__complete", "--config=/missing", "ed"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "edit\n:4\n");
}

#[cfg(unix)]
#[test]
fn bash_completion_uses_the_invoked_binary_without_path_installation() {
    let f = Fixture::new();
    fs::write(
        f.dir.path().join(".ssh/config"),
        "Host BS-one BS-two\n HostName example.org\n",
    )
    .unwrap();
    let script = f.dir.path().join("completion.bash");
    fs::write(&script, f.run(&["completion", "bash"]).stdout).unwrap();
    let Some(out) = optional_shell_output(
        Command::new("bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                r#"
source "$SSHM_COMPLETION"
cd "$SSHM_PROJECT"
COMP_WORDS=(./ssho BS); COMP_CWORD=1
_ssho
printf '%s\n' "${COMPREPLY[@]}"
"#,
            ])
            .env("HOME", f.dir.path())
            .env("SSHM_COMPLETION", &script)
            .env(
                "SSHM_PROJECT",
                Path::new(env!("CARGO_BIN_EXE_ssho")).parent().unwrap(),
            ),
    ) else {
        return;
    };
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "BS-one\nBS-two\n");
    assert!(!f.config_root().exists());
}

#[test]
fn generated_bash_and_fish_adapters_complete_commands_and_quoted_paths() {
    let f = Fixture::new();
    let folder = f.dir.path().join("config paths");
    fs::create_dir(&folder).unwrap();
    fs::create_dir(folder.join("test directory")).unwrap();
    fs::write(
        folder.join("test config"),
        "Host custom\n HostName example.org\n",
    )
    .unwrap();
    for shell in ["bash", "fish"] {
        let script = f.dir.path().join(format!("completion.{shell}"));
        fs::write(&script, f.run(&["completion", shell]).stdout).unwrap();
        let body = if shell == "bash" {
            r#"
ssho() { "$SSHM_BIN" "$@"; }
source "$SSHM_COMPLETION"
COMP_WORDS=(ssho ed); COMP_CWORD=1; _ssho; printf '%s\n' "${COMPREPLY[@]}"
COMP_WORDS=(ssho search --na); COMP_CWORD=2; _ssho; printf '%s\n' "${COMPREPLY[@]}"
COMP_WORDS=(ssho -c "$SSHM_PREFIX"); COMP_CWORD=2; _ssho; printf '%s\n' "${COMPREPLY[@]}"
COMP_WORDS=(ssho -c "$SSHM_CONFIG" cus); COMP_CWORD=3; _ssho; printf '%s\n' "${COMPREPLY[@]}"
COMP_WORDS=(ssho -c "'$SSHM_PREFIX"); COMP_CWORD=2; _ssho; printf '%s\n' "${COMPREPLY[@]}"
printf -v escaped '%q' "$SSHM_PREFIX"
COMP_WORDS=(ssho -c "$escaped"); COMP_CWORD=2; _ssho; printf '%s\n' "${COMPREPLY[@]}"
COMP_WORDS=(ssho --config = "$SSHM_PREFIX"); COMP_CWORD=3; _ssho
[[ ${COMPREPLY[0]} == "$SSHM_CONFIG" && ${COMPREPLY[1]} == "$SSHM_PREFIX directory/" ]] || exit 5
COMP_WORDS=(ssho --config = "$SSHM_CONFIG" cus); COMP_CWORD=4; _ssho
[[ ${COMPREPLY[*]} == custom ]] || exit 6
COMP_WORDS=(ssho search --format = j); COMP_CWORD=4; _ssho
[[ ${COMPREPLY[*]} == json ]] || exit 7
"#
        } else {
            r#"
function ssho; command $SSHM_BIN $argv; end
source "$SSHM_COMPLETION"
complete -C 'ssho ed'
complete -C 'ssho search --na'
complete -C "ssho -c "(string escape -- "$SSHM_PREFIX")
complete -C "ssho -c "(string escape -- "$SSHM_CONFIG")" cus"
complete -C "ssho -c '$SSHM_PREFIX"
complete -C "ssho -c "(string escape --style=script --no-quoted -- "$SSHM_PREFIX")
"#
        };
        let Some(out) = optional_shell_output(
            Command::new(shell)
                .args(if shell == "fish" {
                    vec!["--no-config", "-c", body]
                } else {
                    vec!["--noprofile", "--norc", "-c", body]
                })
                .env("HOME", f.dir.path())
                .env("SSHM_BIN", env!("CARGO_BIN_EXE_ssho"))
                .env("SSHM_COMPLETION", &script)
                .env("SSHM_PREFIX", folder.join("test"))
                .env("SSHM_CONFIG", folder.join("test config")),
        ) else {
            continue;
        };
        assert!(
            out.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            format!(
                "edit\n--names\n{file}\n{dir}/\ncustom\n{file}\n{dir}/\n{file}\n{dir}/\n",
                file = folder.join("test config").display(),
                dir = folder.join("test directory").display()
            ),
            "{shell}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
