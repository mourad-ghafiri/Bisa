//! Workstream scripts (ide/07 §Workstream scripts): shell a project's people
//! wrote, run by the engine around a checkout's life — **pre-create** in the
//! project root before the workstream exists, **post-create** in the new
//! checkout, **clean** in the checkout before it is deleted — and the
//! **run** command (ide/18), the one that serves or runs the checkout, which
//! the engine never runs itself: the IDE's Terminal menu opens it in a
//! terminal the person watches, once this machine has approved it like the
//! others.
//!
//! Three rules hold here and nowhere else:
//!
//! - **The text is a project setting; the trust is a machine fact.** A
//!   project's `settings.json` syncs, so a script is remote-authored shell by
//!   the time it reaches a collaborator. It runs on a machine only when the
//!   SHA-256 of its text is in that machine's `workstreams.script.trusted`
//!   list — written by *Approve* under About › Settings, never by sync. An
//!   unapproved script is a refusal (or, where nobody is there to be refused,
//!   a reported skip), never a silent run.
//! - **The script is the author's, verbatim.** Nothing is rendered or
//!   substituted into it; its facts arrive as `BISA_*` environment
//!   variables, the engine's reserved prefix, so a script cannot be made to
//!   say something its author did not type.
//! - **Bounded and visible.** `sh -c`, a null stdin, no git prompt, a timeout
//!   from `workstreams.script.timeout_secs` after which the child is
//!   terminated; the outcome — the exit status and the tail of what it
//!   printed — is journalled and put on the bus every time.
//!
//! The runner is the shape `effects::run_command_check` and a check start's
//! command each had half of: a working directory *and* an environment *and* a
//! timeout.

use crate::events::EnginePayload;
use crate::{EngineError, Inner};
use bisa_core::{GoalId, Project, SettingScope, WorkItemId, WorkstreamId};
use sha2::Digest as _;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

/// The three moments a lifecycle script runs, and the run command the IDE's
/// Terminal menu opens in a terminal.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    PreCreate,
    PostCreate,
    Clean,
    Run,
}

impl Phase {
    /// Every script a project may set, the run command last.
    pub const ALL: [Phase; 4] = [
        Phase::PreCreate,
        Phase::PostCreate,
        Phase::Clean,
        Phase::Run,
    ];
    /// The three the engine runs around a checkout's life.
    pub const LIFECYCLE: [Phase; 3] = [Phase::PreCreate, Phase::PostCreate, Phase::Clean];

    /// The setting that holds the text.
    pub fn key(self) -> &'static str {
        match self {
            Phase::PreCreate => "workstreams.script.pre_create",
            Phase::PostCreate => "workstreams.script.post_create",
            Phase::Clean => "workstreams.script.clean",
            Phase::Run => "workstreams.script.run",
        }
    }

    /// The wire and environment word.
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::PreCreate => "pre_create",
            Phase::PostCreate => "post_create",
            Phase::Clean => "clean",
            Phase::Run => "run",
        }
    }

    /// The word in a sentence.
    pub fn label(self) -> &'static str {
        match self {
            Phase::PreCreate => "pre-create",
            Phase::PostCreate => "post-create",
            Phase::Clean => "clean",
            Phase::Run => "run",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

pub const TIMEOUT_KEY: &str = "workstreams.script.timeout_secs";
pub const TRUSTED_KEY: &str = "workstreams.script.trusted";
/// Characters of output kept — the tail, because a failing command says why
/// on its last line. Shared with the check starts.
pub const OUTPUT_TAIL_CHARS: usize = 2000;

/// What the caller does with a failure: refuse the operation it guards, or
/// report and carry on — for the paths that never asked a person (an agent
/// run settling its copy, a project being deleted whole).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptPolicy {
    Refuse,
    Report,
}

/// One script as a settings surface sees it: the text, and whether this
/// machine will run it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct ScriptStatus {
    pub phase: Phase,
    pub command: String,
    pub trusted: bool,
}

/// The facts a script is handed, as `BISA_*` variables.
pub struct ScriptContext<'a> {
    pub project: &'a Project,
    /// The project's root checkout on disk (`Workspace::project_root_path`).
    pub project_root: &'a Path,
    pub workstream: WorkstreamId,
    pub path: &'a Path,
    /// The branch — the final one after creation, the intended one before it;
    /// `None` for a copy workstream.
    pub branch: Option<&'a str>,
    pub base: Option<&'a str>,
    /// Where the outcome is journalled: the goal the workstream was made for.
    pub goal: Option<GoalId>,
    pub work_item: Option<WorkItemId>,
    pub agent: Option<&'a str>,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptOutcome {
    /// The exit status; `None` when it timed out or could not be started.
    pub status: Option<i32>,
    /// The tail of stdout then stderr.
    pub output: String,
    pub timed_out: bool,
}

