//! The Tool & Commands Guard: ordered rules over a tool call, first match
//! wins, four verdicts.
//!
//! A tool call is what a harness is about to do on the agent's behalf — a
//! tool name and its raw input. The guard reads three things out of it: the
//! shell command (`command` or `cmd`), every path-like argument (`file_path`,
//! `path`, `notebook_path`, and the words of the command that look like
//! paths), and the name itself. A rule matches one of those; the first
//! enabled rule that matches decides. No rule matching is [`Verdict::Fallthrough`]
//! — the caller's own default, today the tier ceiling.
//!
//! This module never spawns anything: a command is a string it matches, and a
//! unit test below holds it to that.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobMatcher};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::redact::Origin;

/// What a matching rule asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "GuardAction")]
pub enum Action {
    /// Run it, without asking anyone.
    Allow,
    /// Refuse it; the agent hears the reason.
    Deny,
    /// Ask the person who owns the goal.
    Ask,
    /// Ask the classifier first; harmful goes to the person (or is refused),
    /// safe runs.
    Classify,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Allow => "allow",
            Action::Deny => "deny",
            Action::Ask => "ask",
            Action::Classify => "classify",
        }
    }
}

/// Who hosts the harness a call comes from — what a rule's `applies_to` is
/// read against. It says *who*, not which tools the session mounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename = "GuardHost")]
pub enum Host {
    /// A session the platform drives — a worker, a turn, a wake, a one-shot
    /// ask — or the platform's own command: the platform's rules about its
    /// own agents apply, the embedded browser is where they browse.
    Platform,
    /// A person's own harness in a terminal: nothing of the platform's is
    /// injected into it, and the person is at the keyboard.
    Terminal,
}

impl Host {
    pub fn as_str(self) -> &'static str {
        match self {
            Host::Platform => "platform",
            Host::Terminal => "terminal",
        }
    }
}

/// What a rule looks at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schemars(rename = "GuardMatcher")]
pub enum Matcher {
    /// A regular expression over the shell command, whitespace-normalised.
    Command { regex: String },
    /// A glob over every path-like argument, tried as written, as an absolute
    /// path under the session's cwd, and as `~/…` when it is under the home.
    Path { glob: String },
    /// An exact tool name — `Bash`, `Write`, `mcp__bisa__post_message` — or
    /// a prefix ending in `*`: `mcp__gh__*` is every tool of one MCP
    /// server, `mcp__*` every tool of every MCP server. A name is read as
    /// the harness calls it and by its harness-neutral name when the two
    /// differ ([`ToolCall::canonical`]): `fetch` names Claude Code's
    /// `WebFetch`, `web_search` its `WebSearch`.
    Tool { name: String },
    /// Every call.
    Any,
}

/// One guard rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GuardRule {
    pub id: String,
    pub label: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub action: Action,
    pub matcher: Matcher,
    /// What a refusal by this rule adds after its label — the way the agent
    /// should take instead, in a sentence it reads as the tool's answer.
    /// Only a refusal carries it; an ask or a classify has nothing to add.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Where the rule applies: everywhere when unset; `platform` for a rule
    /// that steers an agent to a tool only the platform's own sessions have
    /// (the browser family); `terminal` for a rule about a person's own
    /// harness alone. A rule stored before this field reads as everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies_to: Option<Host>,
    #[serde(default)]
    pub origin: Origin,
}

impl GuardRule {
    /// The words a refusal by this rule reads: the label, and the hint when
    /// the rule carries one.
    pub fn refusal(&self) -> String {
        match &self.hint {
            Some(hint) => format!("refused by the guard rule “{}” — {hint}", self.label),
            None => format!("refused by the guard rule “{}”", self.label),
        }
    }
}

fn yes() -> bool {
    true
}

/// The call under judgement.
#[derive(Clone, Debug)]
pub struct ToolCall<'a> {
    pub tool: &'a str,
    pub input: &'a Value,
    pub cwd: Option<&'a Path>,
    pub home: Option<&'a Path>,
    /// Who hosts the harness making the call.
    pub host: Host,
    /// The tool's harness-neutral name, when the harness's own differs —
    /// `fetch` for Claude Code's `WebFetch` — so a `Tool` rule written in the
    /// vocabulary the tiers speak reads the call too. `None` when the name is
    /// already its own.
    pub canonical: Option<&'a str>,
}

