//! How a person installs what the platform needs, in the words of the
//! official documentation — never run by the platform, only shown to copy
//! (16 — The setup gate). Every command and URL here was read from the
//! project's own site on 2026-09-22: git-scm.com/install, code.claude.com
//! (Claude Code), the openai/codex repository README and
//! developers.openai.com/codex (Codex), opencode.ai/docs (OpenCode) — and on
//! 2026-09-30: the github/copilot-cli repository README and
//! docs.github.com/en/copilot/reference/copilot-cli-reference (GitHub Copilot
//! CLI), docs.x.ai/build and the xai-org/grok-build user guide (Grok Build).

use serde::{Deserialize, Serialize};

/// Where a command runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "InstallPlatform")]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
}

impl Platform {
    pub const ALL: [Platform; 3] = [Platform::MacOs, Platform::Linux, Platform::Windows];
}

/// One install line for one platform. Written once, here, and only ever
/// serialised — the words are the documentation's, never read back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct PlatformCommand {
    pub platform: Platform,
    pub command: &'static str,
}

/// How to install one thing: the official page, the install lines per
/// platform, how to check it landed, and how to sign in when the tool needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct InstallHint {
    pub url: &'static str,
    pub commands: &'static [PlatformCommand],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<&'static str>,
}

const fn on(platform: Platform, command: &'static str) -> PlatformCommand {
    PlatformCommand { platform, command }
}

/// git — https://git-scm.com/install
pub const GIT: InstallHint = InstallHint {
    url: "https://git-scm.com/install",
    commands: &[
        on(Platform::MacOs, "brew install git"),
        on(Platform::MacOs, "xcode-select --install"),
        on(Platform::Linux, "sudo apt-get install git"),
        on(Platform::Linux, "sudo dnf install git"),
        on(Platform::Linux, "sudo pacman -S git"),
        on(Platform::Linux, "sudo zypper install git"),
        on(Platform::Linux, "apk add git"),
        on(
            Platform::Windows,
            "winget install --id Git.Git -e --source winget",
        ),
    ],
    verify: Some("git --version"),
    sign_in: None,
};

/// Claude Code — https://code.claude.com/docs/en/setup
pub const CLAUDE_CODE: InstallHint = InstallHint {
    url: "https://code.claude.com/docs/en/setup",
    commands: &[
        on(
            Platform::MacOs,
            "curl -fsSL https://claude.ai/install.sh | bash",
        ),
        on(Platform::MacOs, "brew install --cask claude-code"),
        on(
            Platform::Linux,
            "curl -fsSL https://claude.ai/install.sh | bash",
        ),
        on(Platform::Linux, "npm install -g @anthropic-ai/claude-code"),
        on(Platform::Windows, "irm https://claude.ai/install.ps1 | iex"),
        on(Platform::Windows, "winget install Anthropic.ClaudeCode"),
    ],
    verify: Some("claude --version"),
    sign_in: Some("run `claude` once and follow the browser prompt to log in"),
};

/// Codex CLI — https://developers.openai.com/codex (install lines from the
/// openai/codex repository README)
pub const CODEX: InstallHint = InstallHint {
    url: "https://developers.openai.com/codex",
    commands: &[
        on(Platform::MacOs, "brew install --cask codex"),
        on(Platform::MacOs, "npm install -g @openai/codex"),
        on(Platform::Linux, "npm install -g @openai/codex"),
        on(Platform::Windows, "npm install -g @openai/codex"),
    ],
    verify: Some("codex --version"),
    sign_in: Some("run `codex` and choose Sign in with ChatGPT"),
};

/// OpenCode — https://opencode.ai/docs
pub const OPENCODE: InstallHint = InstallHint {
    url: "https://opencode.ai/docs",
    commands: &[
        on(
            Platform::MacOs,
            "curl -fsSL https://opencode.ai/install | bash",
        ),
        on(Platform::MacOs, "brew install anomalyco/tap/opencode"),
        on(
            Platform::Linux,
            "curl -fsSL https://opencode.ai/install | bash",
        ),
        on(Platform::Linux, "npm install -g opencode-ai"),
        on(Platform::Windows, "npm install -g opencode-ai"),
    ],
    verify: Some("opencode --version"),
    sign_in: Some("run `opencode`, then `/connect` to sign in to a provider"),
};

/// GitHub Copilot CLI — https://github.com/github/copilot-cli (the install
/// lines, in the README's order; `copilot login` and the order its tokens
/// are read in from the CLI command reference).
pub const COPILOT: InstallHint = InstallHint {
    url: "https://github.com/github/copilot-cli",
    commands: &[
        on(
            Platform::MacOs,
            "curl -fsSL https://gh.io/copilot-install | bash",
        ),
        on(Platform::MacOs, "brew install copilot-cli"),
        on(Platform::MacOs, "npm install -g @github/copilot"),
        on(
            Platform::Linux,
            "curl -fsSL https://gh.io/copilot-install | bash",
        ),
        on(Platform::Linux, "brew install copilot-cli"),
        on(Platform::Linux, "npm install -g @github/copilot"),
        on(Platform::Windows, "winget install GitHub.Copilot"),
        on(Platform::Windows, "npm install -g @github/copilot"),
    ],
    verify: Some("copilot --version"),
    sign_in: Some(
        "run `copilot login` — a COPILOT_GITHUB_TOKEN, GH_TOKEN or GITHUB_TOKEN in the environment is used before it",
    ),
};

