//! `bisa security`: the three security features as this node runs them
//! — the policy, the problems, each harness's reach, the classifier's
//! readiness and the last decisions — and the two previews to try a rule on.
//! Three verbs over the node's three routes, rendering only what the node
//! returns: nothing here reads a rule from disk or holds a secret.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum SecurityCmd {
    /// The policy as this node runs it: the switches, the rules, what could
    /// not be read, which harnesses the guard can stop, the classifier's
    /// readiness and the last decisions — redacted subjects, never a value
    Status,
    /// Try the rules: `--text` for the redactor (on a scratch vault), or
    /// `--tool` with `--command` or `--path` for the guard's rules alone
    Try {
        /// A text the redactor is tried on
        #[arg(long)]
        text: Option<String>,
        /// The tool a call names — Bash, Read, Write, Edit, WebFetch, mcp__…
        #[arg(long)]
        tool: Option<String>,
        /// The call's command, for a command tool
        #[arg(long)]
        command: Option<String>,
        /// The call's path, for a file tool
        #[arg(long)]
        path: Option<String>,
    },
}

pub async fn security(ctx: &Ctx, out: &Out, cmd: SecurityCmd) -> Result<()> {
    let client = ctx.node_client().await.context(bisa_core::text!(
        "cli-security-security-policy-lives-running-node-start"
    ))?;
    match cmd {
        SecurityCmd::Status => {
            let status = client.get("/security/status").await?;
            for line in status_lines(&status) {
                out.human(&line);
            }
            out.json_value(status);
        }
        SecurityCmd::Try {
            text: Some(text), ..
        } => {
            let preview = client
                .post("/security/redact-preview", json!({ "text": text }))
                .await?;
            for line in redact_lines(&preview) {
                out.human(&line);
            }
            out.json_value(preview);
        }
        SecurityCmd::Try {
            tool: Some(tool),
            command,
            path,
            ..
        } => {
            let input = match (command, path) {
                (Some(command), _) => json!({ "command": command }),
                (None, Some(path)) => json!({ "file_path": path }),
                (None, None) => bail!(bisa_core::text!("cli-security-give-call-s-command-path")),
            };
            let preview = client
                .post(
                    "/security/guard-preview",
                    json!({ "tool": tool, "input": input }),
                )
                .await?;
            for line in guard_lines(&preview) {
                out.human(&line);
            }
            out.json_value(preview);
        }
        SecurityCmd::Try { .. } => bail!(bisa_core::text!(
            "cli-security-give-text-tool-with-command-path"
        )),
    }
    Ok(())
}

fn on_off(value: &Value) -> &'static str {
    if value.as_bool().unwrap_or(false) {
        "on"
    } else {
        "off"
    }
}

fn count(list: &Value) -> usize {
    list.as_array().map(Vec::len).unwrap_or(0)
}

fn off_count(list: &Value) -> usize {
    list.as_array()
        .map(|rules| rules.iter().filter(|r| r["enabled"] == false).count())
        .unwrap_or(0)
}

/// The status, one fact per line.
pub fn status_lines(status: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    if status["redactor_enabled"].as_bool().unwrap_or(false) {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-redactor-rules-switched-off-environment-detectors",
            a0 = (count(&status["redact_rules"])).to_string(),
            a1 = (off_count(&status["redact_rules"])).to_string(),
            a2 = (status["env_detectors"].as_u64().unwrap_or(0)).to_string(),
            a3 = (on_off(&status["env_auto"])).to_string()
        )));
    } else {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-redactor-off-nothing-redacted-node"
        )));
    }
    if status["guard_enabled"].as_bool().unwrap_or(false) {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-guard-rules-switched-off-terminal-hooks",
            a0 = (count(&status["guard_rules"])).to_string(),
            a1 = (off_count(&status["guard_rules"])).to_string(),
            a2 = (on_off(&status["terminal_hooks"])).to_string()
        )));
    } else {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-guard-off-no-tool-call-judged"
        )));
    }
    let classifier = &status["classifier"];
    if classifier["enabled"].as_bool().unwrap_or(false) {
        let standing = if status["classifier_ready"].as_bool().unwrap_or(false) {
            "ready".to_string()
        } else {
            bisa_i18n::say(&bisa_core::text!(
                "cli-security-not-ready-classify-rule-asks-you",
                a0 = status["classifier_note"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| bisa_i18n::say(&bisa_core::text!("cli-security-no-note")))
            ))
        };
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-classifier-within-s-harmful",
            a0 = (classifier["agent"].as_str().unwrap_or("?")).to_string(),
            a1 = (classifier["deadline_secs"].as_u64().unwrap_or(0)).to_string(),
            a2 = (classifier["on_harmful"].as_str().unwrap_or("ask")).to_string(),
            standing = standing.to_string()
        )));
    } else {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-classifier-off-classify-rule-asks-you"
        )));
    }
    for p in status["problems"].as_array().into_iter().flatten() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-problem-rule",
            a0 = (p["feature"].as_str().unwrap_or("?")).to_string(),
            a1 = (p["rule"].as_str().unwrap_or("?")).to_string(),
            a2 = (p["reason"].as_str().unwrap_or("")).to_string()
        )));
    }
    let harnesses = status["harnesses"].as_array().cloned().unwrap_or_default();
    if harnesses.is_empty() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-harnesses-none-registered-node"
        )));
    } else {
        lines.push("harnesses:".to_string());
        for h in &harnesses {
            lines.push(format!(
                "  {:<14} {}",
                h["id"].as_str().unwrap_or("?"),
                reach_words(h)
            ));
        }
    }
    let recent = status["recent"].as_array().cloned().unwrap_or_default();
    if recent.is_empty() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-decisions-none-since-node-started"
        )));
    } else {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-decisions-last",
            a0 = (recent.len().min(10)).to_string(),
            a1 = (recent.len()).to_string()
        )));
        for d in recent.iter().take(10) {
            lines.push(format!(
                "  {:<8} {:<10} {}  — {}",
                verdict_word(d["verdict"].as_str().unwrap_or("?")),
                d["tool"].as_str().unwrap_or("?"),
                d["subject"].as_str().unwrap_or(""),
                judge_words(d)
            ));
        }
    }
    lines.push(bisa_i18n::say(&bisa_core::text!(
        "cli-security-vault-secrets-held-values-never-shown",
        a0 = (status["vault_size"].as_u64().unwrap_or(0)).to_string()
    )));
    lines
}

