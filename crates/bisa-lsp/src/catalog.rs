//! Which server for which language (ide/10): compiled-in presets probed on
//! the login shell's `PATH`, then user descriptors from `lsp.servers`. The
//! platform never installs a server; a missing one shows with its hint.

use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerDescriptor {
    pub language: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<serde_json::Value>,
    /// Where to get it, shown when the command is not on `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
}

fn preset(language: &str, command: &str, args: &[&str], hint: &str) -> ServerDescriptor {
    ServerDescriptor {
        language: language.into(),
        command: command.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        initialization_options: None,
        install_hint: Some(hint.into()),
    }
}

/// The presets, one per language id (Monaco's ids).
pub fn presets() -> Vec<ServerDescriptor> {
    vec![
        preset(
            "rust",
            "rust-analyzer",
            &[],
            "rustup component add rust-analyzer",
        ),
        preset(
            "typescript",
            "typescript-language-server",
            &["--stdio"],
            "npm install -g typescript-language-server typescript",
        ),
        preset(
            "javascript",
            "typescript-language-server",
            &["--stdio"],
            "npm install -g typescript-language-server typescript",
        ),
        preset(
            "python",
            "pyright-langserver",
            &["--stdio"],
            "npm install -g pyright",
        ),
        preset(
            "go",
            "gopls",
            &[],
            "go install golang.org/x/tools/gopls@latest",
        ),
        preset(
            "c",
            "clangd",
            &[],
            "install clangd from your platform's LLVM package",
        ),
        preset(
            "cpp",
            "clangd",
            &[],
            "install clangd from your platform's LLVM package",
        ),
    ]
}

/// The language id for a path, by extension. `None` is "no server would help".
pub fn language_for_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    if !path.contains('.') {
        return None;
    }
    Some(match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        _ => return None,
    })
}

/// The server for a language: the user's descriptor when they wrote one,
/// else the preset. `None` for a language nobody configured.
pub fn resolve(language: &str, user: &[ServerDescriptor]) -> Option<ServerDescriptor> {
    user.iter()
        .find(|d| d.language == language)
        .cloned()
        .or_else(|| presets().into_iter().find(|d| d.language == language))
}

/// Descriptors from the `lsp.servers` setting — a JSON array; entries that
/// do not parse are skipped with a warning rather than failing the rest.
pub fn user_descriptors(value: &serde_json::Value) -> Vec<ServerDescriptor> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(
                    |v| match serde_json::from_value::<ServerDescriptor>(v.clone()) {
                        Ok(d) if !d.language.trim().is_empty() && !d.command.trim().is_empty() => {
                            Some(d)
                        }
                        Ok(_) => None,
                        Err(e) => {
                            tracing::warn!("lsp.servers entry skipped: {e}");
                            None
                        }
                    },
                )
                .collect()
        })
        .unwrap_or_default()
}

/// The login shell — what a person's own terminal runs — so the server sees
/// the same `PATH`. `$SHELL`, else `/bin/sh`.
pub fn login_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "/bin/sh".into())
}

/// One shell word, single-quoted; the only escaping a POSIX shell needs.
pub fn shell_word(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', r"'\''"))
}

/// How long a probe may take: a login shell that hangs on a profile is
/// given up on and read as unavailable, never left holding a thread.
const PROBE_BUDGET: std::time::Duration = std::time::Duration::from_secs(3);
/// How long an answer is kept before the shell is asked again.
const PROBE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

static PROBED: std::sync::Mutex<Vec<(String, std::time::Instant, bool)>> =
    std::sync::Mutex::new(Vec::new());

/// Forget every kept probe: the next `available` asks the shell again. For
/// the engine, when the `lsp.*` settings change — a server installed a
/// moment ago is what the person is about to look for.
pub fn forget_probes() {
    if let Ok(mut known) = PROBED.lock() {
        known.clear();
    }
}

/// Whether `command` resolves on the login shell's `PATH`.
///
/// The probe runs the person's whole login profile, so its answer is kept
/// for a minute (`PROBE_TTL`) and every call after the first, from any
/// screen, reads it — a status read never spawns one shell per language.
pub fn available(command: &str) -> bool {
    let now = std::time::Instant::now();
    if let Ok(known) = PROBED.lock() {
        if let Some((_, at, ok)) = known.iter().find(|(c, _, _)| c == command) {
            if now.duration_since(*at) < PROBE_TTL {
                return *ok;
            }
        }
    }
    let ok = probe(command);
    if let Ok(mut known) = PROBED.lock() {
        known.retain(|(c, _, _)| c != command);
        known.push((command.to_string(), now, ok));
    }
    ok
}

fn probe(command: &str) -> bool {
    let Ok(mut child) = Command::new(login_shell())
        .args([
            "-l",
            "-c",
            &format!("command -v {} >/dev/null 2>&1", shell_word(command)),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = std::time::Instant::now() + PROBE_BUDGET;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            _ => {
                if let Err(e) = child.kill() {
                    tracing::debug!("ending a probe shell past its budget: {e}");
                }
                if let Err(e) = child.wait() {
                    tracing::debug!("reaping the probe shell: {e}");
                }
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extensions_map_to_monaco_language_ids() {
        assert_eq!(language_for_path("src/main.rs"), Some("rust"));
        assert_eq!(language_for_path("a/b.tsx"), Some("typescript"));
        assert_eq!(language_for_path("x.mjs"), Some("javascript"));
        assert_eq!(language_for_path("lib.pyi"), Some("python"));
        assert_eq!(language_for_path("main.go"), Some("go"));
        assert_eq!(language_for_path("a.hpp"), Some("cpp"));
        assert_eq!(language_for_path("README.md"), None);
        assert_eq!(language_for_path("Makefile"), None);
    }

    #[test]
    fn a_user_descriptor_beats_the_preset_and_bad_entries_are_skipped() {
        let user = user_descriptors(&json!([
            {"language": "rust", "command": "my-ra", "args": ["--x"]},
            {"language": "", "command": "nope"},
            {"command": "no-language"},
            7
        ]));
        assert_eq!(user.len(), 1);
        let ra = resolve("rust", &user).unwrap();
        assert_eq!(ra.command, "my-ra");
        assert_eq!(ra.args, vec!["--x"]);
        let ts = resolve("typescript", &user).unwrap();
        assert_eq!(ts.command, "typescript-language-server");
        assert!(ts.install_hint.is_some());
        assert!(resolve("cobol", &user).is_none());
        assert!(presets()
            .iter()
            .all(|p| !p.language.is_empty() && !p.command.is_empty()));
    }

    #[test]
    fn shell_words_survive_quotes() {
        assert_eq!(shell_word("a b"), "'a b'");
        assert_eq!(shell_word("it's"), r"'it'\''s'");
    }
}
