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
        // One sentence each, never two halves joined: a count and its noun
        // agree differently in every language.
        out.say(&if usage.len() == 1 {
            bisa_core::text!(
                "cli-usage-one-thing-points-at",
                kind = kind.to_string(),
                id = id.to_string()
            )
        } else {
            bisa_core::text!(
                "cli-usage-things-point-at",
                n = usage.len().to_string(),
                kind = kind.to_string(),
                id = id.to_string()
            )
        });
        for group in ReferenceKind::ALL.iter().copied() {
            let rows: Vec<&Reference> = usage.iter().filter(|r| r.kind == group).collect();
            if rows.is_empty() {
                continue;
            }
            out.human(&format!(
                "  {} ({})",
                bisa_i18n::say(&noun(group)),
                rows.len()
            ));
            for r in rows {
                out.human(&format!("    {:<30} {}", r.id, r.label));
            }
        }
        if let Some(nearest) = usage.as_slice().first() {
            out.human(&format!("\n{}", bisa_i18n::say(&remedy(nearest.kind))));
        }
    }
    out.json_value(json!({"kind": kind.as_str(), "id": id, "usage": usage}));
}

/// What a holder is called, worded from its kind — the command line's own
/// words, in the language it speaks, not the store's English.
fn noun(kind: ReferenceKind) -> bisa_core::Text {
    match kind {
        ReferenceKind::Agent => bisa_core::text!("cli-usage-noun-agent"),
        ReferenceKind::Team => bisa_core::text!("cli-usage-noun-team"),
        ReferenceKind::Channel => bisa_core::text!("cli-usage-noun-channel"),
        ReferenceKind::Goal => bisa_core::text!("cli-usage-noun-goal"),
        ReferenceKind::Project => bisa_core::text!("cli-usage-noun-project"),
        ReferenceKind::WorkItem => bisa_core::text!("cli-usage-noun-work-item"),
        ReferenceKind::Workflow => bisa_core::text!("cli-usage-noun-workflow"),
        ReferenceKind::Governance => bisa_core::text!("cli-usage-noun-gate-policy"),
        ReferenceKind::Account => bisa_core::text!("cli-usage-noun-account"),
    }
}

/// The way out for the nearest holder, worded from its kind.
fn remedy(kind: ReferenceKind) -> bisa_core::Text {
    match kind {
        ReferenceKind::Agent => bisa_core::text!("cli-usage-remedy-agent"),
        ReferenceKind::Team => bisa_core::text!("cli-usage-remedy-team"),
        ReferenceKind::Channel => bisa_core::text!("cli-usage-remedy-channel"),
        ReferenceKind::Goal => bisa_core::text!("cli-usage-remedy-goal"),
        ReferenceKind::Project => bisa_core::text!("cli-usage-remedy-project"),
        ReferenceKind::WorkItem => bisa_core::text!("cli-usage-remedy-work-item"),
        ReferenceKind::Workflow => bisa_core::text!("cli-usage-remedy-workflow"),
        ReferenceKind::Governance => bisa_core::text!("cli-usage-remedy-gate-policy"),
        ReferenceKind::Account => bisa_core::text!("cli-usage-remedy-account"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In English the command line's words for a holder and its way out are
    /// the store's own — the ones its refusal carries — so `usage` and a
    /// refused `rm` say one thing; every kind has both.
    #[test]
    fn a_holders_noun_and_remedy_are_the_stores_words_in_english() {
        for kind in ReferenceKind::ALL.iter().copied() {
            assert_eq!(bisa_i18n::say(&noun(kind)), kind.noun(), "{kind:?}");
            assert_eq!(bisa_i18n::say(&remedy(kind)), kind.remedy(), "{kind:?}");
        }
    }
}