/// What the guard can do about one harness — the honest words.
pub fn reach_words(h: &Value) -> String {
    match (
        h["tool_guard"].as_bool().unwrap_or(false),
        h["input_rewrite"].as_bool().unwrap_or(false),
    ) {
        (true, true) => bisa_i18n::say(&bisa_core::text!("cli-security-reach-judged-restores")),
        (true, false) => bisa_i18n::say(&bisa_core::text!("cli-security-reach-judged")),
        (false, _) => bisa_i18n::say(&bisa_core::text!("cli-security-reach-observed-only")),
    }
}

fn verdict_word(verdict: &str) -> String {
    match verdict {
        "denied" | "deny" => bisa_i18n::say(&bisa_core::text!("cli-security-verdict-refused")),
        "asked" | "ask" => bisa_i18n::say(&bisa_core::text!("cli-security-verdict-asked")),
        "allowed" | "allow" => bisa_i18n::say(&bisa_core::text!("cli-security-verdict-allowed")),
        _ => bisa_i18n::say(&bisa_core::text!("cli-security-verdict-no-rule")),
    }
}

fn judge_words(d: &Value) -> String {
    let by = match d["by"].as_str().unwrap_or("rule") {
        "classifier" => bisa_i18n::say(&bisa_core::text!("cli-security-classifier")),
        "person" => "you".to_string(),
        _ => format!("rule {}", d["rule"].as_str().unwrap_or("?")),
    };
    match d["reason"].as_str() {
        Some(reason) if !reason.is_empty() => format!("{by}: {reason}"),
        _ => by,
    }
}

/// A redaction preview, in words.
pub fn redact_lines(preview: &Value) -> Vec<String> {
    let count = preview["count"].as_u64().unwrap_or(0);
    let mut lines = vec![if count == 0 {
        bisa_i18n::say(&bisa_core::text!(
            "cli-security-nothing-recognised-text-would-reach-agent"
        ))
    } else {
        let kinds: Vec<&str> = preview["kinds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        bisa_i18n::say(&bisa_core::text!(
            "cli-security-recognised",
            count = count.to_string(),
            a0 = (kinds.join(", ")).to_string()
        ))
    }];
    lines.push(preview["text"].as_str().unwrap_or("").to_string());
    lines
}

