#![cfg(unix)]
use ssho::{
    runtime::batch::{arguments, Event, Manager, Options, State, Target},
    types::Host,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};
static FIXTURES: std::sync::Mutex<()> = std::sync::Mutex::new(());
struct Fixture {
    dir: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}
fn fixture(script: &str, count: usize) -> (Fixture, Manager, Vec<Target>) {
    let lock = FIXTURES.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("native ssh");
    fs::write(&exe, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    let root = dir.path().join("config root");
    fs::write(
        &root,
        (0..count)
            .map(|i| format!("Host host{i}\n HostName invalid.test\n"))
            .collect::<String>(),
    )
    .unwrap();
    let targets = (0..count)
        .map(|i| {
            Target::freeze(
                &root,
                &Host {
                    name: format!("host{i}"),
                    source_file: root.clone(),
                    ..Host::default()
                },
            )
            .unwrap()
        })
        .collect();
    (
        Fixture { dir, _lock: lock },
        Manager::with_program(exe),
        targets,
    )
}
fn finish(m: &mut Manager) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut events = vec![];
    while m.has_active() {
        events.extend(m.poll());
        assert!(Instant::now() < deadline, "batch stuck");
        std::thread::sleep(Duration::from_millis(5));
    }
    events.extend(m.poll());
    events
}
#[test]
fn batch_native_arguments_and_frozen_deduplicated_targets() {
    let (_f, mut m, targets) = fixture("printf output; printf error >&2; exit 37", 1);
    let command = "printf '%s' 'literal $HOME ; `x`'";
    let args = arguments(&targets[0], command).unwrap();
    assert_eq!(&args[args.len() - 2..], ["host0", command]);
    for option in [
        "BatchMode=yes",
        "RequestTTY=no",
        "RemoteCommand=none",
        "ControlMaster=no",
        "ControlPath=none",
        "ForkAfterAuthentication=no",
        "SessionType=default",
        "ClearAllForwardings=yes",
    ] {
        assert!(args.iter().any(|a| a == option), "{option}: {args:?}");
    }
    let mut duplicate = targets.clone();
    duplicate.push(targets[0].clone());
    m.start(duplicate, command.into(), Options::default())
        .unwrap();
    assert!(m
        .start(targets, "other".into(), Options::default())
        .is_err());
    let events = finish(&mut m);
    assert_eq!(m.records().len(), 1);
    let r = &m.records()[0];
    assert_eq!(r.state, State::Failed);
    assert_eq!(r.exit_code, Some(37));
    assert_eq!(r.stdout.text(), "output");
    assert_eq!(r.stderr.text(), "error");
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Started { .. }))
            .count(),
        1
    );
    assert!(r.completed_at.unwrap() >= r.started_at.unwrap());
}
#[test]
fn batch_bounds_concurrency_and_drains_truncated_streams() {
    let (_f, mut m, targets) = fixture(
        "head -c 310000 /dev/zero; head -c 320000 /dev/zero >&2; sleep 0.05",
        7,
    );
    m.start(
        targets,
        "ignored".into(),
        Options {
            parallel: 2,
            timeout: Duration::from_secs(3),
        },
    )
    .unwrap();
    let mut max_active = 0;
    let deadline = Instant::now() + Duration::from_secs(6);
    while m.has_active() {
        m.poll();
        max_active = max_active.max(
            m.records()
                .iter()
                .filter(|r| r.state == State::Running)
                .count(),
        );
        assert!(max_active <= 2);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(max_active, 2);
    for r in m.records() {
        assert_eq!(r.state, State::Succeeded);
        assert_eq!(r.stdout.bytes.len(), 262144);
        assert_eq!(r.stderr.bytes.len(), 262144);
        assert!(r.stdout.truncated && r.stderr.truncated);
    }
}
#[test]
fn batch_timeout_cancel_never_launch_pending_and_cleans_descendants() {
    let (f, mut m, targets) = fixture(
        "sleep 60 & echo $! > \"$(dirname \"$0\")/descendant\"; wait",
        4,
    );
    m.start(
        targets,
        "ignored".into(),
        Options {
            parallel: 1,
            timeout: Duration::from_secs(1),
        },
    )
    .unwrap();
    while !f.dir.path().join("descendant").exists() {
        m.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid = fs::read_to_string(f.dir.path().join("descendant")).unwrap();
    m.cancel();
    let events = finish(&mut m);
    assert!(events.iter().all(|e| !matches!(e, Event::Started { .. })));
    assert!(m.records().iter().all(|r| r.state == State::Cancelled));
    assert!(m.records()[1..].iter().all(|r| r.started_at.is_none()));
    assert_stopped(pid.trim());
    let targets = m
        .records()
        .iter()
        .take(1)
        .map(|r| r.target.clone())
        .collect();
    m.start(
        targets,
        "timeout".into(),
        Options {
            parallel: 1,
            timeout: Duration::from_secs(1),
        },
    )
    .unwrap();
    finish(&mut m);
    assert_eq!(m.records()[0].state, State::TimedOut);
    assert_stopped(
        fs::read_to_string(f.dir.path().join("descendant"))
            .unwrap()
            .trim(),
    );
}
fn assert_stopped(pid: &str) {
    #[cfg(target_os = "linux")]
    {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let live = fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
                !matches!(
                    s.rsplit_once(") ").unwrap().1.chars().next(),
                    Some('Z' | 'X')
                )
            });
            if !live {
                break;
            }
            assert!(Instant::now() < deadline, "descendant {pid} survived");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
#[test]
fn batch_normal_exit_cleans_inherited_pipes_and_preserves_exit_code() {
    let (f, mut m, targets) = fixture(
        "sleep 60 & echo $! > \"$(dirname \"$0\")/descendant\"; echo done; exit 7",
        1,
    );
    m.start(targets, "ignored".into(), Options::default())
        .unwrap();
    finish(&mut m);
    assert_eq!(m.records()[0].exit_code, Some(7));
    assert_eq!(m.records()[0].stdout.text(), "done\n");
    assert_stopped(
        fs::read_to_string(f.dir.path().join("descendant"))
            .unwrap()
            .trim(),
    );
}
#[test]
fn batch_rejects_bad_options_alias_and_disappeared_target() {
    let (f, mut m, targets) = fixture("exit 0", 1);
    for (parallel, seconds) in [(0, 30), (17, 30), (4, 0), (4, 3601)] {
        assert!(m
            .start(
                targets.clone(),
                "cmd".into(),
                Options {
                    parallel,
                    timeout: Duration::from_secs(seconds)
                }
            )
            .is_err());
    }
    let mut bad = targets[0].clone();
    bad.alias = "-bad".into();
    assert!(arguments(&bad, "cmd").is_err());
    fs::write(f.dir.path().join("config root"), "Host other\n").unwrap();
    m.start(targets, "cmd".into(), Options::default()).unwrap();
    finish(&mut m);
    assert_eq!(m.records()[0].state, State::Failed);
    assert!(m.records()[0].diagnostic.contains("target"));
}

#[test]
fn batch_continuous_output_keeps_poll_bounded_and_drop_cleans_up() {
    let (f,mut m,targets)=fixture("sleep 60 & echo $! > \"$(dirname \"$0\")/descendant\"; while :; do head -c 65536 /dev/zero; head -c 65536 /dev/zero >&2; done",1);
    m.start(targets, "ignored".into(), Options::default())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while m.records()[0].stdout.bytes.len() < 262144 || m.records()[0].stderr.bytes.len() < 262144 {
        let tick = Instant::now();
        m.poll();
        assert!(
            tick.elapsed() < Duration::from_millis(500),
            "busy capture starved poll"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid = fs::read_to_string(f.dir.path().join("descendant")).unwrap();
    drop(m);
    assert_stopped(pid.trim());
}

#[test]
fn batch_invalid_targets_have_bounded_attempt_work_per_poll() {
    let (_f, mut m, targets) = fixture("exit 0", 1);
    let invalid = (0..100)
        .map(|i| {
            let mut t = targets[0].clone();
            t.alias = format!("absent{i}");
            t
        })
        .collect();
    m.start(invalid, "ignored".into(), Options::default())
        .unwrap();
    m.poll();
    assert!(
        m.records()
            .iter()
            .filter(|r| r.state == State::Failed)
            .count()
            <= 16
    );
    finish(&mut m);
    assert!(m.records().iter().all(|r| r.state == State::Failed));
}

#[test]
fn batch_rejects_same_alias_moved_into_include_without_native_or_started_event() {
    let (f, mut m, targets) = fixture("touch \"$(dirname \"$0\")/invoked\"", 1);
    let child = f.dir.path().join("included");
    fs::write(&child, "Host host0\n HostName invalid.test\n").unwrap();
    let root = f.dir.path().join("config root");
    fs::write(&root, format!("Include {}\n", child.display())).unwrap();
    assert_eq!(
        ssho::config::find_host(&root, "host0").unwrap().source_file,
        child
    );
    m.start(targets, "cmd".into(), Options::default()).unwrap();
    let events = finish(&mut m);
    assert_eq!(m.records()[0].state, State::Failed);
    assert!(m.records()[0].diagnostic.contains("source changed"));
    assert!(m.records()[0].started_at.is_none());
    assert!(!events
        .iter()
        .any(|event| matches!(event, Event::Started { .. })));
    assert!(!f.dir.path().join("invoked").exists());
}
