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
}
