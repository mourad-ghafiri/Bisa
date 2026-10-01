/**
 * The Decision-Making Agent as a person meets it (15 — The Decision-Making
 * Agent; guide/decisions): the third core agent's card on the Agents
 * screen; Settings › Decision Settings › Decision Making — whether it can
 * be asked, who answers and the key it takes, the four ways it is switched
 * on, the points and the ones left to their own rule, a question tried
 * with nothing decided, the judgements it made; and a workflow's `judge`
 * step — its options, how sure a pick must be, `otherwise` — drawn, and on
 * a run. Every step reads the models the screens draw from; no DOM. Run with
 * `node --test --import ./desktop/src/i18n/preload.mjs desktop/src/scenarios/decisionMaking.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  KEYS,
  POINT_IDS,
  answerLines,
  blankKey,
  blankTryForm,
  calibratedNote,
  coreLine,
  fieldsFor,
  judgementLine,
  keyFor,
  keySaved,
  keyTyped,
  mayClearKey,
  maySaveKey,
  movesJudgements,
  movesStatus,
  offList,
  pointRows,
  pointsOffAfter,
  readyLine,
  takesKey,
  tryProblem,
  tryRequest,
} from "../views/_settings/decisionsModel.mjs";
import { classifierFieldsFor } from "../views/_settings/securityRules.mjs";
import { draftOf, saveOf } from "../views/_work/agentDraftModel.mjs";
import { progressRows } from "../views/_goal/progressModel.mjs";
import { addOption, judgeWords, setConfidence, setMeaning } from "../views/_workflow/forms/judgeStepModel.mjs";
import { edgeTone, stepLabel, stepTone } from "../views/_workflow/runView.mjs";
import { blankStep, blankWorkflow, branchesOf, familyOf, START_STEP_ID } from "../views/_workflow/stepKinds.mjs";
import { addStep, connect, relabelBranch, replaceStep, toGraph } from "../views/_workflow/workflowGraph.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const must = (r) => {
  assert.equal(r.ok, true, r.reason);
  return r;
};

const agent = { id: "decision-making-agent", name: "Decision-Making Agent", description: "Judges where a decision point asks it." };
/** The points as the node lists them: the six selected by name are on whatever the switch says. */
const SELECTED = ["model.route", "model.effort", "security.tool", "security.message", "security.content", "workflow.judge"];
const points = (enabled, off = []) => POINT_IDS.map((point) => ({ point, selected_explicitly: SELECTED.includes(point), on: SELECTED.includes(point) || (enabled && !off.includes(point)) }));
const status = (over = {}) => {
  const enabled = over.enabled ?? false;
  return { agent, enabled, provider: "harness", answers_as: "the harness claude-code · claude-sonnet-5-5[1m]", effort: "high", calibrated: false, ready: true, points: points(enabled, over.off ?? []), deadline_secs: 20, confidence_act: 0.7, confidence_security: 0.9, ...over };
};

