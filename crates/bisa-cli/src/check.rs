//! `bisa workspace check`: every file of the workspace read for what the
//! next open would do with it — without opening it, without the engine
//! lock, without the index (`bisa_store::check_files`). After a crash, this
//! is the list: the owner key, the member file, the governance document,
//! every settings layer, every snapshot, every journal tail, and what
//! earlier opens moved under `quarantine/`. One line a finding, the whole
//! list under `--json`, exit 1 while anything is found. It runs beside a
//! node as well as without one: it writes nothing.

use crate::ctx::Ctx;
use crate::output::Out;
use anyhow::Result;
use serde_json::json;

pub fn check(ctx: &Ctx, out: &Out) -> Result<()> {
    let paths = bisa_store::Paths::new(&ctx.data_dir);
    let findings = bisa_store::check_files(&paths)?;
    for finding in &findings {
        out.say(&bisa_core::text!(
            "cli-check-finding",
            text = finding.text.to_string()
        ));
    }
    out.say(&if findings.is_empty() {
        bisa_core::text!("cli-check-clean")
    } else {
        bisa_core::text!("cli-check-problems", n = findings.len().to_string())
    });
    let ok = findings.is_empty();
    out.json_value(json!({"ok": ok, "findings": findings}));
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}
