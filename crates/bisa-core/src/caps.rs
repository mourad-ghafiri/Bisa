//! Capability vocabulary shared across the platform.

use serde::{Deserialize, Serialize};

bitflags::bitflags! {
    /// What a harness adapter can do. The orchestrator degrades gracefully on
    /// missing capabilities instead of assuming (e.g. no STEER -> queue a
    /// follow-up; no RESUME -> replay context into a fresh session).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct HarnessCaps: u32 {
        /// Inject input mid-run, delivered before the next model call.
        const STEER                 = 1 << 0;
        /// Queue input delivered when the agent would otherwise stop.
        const FOLLOW_UP             = 1 << 1;
        /// Re-attach to a previous session by harness-native handle.
        const RESUME                = 1 << 2;
        /// Branch a session into a new one sharing history.
        const FORK                  = 1 << 3;
        /// Accepts MCP server configuration at launch.
        const MCP_SERVERS           = 1 << 4;
        /// Can emit schema-conforming structured output.
        const STRUCTURED_OUTPUT     = 1 << 5;
        /// Accepts image input.
        const IMAGE_INPUT           = 1 << 6;
        /// Raises `InputRequested` — a permission, a question, a sign-in —
        /// and stops until the engine answers through `answer`. The adapter
        /// never decides one itself.
        const INPUT_REQUESTS        = 1 << 7;
        /// Reports token/cost usage.
        const COST_REPORTING        = 1 << 8;
        /// Spawns sub-agents of its own and says so: `SubagentStarted` /
        /// `SubagentEnded`, with the sub-agent's progress wrapped in
        /// `Nested`. Declared only by an adapter that emits them.
        const SUBAGENTS             = 1 << 9;
        /// Stops before a tool runs and obeys the engine's `Deny`: the Tool &
        /// Commands Guard can veto a call. Without it the harness runs under
        /// its own sandbox and the platform only observes.
        const TOOL_GUARD            = 1 << 10;
        /// Runs the tool with the input the engine hands back in `Allow`, so
        /// a redacted placeholder can be restored right before execution.
        const INPUT_REWRITE         = 1 << 11;
        /// Reports its account's usage limits — the five-hour and weekly
        /// windows — from its own source. Without it, `usage()` answers
        /// *unsupported* in the harness's own words.
        const USAGE_REPORTING       = 1 << 12;
        /// Takes an effort — how hard the model works — at launch. Which
        /// levels, the adapter's `efforts` says per model; a model it lists
        /// none for is sent nothing. Without the flag nothing is ever sent,
        /// and a step that pins an effort on such a harness alone is a
        /// problem.
        const EFFORT                = 1 << 13;
    }
}

/// Risk tier of a tool action. Unknown or malformed tool names classify as
/// `Exec` — the safe default (omp's approval-mode rule).
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    /// Reads state, no side effects.
    Read,
    /// Mutates files inside the work-item's workspace.
    Write,
    /// Arbitrary execution or effects outside the workspace.
    Exec,
}

/// The prefix every MCP tool name wears in a harness: `mcp__<server>__<tool>`.
pub const MCP_TOOL_PREFIX: &str = "mcp__";

/// The verbs an MCP tool's own name starts with when it only reads.
const MCP_READ_VERBS: [&str; 10] = [
    "list", "get", "read", "search", "find", "fetch", "describe", "show", "query", "status",
];

impl ToolTier {
    /// Classify a tool by name, defaulting to the most restricted tier. An
    /// MCP tool (`mcp__<server>__<tool>`) classifies by its own verb: one
    /// that lists, gets, reads, searches, finds, fetches, describes, shows,
    /// queries or reports a status reads; every other one is `Exec`, so a
    /// step's ceiling applies to a server nobody wrote a rule for.
    pub fn classify(tool_name: &str) -> ToolTier {
        if let Some(rest) = tool_name.strip_prefix(MCP_TOOL_PREFIX) {
            let tool = rest.split_once("__").map(|(_, t)| t).unwrap_or(rest);
            let verb = tool.split(['_', '-']).next().unwrap_or(tool);
            return if MCP_READ_VERBS.contains(&verb.to_ascii_lowercase().as_str()) {
                ToolTier::Read
            } else {
                ToolTier::Exec
            };
        }
        match tool_name {
            "read" | "read_file" | "grep" | "glob" | "ls" | "list_dir" | "web_search" | "fetch" => {
                ToolTier::Read
            }
            "write" | "write_file" | "edit" | "str_replace" | "multi_edit" => ToolTier::Write,
            _ => ToolTier::Exec,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tools_are_exec() {
        assert_eq!(ToolTier::classify("bash"), ToolTier::Exec);
        assert_eq!(ToolTier::classify("totally_new_tool"), ToolTier::Exec);
        assert_eq!(ToolTier::classify(""), ToolTier::Exec);
    }

    #[test]
    fn an_mcp_tool_classifies_by_its_own_verb() {
        for read in [
            "mcp__gh__list_issues",
            "mcp__gh__get_pull_request",
            "mcp__fs__read_file",
            "mcp__docs__search",
            "mcp__k8s__describe-pod",
            "mcp__ci__status",
        ] {
            assert_eq!(ToolTier::classify(read), ToolTier::Read, "{read}");
        }
        for exec in [
            "mcp__gh__create_issue",
            "mcp__fs__write_file",
            "mcp__computer__click",
            "mcp__shell__run",
            "mcp__gh__",
            "mcp__",
        ] {
            assert_eq!(ToolTier::classify(exec), ToolTier::Exec, "{exec}");
        }
    }

    #[test]
    fn tiers_are_ordered_by_risk() {
        assert!(ToolTier::Read < ToolTier::Write);
        assert!(ToolTier::Write < ToolTier::Exec);
    }

    #[test]
    fn the_effort_flag_is_its_own_bit() {
        assert_eq!(HarnessCaps::EFFORT.bits(), 1 << 13);
        // Fourteen flags, and this one shares its bit with none of them.
        assert_eq!(HarnessCaps::all().bits().count_ones(), 14);
        let others = HarnessCaps::all() - HarnessCaps::EFFORT;
        assert_eq!(others.bits().count_ones(), 13);
        let caps = HarnessCaps::RESUME | HarnessCaps::EFFORT;
        let json = serde_json::to_string(&caps).unwrap();
        assert_eq!(serde_json::from_str::<HarnessCaps>(&json).unwrap(), caps);
    }

    #[test]
    fn caps_serde_roundtrip() {
        let caps = HarnessCaps::STEER | HarnessCaps::RESUME | HarnessCaps::MCP_SERVERS;
        let json = serde_json::to_string(&caps).unwrap();
        let back: HarnessCaps = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, back);
    }
}
