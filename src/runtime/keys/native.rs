use super::*;
use std::ffi::OsString;
/// Deliberately inherits all three terminal streams; never captures passphrases.
pub fn run_native(program: &Path, args: &[OsString]) -> Result<()> {
    let status = Command::new(program).args(args).status().with_context(|| {
        format!(
            "{} unavailable; install native OpenSSH tooling",
            program.display()
        )
    })?;
    anyhow::ensure!(
        status.success(),
        "{} exited with {status}",
        program.display()
    );
    Ok(())
}
pub fn generate(destination: &Path, comment: &str, keygen: &Path) -> Result<()> {
    anyhow::ensure!(
        destination.is_absolute(),
        "choose an absolute private key path"
    );
    anyhow::ensure!(
        !comment.chars().any(char::is_control),
        "comment contains control characters"
    );
    anyhow::ensure!(
        fs::symlink_metadata(destination).is_err()
            && fs::symlink_metadata(public_path(destination)).is_err(),
        "key destination already occupied (private or .pub)"
    );
    let parent = destination
        .parent()
        .context("key destination has no parent")?;
    let stage = tempfile::Builder::new()
        .prefix(".sshm-key-")
        .tempdir_in(parent)?;
    let private = stage.path().join("key");
    run_native(
        keygen,
        &[
            "-t".into(),
            "ed25519".into(),
            "-f".into(),
            private.as_os_str().into(),
            "-C".into(),
            comment.into(),
        ],
    )?;
    publish_pair(&private, destination)
}
/// Hard links are atomic create-if-absent on the destination filesystem.
/// After private publication, that pathname belongs to the user. Never unlink it:
/// another writer may replace it between any ownership check and cleanup.
pub(super) fn publish_pair(staged: &Path, destination: &Path) -> Result<()> {
    let source = fs::symlink_metadata(staged)?;
    let public = public_path(staged);
    anyhow::ensure!(
        source.is_file() && fs::symlink_metadata(&public)?.is_file(),
        "native key generation did not produce a regular pair"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(staged, fs::Permissions::from_mode(0o600))?;
    }
    fs::hard_link(staged, destination)
        .context("private destination occupied or publication failed")?;
    let public_destination = public_path(destination);
    fs::hard_link(&public, &public_destination).with_context(|| {
        format!(
            "public key publication failed at {}; private destination {} was published and is left untouched. Inspect both paths before retrying; recover the public with native ssh-keygen -y if needed, or choose a different destination",
            public_destination.display(), destination.display()
        )
    })?;
    Ok(())
}
pub struct Deployment {
    _stage: tempfile::TempDir,
    args: Vec<String>,
}
impl Deployment {
    pub fn prepare(reviewed: &PublicKey, private: &Path, root: &Path, alias: &str) -> Result<Self> {
        anyhow::ensure!(
            !alias.is_empty()
                && !alias.starts_with('-')
                && !alias.chars().any(|c| c.is_whitespace() || c.is_control()),
            "invalid exact SSH alias"
        );
        anyhow::ensure!(
            read_public(&reviewed.path)? == reviewed.bytes,
            "public key changed after review; refresh and review again"
        );
        anyhow::ensure!(private.is_file(),"private sibling missing: ssh-copy-id duplicate checking requires it; no forced deployment");
        #[cfg(not(unix))]
        {
            anyhow::bail!("safe ssh-copy-id staging requires Unix symlinks");
        }
        #[cfg(unix)]
        {
            // ssh-copy-id evaluates shell text containing -i. A fixed safe staging
            // parent and basename avoid exposing arbitrary original filenames to eval.
            let stage = tempfile::Builder::new()
                .prefix("sshm-public-")
                .tempdir_in("/tmp")?;
            let public = stage.path().join("key.pub");
            fs::write(&public, &reviewed.bytes)?;
            std::os::unix::fs::symlink(fs::canonicalize(private)?, stage.path().join("key"))?;
            let args = vec![
                "-i".into(),
                public.to_string_lossy().into_owned(),
                "-F".into(),
                root.to_string_lossy().into_owned(),
                alias.into(),
            ];
            Ok(Self {
                _stage: stage,
                args,
            })
        }
    }
    pub fn arguments(&self) -> &[String] {
        &self.args
    }
    pub fn run(&self, program: &Path) -> Result<()> {
        run_native(
            program,
            &self.args.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    }
}
pub fn agent_add(private: &Path) -> Result<()> {
    anyhow::ensure!(
        std::env::var_os("SSH_AUTH_SOCK").is_some_and(|s| !s.is_empty()),
        "no existing ssh-agent (SSH_AUTH_SOCK absent); start your agent outside ssho"
    );
    anyhow::ensure!(
        private.is_absolute() && private.is_file(),
        "select an existing absolute private key path"
    );
    run_native(Path::new("ssh-add"), &[private.as_os_str().into()])
}