const COMMAND_KEYS: &[&str] = &["command", "cmd"];
const PATH_KEYS: &[&str] = &[
    "file_path",
    "path",
    "notebook_path",
    "filePath",
    "target_file",
    "directory",
];

impl<'a> ToolCall<'a> {
    /// The shell command, its runs of whitespace collapsed to one space.
    pub fn command(&self) -> Option<String> {
        COMMAND_KEYS
            .iter()
            .find_map(|k| self.input.get(k).and_then(Value::as_str))
            .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|c| !c.is_empty())
    }

    /// Every path-like argument, as written.
    pub fn paths(&self) -> Vec<String> {
        let mut out: Vec<String> = PATH_KEYS
            .iter()
            .filter_map(|k| self.input.get(k).and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        if let Some(command) = self.command() {
            for word in shlex::split(&command)
                .unwrap_or_else(|| command.split(' ').map(str::to_string).collect())
            {
                let candidate = match word.split_once('=') {
                    Some((flag, value)) if flag.starts_with('-') => value.to_string(),
                    _ => word,
                };
                if looks_like_path(&candidate) && !out.contains(&candidate) {
                    out.push(candidate);
                }
            }
        }
        out
    }

    /// The forms a path is tried in against a glob.
    fn candidates(&self, raw: &str) -> Vec<String> {
        let mut forms = vec![raw.to_string()];
        let expanded: PathBuf = match (raw.strip_prefix("~/"), raw == "~", self.home) {
            (Some(rest), _, Some(home)) => home.join(rest),
            (_, true, Some(home)) => home.to_path_buf(),
            _ => PathBuf::from(raw),
        };
        let absolute = if expanded.is_absolute() {
            expanded
        } else if let Some(cwd) = self.cwd {
            cwd.join(&expanded)
        } else {
            expanded
        };
        let absolute = normalise(&absolute);
        push_unique(&mut forms, absolute.to_string_lossy().into_owned());
        if let Some(home) = self.home {
            if let Ok(rest) = absolute.strip_prefix(home) {
                let rel = rest.to_string_lossy();
                push_unique(
                    &mut forms,
                    if rel.is_empty() {
                        "~".to_string()
                    } else {
                        format!("~/{rel}")
                    },
                );
            }
        }
        forms
    }
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) {
        v.push(s);
    }
}

/// A word of a command that could name a file: it has a `/`, starts with `~`
/// or `.`, or reads as a file name with an extension (`server.key`).
fn looks_like_path(word: &str) -> bool {
    if word.is_empty() || word.starts_with('-') || word.contains("://") {
        return false;
    }
    if word.contains('/') || word.starts_with('~') || word.starts_with('.') {
        return true;
    }
    match word.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && (1..=5).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && stem
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        }
        None => false,
    }
}

/// `a/./b/../c` → `a/c`, without touching the filesystem.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The guard's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow {
        rule: String,
    },
    Deny {
        rule: String,
        reason: String,
    },
    Ask {
        rule: String,
    },
    Classify {
        rule: String,
    },
    /// No rule matched — the caller's default applies.
    Fallthrough,
}

/// A line of a script the rules refused, and where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefusedLine {
    /// One-based.
    pub line: usize,
    pub rule: String,
    pub reason: String,
}

/// The rule the engine cites when a command still carries a placeholder
/// nothing can resolve: running it would run the literal.
pub const UNRESOLVED_RULE: &str = "unresolved_placeholder";

/// The deny for a command that still holds placeholders nobody can resolve.
pub fn unresolved_deny(placeholders: &[String]) -> Verdict {
    Verdict::Deny {
        rule: UNRESOLVED_RULE.to_string(),
        reason: format!(
            "the command carries {} this node cannot resolve ({}) — a secret redacted before a restart or on another machine; ask the person to provide it again",
            if placeholders.len() == 1 { "a placeholder" } else { "placeholders" },
            placeholders.join(", ")
        ),
    }
}

enum Compiled {
    Command(Regex),
    Path(GlobMatcher),
    Tool(String),
    Any,
}