/// Grok Build — https://docs.x.ai/build/overview
pub const GROK: InstallHint = InstallHint {
    url: "https://docs.x.ai/build/overview",
    commands: &[
        on(
            Platform::MacOs,
            "curl -fsSL https://x.ai/cli/install.sh | bash",
        ),
        on(
            Platform::Linux,
            "curl -fsSL https://x.ai/cli/install.sh | bash",
        ),
        on(Platform::Windows, "irm https://x.ai/cli/install.ps1 | iex"),
    ],
    verify: Some("grok --version"),
    sign_in: Some("run `grok login`, or set XAI_API_KEY"),
};

/// The hint for a compiled-in harness, by its id; `None` for one whose
/// official install page is not known to the platform.
pub fn install_hint(harness_id: &str) -> Option<InstallHint> {
    match harness_id {
        "claude-code" => Some(CLAUDE_CODE),
        "codex" => Some(CODEX),
        "opencode" => Some(OPENCODE),
        "copilot" => Some(COPILOT),
        "grok" => Some(GROK),
        _ => None,
    }
}

/// How git is installed.
pub fn git_hint() -> InstallHint {
    GIT
}

impl InstallHint {
    /// The lines for one platform, in the order the page gives them.
    pub fn commands_for(&self, platform: Platform) -> Vec<&'static str> {
        self.commands
            .iter()
            .filter(|c| c.platform == platform)
            .map(|c| c.command)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hint_names_its_official_page_and_a_line_for_every_platform() {
        for (name, hint) in [
            ("git", GIT),
            ("claude-code", CLAUDE_CODE),
            ("codex", CODEX),
            ("opencode", OPENCODE),
            ("copilot", COPILOT),
            ("grok", GROK),
        ] {
            assert!(hint.url.starts_with("https://"), "{name}");
            for platform in Platform::ALL {
                assert!(
                    !hint.commands_for(platform).is_empty(),
                    "{name} has no line for {platform:?}"
                );
            }
            assert!(hint.verify.is_some(), "{name} says how to check it landed");
        }
        assert!(GIT.sign_in.is_none(), "git has nobody to sign in to");
        assert!(CLAUDE_CODE.sign_in.is_some() && CODEX.sign_in.is_some());
        assert!(COPILOT.sign_in.is_some() && GROK.sign_in.is_some());
    }

    #[test]
    fn the_hinted_harnesses_are_the_five_with_a_known_page_and_the_rest_have_none() {
        assert_eq!(install_hint("claude-code"), Some(CLAUDE_CODE));
        assert_eq!(install_hint("codex"), Some(CODEX));
        assert_eq!(install_hint("opencode"), Some(OPENCODE));
        assert_eq!(install_hint("copilot"), Some(COPILOT));
        assert_eq!(install_hint("grok"), Some(GROK));
        for other in ["pi", "omp", "acp", "custom", "preset:goose", "nope"] {
            assert_eq!(install_hint(other), None, "{other}");
        }
        assert_eq!(
            CLAUDE_CODE.commands_for(Platform::Windows),
            [
                "irm https://claude.ai/install.ps1 | iex",
                "winget install Anthropic.ClaudeCode"
            ]
        );
    }

    #[test]
    fn the_two_acp_harnesses_are_installed_and_signed_in_as_their_pages_say() {
        assert_eq!(COPILOT.url, "https://github.com/github/copilot-cli");
        assert_eq!(
            COPILOT.commands_for(Platform::Windows),
            [
                "winget install GitHub.Copilot",
                "npm install -g @github/copilot"
            ]
        );
        assert_eq!(COPILOT.verify, Some("copilot --version"));
        // A token in the environment outranks the sign-in: the line says so,
        // or a person who signed in wonders whose account is being used.
        let sign_in = COPILOT.sign_in.unwrap_or_default();
        assert!(sign_in.contains("`copilot login`") && sign_in.contains("GH_TOKEN"));

        assert_eq!(GROK.url, "https://docs.x.ai/build/overview");
        assert_eq!(
            GROK.commands_for(Platform::MacOs),
            ["curl -fsSL https://x.ai/cli/install.sh | bash"]
        );
        assert_eq!(
            GROK.commands_for(Platform::Windows),
            ["irm https://x.ai/cli/install.ps1 | iex"]
        );
        assert_eq!(GROK.verify, Some("grok --version"));
        assert!(GROK.sign_in.unwrap_or_default().contains("`grok login`"));
    }

    #[test]
    fn a_hint_serialises_with_its_platform_words() {
        let v = serde_json::to_value(GIT).unwrap();
        assert_eq!(v["url"], "https://git-scm.com/install");
        assert_eq!(v["commands"][0]["platform"], "mac_os");
        assert_eq!(v["verify"], "git --version");
        assert!(
            v.get("sign_in").is_none(),
            "nothing to sign in to is left out"
        );
    }
}