/// A guard preview, in words.
pub fn guard_lines(preview: &Value) -> Vec<String> {
    let verdict = preview["verdict"].as_str().unwrap_or("fallthrough");
    let rule = preview["label"]
        .as_str()
        .or(preview["rule"].as_str())
        .unwrap_or("");
    let mut lines = vec![match verdict {
        "deny" => bisa_i18n::say(&bisa_core::text!(
            "cli-security-refused",
            rule = rule.to_string()
        )),
        "ask" => bisa_i18n::say(&bisa_core::text!(
            "cli-security-asks-you-first",
            rule = rule.to_string()
        )),
        "classify" => bisa_i18n::say(&bisa_core::text!(
            "cli-security-classifier-reads-first",
            rule = rule.to_string()
        )),
        "allow" => bisa_i18n::say(&bisa_core::text!(
            "cli-security-allowed",
            rule = rule.to_string()
        )),
        _ => bisa_i18n::say(&bisa_core::text!(
            "cli-security-no-rule-applies-harness-s-own"
        )),
    }];
    if let Some(reason) = preview["reason"].as_str() {
        lines.push(reason.to_string());
    }
    let paths: Vec<&str> = preview["paths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !paths.is_empty() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-security-paths-read",
            a0 = (paths.join(", ")).to_string()
        )));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A status as the route answers it — recorded, no node.
    fn status() -> Value {
        json!({
            "redactor_enabled": true,
            "redact_rules": [
                { "id": "github_token", "enabled": true },
                { "id": "jwt", "enabled": false },
                { "id": "env:FAKE_TOKEN", "enabled": true }
            ],
            "env_auto": true,
            "env_detectors": 1,
            "guard_enabled": false,
            "guard_rules": [{ "id": "privilege_escalation", "enabled": true }],
            "terminal_hooks": true,
            "problems": [{ "feature": "redactor", "rule": "team_key", "reason": "unclosed group" }],
            "classifier": { "enabled": true, "agent": "general-agent", "deadline_secs": 20, "on_harmful": "ask" },
            "classifier_ready": false,
            "classifier_note": "agent \"general-agent\" is missing or disabled",
            "harnesses": [
                { "id": "claude-code", "tool_guard": true, "input_rewrite": true },
                { "id": "codex", "tool_guard": false, "input_rewrite": false }
            ],
            "recent": [
                { "at": 1, "tool": "Bash", "subject": "fake-tool push", "verdict": "allowed", "by": "person", "rule": "ask_fake_tool", "reason": "remembered from your earlier answer on this goal or run" },
                { "at": 0, "tool": "Bash", "subject": "sudo ls", "verdict": "denied", "by": "rule", "rule": "privilege_escalation" }
            ],
            "vault_size": 2
        })
    }

    #[test]
    fn the_status_reads_as_one_fact_per_line_and_says_what_is_off() {
        let lines = status_lines(&status());
        assert_eq!(
            lines[0],
            "redactor: on · 3 rules (1 switched off) · 1 environment detectors (on)"
        );
        assert_eq!(lines[1], "guard: OFF — no tool call is judged on this node");
        assert!(lines[2]
            .starts_with("classifier: general-agent within 20s, on harmful: ask — not ready"));
        assert!(lines[2].contains("missing or disabled"));
        assert_eq!(lines[3], "problem: redactor rule team_key: unclosed group");
        assert_eq!(lines[4], "harnesses:");
        assert!(
            lines[5].contains("claude-code")
                && lines[5].contains("judged before it runs · restores placeholders")
        );
        assert!(lines[6].contains("codex") && lines[6].contains("observed only"));
        assert_eq!(lines[7], "decisions (last 2 of 2):");
        assert!(lines[8].contains("allowed") && lines[8].contains("you: remembered"));
        assert!(lines[9].contains("refused") && lines[9].contains("rule privilege_escalation"));
        assert_eq!(lines[10], "vault: 2 secrets held (values never shown)");
        let text = lines.join("\n");
        assert!(!text.contains("fake-env"), "no value is ever rendered");
    }

    #[test]
    fn a_switched_off_redactor_and_an_off_classifier_say_so() {
        let mut s = status();
        s["redactor_enabled"] = json!(false);
        s["classifier"]["enabled"] = json!(false);
        s["harnesses"] = json!([]);
        s["recent"] = json!([]);
        let lines = status_lines(&s);
        assert_eq!(lines[0], "redactor: OFF — nothing is redacted on this node");
        assert_eq!(
            lines[2],
            "classifier: off — a classify rule asks you instead"
        );
        assert!(lines.contains(&"harnesses: none registered on this node".to_string()));
        assert!(lines.contains(&"decisions: none since this node started".to_string()));
    }

    #[test]
    fn the_previews_read_as_words() {
        let redact = redact_lines(
            &json!({ "text": "key «secret:github_token:0a1b2c»", "count": 1, "kinds": ["github_token"] }),
        );
        assert_eq!(redact[0], "1 recognised (github_token)");
        assert_eq!(redact[1], "key «secret:github_token:0a1b2c»");
        assert_eq!(
            redact_lines(&json!({ "text": "ls", "count": 0, "kinds": [] }))[0],
            "nothing recognised — the text would reach an agent as it is"
        );
        let guard = guard_lines(
            &json!({ "verdict": "deny", "rule": "dotenv", "label": ".env files", "reason": "refused by the guard rule “.env files”", "paths": ["/x/.env"] }),
        );
        assert_eq!(guard[0], "refused — .env files");
        assert_eq!(guard[1], "refused by the guard rule “.env files”");
        assert_eq!(guard[2], "paths read: /x/.env");
        assert!(
            guard_lines(&json!({ "verdict": "fallthrough", "paths": [] }))[0]
                .starts_with("no rule applies")
        );
        assert_eq!(
            guard_lines(&json!({ "verdict": "classify", "rule": "publishing", "paths": [] }))[0],
            "the classifier reads it first — publishing"
        );
    }
}
