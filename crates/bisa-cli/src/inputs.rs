//! What a person types for a run's inputs — `--input name=value` — read by
//! the kind each input declares.
//!
//! The words are kept as they were typed until the workflow they are for is
//! known: `--input tests=true` is the command `true` for an input that asks
//! for text and a yes for one that asks for a yes or a no, and only the
//! definition says which. The rule itself is the core's
//! ([`bisa_core::InputKind::read`]); what is here is where a verb finds the
//! definition.

use anyhow::{bail, Context, Result};
use bisa_core::{GoalId, Home, InputDef};
use bisa_store::Workspace;
use serde_json::Value;
use std::collections::BTreeMap;

/// The pairs as they were typed, not yet read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Typed(Vec<(String, String)>);

impl Typed {
    /// `name=value`, the name trimmed and the value as it was typed.
    pub(crate) fn parse(pairs: &[String]) -> Result<Self> {
        let mut typed = Vec::with_capacity(pairs.len());
        for raw in pairs {
            let (name, value) = raw.split_once('=').with_context(|| {
                bisa_core::text!(
                    "cli-main-input-must-be-name-value",
                    raw = format!("{raw:?}")
                )
            })?;
            let name = name.trim();
            if name.is_empty() {
                bail!(bisa_core::text!(
                    "cli-main-input-name-empty",
                    raw = format!("{raw:?}")
                ));
            }
            typed.push((name.to_string(), value.to_string()));
        }
        Ok(Self(typed))
    }

    /// Each pair as the value its input's kind makes of it. A name the
    /// definition does not declare keeps the word it was given: the start
    /// refuses it by name, whatever it holds.
    pub(crate) fn read(&self, declared: &[InputDef]) -> BTreeMap<String, Value> {
        self.0
            .iter()
            .map(|(name, typed)| {
                let value = declared
                    .iter()
                    .find(|def| def.name.as_str() == name)
                    .map_or_else(|| Value::String(typed.clone()), |def| def.kind.read(typed));
                (name.clone(), value)
            })
            .collect()
    }

    /// Read for the workflow `raw` names.
    pub(crate) fn read_for(&self, ws: &Workspace, raw: &str) -> BTreeMap<String, Value> {
        self.read(&declared_by(ws, raw))
    }

    /// Read for the workflow `goal` runs — the one it was given, or the
    /// design proposed for it.
    pub(crate) fn read_for_goal(&self, ws: &Workspace, goal: GoalId) -> BTreeMap<String, Value> {
        let declared = ws
            .get_goal(goal)
            .ok()
            .and_then(|goal| goal.workflow)
            .and_then(|workflow| ws.get_workflow(workflow).ok())
            .map(|workflow| workflow.inputs)
            .unwrap_or_default();
        self.read(&declared)
    }

    /// Read for what `home` decides: a goal's gate may adopt a design and
    /// start its run with these; a run of the workspace has its inputs
    /// already.
    pub(crate) fn read_for_home(&self, ws: &Workspace, home: &Home) -> BTreeMap<String, Value> {
        match home {
            Home::Goal { goal } => self.read_for_goal(ws, *goal),
            Home::Run { .. } => self.read(&[]),
        }
    }
}

/// The inputs the workflow `raw` names declares: a workflow here, by its id
/// or its slug, or a template of the catalog that is not installed yet. What
/// names nothing this workspace can read declares nothing — and whoever
/// starts the run says what is wrong with the name.
fn declared_by(ws: &Workspace, raw: &str) -> Vec<InputDef> {
    if let Ok(workflow) = crate::workflow::resolve_workflow(ws, raw, false) {
        return workflow.inputs;
    }
    ws.catalog_workflow_inputs(raw.trim())
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bisa_core::{InputKind, InputName};
    use serde_json::json;

    fn typed(pairs: &[&str]) -> Typed {
        let pairs: Vec<String> = pairs.iter().map(|pair| pair.to_string()).collect();
        Typed::parse(&pairs).expect("pairs")
    }

    fn input(name: &str, kind: InputKind) -> InputDef {
        InputDef {
            name: InputName::new(name).unwrap(),
            label: name.to_string(),
            kind,
            default: None,
            required: false,
        }
    }

    /// `--input tests=true` was refused — *input `tests` wants text, got
    /// true* — and `--input release=2.4` the same: the word was read as JSON
    /// before anybody knew what it was for.
    #[test]
    fn a_word_is_read_by_the_kind_its_input_declares() {
        let declared = [
            input("tests", InputKind::Text),
            input("release", InputKind::Text),
            input("rounds", InputKind::Number),
            input("dry", InputKind::Bool),
        ];
        let read = typed(&[
            "tests=true",
            "release=2.4",
            "rounds=3",
            "dry=true",
            "else=12",
        ])
        .read(&declared);
        assert_eq!(read["tests"], json!("true"));
        assert_eq!(read["release"], json!("2.4"));
        assert_eq!(read["rounds"], json!(3));
        assert_eq!(read["dry"], json!(true));
        assert_eq!(
            read["else"],
            json!("12"),
            "what is not declared is kept as it was typed"
        );
        for (name, def) in declared.iter().map(|def| (def.name.as_str(), def)) {
            assert!(def.kind.accepts(&read[name]), "{name}: {}", read[name]);
        }
    }

    #[test]
    fn a_value_keeps_every_sign_after_the_first() {
        let read = typed(&[" command = test -f a=b ", "empty="]).read(&[]);
        assert_eq!(read["command"], json!(" test -f a=b "));
        assert_eq!(read["empty"], json!(""));
    }

    #[test]
    fn a_pair_with_no_sign_or_no_name_is_refused() {
        for pair in ["tests", "=true", "  =x"] {
            assert!(Typed::parse(&[pair.to_string()]).is_err(), "{pair:?}");
        }
    }
}
