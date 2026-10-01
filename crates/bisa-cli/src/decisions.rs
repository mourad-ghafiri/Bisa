//! `bisa decisions`: the Decision-Making Agent as this node runs it — who
//! answers, whether it can be asked, which decision points the switch reaches
//! — the judgements it made, a question to try it with, and the API key of a
//! remote provider. Four verbs over the node's routes, rendering only what the
//! node returns. A key is read from stdin or a file, never from the command
//! line, and is never printed.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum DecisionsCmd {
    /// Who answers, whether it can be asked, whether a key is stored, and
    /// every decision point with whether the switch reaches it
    Status,
    /// The newest judgements, newest first
    List {
        /// How many to read (50 by default, 200 at most)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Put one question to the provider as it is set up; nothing is decided
    /// and nothing is recorded
    Try {
        /// What is judged
        #[arg(long)]
        state: String,
        /// A yes/no question, answered as a probability
        #[arg(long, conflicts_with = "choice")]
        noul: Option<String>,
        /// A which-one question; give its options with --option
        #[arg(long)]
        choice: Option<String>,
        /// An option of --choice, `id=what it means` (repeatable, two at least)
        #[arg(long = "option")]
        options: Vec<String>,
    },
    /// The API key of a remote provider (jev | rlcd), kept in this machine's
    /// keystore
    Key {
        #[command(subcommand)]
        command: KeyCmd,
    },
}

#[derive(Subcommand)]
pub enum KeyCmd {
    /// Store the key, read from `@stdin` or `@path`
    Set {
        /// jev | rlcd
        provider: String,
        /// `@stdin`, or `@` and the path of a file holding the key
        #[arg(long)]
        from: String,
    },
    /// Forget the key
    Clear {
        /// jev | rlcd
        provider: String,
    },
}

pub async fn decisions(ctx: &Ctx, out: &Out, cmd: DecisionsCmd) -> Result<()> {
    let client = ctx.node_client().await.context(bisa_core::text!(
        "cli-decisions-decision-making-agent-lives-running-node-start"
    ))?;
    match cmd {
        DecisionsCmd::Status => {
            let status = client.get("/decisions/status").await?;
            for line in status_lines(&status) {
                out.human(&line);
            }
            out.json_value(status);
        }
        DecisionsCmd::List { limit } => {
            let path = match limit {
                Some(n) => format!("/decisions?limit={n}"),
                None => "/decisions".to_string(),
            };
            let records = client.get(&path).await?;
            for line in record_lines(&records) {
                out.human(&line);
            }
            // Named, as every list this command line prints is: an object
            // a field can be added to, never a bare array.
            out.json_value(json!({ "judgements": records }));
        }
        DecisionsCmd::Try {
            state,
            noul,
            choice,
            options,
        } => {
            let question = question_of(noul, choice, &options)?;
            let response = client
                .post(
                    "/decisions/try",
                    json!({ "state": state, "questions": { "q": question } }),
                )
                .await?;
            for line in answer_lines(&response) {
                out.human(&line);
            }
            out.json_value(response);
        }
        DecisionsCmd::Key {
            command: KeyCmd::Set { provider, from },
        } => {
            let key = read_key(&from)?;
            let said = client
                .put(&format!("/decisions/key/{provider}"), json!({ "key": key }))
                .await?;
            out.say(&bisa_core::text!(
                "cli-decisions-key-stored",
                provider = provider.to_string()
            ));
            out.json_value(said);
        }
        DecisionsCmd::Key {
            command: KeyCmd::Clear { provider },
        } => {
            let said = client.delete(&format!("/decisions/key/{provider}")).await?;
            out.say(&bisa_core::text!(
                "cli-decisions-no-key-stored",
                provider = provider.to_string()
            ));
            out.json_value(said);
        }
    }
    Ok(())
}

/// The one question `try` asks, from its flags.
fn question_of(noul: Option<String>, choice: Option<String>, options: &[String]) -> Result<Value> {
    match (noul, choice) {
        (Some(instructions), None) => Ok(json!({ "type": "noul", "instructions": instructions })),
        (None, Some(instructions)) => {
            let mut criteria = serde_json::Map::new();
            for option in options {
                let (id, meaning) = option.split_once('=').with_context(|| {
                    bisa_core::text!(
                        "cli-decisions-option-wants-id-what-means-got",
                        option = format!("{option:?}")
                    )
                })?;
                criteria.insert(id.trim().to_string(), json!(meaning.trim()));
            }
            if criteria.len() < 2 {
                bail!(bisa_core::text!(
                    "cli-decisions-choice-between-two-options-least-give"
                ));
            }
            Ok(json!({ "type": "choice", "instructions": instructions, "criteria": criteria }))
        }
        _ => bail!(bisa_core::text!(
            "cli-decisions-ask-one-question-noul-choice-with"
        )),
    }
}