struct CompiledRule {
    rule: GuardRule,
    matcher: Compiled,
}

/// Why a rule could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("rule {rule}: {reason}")]
pub struct BadRule {
    pub rule: String,
    pub reason: String,
}

/// The compiled, enabled rules in order.
#[derive(Default)]
pub struct Guard {
    rules: Vec<CompiledRule>,
}

impl std::fmt::Debug for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guard")
            .field("rules", &self.rules.len())
            .finish()
    }
}

/// An exact tool name matches itself; a pattern ending in `*` matches every
/// tool that starts with what comes before it.
fn tool_matches(pattern: &str, tool: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => tool.starts_with(prefix),
        None => pattern == tool,
    }
}

impl Guard {
    /// Compiles the enabled rules in the order given. A rule that does not
    /// compile is reported and skipped.
    pub fn compile(rules: &[GuardRule]) -> (Self, Vec<BadRule>) {
        let mut compiled = Vec::new();
        let mut problems = Vec::new();
        for rule in rules.iter().filter(|r| r.enabled) {
            let matcher = match &rule.matcher {
                Matcher::Command { regex } => match Regex::new(regex) {
                    Ok(re) => Compiled::Command(re),
                    Err(e) => {
                        problems.push(BadRule {
                            rule: rule.id.clone(),
                            reason: e.to_string(),
                        });
                        continue;
                    }
                },
                Matcher::Path { glob } => match Glob::new(glob) {
                    Ok(g) => Compiled::Path(g.compile_matcher()),
                    Err(e) => {
                        problems.push(BadRule {
                            rule: rule.id.clone(),
                            reason: e.to_string(),
                        });
                        continue;
                    }
                },
                Matcher::Tool { name } => Compiled::Tool(name.clone()),
                Matcher::Any => Compiled::Any,
            };
            compiled.push(CompiledRule {
                rule: rule.clone(),
                matcher,
            });
        }
        (Guard { rules: compiled }, problems)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The first enabled rule that matches decides. A rule that applies to
    /// one host alone ([`GuardRule::applies_to`]) is passed over for a call
    /// from the other, as if it were not there; a `Tool` rule reads the
    /// call's own name and its harness-neutral one.
    pub fn evaluate(&self, call: &ToolCall<'_>) -> Verdict {
        let command = call.command();
        let paths = call.paths();
        for CompiledRule { rule, matcher } in &self.rules {
            if rule.applies_to.is_some_and(|host| host != call.host) {
                continue;
            }
            let hit = match matcher {
                Compiled::Command(re) => command.as_deref().is_some_and(|c| re.is_match(c)),
                Compiled::Path(glob) => paths
                    .iter()
                    .any(|p| call.candidates(p).iter().any(|c| glob.is_match(c))),
                Compiled::Tool(name) => {
                    tool_matches(name, call.tool)
                        || call.canonical.is_some_and(|c| tool_matches(name, c))
                }
                Compiled::Any => true,
            };
            if hit {
                return match rule.action {
                    Action::Allow => Verdict::Allow {
                        rule: rule.id.clone(),
                    },
                    Action::Deny => Verdict::Deny {
                        rule: rule.id.clone(),
                        reason: rule.refusal(),
                    },
                    Action::Ask => Verdict::Ask {
                        rule: rule.id.clone(),
                    },
                    Action::Classify => Verdict::Classify {
                        rule: rule.id.clone(),
                    },
                };
            }
        }
        Verdict::Fallthrough
    }

    /// Whether the tool matcher `pattern` names `tool`: itself, or any tool
    /// under a prefix ending in `*`.
    pub fn tool_matches(pattern: &str, tool: &str) -> bool {
        tool_matches(pattern, tool)
    }

    /// Reads a script line by line, each non-empty, non-comment line as a
    /// `sh` call of the platform's own, and names the first one a rule
    /// refuses. Only a refusal counts: a script runs whole or not at all, so
    /// an *ask* or a *classify* on one line is not a question anyone can
    /// answer for it, and the caller's own trust decides the rest.
    pub fn evaluate_lines(
        &self,
        script: &str,
        cwd: Option<&Path>,
        home: Option<&Path>,
    ) -> Option<RefusedLine> {
        script
            .lines()
            .enumerate()
            .map(|(i, line)| (i + 1, line.trim()))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
            .find_map(|(number, line)| {
                let input = serde_json::json!({ "command": line });
                match self.evaluate(&ToolCall {
                    tool: "sh",
                    input: &input,
                    cwd,
                    home,
                    host: Host::Platform,
                    canonical: None,
                }) {
                    Verdict::Deny { rule, reason } => Some(RefusedLine {
                        line: number,
                        rule,
                        reason,
                    }),
                    _ => None,
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_script_is_read_line_by_line_and_the_first_refusal_names_its_line() {
        let g = guard(vec![
            rule(
                "no_push",
                Action::Deny,
                Matcher::Command {
                    regex: r"^git push\b".into(),
                },
            ),
            rule(
                "ask_curl",
                Action::Ask,
                Matcher::Command {
                    regex: r"^curl\b".into(),
                },
            ),
        ]);
        let script = "#!/bin/sh\n# git push is only a comment here\ncargo build\ncurl https://x.test\n\n  git push origin main\necho done\n";
        let hit = g.evaluate_lines(script, None, None).expect("the push line");
        assert_eq!(hit.line, 6);
        assert_eq!(hit.rule, "no_push");
        assert!(hit.reason.contains("no push"));
        assert_eq!(
            g.evaluate_lines("cargo build\ncurl https://x.test\n", None, None),
            None,
            "an ask on a line is not a refusal of the script"
        );
        assert_eq!(g.evaluate_lines("", None, None), None);
    }

    #[test]
    fn a_refusal_names_the_rule_and_its_hint_when_it_carries_one() {
        let bare = rule(
            "plain",
            Action::Deny,
            Matcher::Command {
                regex: "^rm".into(),
            },
        );
        assert_eq!(bare.refusal(), "refused by the guard rule “plain”");
        let pointed = GuardRule {
            hint: Some("use the browser tools".into()),
            ..bare
        };
        assert_eq!(
            pointed.refusal(),
            "refused by the guard rule “plain” — use the browser tools"
        );
        let (g, _) = Guard::compile(&[pointed]);
        let input = serde_json::json!({ "command": "rm -rf x" });
        match g.evaluate(&ToolCall {
            tool: "Bash",
            input: &input,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        }) {
            Verdict::Deny { reason, .. } => assert!(reason.ends_with("use the browser tools")),
            other => panic!("{other:?}"),
        }
    }

    fn rule(id: &str, action: Action, matcher: Matcher) -> GuardRule {
        GuardRule {
            id: id.into(),
            label: id.replace('_', " "),
            enabled: true,
            action,
            matcher,
            hint: None,
            applies_to: None,
            origin: Origin::User,
        }
    }

    fn guard(rules: Vec<GuardRule>) -> Guard {
        let (g, problems) = Guard::compile(&rules);
        assert!(problems.is_empty(), "{problems:?}");
        g
    }

    fn bash(command: &str) -> Value {
        json!({ "command": command })
    }

    #[test]
    fn the_first_matching_rule_wins_and_nothing_matching_falls_through() {
        let g = guard(vec![
            rule(
                "allow_status",
                Action::Allow,
                Matcher::Command {
                    regex: r"^git status\b".into(),
                },
            ),
            rule(
                "ask_git",
                Action::Ask,
                Matcher::Command {
                    regex: r"^git\b".into(),
                },
            ),
            rule(
                "classify_curl",
                Action::Classify,
                Matcher::Command {
                    regex: r"\bcurl\b".into(),
                },
            ),
        ]);
        let input = bash("git   status --short");
        let call = ToolCall {
            tool: "Bash",
            input: &input,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        };
        assert_eq!(
            g.evaluate(&call),
            Verdict::Allow {
                rule: "allow_status".into()
            }
        );
        let input = bash("git push");
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &input,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Ask {
                rule: "ask_git".into()
            }
        );
        let input = bash("curl https://example.test");
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &input,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Classify {
                rule: "classify_curl".into()
            }
        );
        let input = bash("cargo test");
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &input,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Fallthrough
        );
    }

    #[test]
    fn a_disabled_rule_does_not_match_and_a_deny_names_its_label() {
        let mut off = rule("never", Action::Deny, Matcher::Any);
        off.enabled = false;
        let g = guard(vec![
            off,
            rule(
                "no_sudo",
                Action::Deny,
                Matcher::Command {
                    regex: r"(^|[;&|] *)sudo\b".into(),
                },
            ),
        ]);
        let input = bash("sudo ls");
        match g.evaluate(&ToolCall {
            tool: "Bash",
            input: &input,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        }) {
            Verdict::Deny { rule, reason } => {
                assert_eq!(rule, "no_sudo");
                assert!(reason.contains("no sudo"), "{reason}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_path_rule_sees_inputs_and_command_words_in_every_form() {
        let g = guard(vec![
            rule(
                "ssh",
                Action::Deny,
                Matcher::Path {
                    glob: "~/.ssh/**".into(),
                },
            ),
            rule(
                "dotenv",
                Action::Deny,
                Matcher::Path {
                    glob: "**/.env*".into(),
                },
            ),
        ]);
        let home = Path::new("/Users/someone");
        let cwd = Path::new("/Users/someone/work/proj");
        let read = json!({ "file_path": "/Users/someone/.ssh/id_ed25519" });
        assert!(matches!(
            g.evaluate(&ToolCall {
                tool: "Read",
                input: &read,
                cwd: Some(cwd),
                home: Some(home),
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Deny { .. }
        ));
        let cat = bash("cat ~/.ssh/config");
        assert!(matches!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &cat,
                cwd: Some(cwd),
                home: Some(home),
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Deny { .. }
        ));
        let rel = bash("cat .env.local");
        assert!(matches!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &rel,
                cwd: Some(cwd),
                home: Some(home),
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Deny { .. }
        ));
        let dotdot = bash("cat ../../.ssh/known_hosts");
        assert!(matches!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &dotdot,
                cwd: Some(cwd),
                home: Some(home),
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Deny { .. }
        ));
        let fine = json!({ "file_path": "/Users/someone/work/proj/src/main.rs" });
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "Read",
                input: &fine,
                cwd: Some(cwd),
                home: Some(home),
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Fallthrough
        );
        let flag = bash("tool --config=~/.ssh/config");
        assert!(matches!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &flag,
                cwd: Some(cwd),
                home: Some(home),
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Deny { .. }
        ));
    }

    #[test]
    fn a_tool_rule_is_exact_and_any_matches_everything() {
        let g = guard(vec![
            rule(
                "no_fetch",
                Action::Deny,
                Matcher::Tool {
                    name: "WebFetch".into(),
                },
            ),
            rule("rest", Action::Ask, Matcher::Any),
        ]);
        let empty = json!({});
        assert!(matches!(
            g.evaluate(&ToolCall {
                tool: "WebFetch",
                input: &empty,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Deny { .. }
        ));
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "WebFetchX",
                input: &empty,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Ask {
                rule: "rest".into()
            }
        );
    }

    #[test]
    fn a_tool_pattern_ending_in_a_star_is_a_prefix() {
        let g = guard(vec![
            rule(
                "gh_ok",
                Action::Allow,
                Matcher::Tool {
                    name: "mcp__gh__*".into(),
                },
            ),
            rule(
                "mcp_rest",
                Action::Classify,
                Matcher::Tool {
                    name: "mcp__*".into(),
                },
            ),
        ]);
        let empty = json!({});
        let call = |tool: &'static str| ToolCall {
            tool,
            input: &empty,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        };
        assert_eq!(
            g.evaluate(&call("mcp__gh__create_issue")),
            Verdict::Allow {
                rule: "gh_ok".into()
            }
        );
        assert_eq!(
            g.evaluate(&call("mcp__computer__click")),
            Verdict::Classify {
                rule: "mcp_rest".into()
            }
        );
        assert_eq!(g.evaluate(&call("Bash")), Verdict::Fallthrough);
        assert!(Guard::tool_matches("mcp__*", "mcp__x__y"));
        assert!(!Guard::tool_matches("mcp__gh__*", "mcp__gh"));
        assert!(Guard::tool_matches("WebFetch", "WebFetch"));
        assert!(!Guard::tool_matches("WebFetch", "WebFetchX"));
    }

    #[test]
    fn a_bad_regex_or_glob_is_reported_and_skipped() {
        let (g, problems) = Guard::compile(&[
            rule(
                "bad_re",
                Action::Deny,
                Matcher::Command { regex: "(".into() },
            ),
            rule("bad_glob", Action::Deny, Matcher::Path { glob: "[".into() }),
            rule("ok", Action::Allow, Matcher::Any),
        ]);
        assert_eq!(
            problems.iter().map(|p| p.rule.as_str()).collect::<Vec<_>>(),
            vec!["bad_re", "bad_glob"]
        );
        let empty = json!({});
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &empty,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Allow { rule: "ok".into() }
        );
    }

    #[test]
    fn the_unresolved_deny_names_the_placeholders() {
        match unresolved_deny(&["«secret:k:abcdef»".to_string()]) {
            Verdict::Deny { rule, reason } => {
                assert_eq!(rule, UNRESOLVED_RULE);
                assert!(reason.contains("«secret:k:abcdef»") && reason.contains("a placeholder"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn command_words_that_are_urls_or_flags_are_not_paths() {
        let input = bash("curl -sSf https://example.test/a/b -o ./out.txt");
        let call = ToolCall {
            tool: "Bash",
            input: &input,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        };
        assert_eq!(call.paths(), vec!["./out.txt"]);
        let none = json!({ "cmd": "ls" });
        assert!(ToolCall {
            tool: "Bash",
            input: &none,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical: None,
        }
        .paths()
        .is_empty());
    }

    /// A rule for one host alone is passed over for a call from the other —
    /// the rules after it still read the call, so the skip is never a verdict.
    #[test]
    fn a_rule_for_one_host_is_skipped_for_the_other() {
        let mut in_a_terminal = rule(
            "terminal_asks_git",
            Action::Ask,
            Matcher::Command {
                regex: r"^git\b".into(),
            },
        );
        in_a_terminal.applies_to = Some(Host::Terminal);
        let mut platform_only = rule(
            "platform_refuses_git",
            Action::Deny,
            Matcher::Command {
                regex: r"^git push\b".into(),
            },
        );
        platform_only.applies_to = Some(Host::Platform);
        let g = guard(vec![
            in_a_terminal,
            platform_only,
            rule(
                "everyone_classifies_git",
                Action::Classify,
                Matcher::Command {
                    regex: r"^git\b".into(),
                },
            ),
        ]);
        let input = bash("git push origin main");
        let from = |host: Host| ToolCall {
            tool: "Bash",
            input: &input,
            cwd: None,
            home: None,
            host,
            canonical: None,
        };
        assert_eq!(
            g.evaluate(&from(Host::Terminal)),
            Verdict::Ask {
                rule: "terminal_asks_git".into()
            },
            "a terminal reads its own rule first"
        );
        assert!(
            matches!(
                g.evaluate(&from(Host::Platform)),
                Verdict::Deny { ref rule, .. } if rule == "platform_refuses_git"
            ),
            "the platform skips the terminal's rule and meets its own"
        );
        let status = bash("git status");
        assert_eq!(
            g.evaluate(&ToolCall {
                tool: "Bash",
                input: &status,
                cwd: None,
                home: None,
                host: Host::Platform,
                canonical: None,
            }),
            Verdict::Classify {
                rule: "everyone_classifies_git".into()
            },
            "a skipped rule hands the call on to the rules after it"
        );
    }

    /// A tool rule names the tool as the harness calls it, or by the
    /// harness-neutral name the tiers speak; the two never mix with a prefix.
    #[test]
    fn a_tool_rule_reads_the_harnesss_own_name_and_the_neutral_one() {
        let g = guard(vec![
            rule(
                "ask_fetch",
                Action::Ask,
                Matcher::Tool {
                    name: "fetch".into(),
                },
            ),
            rule(
                "deny_claude_name",
                Action::Deny,
                Matcher::Tool {
                    name: "WebSearch".into(),
                },
            ),
            rule(
                "mcp_rest",
                Action::Classify,
                Matcher::Tool {
                    name: "mcp__*".into(),
                },
            ),
        ]);
        let empty = json!({});
        let call = |tool: &'static str, canonical: Option<&'static str>| ToolCall {
            tool,
            input: &empty,
            cwd: None,
            home: None,
            host: Host::Platform,
            canonical,
        };
        assert_eq!(
            g.evaluate(&call("WebFetch", Some("fetch"))),
            Verdict::Ask {
                rule: "ask_fetch".into()
            },
            "the neutral name reads the rule written in the tiers' words"
        );
        assert_eq!(
            g.evaluate(&call("fetch", None)),
            Verdict::Ask {
                rule: "ask_fetch".into()
            },
            "a harness that already calls it fetch matches by its own name"
        );
        assert_eq!(
            g.evaluate(&call("WebFetch", None)),
            Verdict::Fallthrough,
            "without the neutral name the harness's own is all there is"
        );
        assert!(
            matches!(
                g.evaluate(&call("WebSearch", Some("web_search"))),
                Verdict::Deny { .. }
            ),
            "the harness's own name still matches a rule that spells it"
        );
        assert_eq!(
            g.evaluate(&call("mcp__gh__fetch_issue", None)),
            Verdict::Classify {
                rule: "mcp_rest".into()
            },
            "a prefix pattern reads the raw name alone"
        );
    }

    #[test]
    fn this_module_never_spawns_a_process() {
        let source = include_str!("guard.rs");
        let body = source.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(
            !body.contains("std::process") && !body.contains("Command::new"),
            "the guard matches commands; it never runs one"
        );
    }

    // added by the coverage pass: guard.rs

    // --- the words, the home forms and the debug ---

    #[test]
    fn an_action_and_a_host_print_their_words_and_a_guard_debugs_as_a_count() {
        assert_eq!(Action::Allow.as_str(), "allow");
        assert_eq!(Action::Deny.as_str(), "deny");
        assert_eq!(Action::Ask.as_str(), "ask");
        assert_eq!(Action::Classify.as_str(), "classify");
        assert_eq!(Host::Platform.as_str(), "platform");
        assert_eq!(Host::Terminal.as_str(), "terminal");
        let g = guard(vec![rule(
            "secret_file",
            Action::Deny,
            Matcher::Path {
                glob: "**/secret.txt".into(),
            },
        )]);
        let debug = format!("{g:?}");
        assert!(debug.contains("rules: 1"), "{debug}");
        assert!(
            !debug.contains("secret.txt"),
            "the rules are a count: {debug}"
        );
    }

    /// The home itself — `~` alone — is a path in every form, and a relative
    /// path is normalised without touching the disk: `.` vanishes, `..` pops
    /// what it can and stays where it cannot.
    #[test]
    fn the_home_alone_is_a_path_in_every_form_and_a_relative_path_is_normalised() {
        let home = Path::new("/home/me");
        let input = json!({ "file_path": "~" });
        let call = ToolCall {
            tool: "Read",
            input: &input,
            cwd: Some(home),
            home: Some(home),
            host: Host::Platform,
            canonical: None,
        };
        let forms = call.candidates("~");
        assert!(forms.contains(&"~".to_string()), "{forms:?}");
        assert!(forms.contains(&"/home/me".to_string()), "{forms:?}");
        let forms = call.candidates("/home/me");
        assert!(forms.contains(&"~".to_string()), "{forms:?}");
        assert_eq!(normalise(Path::new("a/./b/../c")), PathBuf::from("a/c"));
        assert_eq!(normalise(Path::new("../x")), PathBuf::from("../x"));
        assert_eq!(normalise(Path::new("./a/../../b")), PathBuf::from("../b"));
    }

    // added by the coverage pass: guard2.rs

    #[test]
    fn a_path_outside_the_home_has_no_tilde_form() {
        let home = Path::new("/home/me");
        let input = json!({ "file_path": "/etc/hosts" });
        let call = ToolCall {
            tool: "Read",
            input: &input,
            cwd: Some(home),
            home: Some(home),
            host: Host::Platform,
            canonical: None,
        };
        let forms = call.candidates("/etc/hosts");
        assert_eq!(forms, vec!["/etc/hosts".to_string()]);
    }
}
