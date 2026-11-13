//! The application reads public files only. Private keys belong to native tools.
mod inventory;
mod native;
use crate::types::Host;
use anyhow::{Context, Result};
pub use inventory::{inventory, inventory_with_candidates, Entry, Scanner, INVENTORY_LIMIT};
pub use native::{agent_add, generate, run_native, Deployment};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
const PUBLIC_LIMIT: u64 = 64 * 1024;
#[derive(Clone, Debug)]
pub struct PublicKey {
    pub path: PathBuf,
    pub fingerprint: String,
    bytes: Vec<u8>,
}
pub fn public_path(private: &Path) -> PathBuf {
    let mut name = private.as_os_str().to_os_string();
    name.push(".pub");
    PathBuf::from(name)
}
fn read_public(path: &Path) -> Result<Vec<u8>> {
    anyhow::ensure!(
        path.extension().is_some_and(|s| s == "pub"),
        "public path must end in .pub"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("public file missing or inaccessible: {}", path.display()))?;
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= PUBLIC_LIMIT,
        "public file must be regular and at most 64 KiB"
    );
    let mut bytes = Vec::new();
    file.take(PUBLIC_LIMIT + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= PUBLIC_LIMIT,
        "public file exceeds 64 KiB"
    );
    anyhow::ensure!(
        bytes.starts_with(b"ssh-") || bytes.starts_with(b"ecdsa-") || bytes.starts_with(b"sk-"),
        "expected an OpenSSH public key (.pub), never a private key"
    );
    let text = std::str::from_utf8(&bytes).context("public key must be UTF-8 OpenSSH text")?;
    anyhow::ensure!(
        text.lines().filter(|line| !line.trim().is_empty()).count() == 1,
        "expected exactly one public key; split multi-key public files before review"
    );
    Ok(bytes)
}
pub fn review_public(path: &Path, keygen: &Path) -> Result<PublicKey> {
    let bytes = read_public(path)?;
    let stage = tempfile::tempdir()?;
    let snapshot = stage.path().join("key.pub");
    fs::write(&snapshot, &bytes)?;
    let mut command = Command::new(keygen);
    command
        .arg("-lf")
        .arg(&snapshot)
        .env_remove("DISPLAY")
        .env_remove("SSH_ASKPASS");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Native fingerprint output is file-backed in capture: cap disk writes as
        // well as retained output. This child only reads a public snapshot.
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: PUBLIC_LIMIT as libc::rlim_t,
                    rlim_max: PUBLIC_LIMIT as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let (status, out, err) = super::effective::capture(&mut command, Duration::from_secs(2))
        .context("ssh-keygen fingerprint failed (install OpenSSH or retry)")?;
    anyhow::ensure!(
        status.success(),
        "ssh-keygen fingerprint failed: {}",
        err.trim()
    );
    let fingerprint = out.lines().next().unwrap_or("").trim().to_string();
    anyhow::ensure!(
        !fingerprint.is_empty(),
        "ssh-keygen returned no fingerprint"
    );
    Ok(PublicKey {
        path: path.to_owned(),
        fingerprint,
        bytes,
    })
}
/// Add the selection as first identity, retaining every other declared identity.
pub fn attach(host: &Host, private: &Path) -> Host {
    let mut next = host.clone();
    next.identity = private.to_string_lossy().into_owned();
    next.extra_identities = std::iter::once(&host.identity)
        .chain(&host.extra_identities)
        .filter(|s| !s.is_empty() && **s != next.identity)
        .cloned()
        .collect();
    next
}
#[cfg(test)]
use native::publish_pair;
#[cfg(test)]
mod tests;