test("the third core agent's card: off until it is switched on, then who answers for it — and its door is Settings › Decision Making", () => {
  assert.equal(coreLine(null), "", "while the status is read the card says so itself");
  assert.equal(coreLine(status()), "off");
  assert.equal(coreLine(status({ enabled: true })), "on for the workspace · the harness claude-code · claude-sonnet-5-5[1m]");
  assert.equal(coreLine(null, "the node is unreachable"), "could not be read: the node is unreachable", "a read that failed is said, never *reading…* for good");
  const agents = src("../views/Agents.tsx");
  assert.ok(agents.includes('navigate({ name: "settings" }, settingsSearch("decision-making"))'), "it has no page of its own, and nothing messages it");
  assert.ok(agents.includes("coreLine(status.data, status.error)") && agents.includes("if (movesStatus(e.payload)) reload();") && agents.includes("useReloadOnReconnect(reload);"), "the card follows the switch, and the node coming back");
  const card = agents.slice(agents.indexOf("function DecisionMakingAgentCard() {"), agents.indexOf("function ModelPlanSummary("));
  assert.ok(card.includes("<ICON.decisions") && card.includes("status.data?.agent.name"), "the card is read");
  assert.ok(!/api\.(decisionsTry|openDm|patchAgent|deleteAgent)|openDm\(|AgentMenu/.test(card), "the card asks nothing of it, opens no conversation with it, and offers no verb on it");
});

test("whether it can be asked: ready, why not — and off for the workspace is not off where it is selected by name", () => {
  assert.equal(readyLine(null).text, "Reading the Decision-Making Agent…");
  assert.deepEqual(readyLine(status({ enabled: true })), { tone: "ok", text: "Ready — answers as the harness claude-code · claude-sonnet-5-5[1m]." });
  assert.match(calibratedNote(status()), /own estimate/, "a harness's probabilities are its own");
  assert.equal(calibratedNote(status({ provider: "jev", calibrated: true })), null);
  // Jev chosen and no key stored: said when the switch is on…
  const noKey = { provider: "jev", answers_as: "jev-latest", calibrated: true, ready: false, problem: "no key is stored for jev", key_stored: false };
  assert.deepEqual(readyLine(status({ enabled: true, ...noKey })), { tone: "warn", text: "no key is stored for jev" });
  // …and when it is off: a `judge` step, the classifier's provider, an `auto_route` plan still ask — and would get no answer.
  assert.deepEqual(readyLine(status(noKey)), { tone: "warn", text: "Off for the workspace — and where it is selected it cannot be asked: no key is stored for jev" });
  assert.equal(readyLine(status()).tone, "quiet");
  assert.match(readyLine(status()).text, /^Off for the workspace — asked only where it is selected by name/);
});

test("who answers: the provider shows only its own fields, and a key is its provider's own — typed, kept hidden in its box, never read back", () => {
  assert.deepEqual(fieldsFor("harness"), [KEYS.harness.id, KEYS.harness.model, KEYS.harness.effort]);
  assert.deepEqual(fieldsFor("agent"), [KEYS.agent.id]);
  assert.deepEqual(fieldsFor("jev"), [KEYS.jev.model]);
  assert.deepEqual(fieldsFor("rlcd"), [KEYS.rlcd.endpoint, KEYS.rlcd.model, KEYS.rlcd.auth]);
  assert.deepEqual(["harness", "agent", "jev", "rlcd"].map((p) => takesKey(p, "bearer")), [false, false, true, true]);
  assert.equal(takesKey("rlcd", "none"), false, "an endpoint that takes none asks for none");

  // A key typed for Jev and saved: it stays in its box while the window lives, and Save waits for a change.
  let box = keyTyped(blankKey("jev"), "jev", "not-a-real-key");
  assert.ok(maySaveKey(box, "jev"));
  box = keySaved(box, "jev");
  assert.ok(!maySaveKey(box, "jev"));
  assert.ok(mayClearKey({ key_stored: true }));
  // The provider moves to an endpoint the person names: Jev's key is not under it — not drawn, not revealed, not sent.
  assert.deepEqual(keyFor(box, "rlcd"), { provider: "rlcd", typed: "", sent: null });
  assert.ok(!maySaveKey(box, "rlcd"));
  const panel = src("../views/_settings/DecisionsPanel.tsx");
  assert.ok(panel.includes("const own = keyFor(box, provider);") && panel.includes("value={own.typed}") && panel.includes("api.setDecisionKey(provider, own.typed)"), "the panel draws and sends the provider's own box");
  assert.ok(panel.includes("<SecretInput") && panel.includes("stored={status.data?.key_stored}"), "hidden as typed; a stored key draws the mask");
  assert.ok(!/localStorage|writePref|useViewState/.test(panel), "never in storage");
  assert.ok(!/get<[^>]*>\(`\/decisions\/key/.test(src("../api.ts")), "no route reads a key back");
});

test("the four ways it is switched on: for the workspace, for one agent, for one workflow, and by naming it", () => {
  // 1. For the workspace: the switch, then every point it reaches — the ones selected by name are never its to switch.
  const on = pointRows(status({ enabled: true }), null);
  assert.deepEqual(on.map((r) => r.id), [...POINT_IDS]);
  assert.deepEqual(on.filter((r) => r.selected).map((r) => r.id), SELECTED);
  assert.ok(on.filter((r) => r.selected).every((r) => r.on && r.held && /On where it is selected\.$/.test(r.hint)));
  assert.deepEqual(on.filter((r) => !r.selected).map((r) => [r.id, r.on, r.held]), [
    ["assign.pick", true, false],
    ["dispatch.triage", true, false],
    ["goal.adopt", true, false],
    ["browser.headless", true, false],
    ["agent.decide", true, false],
  ]);
  const panel = src("../views/_settings/DecisionsPanel.tsx");
  assert.ok(panel.includes('<RegistryPanel group="decisions" only={[KEYS.enabled]} />'), "the switch is the registry's own control");

  // 2. For one agent: the third thing a core agent may change, and nothing else of it is sent.
  const general = { id: "general-agent", name: "General Agent", system_prompt: "…", harness: "claude-code", models: { strategy: "fallback", models: [] }, respond: "owner_only", skills: [], mcps: [], tags: [], origin: { origin: "core" } };
  const save = saveOf({ ...draftOf(general), decision_making: true }, general, true);
  assert.deepEqual(Object.keys(save.body), ["harness", "models", "decision_making"]);
  assert.equal(save.body.decision_making, true);
  assert.ok(src("../views/_work/AgentEditor.tsx").includes('t("work-agent-editor-let-decision-making-agent-decide-agent")'));

  // 3. For one workflow: its own switch, beside its name, description and tags — a field of the definition, saved with it.
  const inspector = src("../views/_workflow/Inspector.tsx");
  assert.ok(inspector.includes("checked={Boolean(value.decision_making)}") && inspector.includes("onChange({ ...value, decision_making })"));
  assert.ok(inspector.includes('t("workflow-inspector-let-decision-making-agent-decide-runs-workflow")'));

  // 4. By naming it: a judge step, an auto-route plan, an effort of auto, the classifier's provider — on whatever the switch says.
  const off = pointRows(status(), null);
  assert.ok(off.filter((r) => r.selected).every((r) => r.on), "still on with the workspace's switch off");
  assert.ok(off.every((r) => r.held), "and no switch is the person's to flip until the workspace's is on");
  assert.deepEqual(classifierFieldsFor("decision_making_agent"), [], "the classifier, read by it, takes no field of its own: the door is Settings › Decision Making");
  assert.ok(src("../views/_settings/SecurityPanels.tsx").includes('provider === "decision_making_agent"') && src("../views/_settings/SecurityPanels.tsx").includes('settingsSearch("decision-making")'));
  assert.equal(familyOf("judge"), "gateway");
});

test("the points left to their own rule: two switched off one after the other are both kept, and none flips while a write is on its way", () => {
  const enabled = status({ enabled: true });
  // The list as it was read is empty; the first switch is written…
  let read = [];
  let written = null;
  const first = pointsOffAfter(offList(read, written), "assign.pick", false);
  assert.deepEqual(first, ["assign.pick"]);
  assert.ok(pointRows(enabled, "assign.pick").every((r) => r.held), "while it is on its way every switch waits");
  written = first;
  // …and before the node's read has caught up, a second one is built on what was written — never on what was read.
  const second = pointsOffAfter(offList(read, written), "dispatch.triage", false);
  assert.deepEqual(second.sort(), ["assign.pick", "dispatch.triage"], "the first is not taken back");
  // The read catches up: it is the truth again, and what the panel draws.
  read = second;
  written = null;
  assert.deepEqual(offList(read, written), second);
  const rows = pointRows(status({ enabled: true, off: second }), null);
  assert.deepEqual(rows.filter((r) => !r.on).map((r) => r.id), ["assign.pick", "dispatch.triage"]);
  // Switched on again: off the list, idempotently.
  assert.deepEqual(pointsOffAfter(second, "assign.pick", true), ["dispatch.triage"]);
  assert.deepEqual(pointsOffAfter(["dispatch.triage"], "assign.pick", true), ["dispatch.triage"]);
  const panel = src("../views/_settings/DecisionsPanel.tsx");
  assert.ok(panel.includes("if (busy !== null) return;") && panel.includes("pointsOffAfter(offList(read, written), point, on)") && panel.includes("setWritten(next);"));
});

test("a question tried: nothing decided, nothing recorded — and the answer on screen is the answer to the question on screen", () => {
  const blank = blankTryForm();
  assert.equal(tryProblem(blank), "give the question instructions");
  const urgent = { ...blank, state: "Help! My payouts have been failing for 3 days.", instructions: "Does this convey urgency?" };
  assert.equal(tryProblem(urgent), null);
  assert.deepEqual(tryRequest(urgent), { state: urgent.state, questions: { q: { type: "noul", instructions: "Does this convey urgency?" } } });
  assert.deepEqual(answerLines({ model: "jev-1.13.0", answers: { q: { type: "noul", noul: 0.95 } }, usage: { input_tokens: 426, output_tokens: 73 } }), ["q: yes (90% sure)"]);
  // Which one: a choice needs two options, each with what choosing it means.
  const which = { ...blank, kind: "choice", state: '{"task":"rename a variable"}', instructions: "Which model suits the task?", options: [{ id: "small", meaning: "quick edits" }, { id: "", meaning: "" }] };
  assert.equal(tryProblem(which), "a choice needs at least two options");
  const asked = { ...which, options: [{ id: "small", meaning: "quick edits" }, { id: "large", meaning: "design work" }] };
  assert.deepEqual(tryRequest(asked).questions.q, { type: "choice", instructions: "Which model suits the task?", criteria: { small: "quick edits", large: "design work" } });
  assert.deepEqual(answerLines({ model: "jev-1.13.0", answers: { q: { type: "choice", choice: "small", probabilities: { small: 0.8, large: 0.2 }, confidence: 0.7 } } }), ["q: small (70% sure)"]);
  const panel = src("../views/_settings/DecisionsPanel.tsx");
  assert.ok(panel.includes("api.decisionsTry(tryRequest(form))"));
  assert.ok(/const set = \(patch[^]*?setResult\(null\);\s*\};/.test(panel), "an edit takes the last answer off the screen");
});

test("the judgements it made: newest first, each saying the point, how it came out and why — and the panel follows them", () => {
  const record = (outcome, over = {}) => ({
    seq: 7,
    at: 1000,
    run: "r1",
    step: "urgent",
    judgement: { point: "workflow.judge", provider: "harness", model: "claude-sonnet-5-5[1m]", calibrated: false, questions: {}, answers: { choice: { type: "choice", choice: "page", probabilities: { page: 0.9, business_hours: 0.1 }, confidence: 0.85 } }, outcome, ...over },
  });
  assert.equal(judgementLine(record("applied"), 1060), "1 min ago · A workflow's own judge step · applied · choice: page (85% sure) · claude-sonnet-5-5[1m]");
  assert.match(judgementLine(record("unsure", { reason: "sure to 0.62, and this step judges from 0.80" }), 1060), / · unsure · .* — sure to 0\.62, and this step judges from 0\.80$/);
  assert.match(judgementLine(record("failed", { answers: {}, reason: "the provider did not answer in 20s" }), 1060), / · failed · claude-sonnet-5-5\[1m\] — the provider did not answer in 20s$/);
  assert.ok(movesJudgements({ type: "judged" }) && movesStatus({ type: "judged" }));
  assert.ok(movesStatus({ type: "settings_changed", scope: "workspace", keys: ["decisions.provider"] }));
  assert.ok(!movesStatus({ type: "settings_changed", scope: "workspace", keys: ["appearance.theme"] }));
  const panel = src("../views/_settings/DecisionsPanel.tsx");
  assert.ok(panel.includes("api.decisionsRecent(50, s)"), "a bounded list");
  assert.equal(panel.split("useReloadOnReconnect(reload);").length, 3, "the status and the judgements are read again when the node comes back");
});

test("a judge step, drawn: two options and what each means, otherwise, how sure a pick must be — and what the form refuses", () => {
  let wf = blankWorkflow("Support triage");
  for (const [kind, id] of [["agent", "summarize"], ["judge", "urgent"], ["notify", "page-oncall"], ["agent", "open-ticket"]]) wf = addStep(wf, kind, id).wf;
  let judge = wf.steps.find((s) => s.id === "urgent");
  assert.deepEqual([judge.options, judge.otherwise, "min_confidence" in judge], [[], "otherwise", false]);
  assert.equal(judgeWords(judge).options, "A judgement chooses between at least two options: add 2 more.");

  // The state, the question, two options named and meant.
  judge = { ...judge, state: "{steps.summarize.output.report}", instructions: "Is this ticket urgent enough to page someone right now?" };
  judge = addOption(addOption(judge));
  judge = must(relabelBranch(judge, "branch-1", "page")).step;
  judge = must(relabelBranch(judge, "branch-2", "business_hours")).step;
  judge = setMeaning(setMeaning(judge, 0, "an outage, data loss, or a payment failure affecting customers now"), 1, "a real bug with no immediate harm");
  assert.equal(relabelBranch(judge, "page", "business_hours").ok, false, "two options never share a branch");
  // `otherwise` may be one of them: unsure, it takes the quiet road.
  judge = must(relabelBranch(judge, "otherwise", "later")).step;
  assert.deepEqual(branchesOf(judge), ["page", "business_hours", "later"]);
  assert.deepEqual(judgeWords(judge), { options: null, otherwise: "When it is not sure enough, or gives no answer that holds, the run takes later." });

  // How sure: 0.8 — and 80 is refused, never written for the node to refuse.
  assert.equal(setConfidence(judge, "80").ok, false);
  judge = must(setConfidence(judge, "0.8")).step;
  assert.equal(judge.min_confidence, 0.8);
  wf = replaceStep(wf, "urgent", judge);

  // A handle per option and one for `otherwise`: a flow drawn from each.
  for (const [from, to, branch] of [[START_STEP_ID, "summarize"], ["summarize", "urgent"], ["urgent", "page-oncall", "page"], ["urgent", "open-ticket", "business_hours"], ["urgent", "open-ticket", "later"]]) {
    wf = must(connect(wf, from, to, branch ?? null)).wf;
  }
  assert.equal(connect(wf, "urgent", "page-oncall").ok, false, "a judge's flow always carries a branch");
  assert.deepEqual(toGraph(wf).edges.filter((e) => e.from === "urgent").map((e) => [e.to, e.branch]), [["page-oncall", "page"], ["open-ticket", "business_hours"], ["open-ticket", "later"]]);
  // No switch in its form: naming the step is switching the Decision-Making Agent on for it.
  const form = src("../views/_workflow/forms/JudgeStepForm.tsx");
  assert.ok(!form.includes("<Switch"), "the form carries no switch");
  assert.ok(form.includes("judgeWords(step)") && form.includes("<Confidence step={step}"));
  assert.deepEqual(blankStep("judge", "j").boundaries ?? [], [], "a judge cannot be stopped mid-flight: it carries no boundary event");
});

test("a judge step on a run: the branch it chose is lit, and one that was not sure takes otherwise — never a failed step", () => {
  const steps = [
    { id: "begin", name: "By hand", kind: "start", on: { event: "manual" }, then: [{ to: "urgent" }] },
    {
      id: "urgent",
      name: "Urgent?",
      kind: "judge",
      state: "{inputs.ticket}",
      instructions: "Is this urgent?",
      options: [{ branch: "page", meaning: "an outage" }, { branch: "business_hours", meaning: "a bug" }],
      otherwise: "later",
      min_confidence: 0.8,
      then: [{ to: "page-oncall", branch: "page" }, { to: "open-ticket", branch: "business_hours" }, { to: "open-ticket", branch: "later" }],
    },
    { id: "page-oncall", name: "Page", kind: "notify", template: "Paging.", then: [] },
    { id: "open-ticket", name: "Ticket", kind: "agent", instructions: "Open it.", then: [] },
  ];
  const workflow = { id: "01WF", name: "Support triage", revision: 2, steps, inputs: [] };
  const tone = (run, to, branch) => edgeTone(run, toGraph(workflow).edges.find((e) => e.from === "urgent" && e.to === to && e.branch === branch));
  const base = { id: "r1", workflow, started_at: 1, outcome: null, cancelled: null };

  // While it is asked it runs — and no person is owed anything: a judgement never signs a gate.
  const asking = { ...base, steps: { begin: { state: { state: "done" } }, urgent: { state: { state: "running" }, started_at: 2 } } };
  assert.equal(stepLabel(asking, "urgent"), "running");
  assert.deepEqual(progressRows(asking, { current: ["urgent"], steps: [] }, 5).find((r) => r.id === "urgent").actions, []);
  assert.equal(progressRows(asking, { current: ["urgent"], steps: [] }, 5).find((r) => r.id === "urgent").holder, "agents");

  // Sure enough: the option it picked is the branch taken, read off its output.
  const sure = { ...base, steps: { begin: { state: { state: "done" } }, urgent: { state: { state: "done", branches: ["page"] }, output: { choice: "page", confidence: 0.91, judged: true } }, "page-oncall": { state: { state: "running" } } } };
  assert.equal(stepLabel(sure, "urgent"), "done → page");
  assert.equal(stepTone(sure, "urgent"), "ok");
  assert.deepEqual([tone(sure, "page-oncall", "page"), tone(sure, "open-ticket", "business_hours"), tone(sure, "open-ticket", "later")], ["taken", "skipped", "skipped"]);

  // Not sure enough, or no answer: `otherwise` — the step is done, never failed.
  const unsure = { ...base, steps: { begin: { state: { state: "done" } }, urgent: { state: { state: "done", branches: ["later"] }, output: { choice: null, confidence: 0.55, judged: false } }, "open-ticket": { state: { state: "running" } } } };
  assert.equal(stepLabel(unsure, "urgent"), "done → later");
  assert.deepEqual([tone(unsure, "page-oncall", "page"), tone(unsure, "open-ticket", "later")], ["skipped", "taken"]);
  const row = progressRows(unsure, { current: ["open-ticket"], steps: [] }, 5).find((r) => r.id === "urgent");
  assert.deepEqual([row.state.state, row.error, row.output], ["done", null, { choice: null, confidence: 0.55, judged: false }]);
});
