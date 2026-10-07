//! `bisa workspace reindex`: rebuild `index.sqlite` from the truth
//! files. The index is a cache the workspace already rebuilds on its own —
//! at a schema change, when `quick_check` fails at open — so this verb is for
//! the doubt nothing else answers: a list that disagrees with a file. It runs
//! only when no engine holds the workspace, because a node's writes and a
//! rebuild would race over one connection.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::{bail, Result};
use serde_json::json;

pub fn reindex(ctx: &Ctx, out: &Out) -> Result<()> {
    let paths = bisa_store::Paths::new(&ctx.data_dir);
    if let Some(holder) = bisa_engine::EngineLock::holder(&paths)? {
        bail!(bisa_core::text!(
            "cli-reindex-node-holds-workspace-pid-stop-first",
            a0 = (holder.pid).to_string()
        ));
    }
    let ws = ctx.workspace()?;
    let started = std::time::Instant::now();
    // The stages as they pass, so a long rebuild reads as work; the records
    // it skipped, named, so nothing is lost in silence.
    ws.rebuild_index_observed(&Stages { out })?;
    let secs = started.elapsed().as_secs_f64();
    out.say(&bisa_core::text!(
        "cli-reindex-index-rebuilt-from-truth-files-s",
        secs = format!("{secs:.1}"),
        a0 = (ws.list_goals(None)?.len()).to_string(),
        a1 = (ws.list_projects()?.len()).to_string()
    ));
    let problems = ws.problems();
    for problem in &problems {
        out.say(&bisa_core::text!(
            "cli-reindex-problem",
            text = problem.text.to_string()
        ));
    }
    out.json_value(json!({"rebuilt": true, "seconds": secs, "problems": problems}));
    Ok(())
}

/// The rebuild's phases, said as they pass.
struct Stages<'a> {
    out: &'a Out,
}

impl bisa_store::BootObserver for Stages<'_> {
    fn phase(&self, phase: bisa_store::BootPhase) {
        if let bisa_store::BootPhase::RebuildingIndex { done, of } = phase {
            self.out.say(&bisa_core::text!(
                "cli-reindex-rebuilding-goals",
                done = done.to_string(),
                of = of.to_string()
            ));
        }
    }
}