/// A key from `@stdin` or `@path`. Never a bare value: a command line is
/// kept in a shell's history.
fn read_key(from: &str) -> Result<String> {
    let key = if from == "@stdin" {
        std::io::read_to_string(std::io::stdin())
            .context(bisa_core::text!("cli-decisions-reading-key-from-stdin"))?
    } else if let Some(path) = from.strip_prefix('@') {
        std::fs::read_to_string(path).with_context(|| {
            bisa_core::text!("cli-decisions-reading-key-from", path = path.to_string())
        })?
    } else {
        bail!(bisa_core::text!(
            "cli-decisions-from-wants-stdin-path-key-command"
        ));
    };
    let key = key.trim().to_string();
    if key.is_empty() {
        bail!(bisa_core::text!("cli-decisions-key-empty"));
    }
    Ok(key)
}

fn status_lines(status: &Value) -> Vec<String> {
    let word = |key: &str| status[key].as_str().unwrap_or("?").to_string();
    // The model, and the effort it works at when the status names one: a
    // harness answers, and takes an effort for that model.
    let answers_as = match status["effort"].as_str() {
        Some(effort) => format!("{} · {effort}", word("answers_as")),
        None => word("answers_as"),
    };
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!(
        "cli-decisions-status-line",
        name = status["agent"]["name"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| bisa_i18n::say(&bisa_core::text!(
                "cli-decisions-decision-making-agent"
            ))),
        standing = if status["enabled"].as_bool() == Some(true) {
            bisa_i18n::say(&bisa_core::text!("cli-decisions-workspace"))
        } else {
            bisa_i18n::say(&bisa_core::text!("cli-decisions-off-workspace"))
        },
        answers_as = answers_as,
        provider = word("provider")
    ))];
    lines.push(match status["problem"].as_str() {
        Some(problem) => bisa_i18n::say(&bisa_core::text!(
            "cli-decisions-cannot-be-asked",
            problem = problem.to_string()
        )),
        None => bisa_i18n::say(&bisa_core::text!("cli-decisions-can-be-asked")),
    });
    if let Some(stored) = status["key_stored"].as_bool() {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-decisions-api-key",
            a0 = (if stored {
                bisa_i18n::say(&bisa_core::text!("cli-decisions-stored"))
            } else {
                bisa_i18n::say(&bisa_core::text!("cli-decisions-none-stored"))
            })
            .to_string()
        )));
    }
    if status["calibrated"].as_bool() == Some(false) {
        lines.push(bisa_i18n::say(&bisa_core::text!(
            "cli-decisions-probabilities-model-s-own-estimate-not"
        )));
    }
    lines.push(bisa_i18n::say(&bisa_core::text!(
        "cli-decisions-acts-from-calls-command-safe-from",
        a0 = (status["confidence_act"]).to_string(),
        a1 = (status["confidence_security"]).to_string(),
        a2 = (status["deadline_secs"]).to_string()
    )));
    for point in status["points"].as_array().into_iter().flatten() {
        lines.push(format!(
            "  {:<18} {}",
            point["point"].as_str().unwrap_or("?"),
            match (
                point["selected_explicitly"].as_bool() == Some(true),
                point["on"].as_bool() == Some(true)
            ) {
                (true, _) => bisa_i18n::say(&bisa_core::text!("cli-decisions-where-selected")),
                (false, true) => bisa_i18n::say(&bisa_core::text!("cli-decisions-point-on")),
                (false, false) => bisa_i18n::say(&bisa_core::text!("cli-decisions-point-off")),
            }
        ));
    }
    lines
}

fn record_lines(records: &Value) -> Vec<String> {
    let rows = records.as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return vec![bisa_i18n::say(&bisa_core::text!(
            "cli-decisions-no-judgement-yet"
        ))];
    }
    rows.iter()
        .map(|row| {
            let j = &row["judgement"];
            format!(
                "{}  {:<18} {:<8} {}{}",
                row["at"],
                j["point"].as_str().unwrap_or("?"),
                j["outcome"].as_str().unwrap_or("?"),
                j["model"].as_str().unwrap_or("?"),
                j["reason"]
                    .as_str()
                    .map(|r| format!(" — {r}"))
                    .unwrap_or_default()
            )
        })
        .collect()
}

