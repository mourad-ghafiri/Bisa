//! `bisa step`: what a person does to one step of a run — a goal's current
//! run, named by the goal's id, or any run by its own id, a goal's or a run
//! of the workspace — answer a `human` step, release a `wait` step, or mark
//! a `human` step done by hand.
//!
//! Every verb goes through an engine: an answer is a signed decision when a
//! gate is open for the step, and a release is the wait runtime's to record.
//! A running daemon owns the live gates; without one a short-lived embedded
//! engine applies the event to the durable run.

use crate::ctx::Ctx;
use crate::output::Out;
use crate::target::Target;
use anyhow::{bail, Context as _, Result};
use bisa_core::{Answer, RunId, StepId, WorkflowRun};
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand)]
pub enum StepCmd {
    /// Answer a waiting `human` step
    Answer {
        /// A goal's id (its current run), or a run's
        id: String,
        step: String,
        /// Your answer in free text. Optional when you pick an option or say
        /// you are not sure.
        text: Option<String>,
        /// An option id the step offered (repeatable)
        #[arg(long = "option", short = 'o')]
        options: Vec<String>,
        /// You do not know. Not a decline.
        #[arg(long)]
        unsure: bool,
    },
    /// Release a `wait` step a person is holding
    Release {
        /// A goal's id (its current run), or a run's
        id: String,
        step: String,
    },
    /// A `human` step you did by hand: it is done
    Done {
        /// A goal's id (its current run), or a run's
        id: String,
        step: String,
    },
}

pub async fn step(ctx: &Ctx, out: &Out, cmd: StepCmd) -> Result<()> {
    match cmd {
        StepCmd::Answer {
            id,
            step,
            text,
            options,
            unsure,
        } => {
            let answer = Answer {
                selected: options,
                text,
                unsure,
            };
            if answer.is_empty() {
                bail!(bisa_core::text!(
                    "cli-step-say-something-pass-your-answer-as"
                ));
            }
            let (run, step) = ids(ctx, &id, &step)?;
            let run = if let Some(client) = ctx.node_client().await {
                run_of(
                    client
                        .post(
                            &format!("/runs/{run}/steps/{step}/answer"),
                            json!({"answer": answer}),
                        )
                        .await?,
                )?
            } else {
                let (engine, _) = ctx.engine().await?;
                let r = engine.answer_step(run, &step, &answer);
                engine.shutdown().await;
                r?
            };
            report(out, &run, &step, "answered");
            Ok(())
        }
        StepCmd::Release { id, step } => {
            let (run, step) = ids(ctx, &id, &step)?;
            let run = if let Some(client) = ctx.node_client().await {
                run_of(
                    client
                        .post(&format!("/runs/{run}/steps/{step}/release"), json!({}))
                        .await?,
                )?
            } else {
                let (engine, _) = ctx.engine().await?;
                // A person's release carries nothing: the step's output is
                // what a call's payload would have been, and there is none.
                let r = engine.release_step(run, &step, None);
                engine.shutdown().await;
                r?
            };
            report(out, &run, &step, "released");
            Ok(())
        }
        StepCmd::Done { id, step } => {
            let (run, step) = ids(ctx, &id, &step)?;
            let run = if let Some(client) = ctx.node_client().await {
                run_of(
                    client
                        .post(&format!("/runs/{run}/steps/{step}/done"), json!({}))
                        .await?,
                )?
            } else {
                let (engine, _) = ctx.engine().await?;
                let r = engine.mark_step_done(run, &step);
                engine.shutdown().await;
                r?
            };
            report(out, &run, &step, "done");
            Ok(())
        }
    }
}

/// The run a step verb acts on — the goal's current run, or the run the id
/// names — and the step.
fn ids(ctx: &Ctx, id: &str, step: &str) -> Result<(RunId, StepId)> {
    let step = StepId::new(step)
        .with_context(|| bisa_core::text!("cli-step-not-step-id", step = format!("{step:?}")))?;
    let ws = ctx.workspace()?;
    let run = Target::resolve(&ws, id)?.run(&ws)?;
    Ok((run, step))
}

fn run_of(v: serde_json::Value) -> Result<WorkflowRun> {
    serde_json::from_value(v["run"].clone()).context(bisa_core::text!(
        "cli-step-unexpected-run-payload-from-node"
    ))
}

fn report(out: &Out, run: &WorkflowRun, step: &StepId, verb: &str) {
    let state = run.steps.get(step).map(|r| r.state.as_str()).unwrap_or("?");
    out.say(&bisa_core::text!(
        "cli-step-step-run",
        step = step.to_string(),
        verb = verb.to_string(),
        state = state.to_string(),
        a0 = (run.status().as_str()).to_string()
    ));
    out.json_value(json!({"run": run, "step": step.to_string(), "status": run.status()}));
}