impl ScriptOutcome {
    pub fn ok(&self) -> bool {
        self.status == Some(0)
    }

    /// Why it is not a success, in a sentence fragment.
    pub fn reason(&self, timeout: Duration) -> String {
        match (self.timed_out, self.status) {
            (true, _) => format!("timed out after {}s and was terminated", timeout.as_secs()),
            (false, Some(0)) => "succeeded".to_string(),
            (false, Some(code)) => format!("exited with status {code}"),
            (false, None) => "could not be started".to_string(),
        }
    }
}

/// The SHA-256 of a script's text, hex — what the machine's trust list holds.
pub fn digest(text: &str) -> String {
    hex::encode(sha2::Sha256::digest(text.as_bytes()))
}

/// The last [`OUTPUT_TAIL_CHARS`] characters, trimmed.
pub fn tail(text: &str) -> String {
    let tail: String = text
        .chars()
        .rev()
        .take(OUTPUT_TAIL_CHARS)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    tail.trim().to_string()
}

fn text_of(inner: &Inner, project: &Project, phase: Phase) -> Result<String, EngineError> {
    Ok(inner
        .ws
        .setting(phase.key(), Some(project.id))?
        .value
        .as_str()
        .map(str::trim)
        .unwrap_or("")
        .to_string())
}

fn trusted_digests(inner: &Inner) -> Result<Vec<String>, EngineError> {
    Ok(inner
        .ws
        .setting(TRUSTED_KEY, None)?
        .value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn timeout_of(inner: &Inner, project: &Project) -> Result<Duration, EngineError> {
    let secs = inner
        .ws
        .setting(TIMEOUT_KEY, Some(project.id))?
        .value
        .as_u64()
        .unwrap_or(300)
        .max(1);
    Ok(Duration::from_secs(secs))
}

/// The run command (ide/18): what the IDE's Terminal menu opens in a
/// terminal, once this machine has approved it. `None` when the project
/// sets none.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct RunCommand {
    pub command: String,
    pub trusted: bool,
    /// The SHA-256 of the text — what approving puts on the trust list.
    pub digest: String,
}

/// The project's run command and whether this machine will run it.
pub fn run_command(inner: &Inner, project: &Project) -> Result<Option<RunCommand>, EngineError> {
    let command = text_of(inner, project, Phase::Run)?;
    if command.is_empty() {
        return Ok(None);
    }
    let digest = digest(&command);
    let trusted = trusted_digests(inner)?.contains(&digest);
    Ok(Some(RunCommand {
        command,
        trusted,
        digest,
    }))
}

/// The project's four scripts and whether this machine will run each.
pub fn status(inner: &Inner, project: &Project) -> Result<Vec<ScriptStatus>, EngineError> {
    let trusted = trusted_digests(inner)?;
    Phase::ALL
        .iter()
        .map(|&phase| {
            let command = text_of(inner, project, phase)?;
            let trusted = command.is_empty() || trusted.contains(&digest(&command));
            Ok(ScriptStatus {
                phase,
                command,
                trusted,
            })
        })
        .collect()
}

/// Approve the project's current scripts on this machine: their digests join
/// `workstreams.script.trusted`. Nothing else in the tree writes that list.
pub fn approve(inner: &Inner, project: &Project) -> Result<Vec<ScriptStatus>, EngineError> {
    let mut trusted = trusted_digests(inner)?;
    for phase in Phase::ALL {
        let command = text_of(inner, project, phase)?;
        if command.is_empty() {
            continue;
        }
        let d = digest(&command);
        if !trusted.contains(&d) {
            trusted.push(d);
        }
    }
    crate::settings::set(
        inner,
        SettingScope::Machine,
        None,
        TRUSTED_KEY,
        serde_json::Value::Array(trusted.into_iter().map(serde_json::Value::String).collect()),
    )?;
    status(inner, project)
}

/// Run `command` in `cwd` with `env`, bounded by `timeout`; the child is
/// terminated when the bound passes. Never an error: what happened is data.
pub async fn run(
    command: &str,
    cwd: &Path,
    env: &BTreeMap<String, String>,
    env_remove: &[String],
    timeout: Duration,
) -> ScriptOutcome {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(cwd)
        .envs(env)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    for name in env_remove {
        cmd.env_remove(name);
    }
    let child = cmd.output();
    match tokio::time::timeout(timeout, child).await {
        Ok(Ok(out)) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            ScriptOutcome {
                status: out.status.code(),
                output: tail(&text),
                timed_out: false,
            }
        }
        Ok(Err(e)) => ScriptOutcome {
            status: None,
            output: format!("could not be started: {e}"),
            timed_out: false,
        },
        Err(_) => ScriptOutcome {
            status: None,
            output: String::new(),
            timed_out: true,
        },
    }
}