fn answer_lines(response: &Value) -> Vec<String> {
    let mut lines = vec![bisa_i18n::say(&bisa_core::text!(
        "cli-decisions-answered",
        a0 = (response["model"].as_str().unwrap_or("?")).to_string()
    ))];
    for (id, answer) in response["answers"].as_object().into_iter().flatten() {
        lines.push(match answer["type"].as_str() {
            Some("noul") => format!("  {id}: {}", answer["noul"]),
            Some("choice") => format!(
                "  {id}: {} (confidence {})",
                answer["choice"].as_str().unwrap_or("?"),
                answer["confidence"]
            ),
            Some("score") => format!(
                "  {id}: {} (confidence {})",
                answer["score"], answer["confidence"]
            ),
            _ => format!("  {id}: {answer}"),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_asks_one_question_from_its_flags() {
        let noul = question_of(Some("Is it urgent?".into()), None, &[]).unwrap();
        assert_eq!(
            noul,
            json!({ "type": "noul", "instructions": "Is it urgent?" })
        );
        let choice = question_of(
            None,
            Some("Which?".into()),
            &["a=the first".into(), "b = the second".into()],
        )
        .unwrap();
        assert_eq!(
            choice["criteria"],
            json!({ "a": "the first", "b": "the second" })
        );
        assert!(question_of(None, Some("Which?".into()), &["a=only".into()]).is_err());
        assert!(question_of(
            None,
            Some("Which?".into()),
            &["no-equals".into(), "b=x".into()]
        )
        .is_err());
        assert!(question_of(None, None, &[]).is_err());
    }

    #[test]
    fn a_key_never_comes_from_the_command_line() {
        let refused = read_key("a-key-typed-in-the-open").unwrap_err().to_string();
        assert!(refused.contains("@stdin"), "{refused}");
        assert!(read_key("@/nonexistent/decision-key-file").is_err());
    }

    #[test]
    fn the_status_says_who_answers_and_never_a_key() {
        let status = json!({
            "agent": { "id": "decision-making-agent", "name": "Decision-Making Agent" },
            "enabled": false, "provider": "jev", "answers_as": "jev-latest",
            "calibrated": true, "ready": false, "problem": "no API key is stored for Jev",
            "key_stored": false, "deadline_secs": 20,
            "confidence_act": 0.7, "confidence_security": 0.9,
            "points": [
                { "point": "model.route", "selected_explicitly": true, "on": true },
                { "point": "assign.pick", "selected_explicitly": false, "on": false }
            ]
        });
        let text = status_lines(&status).join("\n");
        for word in [
            "off for the workspace",
            "answers as jev-latest (jev)",
            "cannot be asked: no API key is stored for Jev",
            "API key: none stored",
            "where it is selected",
        ] {
            assert!(text.contains(word), "{word} is missing from:\n{text}");
        }
        assert_eq!(record_lines(&json!([])), ["no judgement yet"]);
    }

    #[test]
    fn the_status_opens_with_the_agents_name_as_the_node_gave_it() {
        let status = json!({
            "agent": { "id": "decision-making-agent", "name": "Decision-Making Agent" },
            "enabled": true, "provider": "harness",
            "answers_as": "claude-code/claude-sonnet-5", "points": []
        });
        let lines = status_lines(&status);
        assert_eq!(
            lines[0],
            "Decision-Making Agent · on for the workspace · answers as \
             claude-code/claude-sonnet-5 (harness)"
        );
    }

    /// A harness that answers says how hard its model works, beside the
    /// model; a provider that names no effort says none.
    #[test]
    fn the_status_names_the_effort_beside_the_model_when_there_is_one() {
        let status = json!({
            "agent": { "id": "decision-making-agent", "name": "Decision-Making Agent" },
            "enabled": true, "provider": "harness",
            "answers_as": "claude-code/claude-sonnet-5-5[1m]", "effort": "high",
            "points": []
        });
        assert_eq!(
            status_lines(&status)[0],
            "Decision-Making Agent · on for the workspace · answers as \
             claude-code/claude-sonnet-5-5[1m] · high (harness)"
        );
        let remote = json!({
            "enabled": true, "provider": "jev", "answers_as": "jev-latest", "points": []
        });
        assert!(
            status_lines(&remote)[0].ends_with("answers as jev-latest (jev)"),
            "{}",
            status_lines(&remote)[0]
        );
    }

    /// A status that names no agent is still a status: the line opens with the
    /// catalog's word for it, never with a hole.
    #[test]
    fn a_status_that_names_no_agent_opens_with_the_catalogs_name() {
        let status = json!({
            "enabled": false, "provider": "harness",
            "answers_as": "claude-code/claude-sonnet-5", "points": []
        });
        let lines = status_lines(&status);
        assert!(
            lines[0].starts_with("Decision-Making Agent · off for the workspace"),
            "{}",
            lines[0]
        );
    }
}
