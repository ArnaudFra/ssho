use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Host {
    pub name: String,
    pub hostname: String,
    pub user: String,
    pub port: String,
    pub identity: String,
    pub extra_identities: Vec<String>,
    pub proxy_jump: String,
    pub proxy_command: String,
    pub options: String,
    pub remote_command: String,
    pub request_tty: String,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub source_file: PathBuf,
    pub line_number: usize,
}

impl Host {
    pub fn hidden(&self) -> bool {
        self.tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("hidden"))
    }
    pub fn matches(&self, query: &str) -> bool {
        query.split_whitespace().all(|word| {
            let word = word.to_lowercase();
            [&self.name, &self.hostname, &self.user]
                .into_iter()
                .chain(self.tags.iter())
                .any(|value| value.to_lowercase().contains(&word))
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Forward {
    #[serde(rename = "type")]
    pub kind: String,
    pub local_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub bind_address: String,
}

impl Forward {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            ["local", "remote", "dynamic"].contains(&self.kind.as_str()),
            "invalid forwarding type"
        );
        anyhow::ensure!(
            self.local_port.parse::<u16>().is_ok_and(|p| p > 0),
            "port must be between 1 and 65535"
        );
        if self.kind != "dynamic" {
            anyhow::ensure!(
                self.remote_port.parse::<u16>().is_ok_and(|p| p > 0),
                "destination port must be between 1 and 65535"
            );
            anyhow::ensure!(
                !self
                    .remote_host
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control()),
                "invalid destination host"
            );
        }
        anyhow::ensure!(
            !self
                .bind_address
                .chars()
                .any(|c| c.is_whitespace() || c.is_control()),
            "invalid bind address"
        );
        Ok(())
    }
    pub fn ssh_args(&self) -> Result<Vec<String>> {
        self.validate()?;
        let bracket = |s: &str| {
            if s.contains(':') && !s.starts_with('[') {
                format!("[{s}]")
            } else {
                s.to_owned()
            }
        };
        let mut specification = String::new();
        if !self.bind_address.is_empty() {
            specification.push_str(&bracket(&self.bind_address));
            specification.push(':');
        }
        specification.push_str(&self.local_port);
        if self.kind != "dynamic" {
            specification.push(':');
            specification.push_str(&bracket(if self.remote_host.is_empty() {
                "localhost"
            } else {
                &self.remote_host
            }));
            specification.push(':');
            specification.push_str(&self.remote_port);
        }
        let flag = match self.kind.as_str() {
            "local" => "-L",
            "remote" => "-R",
            _ => "-D",
        };
        Ok(vec![flag.into(), specification])
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub host_name: String,
    pub last_connect: DateTime<Utc>,
    pub connect_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_result: Option<ConnectionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_completion: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub port_forwarding: Option<Forward>,
}

/// Process completion is not a claim about authentication or tunnel establishment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectionResult {
    pub completed_at: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub launch_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_all_words_across_user_name_and_tags() {
        let host = Host {
            name: "prod-db".into(),
            hostname: "db.example.org".into(),
            user: "deploy".into(),
            tags: vec!["Work".into()],
            ..Host::default()
        };
        assert!(host.matches("PROD work deploy"));
        assert!(!host.matches("prod staging"));
        assert!(host.matches(""));
    }

    #[test]
    fn hidden_tag_is_case_insensitive_and_exact() {
        assert!(Host {
            tags: vec!["Hidden".into()],
            ..Host::default()
        }
        .hidden());
        assert!(!Host {
            tags: vec!["unhidden".into()],
            ..Host::default()
        }
        .hidden());
    }

    #[test]
    fn tunnels_validate_ports_and_bracket_ipv6() {
        let mut f = Forward {
            kind: "local".into(),
            local_port: "15432".into(),
            remote_host: "::1".into(),
            remote_port: "5432".into(),
            bind_address: "127.0.0.1".into(),
        };
        assert_eq!(f.ssh_args().unwrap(), ["-L", "127.0.0.1:15432:[::1]:5432"]);
        f.local_port = "0".into();
        assert!(f.validate().is_err());
        f.local_port = "70000".into();
        assert!(f.validate().is_err());
        f.local_port = "1080".into();
        f.kind = "dynamic".into();
        f.remote_host.clear();
        f.remote_port.clear();
        assert_eq!(f.ssh_args().unwrap(), ["-D", "127.0.0.1:1080"]);
    }
}