fn env_for(ctx: &ScriptContext<'_>, phase: Phase) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("BISA_SCRIPT_PHASE".to_string(), phase.as_str().to_string()),
        (
            "BISA_PROJECT_PATH".to_string(),
            ctx.project_root.display().to_string(),
        ),
        (
            "BISA_PROJECT_SLUG".to_string(),
            ctx.project.slug.to_string(),
        ),
        ("BISA_WORKSTREAM_ID".to_string(), ctx.workstream.to_string()),
        (
            "BISA_WORKSTREAM_PATH".to_string(),
            ctx.path.display().to_string(),
        ),
        (
            "BISA_BRANCH".to_string(),
            ctx.branch.unwrap_or("").to_string(),
        ),
        ("BISA_BASE".to_string(), ctx.base.unwrap_or("").to_string()),
    ])
}

/// What the journal and the bus say about a run.
fn report(inner: &Inner, ctx: &ScriptContext<'_>, phase: Phase, ok: bool, words: &str) {
    // The log says that it ran, for which checkout, and how it ended — a
    // hand-opened workstream has no goal and so no journal, and this line is
    // then the only one that outlives the toast. Never the output: a script
    // prints what it likes, an environment included.
    if ok {
        tracing::info!(project = %ctx.project.id, workstream = %ctx.workstream, phase = phase.as_str(), "a workstream script ran");
    } else {
        tracing::warn!(project = %ctx.project.id, workstream = %ctx.workstream, phase = phase.as_str(), output_bytes = words.len(), "a workstream script failed; its output is on the bus and, for a goal's workstream, in the journal");
    }
    // The script's home: the checkout's goal, or the run of the workspace
    // whose item opened it.
    let home = ctx.goal.map(bisa_core::Home::from).or_else(|| {
        ctx.work_item
            .and_then(|item| inner.ws.home_of_work_item(item).ok())
    });
    if let Some(home) = home {
        crate::projects::journal_fact(
            inner,
            home,
            ctx.work_item,
            ctx.agent,
            "ran",
            &format!("the {phase} script"),
            Some(words),
        );
    }
    let payload = EnginePayload::WorkstreamScriptRan {
        workstream: ctx.workstream,
        project: ctx.project.id,
        phase,
        ok,
        output: words.to_string(),
    };
    inner.emit(match &home {
        Some(home) => inner.home_scope(home).event(ctx.work_item, payload),
        None => crate::events::EngineEvent::global(payload),
    });
}

