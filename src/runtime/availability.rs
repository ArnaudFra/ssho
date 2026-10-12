//! Local native ICMP probes. Destination is the effective SSH Hostname, never a proxy.
use super::*;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug)]
enum Platform {
    Linux,
    Macos,
    Freebsd,
    Unsupported,
}
impl Platform {
    fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "freebsd") {
            Self::Freebsd
        } else {
            Self::Unsupported
        }
    }
}
fn native_command(platform: Platform, hostname: &str) -> Result<Command> {
    anyhow::ensure!(
        !hostname.is_empty()
            && !hostname.starts_with('-')
            && !hostname
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
            && !hostname.contains(['[', ']']),
        "invalid ICMP hostname"
    );
    let mut command = Command::new(
        if matches!(platform, Platform::Macos) && hostname.contains(':') {
            "ping6"
        } else {
            "ping"
        },
    );
    command.env("LC_ALL", "C").args(["-n", "-c", "1"]);
    match platform {
        Platform::Linux => {
            command.args(["-W", "4", "-w", "4"]);
        }
        Platform::Macos if hostname.contains(':') => {} // ping6 -W/-t are NOT timeout options.
        Platform::Macos | Platform::Freebsd => {
            command.args(["-W", "4000", "-t", "4"]);
        }
        Platform::Unsupported => anyhow::bail!(
            "ICMP unsupported on this platform: native ping reply status is not verified"
        ),
    }
    command.arg(hostname);
    Ok(command)
}
fn run(
    command: &mut Command,
    platform: Platform,
    timeout: Duration,
    cancel: &AtomicBool,
) -> (PingStatus, Option<String>) {
    if matches!(platform, Platform::Unsupported) {
        return (
            PingStatus::Unknown,
            Some("ICMP unsupported: native reply status is not verified".into()),
        );
    }
    match effective::capture_controlled(command, timeout, Some(cancel)) {
        Ok((status, _, stderr)) => {
            let code = status.code();
            if code == Some(0) {
                return (PingStatus::Online, None);
            }
            let no_reply = match platform {
                Platform::Linux => 1,
                _ => 2,
            };
            let (status, cause) = if code == Some(no_reply) {
                (PingStatus::Offline, "no ICMP echo reply")
            } else {
                (
                    PingStatus::Unknown,
                    "native ping failed (tool, permission or network error)",
                )
            };
            (
                status,
                Some(format!("{cause}; exit {code:?}: {}", stderr.trim())),
            )
        }
        Err(error) => (PingStatus::Unknown, Some(format!("ICMP local: {error:#}"))),
    }
}
pub(super) fn probe(host: &Host, timeout: Duration, cancel: &AtomicBool) -> PingResult {
    let started = Instant::now();
    let platform = Platform::current();
    let (status, error) = match native_command(platform, &host.hostname) {
        Ok(mut command) => run(&mut command, platform, timeout, cancel),
        Err(error) => (PingStatus::Unknown, Some(format!("{error:#}"))),
    };
    PingResult {
        name: host.name.clone(),
        method: crate::storage::AvailabilityMethod::Icmp,
        status,
        duration: started.elapsed(),
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn availability_arguments_validate_destinations_and_platform_flags() {
        for host in ["example.test", "127.0.0.1", "::1", "fe80::1%lo0"] {
            let linux = native_command(Platform::Linux, host).unwrap();
            assert_eq!(linux.get_program(), "ping");
            assert_eq!(
                linux
                    .get_args()
                    .map(|a| a.to_str().unwrap())
                    .collect::<Vec<_>>(),
                ["-n", "-c", "1", "-W", "4", "-w", "4", host]
            );
            let bsd = native_command(Platform::Freebsd, host).unwrap();
            assert_eq!(
                bsd.get_args()
                    .map(|a| a.to_str().unwrap())
                    .collect::<Vec<_>>(),
                ["-n", "-c", "1", "-W", "4000", "-t", "4", host]
            );
        }
        for host in ["", "-help", "a b", "a\nb", "a\0b", " a", "[::1]"] {
            assert!(native_command(Platform::Linux, host).is_err(), "{host:?}");
        }
        let mac4 = native_command(Platform::Macos, "example.test").unwrap();
        assert_eq!(mac4.get_program(), "ping");
        assert_eq!(
            mac4.get_args()
                .map(|a| a.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["-n", "-c", "1", "-W", "4000", "-t", "4", "example.test"]
        );
        let mac6 = native_command(Platform::Macos, "::1").unwrap();
        assert_eq!(mac6.get_program(), "ping6");
        assert_eq!(
            mac6.get_args()
                .map(|a| a.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["-n", "-c", "1", "::1"]
        );
        assert!(native_command(Platform::Unsupported, "::1")
            .unwrap_err()
            .to_string()
            .contains("unsupported"));
    }
    #[cfg(unix)]
    #[test]
    fn availability_fake_ping_maps_reply_no_reply_and_errors_honestly() {
        for (platform, code, expected) in [
            (Platform::Linux, 0, PingStatus::Online),
            (Platform::Linux, 1, PingStatus::Offline),
            (Platform::Linux, 2, PingStatus::Unknown),
            (Platform::Macos, 2, PingStatus::Offline),
            (Platform::Freebsd, 2, PingStatus::Offline),
            (Platform::Macos, 1, PingStatus::Unknown),
            (Platform::Unsupported, 0, PingStatus::Unknown),
        ] {
            let mut command = Command::new("sh");
            command.args(["-c", &format!("echo fixture-cause >&2; exit {code}")]);
            let (status, cause) = run(
                &mut command,
                platform,
                Duration::from_secs(1),
                &AtomicBool::new(false),
            );
            assert_eq!(status, expected, "{platform:?} exit {code}");
            if expected != PingStatus::Online {
                assert!(
                    cause.unwrap().contains("fixture-cause")
                        || matches!(platform, Platform::Unsupported)
                );
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn availability_missing_denied_timeout_cancel_and_output_are_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-ping");
        let (status, error) = run(
            &mut Command::new(&missing),
            Platform::Linux,
            Duration::from_secs(1),
            &AtomicBool::new(false),
        );
        assert_eq!(status, PingStatus::Unknown);
        assert!(error.unwrap().contains("No such file"));
        std::fs::write(&missing, "#!/bin/sh\nexit 0\n").unwrap();
        let (_, error) = run(
            &mut Command::new(&missing),
            Platform::Linux,
            Duration::from_secs(1),
            &AtomicBool::new(false),
        );
        assert!(error.unwrap().contains("Permission denied"));
        let mut command = Command::new("sh");
        command.args(["-c", "echo stuck >&2; sleep 30 & wait"]);
        let started = Instant::now();
        let (status, error) = run(
            &mut command,
            Platform::Linux,
            Duration::from_millis(100),
            &AtomicBool::new(false),
        );
        assert_eq!(status, PingStatus::Unknown);
        assert!(error.unwrap().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        let (_, error) = run(
            &mut command,
            Platform::Linux,
            Duration::from_secs(5),
            &AtomicBool::new(true),
        );
        assert!(error.unwrap().contains("cancelled"));
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "head -c 1000000 /dev/zero; head -c 1000000 /dev/zero >&2",
        ]);
        let (status, error) = run(
            &mut command,
            Platform::Linux,
            Duration::from_secs(1),
            &AtomicBool::new(false),
        );
        assert_eq!(status, PingStatus::Unknown);
        assert!(error.unwrap().contains("output limit"));
    }
}
