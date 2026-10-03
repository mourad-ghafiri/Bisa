/**
 * The name a terminal's attached lines carry. Run with `npm test` from `desktop/`.
 */
import test from "node:test";
import assert from "node:assert/strict";

import { terminalChipName } from "./terminalChipModel.mjs";
import { emptyTerminals, openTerminal } from "./terminalsModel.mjs";

test("a terminal chip is named in the tab's words — never the bare literal, never the internal key", () => {
  let state = emptyTerminals();
  state = openTerminal(state, { scope: "workstream", id: "w1", label: "fix-total" });
  state = openTerminal(state, { scope: "workstream", id: "w1", label: "fix-total", harness: "claude" });
  const [shell, claude] = state.sessions;
  const name = terminalChipName(shell, state.sessions);
  assert.equal(name, "shell · fix-total");
  assert.ok(!name.includes(shell.key), "the tab's key stays inside");
  assert.equal(terminalChipName(claude, state.sessions), "claude · fix-total");
});

test("two tabs that read alike are numbered along the strip, so the second never takes the first one's chip", () => {
  let state = emptyTerminals();
  state = openTerminal(state, { scope: "workstream", id: "w1", label: "fix-total" });
  state = openTerminal(state, { scope: "workstream", id: "w1", label: "fix-total" });
  const [first, second] = state.sessions;
  assert.equal(terminalChipName(first, state.sessions), "shell · fix-total (1)");
  assert.equal(terminalChipName(second, state.sessions), "shell · fix-total (2)");
});