/// Run the project's script for `phase`, if any, in `cwd`.
///
/// No text → nothing happens. An unapproved text, a non-zero exit, a timeout
/// or a spawn failure → under [`ScriptPolicy::Refuse`] an
/// [`EngineError::WorkstreamScript`] the caller lets stand; under
/// [`ScriptPolicy::Report`] a journalled, broadcast note and `Ok`.
pub async fn run_phase(
    inner: &Inner,
    ctx: &ScriptContext<'_>,
    phase: Phase,
    cwd: &Path,
    policy: ScriptPolicy,
) -> Result<(), EngineError> {
    // The run command is the IDE's Terminal menu's, opened in a terminal the
    // person watches; it is never a lifecycle script the engine runs on its
    // own.
    if phase == Phase::Run {
        return Err(EngineError::Invalid(bisa_core::text!(
            "error-engine-invalid-run-command-opened-from-ide-s-terminal"
        )));
    }
    let command = text_of(inner, ctx.project, phase)?;
    if command.is_empty() {
        return Ok(());
    }
    if !trusted_digests(inner)?.contains(&digest(&command)) {
        let reason = "is not approved on this machine — review and approve it under About › Settings › Workstream scripts";
        report(inner, ctx, phase, false, &format!("skipped: {reason}"));
        return match policy {
            ScriptPolicy::Refuse => Err(EngineError::WorkstreamScript {
                phase,
                reason: reason.to_string(),
                output: String::new(),
            }),
            ScriptPolicy::Report => Ok(()),
        };
    }
    // Trusted by hash, still read by the rules: a script that spells a
    // refused shape does not run on this machine whoever approved it.
    if let Some(hit) = crate::security::refuse_by_rules_lines(inner, &command, Some(cwd)) {
        let reason = format!("line {} {}", hit.line, hit.reason);
        report(inner, ctx, phase, false, &format!("refused: {reason}"));
        return match policy {
            ScriptPolicy::Refuse => Err(EngineError::WorkstreamScript {
                phase,
                reason,
                output: String::new(),
            }),
            ScriptPolicy::Report => Ok(()),
        };
    }
    let timeout = timeout_of(inner, ctx.project)?;
    // The proxy the platform follows, handed to the script as its tools read it.
    let (env, env_remove) = crate::network::session_env(inner, env_for(ctx, phase));
    let outcome = run(&command, cwd, &env, &env_remove, timeout).await;
    let reason = outcome.reason(timeout);
    let words = if outcome.output.is_empty() {
        reason.clone()
    } else {
        format!("{reason}\n{}", outcome.output)
    };
    report(inner, ctx, phase, outcome.ok(), &words);
    if outcome.ok() {
        return Ok(());
    }
    match policy {
        ScriptPolicy::Refuse => Err(EngineError::WorkstreamScript {
            phase,
            reason,
            output: outcome.output,
        }),
        ScriptPolicy::Report => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_is_the_texts_and_changes_with_it() {
        assert_eq!(digest("npm install"), digest("npm install"));
        assert_ne!(digest("npm install"), digest("npm install "));
        assert_eq!(digest("").len(), 64);
    }

    #[test]
    fn the_phases_have_one_word_each_and_a_setting_apiece() {
        let keys: Vec<_> = Phase::ALL.iter().map(|p| p.key()).collect();
        assert_eq!(
            keys,
            [
                "workstreams.script.pre_create",
                "workstreams.script.post_create",
                "workstreams.script.clean",
                "workstreams.script.run"
            ]
        );
        assert_eq!(
            Phase::LIFECYCLE.to_vec(),
            Phase::ALL[..3].to_vec(),
            "the engine runs the first three; the run command is the terminal's"
        );
        for p in Phase::ALL {
            assert!(
                bisa_core::SettingDef::lookup(p.key()).is_some(),
                "{} is registered",
                p.key()
            );
        }
        assert!(bisa_core::SettingDef::lookup(TIMEOUT_KEY).is_some());
        assert!(bisa_core::SettingDef::lookup(TRUSTED_KEY).is_some());
        assert_eq!(Phase::PreCreate.to_string(), "pre-create");
        assert_eq!(
            serde_json::to_value(Phase::PostCreate).unwrap(),
            serde_json::json!("post_create")
        );
    }

    #[test]
    fn the_tail_keeps_the_last_words() {
        let long = "x".repeat(OUTPUT_TAIL_CHARS + 10) + "\nthe reason\n";
        let t = tail(&long);
        assert!(t.ends_with("the reason"));
        assert!(t.chars().count() <= OUTPUT_TAIL_CHARS);
        assert_eq!(tail("  hi \n"), "hi");
    }

    #[tokio::test]
    async fn a_run_reports_its_status_output_and_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let env = BTreeMap::from([("BISA_SCRIPT_PHASE".to_string(), "clean".to_string())]);
        let ok = run(
            "echo \"$BISA_SCRIPT_PHASE\"; pwd",
            dir.path(),
            &env,
            &[],
            Duration::from_secs(10),
        )
        .await;
        assert!(ok.ok(), "{ok:?}");
        assert!(ok.output.starts_with("clean"), "{}", ok.output);
        assert!(
            ok.output
                .contains(&dir.path().canonicalize().unwrap().display().to_string())
                || ok.output.contains(&dir.path().display().to_string())
        );
        let failed = run(
            "echo nope >&2; exit 3",
            dir.path(),
            &env,
            &[],
            Duration::from_secs(10),
        )
        .await;
        assert_eq!(failed.status, Some(3));
        assert_eq!(failed.output, "nope");
        assert_eq!(
            failed.reason(Duration::from_secs(10)),
            "exited with status 3"
        );
        let slow = run("sleep 5", dir.path(), &env, &[], Duration::from_millis(200)).await;
        assert!(slow.timed_out && slow.status.is_none());
        assert!(slow
            .reason(Duration::from_millis(200))
            .contains("terminated"));
    }
}
