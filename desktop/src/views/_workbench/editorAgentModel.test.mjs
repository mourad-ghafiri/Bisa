import test from "node:test";
import assert from "node:assert/strict";

import { agentChoices, askContent, chosenAgent, editContent, GENERAL_AGENT, messageBody, rangeLabel, sentWords, toolbarPlacement } from "./editorAgentModel.mjs";

test("rangeLabel is basename plus one line or a range", () => {
  assert.equal(rangeLabel("src/main.rs", 12, 12), "main.rs:12");
  assert.equal(rangeLabel("src/main.rs", 12, 18), "main.rs:12–18");
  assert.equal(rangeLabel("README", 1, 3), "README:1–3");
});

test("editContent carries the instruction then the do-not-commit contract", () => {
  const c = editContent("rename foo to bar", "the selection main.rs:12–18");
  assert.match(c, /rename foo to bar/);
  assert.match(c, /Edit the selection main\.rs:12–18 directly in the file/);
  assert.match(c, /Do not commit or push — I will keep or undo the change in this conversation\./);
  // An empty instruction still states the target and the contract.
  assert.match(editContent("  ", "the file x.ts"), /^Edit the file x\.ts directly/);
});

test("askContent is the question verbatim, tagged with the target", () => {
  assert.match(askContent("what does this do?", "the selection a.ts:1–2"), /what does this do\?\n\n\(About the selection a\.ts:1–2\.\)/);
  assert.equal(askContent("", "the file a.ts"), "Tell me about the file a.ts.");
});

test("messageBody addresses the agent and carries the chips", () => {
  const chip = { kind: "selection", path: "a.ts", range: { start: 1, end: 2 }, text: "x" };
  const edit = messageBody({ mode: "edit", agentId: GENERAL_AGENT, text: "fix it", target: "the selection a.ts:1–2", chips: [chip] });
  assert.deepEqual(edit.mentions, ["general-agent"]);
  assert.deepEqual(edit.context, [chip]);
  assert.match(edit.content, /fix it/);
  assert.match(edit.content, /Do not commit/);
  const ask = messageBody({ mode: "ask", agentId: "codex", text: "why?", target: "the file a.ts", chips: [chip] });
  assert.deepEqual(ask.mentions, ["codex"]);
  assert.match(ask.content, /why\?/);
});

test("the toolbar sits above the selection when there is room, below it when there is not, and never off the editor", () => {
  const box = { width: 800, height: 600 };
  const bar = { width: 240, height: 28 };
  const at = (start, end) => toolbarPlacement({ anchors: { start, end }, box, bar });

  const above = at({ top: 200, left: 100 }, { bottom: 260, left: 300 });
  assert.deepEqual(above, { top: 166, left: 100, above: true }, "above the first line, aligned to where the selection starts");

  const below = at({ top: 10, left: 100 }, { bottom: 70, left: 300 });
  assert.deepEqual(below, { top: 76, left: 300, above: false }, "no room above: under the last line, where it ends");

  assert.equal(at({ top: 200, left: 780 }, null).left, 560, "a selection at the right edge does not push the bar out");
  assert.equal(at(null, { bottom: 595, left: 10 }).top, 572, "nor does one at the bottom");
  assert.equal(at(null, null), null, "a selection scrolled out of sight gets no bar at all");
  assert.equal(toolbarPlacement({ anchors: {}, box, bar }), null);
  assert.equal(at(null, { bottom: 100, left: 40 }).above, false, "with no start anchor it can only go below");
});

test("the toolbar offers the agents every other addressing surface offers, the one you used last first", () => {
  // `origin` is what marks the core agent, as the store spells it.
  const agents = [
    { id: "general-agent", name: "General Agent", origin: "core", enabled: true },
    { id: "reviewer", name: "Reviewer", origin: "local", enabled: true },
    { id: "dev", name: "Developer", origin: "local", enabled: true },
    { id: "shy", name: "Shy", origin: "local", enabled: false },
  ];
  const core = { id: "general-agent", name: "General Agent" };
  assert.deepEqual(agentChoices(agents, "dev", core).map((a) => a.id), ["dev", "reviewer", "general-agent"], "the remembered one leads; the default is offered even though it is the core agent");
  assert.deepEqual(agentChoices(agents, null, core).map((a) => a.id), ["reviewer", "dev", "general-agent"]);
  assert.deepEqual(agentChoices(agents, "gone", null).map((a) => a.id), ["reviewer", "dev"], "a remembered agent that is no longer offered is simply not first");
  assert.ok(!agentChoices(agents, null, null).some((a) => a.id === "shy"), "a disabled agent is offered nowhere");
  assert.deepEqual(agentChoices([], null, core).map((a) => a.id), ["general-agent"]);
});

test("the toolbar never offers the Workflow Agent — not in the list, not as the project's default", () => {
  const workflow = { id: "workflow-agent", name: "Workflow Agent", origin: "core", enabled: true };
  const agents = [workflow, { id: "reviewer", name: "Reviewer", origin: "local", enabled: true }];
  assert.deepEqual(agentChoices(agents, null, null).map((a) => a.id), ["reviewer"]);
  assert.deepEqual(agentChoices(agents, null, workflow).map((a) => a.id), ["reviewer"], "a default naming the Workflow Agent is not appended: a workstream never reaches it");
  assert.deepEqual(agentChoices([], null, workflow), [], "and with nobody else the pill names nobody rather than an agent that will not answer");
});

test("the chosen agent is the wanted one when offered, else the lead of the list, else nobody", () => {
  const choices = [
    { id: "general-agent", name: "General Agent" },
    { id: "reviewer", name: "Reviewer" },
  ];
  assert.equal(chosenAgent(choices, "reviewer")?.id, "reviewer");
  assert.equal(chosenAgent(choices, "general-agent")?.id, "general-agent", "the General Agent, the project's default unless it says otherwise, is chosen up front");
  assert.equal(chosenAgent(choices, "gone")?.id, "general-agent", "a wanted agent nobody offers falls to the list's lead");
  assert.equal(chosenAgent(choices, null)?.id, "general-agent");
  assert.equal(chosenAgent([], "general-agent"), null, "with nobody offered, nobody is chosen");
});

test("what the toast says names where an edit lands", () => {
  assert.equal(sentWords("ask", "Reviewer"), "Asked Reviewer.");
  assert.match(sentWords("edit", "Reviewer"), /^Asked Reviewer to edit — the change waits in the conversation/);
});
