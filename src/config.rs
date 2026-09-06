//! SSH configuration reading, adapted from SSHM (MIT).
//! The list follows SSHM's display rules rather than OpenSSH's effective lookup:
//! scalar directives use their last value, wildcards are omitted, Match bodies
//! are skipped, and each concrete alias produces a separate list entry.
use crate::types::Host;
use anyhow::Result;
use std::path::{Path, PathBuf};

mod parser;
use parser::Parsed;

pub fn load_hosts(path: &Path) -> Result<Vec<Host>> {
    Ok(Parsed::load(path)?.hosts)
}

pub fn find_host(path: &Path, name: &str) -> Result<Host> {
    load_hosts(path)?
        .into_iter()
        .find(|h| h.name == name)
        .ok_or_else(|| anyhow::anyhow!("host '{name}' not found in any configuration file"))
}

/// Main file first, then included files in first-encounter, sorted-glob order.
pub fn config_files(path: &Path) -> Result<Vec<PathBuf>> {
    Ok(Parsed::load(path)?.files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn fixture(text: &str) -> (TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        let app = dir.path().join("app");
        fs::write(&path, text).unwrap();
        (dir, path, app)
    }

    #[test]
    fn parser_keeps_aliases_tags_negations_match_and_repeated_options() {
        let (_dir, path, _) = fixture("# Tags: prod, hidden\nHost=web alias !old web*\n HostName first\n HostName=final.example\n Port 2200\n IdentityFile '/a b'\n IdentityFile C:\\Users\\me\\id\n ProxyJump \"jump\" # note\n ProxyCommand ssh -W \"%h:%p\" jump # literal\n RemoteCommand echo '#hello'\n SetEnv FOO=\"a  b\"\n SetEnv BAR=b\nMatch host web\n User skipped\nHost next\n User actual\n");
        let hosts = load_hosts(&path).unwrap();
        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["web", "alias", "next"]
        );
        let h = &hosts[0];
        assert_eq!(h.hostname, "final.example");
        assert_eq!(h.aliases, ["web", "alias"]);
        assert_eq!(h.tags, ["prod", "hidden"]);
        assert_eq!(h.identity, "/a b");
        assert_eq!(h.extra_identities, [r"C:\Users\me\id"]);
        assert_eq!(h.proxy_jump, "\"jump\"");
        assert_eq!(h.proxy_command, "ssh -W \"%h:%p\" jump # literal");
        assert_eq!(h.remote_command, "echo '#hello'");
        assert_eq!(h.options, "SetEnv FOO=\"a  b\"\nSetEnv BAR=b");
        assert_eq!(h.user, "");
        assert_eq!(h.line_number, 2);
        assert_eq!(hosts[2].port, "22");
    }

    #[test]
    fn includes_relative_globs_cycles_and_non_config_exclusion() {
        let (dir, path, _) =
            fixture("Include \"parts/*.conf\" parts/nested\nHost root\n HostName root.example\n");
        let parts = dir.path().join("parts");
        fs::create_dir(&parts).unwrap();
        fs::write(
            parts.join("a.conf"),
            "Include ../config\nHost a\n HostName a.example\n",
        )
        .unwrap();
        fs::write(parts.join("b.conf"), "Host b\n HostName b.example\n").unwrap();
        fs::write(
            parts.join("nested"),
            "Include *\nHost nested\n HostName nested.example\n",
        )
        .unwrap();
        fs::write(parts.join("README.md"), "Host excluded\n").unwrap();
        fs::write(parts.join("bad.backup"), "Host excluded\n").unwrap();
        let hosts = load_hosts(&path).unwrap();
        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "nested", "root"]
        );
        let files = config_files(&path).unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0], path);
        assert_eq!(files[1], parts.join("a.conf"));
    }
}
