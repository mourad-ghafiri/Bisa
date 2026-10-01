//! `bisa <kind> usage <id>`: what points at a thing, before you try to
//! delete it.
//!
//! The four `rm` verbs refuse while anything still references what they were
//! asked to remove, and the refusal names the first few holders — enough to
//! act on, not enough to plan with. This is the full list, grouped by the kind
//! of thing holding the reference, so "why can I not delete this" has an
//! answer that does not involve attempting the delete.
//!
//! One renderer for all four kinds. A skill and an MCP server are only ever
//! held by agents; an agent and a team are held by half the workspace. The
//! grouping is what keeps the second case readable.

use crate::output::Out;
use bisa_store::{Reference, ReferenceKind, Usage, UsageKind};
use serde_json::json;

/// Print one usage answer: the holders grouped by kind, then the remedy for
/// the nearest one.
///
/// An empty answer says so in the affirmative — "it can be removed" — because
/// silence after a question about deletability reads as a failure rather than
/// a green light.
pub fn render(out: &Out, kind: UsageKind, id: &str, usage: &Usage) {
    if usage.is_empty() {
        out.say(&bisa_core::text!(
            "cli-usage-nothing-points-bisa-rm-would-succeed",
            kind = kind.to_string(),
            id = id.to_string()
        ));
    } else {
        let verb = if usage.len() == 1 {
            bisa_i18n::say(&bisa_core::text!("cli-usage-thing-points"))
        } else {
            bisa_i18n::say(&bisa_core::text!("cli-usage-things-point"))
        };
        out.human(&format!("{} {verb} at {kind} {id}:", usage.len()));
        for group in ReferenceKind::ALL.iter().copied() {
            let rows: Vec<&Reference> = usage.iter().filter(|r| r.kind == group).collect();
            if rows.is_empty() {
                continue;
            }
            out.human(&format!("  {} ({})", group.noun(), rows.len()));
            for r in rows {
                out.human(&format!("    {:<30} {}", r.id, r.label));
            }
        }
        if let Some(nearest) = usage.as_slice().first() {
            out.human(&format!("\n{}", nearest.kind.remedy()));
        }
    }
    out.json_value(json!({"kind": kind.as_str(), "id": id, "usage": usage}));
}
