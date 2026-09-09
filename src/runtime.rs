use crate::{
    storage::{AppPaths, History},
    types::Forward,
};
use anyhow::{Context, Result};
use std::{path::Path, process::Command};

pub fn connect(
    paths: &AppPaths,
    config: &Path,
    name: &str,
    tty: bool,
    remote: &[String],
    forward: Option<&Forward>,
) -> Result<i32> {
    crate::config::find_host(config, name)?;
    let arguments = ssh_arguments(config, name, tty, remote, forward)?;
    paths.initialize()?;
    if let Err(error) =
        History::load(&paths.root).and_then(|mut history| history.record(name, forward))
    {
        eprintln!("Warning: Could not record connection history: {error:#}");
    }
    if remote.is_empty() && forward.is_none() {
        eprintln!("Connecting to {name}...");
    }
    let status = Command::new("ssh")
        .args(arguments)
        .status()
        .context("Error executing SSH command; ensure OpenSSH is installed")?;
    Ok(exit_code(status))
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

pub fn ssh_arguments(
    config: &Path,
    name: &str,
    tty: bool,
    remote: &[String],
    forward: Option<&Forward>,
) -> Result<Vec<String>> {
    anyhow::ensure!(
        !name.starts_with('-') && !name.chars().any(char::is_control),
        "invalid SSH host name"
    );
    let mut arguments = vec!["-F".into(), config.to_string_lossy().into_owned()];
    if tty {
        arguments.push("-t".into())
    }
    if let Some(forward) = forward {
        arguments.extend(forward.ssh_args()?)
    }
    arguments.push(name.into());
    arguments.extend_from_slice(remote);
    Ok(arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_process_arguments_preserve_each_remote_argument() {
        let remote = vec!["ls".into(), "-la".into(), "/a b".into()];
        assert_eq!(
            ssh_arguments(Path::new("/tmp/a b"), "prod", true, &remote, None).unwrap(),
            ["-F", "/tmp/a b", "-t", "prod", "ls", "-la", "/a b"]
        );
    }
    #[test]
    fn forwarding_argument_precedes_host_and_never_requires_shell() {
        let f = Forward {
            kind: "dynamic".into(),
            local_port: "1080".into(),
            bind_address: "127.0.0.1".into(),
            ..Forward::default()
        };
        assert_eq!(
            ssh_arguments(Path::new("config"), "prod", false, &[], Some(&f)).unwrap(),
            ["-F", "config", "-D", "127.0.0.1:1080", "prod"]
        );
    }
}
