#![cfg(unix)]
use ssho::{
    runtime::tunnels::{Manager, Snapshot, State},
    types::Forward,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Command,
    time::{Duration, Instant},
};
// Keep executable-file creation outside concurrent fork windows: another test's
// fork can briefly inherit a writer before exec/CLOEXEC, yielding Linux ETXTBSY.
static FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    dir: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}
impl Fixture {
    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}
fn fixture(script: &str) -> (Fixture, Manager, Snapshot) {
    let guard = FIXTURES.lock().unwrap_or_else(|poison| poison.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("ssh");
    fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    let config = dir.path().join("config");
    fs::write(&config, "Host test\n HostName localhost\n").unwrap();
    let snapshot = Snapshot {
        config,
        host: "test".into(),
        profile: Some("one".into()),
        forward: Forward {
            kind: "dynamic".into(),
            local_port: "1080".into(),
            ..Forward::default()
        },
    };
    let manager = Manager::with_program(exe);
    (Fixture { dir, _guard: guard }, manager, snapshot)
}
#[test]
fn owned_running_stop_drop_and_unrelated_child() {
    let (_dir, mut m, snapshot) = fixture("sleep 60 & wait");
    let mut sentinel = Command::new("sleep").arg("60").spawn().unwrap();
    let a = m.start(snapshot.clone()).unwrap();
    let b = m.start(snapshot).unwrap();
    m.poll();
    assert!(m.has_active());
    assert_eq!(m.records()[0].state, State::Active);
    m.stop(a).unwrap();
    assert_eq!(m.records()[0].state, State::Stopped);
    assert_eq!(m.records()[1].id, b);
    assert_eq!(m.records()[1].state, State::Active);
    m.stop(a).unwrap(); // ownership consumed, harmless repeated stop
    drop(m);
    assert!(sentinel.try_wait().unwrap().is_none());
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}
#[test]
fn bind_error_and_bounded_diagnostics() {
    let (_dir, mut m, snapshot) =
        fixture("head -c 300000 /dev/zero >&2; echo 'bind: Address already in use' >&2; exit 255");
    m.start(snapshot).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while m.has_active() && Instant::now() < deadline {
        m.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    let r = &m.records()[0];
    assert_eq!(r.state, State::Failed);
    assert_eq!(r.exit_code, Some(255));
    assert!(r.diagnostic.contains("Address already in use"));
    assert!(r.diagnostic.len() <= 32768);
}
#[test]
fn tunnel_flags_stdin_and_policy() {
    let (dir, mut m, snapshot) =
        fixture("printf '%s\\n' \"$@\" > \"$0.args\"; if read line; then exit 19; fi; exit 0");
    m.start(snapshot).unwrap();
    for _ in 0..100 {
        m.poll();
        if !m.has_active() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let args = fs::read_to_string(dir.path().join("ssh.args")).unwrap();
    for arg in [
        "-N",
        "-T",
        "ExitOnForwardFailure=yes",
        "BatchMode=yes",
        "RemoteCommand=none",
        "RequestTTY=no",
    ] {
        assert!(args.lines().any(|a| a == arg), "{args}");
    }
    assert!(!args.contains("StrictHostKeyChecking"));
    assert!(!args.contains("UserKnownHostsFile"));
    assert_eq!(m.records()[0].state, State::Completed);
}
#[test]
fn modes_atomic_legacy_and_unknown_siblings() {
    use ssho::storage::tunnels::{LaunchMode, TunnelProfiles};
    let (dir, _m, snapshot) = fixture("exit 0");
    let mut store = TunnelProfiles::load(dir.path(), &snapshot.config).unwrap();
    store
        .save("test", None, "legacy", &snapshot.forward)
        .unwrap();
    assert_eq!(store.modes()["test"]["legacy"], LaunchMode::Shell);
    let path = dir.path().join("tunnel_profiles.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    // Profiles are stored under the canonical path of the configuration, which
    // differs from the given one when the temporary directory is behind a link.
    let scope = document["configs"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    document["configs"][&scope]["hosts"]["test"]["profiles"]["legacy"]["future"] =
        serde_json::json!({"preserve":true});
    fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    store
        .save_with_mode(
            "test",
            Some("legacy"),
            "renamed",
            &snapshot.forward,
            LaunchMode::TunnelOnly,
        )
        .unwrap();
    let loaded = TunnelProfiles::load(dir.path(), &snapshot.config).unwrap();
    assert_eq!(loaded.modes()["test"]["renamed"], LaunchMode::TunnelOnly);
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        document["configs"][&scope]["hosts"]["test"]["profiles"]["renamed"]["future"]["preserve"],
        true
    );
    let before = fs::read(dir.path().join("tunnel_profiles.json")).unwrap();
    assert!(store
        .save_with_mode(
            "test",
            None,
            "renamed",
            &snapshot.forward,
            LaunchMode::Shell
        )
        .is_err());
    assert_eq!(
        before,
        fs::read(dir.path().join("tunnel_profiles.json")).unwrap()
    );
    assert_eq!(store.modes()["test"]["renamed"], LaunchMode::TunnelOnly);
    let external = dir.path().join("external-profile");
    fs::rename(&path, &external).unwrap();
    std::os::unix::fs::symlink(&external, &path).unwrap();
    let mut changed = snapshot.forward.clone();
    changed.local_port = "1081".into();
    assert!(store
        .save_with_mode(
            "test",
            Some("renamed"),
            "renamed",
            &changed,
            LaunchMode::Shell
        )
        .is_err());
    assert_eq!(store.modes()["test"]["renamed"], LaunchMode::TunnelOnly);
    assert_eq!(store.hosts()["test"]["renamed"], snapshot.forward);
    assert_eq!(fs::read(external).unwrap(), before);
}

#[cfg(target_os = "linux")]
fn alive(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
        !matches!(
            s.rsplit_once(") ").unwrap().1.chars().next(),
            Some('Z' | 'X')
        )
    })
}
#[cfg(target_os = "linux")]
#[test]
fn descendants_stopped_on_stop_drop_and_normal_failure() {
    for ending in ["stop", "drop", "exit"] {
        let script = if ending == "exit" {
            "sleep 60 & echo $! > \"$0.pid\"; exit 37"
        } else {
            "sleep 60 & echo $! > \"$0.pid\"; wait"
        };
        let (dir, mut m, snapshot) = fixture(script);
        let id = m.start(snapshot).unwrap();
        let path = dir.path().join("ssh.pid");
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let pid: i32 = fs::read_to_string(path).unwrap().trim().parse().unwrap();
        match ending {
            "stop" => m.stop(id).unwrap(),
            "exit" => {
                for _ in 0..100 {
                    m.poll();
                    if !m.has_active() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                assert_eq!(m.records()[0].exit_code, Some(37));
            }
            _ => {}
        }
        drop(m);
        for _ in 0..600 {
            if !alive(pid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!alive(pid), "descendant {pid} survived {ending}");
    }
}
#[test]
fn continuous_stderr_poll_and_stop_are_bounded() {
    let (_dir, mut m, snapshot) = fixture("while :; do head -c 65536 /dev/zero >&2; done");
    let id = m.start(snapshot).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let start = Instant::now();
    for _ in 0..100 {
        m.poll();
    }
    m.stop(id).unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(m.records()[0].diagnostic.len() <= 32768);
}
#[test]
fn spawn_error_retained_without_external_pid() {
    let (_dir, _m, snapshot) = fixture("exit 0");
    let mut manager = Manager::with_program(snapshot.config.with_file_name("missing-ssh"));
    assert!(manager.start(snapshot).is_err());
    assert!(!manager.has_active());
    assert_eq!(manager.records()[0].state, State::Failed);
    assert!(manager.records()[0]
        .diagnostic
        .contains("launch tunnel SSH"));
    manager.stop(99999).unwrap();
}
#[test]
fn diagnostics_drain_during_foreground_handoff_without_ui_polling() {
    let (dir, mut manager, snapshot) =
        fixture("head -c 1000000 /dev/zero >&2; echo reached > \"$0.done\"; sleep 60");
    manager.start(snapshot).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    let marker = dir.path().join("ssh.done");
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        marker.exists(),
        "stderr pipe blocked while UI was handed off to a shell"
    );
}

#[test]
fn fast_failure_always_keeps_final_stderr() {
    let (_dir, mut manager, snapshot) = fixture("echo short-bind-failure >&2; exit 255");
    for _ in 0..20 {
        manager.start(snapshot.clone()).unwrap();
        for _ in 0..100 {
            manager.poll();
            if !manager.has_active() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let record = manager.records().last().unwrap();
        assert_eq!(record.state, State::Failed);
        assert!(record.diagnostic.contains("short-bind-failure"));
    }
}
#[cfg(target_os = "linux")]
#[test]
fn capture_cancel_does_not_wait_for_escaped_descriptor_holder() {
    let (dir, mut manager, snapshot) = fixture("setsid sh -c 'echo $$ > \"$1\"; sleep 60' fixture \"$0.escaped\" &\nsleep 0.05; echo final-escaped-cause >&2; exit 37");
    manager.start(snapshot).unwrap();
    let path = dir.path().join("ssh.escaped");
    for _ in 0..100 {
        if path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid: i32 = fs::read_to_string(path).unwrap().trim().parse().unwrap();
    let started = Instant::now();
    while manager.has_active() && started.elapsed() < Duration::from_secs(2) {
        manager.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    // This deliberately detached fixture is no longer in SSH's private group.
    // Only the test that created it cleans it up; the manager never imports its PID.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!manager.has_active());
    assert!(manager.records()[0]
        .diagnostic
        .contains("final-escaped-cause"));
}
