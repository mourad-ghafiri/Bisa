//! `ssh_config`, read: the `Host` blocks and what each sets. Pure — the text
//! is handed in; nothing here opens a file.

use std::path::Path;

/// One `Host` block: its patterns and the options under it, in file order.
/// Option names are lowercased the way OpenSSH reads them.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct HostBlock {
    pub patterns: Vec<String>,
    pub options: Vec<(String, String)>,
}

impl HostBlock {
    /// The first value of `name`, case-insensitively.
    pub fn option(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The host this block reaches: its `HostName`, else its first pattern.
    pub fn hostname(&self) -> Option<&str> {
        self.option("hostname")
            .or_else(|| self.patterns.first().map(String::as_str))
    }
}

/// The `Host` blocks of an `ssh_config` text. Options before the first
/// `Host` line belong to every host and are kept under a `*` block; `Match`
/// blocks are skipped — their conditions are not this crate's to evaluate.
pub fn parse_ssh_config(text: &str) -> Vec<HostBlock> {
    let mut blocks: Vec<HostBlock> = Vec::new();
    let mut current: Option<HostBlock> = None;
    let mut skipping_match = false;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = split_option(line);
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                skipping_match = false;
                current = Some(HostBlock {
                    patterns: value
                        .split_whitespace()
                        .map(|p| p.trim_matches('"').to_string())
                        .collect(),
                    options: Vec::new(),
                });
            }
            "match" => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
                skipping_match = true;
            }
            k => {
                if skipping_match {
                    continue;
                }
                let block = current.get_or_insert_with(|| HostBlock {
                    patterns: vec!["*".to_string()],
                    options: Vec::new(),
                });
                block
                    .options
                    .push((k.to_string(), value.trim_matches('"').to_string()));
            }
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

/// `Key value` or `Key=value`, as OpenSSH accepts both.
fn split_option(line: &str) -> (&str, &str) {
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let key = &line[..end];
    let value = line[end..]
        .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
        .trim();
    (key, value)
}

/// The blocks that concern git hosts: a pattern or a `HostName` that is one
/// of `hosts`, or a pattern that names one (`github-work` for `github.com`).
/// The `*` block is kept when it exists, since it applies to them all.
pub fn git_host_blocks(blocks: &[HostBlock], hosts: &[&str]) -> Vec<HostBlock> {
    let stems: Vec<String> = hosts
        .iter()
        .map(|h| h.split('.').next().unwrap_or(h).to_ascii_lowercase())
        .collect();
    let concerns_git = |name: &str| {
        let n = name.to_ascii_lowercase();
        hosts.iter().any(|h| n.eq_ignore_ascii_case(h))
            || stems.iter().any(|s| n.contains(s.as_str()))
    };
    blocks
        .iter()
        .filter(|b| {
            b.patterns.iter().any(|p| p == "*")
                || b.patterns.iter().any(|p| concerns_git(p))
                || b.option("hostname").is_some_and(concerns_git)
        })
        .cloned()
        .collect()
}

/// The `Host` block a person pastes into their own `ssh_config` to bind
/// `alias` to `hostname` with `key` — the platform never writes it for them.
pub fn host_block_text(alias: &str, hostname: &str, user: &str, key: Option<&Path>) -> String {
    let mut text = format!("Host {alias}\n    HostName {hostname}\n    User {user}\n");
    if let Some(key) = key {
        text.push_str(&format!(
            "    IdentityFile {}\n    IdentitiesOnly yes\n",
            key.display()
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
# personal
AddKeysToAgent yes

Host github.com
    User git
    IdentityFile ~/.ssh/id_ed25519

Host github-acme
  HostName github.com
  User git
  IdentityFile=~/.ssh/id_ed25519_acme
  IdentitiesOnly yes

Match host prod.example
  User deploy

Host server-one prod.example
  Port 2222
"#;

    #[test]
    fn host_blocks_are_read_with_their_options_and_match_blocks_are_skipped() {
        let blocks = parse_ssh_config(CONFIG);
        assert_eq!(blocks.len(), 4);
        assert_eq!(
            blocks[0].patterns,
            ["*"],
            "options before the first Host apply to every host"
        );
        assert_eq!(blocks[0].option("addkeystoagent"), Some("yes"));
        assert_eq!(blocks[1].patterns, ["github.com"]);
        assert_eq!(
            blocks[1].option("IdentityFile"),
            Some("~/.ssh/id_ed25519"),
            "case-insensitive"
        );
        assert_eq!(
            blocks[2].hostname(),
            Some("github.com"),
            "HostName wins over the alias"
        );
        assert_eq!(
            blocks[2].option("identityfile"),
            Some("~/.ssh/id_ed25519_acme"),
            "Key=value is read too"
        );
        assert_eq!(blocks[3].patterns, ["server-one", "prod.example"]);
        assert_eq!(
            blocks[3].option("user"),
            None,
            "the Match block's option was skipped"
        );
        assert_eq!(blocks[1].hostname(), Some("github.com"));
    }

    #[test]
    fn the_git_hosts_blocks_are_the_ones_that_name_a_git_host_or_apply_to_all() {
        let blocks = parse_ssh_config(CONFIG);
        let git = git_host_blocks(&blocks, crate::KNOWN_GIT_HOSTS);
        let patterns: Vec<&str> = git.iter().map(|b| b.patterns[0].as_str()).collect();
        assert_eq!(patterns, ["*", "github.com", "github-acme"]);
    }

    #[test]
    fn a_host_block_text_is_what_a_person_pastes() {
        let text = host_block_text(
            "github-acme",
            "github.com",
            "git",
            Some(Path::new("/Users/ada/.ssh/id_ed25519_acme")),
        );
        assert_eq!(
            text,
            "Host github-acme\n    HostName github.com\n    User git\n    IdentityFile /Users/ada/.ssh/id_ed25519_acme\n    IdentitiesOnly yes\n"
        );
        assert!(!host_block_text("gh", "github.com", "git", None).contains("IdentityFile"));
        let back = parse_ssh_config(&text);
        assert_eq!(
            back[0].hostname(),
            Some("github.com"),
            "the text parses back"
        );
    }
}
