/**
 * A goal's life, as a person lives it: captured in each of the three modes,
 * designed or drawn, adopted with the project its work is done in, run and
 * run again until its list of runs needs a bound, closed with a word on why
 * or replaced by the goal that takes its place, then put away. No window is
 * opened and nothing is sent: the scenario steps the models the capture
 * dialog, the goal's page, its card on the list and the retirement dialog
 * step, and holds every body they build to the schema the node reads it by.
 *
 * Run with `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/goals.test.mjs`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { designView, kindWord } from "../views/_goal/designStatus.mjs";
import { GOAL_MODES, afterCapture, captureHint, captureToast, designs, modeSegments } from "../views/_goal/goalMode.mjs";
import { closeBody, closeWords, headerWords, pageFacts, replacements } from "../views/_goal/goalPageModel.mjs";
import { tabOf } from "../views/_goal/goalTabs.mjs";
import { finishedWords, liveSteps } from "../views/_goal/progressModel.mjs";
import { adoptAction, bandActions } from "../views/_goal/proposalRouting.mjs";
import { listenVerb, panelFrozen, runRows, runVerbs } from "../views/_goal/runControl.mjs";
import { cardMenu, listeningChip, queuedChip, rowVerbs, statusWord } from "../views/_goals/goalCardModel.mjs";
import { attachRefusedWords, canSubmit, goalBody } from "../views/_work/newGoalModel.mjs";
import { confirmWords, defaultChoices, deleteAvailable, destroys, planOf, retireSections, titleWords } from "../views/_work/retireModel.mjs";
import { RUNS_SHOWN, paneWindow } from "../views/_workflow/workflowRunsModel.mjs";
import { initialValues, inputHint, toRequest, validateInputs } from "../views/_workflow/workflowForm.mjs";
import { misfits } from "./schemaFit.mjs";

const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
const SCHEMA = JSON.parse(read("../../api-schema.json"));
const fits = (name, body) => misfits(SCHEMA, name, body, {});

const ID = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const OTHER = "01ARZ3NDEKTSV4RRFFQ69G5FAW";
const PROJECT = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
const guidanceOf = (mode, design = null) => ({ mode, design_enabled: true, phase: designs(mode) ? "design" : "manual", design, open_questions: [] });
const goalOf = (mode, over = {}) => ({ id: ID, mode, title: "Ship the report", statement: "Ship the quarterly report.", closed: null, workflow: null, run: null, listening: null, ...over });
const summary = (n, status, over = {}) => ({ id: `r${n}`, number: n, status, workflow: "01W", revision: 1, queued_at: n, ...over });

test("capture, in each of the three modes: what is sent, what is said, and where the person lands", () => {
  assert.deepEqual([...GOAL_MODES], ["auto", "guided", "manual"]);
  assert.deepEqual(modeSegments().map((s) => s.label), ["Auto", "Guided", "Manual"]);
  assert.equal(canSubmit({ statement: "  ", busy: false, uploading: false }), false, "a capture waits for a statement");
  const said = {};
  for (const mode of GOAL_MODES) {
    const body = goalBody({ statement: " Ship the quarterly report. ", mode, assignees: [], tags: [], documents: [] });
    assert.deepEqual(fits("NewGoalBody", body), [], `${mode}: the body is the node's shape`);
    assert.deepEqual(body, { statement: "Ship the quarterly report.", mode }, "only what was set travels");
    said[mode] = [captureHint(mode), captureToast(mode), afterCapture(mode)];
    // Before any run, the goal's page says who owes the workflow.
    const card = designView(guidanceOf(mode), goalOf(mode), { now: 10 });
    assert.equal(card.kind, designs(mode) ? "scheduled" : "manual");
    assert.equal(kindWord(card.kind), card.kind);
    assert.equal(card.offerPick, !designs(mode), "nobody picks a workflow over the Workflow Agent's head");
    assert.equal(runVerbs({ goal: goalOf(mode), run: null, runs: [], guidance: guidanceOf(mode), proposed: false, startable: false }).start, null, "and nothing starts with no workflow");
  }
  assert.equal(new Set(Object.values(said).map(([hint]) => hint)).size, 3, "each mode says what it will do");
  assert.equal(new Set(Object.values(said).map(([, toast]) => toast)).size, 3);
  assert.deepEqual([said.auto[2], said.guided[2], said.manual[2]], [null, null, { tab: "workflow", edit: "1" }], "a manual goal opens on its designer");
  assert.equal(tabOf(said.manual[2].tab), "workflow");
  assert.equal(tabOf("nowhere"), "progress", "a stale link never lands nowhere");
  // Who carries it — the agents and teams the dialog picked — travels in the wire's word: the Workflow Agent staffs from them alone.
  const forTeam = goalBody({ statement: "Ship", mode: "auto", assignees: ["team:01TEAM"], tags: ["ops"], documents: [] });
  assert.deepEqual(forTeam.assignees, ["team:01TEAM"]);
  assert.deepEqual(fits("NewGoalBody", forTeam), []);
  const carried = goalBody({ statement: "Ship", mode: "guided", assignees: ["agent:developer", "team:01TEAM"], tags: [], documents: [] });
  assert.deepEqual(carried.assignees, ["agent:developer", "team:01TEAM"]);
  assert.deepEqual(fits("NewGoalBody", carried), []);
  // Projects handed over with the capture are attached one by one: a refusal is said once, and the goal stands.
  assert.equal(attachRefusedWords([]), null);
  assert.equal(attachRefusedWords([{ project: PROJECT, reason: "the project is archived" }]), "The goal stands, but 1 project could not be attached to it: the project is archived");
});

test("a guided goal: the proposal is the landing, adopted with the project its work is done in — which the start attaches", () => {
  const goal = goalOf("guided", { workflow: "01W" });
  const gate = { id: "g1", gate: "approval", subject: "adopt:01W@1" };
  const proposal = { gate_id: "g1", gate_kind: "approval", subject: "adopt:01W@1", question: "Adopt?", proposal: { name: "Report", steps: [] }, home: { home: "goal", goal: ID } };
  const question = { gate_id: "g2", gate_kind: "escalation", subject: "guard:Bash", question: "Allow?", home: { home: "goal", goal: ID } };
  const design = { id: "01W", name: "Report", origin: { origin: "goal" }, inputs: [{ name: "project", label: "Project", kind: "project", required: true }, { name: "tone", label: "Tone", kind: "choice", options: ["plain", "warm"], default: "plain" }], steps: [] };
  const facts = pageFacts({ goal, run: null, pendingGates: [gate], startable: design });
  assert.deepEqual([facts.proposed, facts.adoptGate, facts.ownDesign?.id, facts.closed], [true, gate, "01W", false]);
  assert.equal(adoptAction([question, proposal]), proposal, "the Progress tab shows the plan");
  assert.deepEqual(bandActions([question, proposal]), [question], "and the band everything else — shown once, not twice");
  const verbs = runVerbs({ goal, run: null, runs: [], guidance: guidanceOf("guided", { phase: "design", status: "proposed", since: 1, detail: null, session: null, live: false }), proposed: facts.proposed, startable: true });
  assert.deepEqual(verbs, { start: { label: "Adopt and start…", queues: false, adopt: true }, stop: null, restart: null });

  // The run form: the project is asked for, and the line under it says what picking one does.
  let values = initialValues(design.inputs);
  assert.deepEqual(values, { project: "", tone: "plain" });
  assert.deepEqual(validateInputs(design.inputs, values), { project: "Required." });
  assert.equal(inputHint(design.inputs[0], "Required.", "goal"), "Required. Picking one attaches it to the goal when the run starts: you say where the work is done. Required.");
  values = { ...values, project: PROJECT };
  assert.deepEqual(validateInputs(design.inputs, values), {});
  assert.equal(inputHint(design.inputs[0], undefined, "goal"), "Required. Picking one attaches it to the goal when the run starts: you say where the work is done.");
  assert.deepEqual(toRequest(design.inputs, values), { project: PROJECT, tone: "plain" }, "typed by kind, as the node reads them");
});

test("a goal run again and again: the list keeps a bound, the runs still to end lead it, and the page and the card offer the same verbs", () => {
  const goal = goalOf("manual", { workflow: "01W", run: "r130" });
  const finished = Array.from({ length: 129 }, (_, i) => summary(i + 1, i % 9 === 0 ? "failed" : "done", { started_at: i + 1, finished_at: i + 2 }));
  const runs = [...finished, summary(130, "waiting", { started_at: 130 }), summary(131, "queued", { position: 1 }), summary(132, "queued", { position: 2 })];
  const rows = runRows(runs);
  assert.deepEqual(rows.slice(0, 3).map((r) => [r.index, r.words.word, r.live, r.withdraw]), [
    [132, "queued · 2nd in line", true, true],
    [131, "queued · next in line", true, true],
    [130, "waiting", true, false],
  ]);
  const first = paneWindow(rows);
  assert.deepEqual([first.rows.length, first.hidden, first.more], [RUNS_SHOWN, 82, RUNS_SHOWN], "a page, and what Show older brings");
  const older = paneWindow(rows, RUNS_SHOWN * 2);
  assert.deepEqual([older.rows.length, older.hidden, older.more], [100, 32, 32]);
  assert.equal(paneWindow(rows, RUNS_SHOWN * 3).hidden, 0);
  assert.ok(read("../views/_workflow/RunHistory.tsx").includes("paneWindow(all, shown)"), "the goal's list is the Runs pane's own rule");

  const run = { id: "r130", started_at: 130, outcome: null, cancelled: null, workflow: { steps: [{ id: "start" }, { id: "review" }] }, steps: { start: { state: { state: "done" } }, review: { state: { state: "waiting" } } } };
  const verbs = runVerbs({ goal, run, runs, guidance: guidanceOf("manual"), proposed: false, startable: true });
  assert.deepEqual(verbs, { start: { label: "New run…", queues: true, adopt: false }, stop: { label: "Stop", live: true, queued: 2 }, restart: { label: "Restart" } });
  const row = { id: ID, status: "waiting", holder: "you", workflow: "01W", run: "r130", run_status: "waiting", queued: 2, mode: "manual" };
  assert.deepEqual(rowVerbs(row), verbs, "the card answers as the page does, from the row alone");
  assert.deepEqual([statusWord(row.status), queuedChip(row)], ["waiting", "queued 2"]);
  assert.equal(panelFrozen(run, runs), true, "a live run freezes the goal's details");
  assert.deepEqual(liveSteps(run).map((s) => s.id), ["review"], "what the card's Act acts on");
  assert.equal(finishedWords({ outcome: "done", steps: {} }), "The run finished done.");
});

test("opening a goal: the header says the goal's words — the title over its sentence, or the sentence alone — never its id, and the arrow at its left returns to the Goals list", () => {
  // A goal captured with a title and one captured as a bare sentence, as the node answers them.
  assert.deepEqual(headerWords(goalOf("guided")), { title: "Ship the report", subtitle: "Ship the quarterly report." });
  assert.deepEqual(headerWords(goalOf("guided", { title: null })), { title: "Ship the quarterly report.", subtitle: null }, "a goal with no title is its sentence — never `Goal <id>`");
  assert.ok(!headerWords(goalOf("guided", { title: null })).title.includes(ID.slice(-6)));
  // The header draws those words and nothing of the id, and wears the door back to the list the workflow's header has.
  const header = read("../views/_goal/GoalHeader.tsx");
  assert.ok(header.includes("const words = headerWords(goal);") && header.includes("title={<span className=\"line-clamp-2 whitespace-normal\" title={words.title}>{words.title}</span>}"), "the title is the model's words, a long sentence folding to two lines and whole in the tooltip");
  assert.ok(header.includes("subtitle={words.subtitle ? <span className=\"line-clamp-2\">{words.subtitle}</span> : undefined}"), "the statement under a title alone");
  assert.ok(!header.includes("slice(-6)") && !header.includes("goal.id.slice"), "the id is drawn nowhere in the header");
  assert.ok(header.includes('back={{ label: t("goal-goal-header-back-to-goals"), onClick: () => navigate({ name: "goals" }) }}'), "the arrow at the header's left, to the Goals list — as the workflow's header returns to Workflows");
  assert.ok(read("../views/WorkflowDesigner.tsx").includes('back={{ label: t("screens-workflow-designer-back-to-workflows"), onClick: () => navigate({ name: "workflows" }) }}'), "the two doors are one shape of `PageHeader`");
  const catalog = read("../../../locales/en/desktop/goal.ftl");
  assert.ok(catalog.includes("goal-goal-header-back-to-goals = Back to Goals") && catalog.includes("goal-goal-header-untitled-goal = Untitled goal"));
  assert.ok(!/^goal-goal-header-[a-z-]+ = Goal \{ \$goal \}$/m.test(catalog), "no message says the goal by its id any more");
});

test("closing: a word on why is kept, a replacement is sent as `superseded_by`, and a closed goal offers nothing to run — its card still opens and can go", () => {
  const goals = [
    { id: ID, title: "Ship the report", statement: "Ship the quarterly report.", status: "running", archived: null },
    { id: OTHER, title: "Ship the annual report", statement: "…", status: "draft", archived: null },
    { id: "01ARZ3NDEKTSV4RRFFQ69G5FAY", title: "An older try", statement: "…", status: "closed", archived: null },
  ];
  assert.deepEqual(replacements(goals, ID), [{ id: OTHER, label: "Ship the annual report" }], "a goal that still stands, and never itself");

  // Abandoned, with the person's word on why.
  let form = { rationale: " the client left ", replacedBy: null };
  assert.deepEqual(closeBody(form, ID), { rationale: "the client left" });
  assert.deepEqual(fits("CloseBody", closeBody(form, ID)), []);
  assert.equal(closeWords(form, goals, ID).records, "The goal is recorded as abandoned — it can be read, never resumed.");

  // Replaced: the supersession travels under the node's own key, alone.
  form = { ...form, replacedBy: OTHER };
  assert.deepEqual(closeBody(form, ID), { superseded_by: OTHER });
  assert.deepEqual(fits("CloseBody", closeBody(form, ID)), []);
  assert.deepEqual(closeWords(form, goals, ID), { records: "The goal is recorded as superseded by Ship the annual report — it can be read, never resumed.", closed: "Closed — superseded by Ship the annual report." });
  // The guard bites: the key the screen once could not send, misspelt, is refused.
  assert.deepEqual(fits("CloseBody", { supersededBy: OTHER }), ["$.supersededBy: a key `CloseBody` does not declare"]);

  // Nothing said sends nothing — never `""` where a goal's id is read.
  assert.deepEqual(closeBody({ rationale: "", replacedBy: "" }, ID), {});

  // Closed: no run verb, no listening verb, on the page and on the card.
  const closed = goalOf("manual", { workflow: "01W", run: "r1", closed: { reason: { reason: "superseded", by: OTHER }, at: 9 }, listening: { since: 1 } });
  assert.deepEqual(runVerbs({ goal: closed, run: null, runs: [], guidance: guidanceOf("manual"), proposed: false, startable: true }), { start: null, stop: null, restart: null });
  assert.equal(listenVerb(closed), null);
  assert.deepEqual(rowVerbs({ id: ID, status: "closed", workflow: "01W", run: "r1", run_status: "cancelled", queued: 0 }), { start: null, stop: null, restart: null });
  // The card's ⋮ keeps the two every card has: Open, and Delete… — the retirement dialog, the way out of a closed goal too.
  assert.deepEqual(cardMenu(rowVerbs({ id: ID, status: "closed", workflow: "01W", run: "r1", run_status: "cancelled", queued: 0 })).map((i) => i.id), ["open", "delete"]);
  assert.equal(pageFacts({ goal: closed, run: null, pendingGates: [], startable: null }).closed, true);
  assert.equal(listeningChip({ listening: null }), null);
});

test("putting it away: the preview is read first, the title and the button follow the choice, and the plan is the node's", () => {
  const preview = { agents: 0, harnesses: 0, run: null, runs: [], history: 0, refusal: null, designs: 1, used_by: [], projects_born: [{ id: "web", slug: "web", name: "web", adopted: false, workstreams: 1, workstream_ids: ["ws-web"], sessions: 0, archived: false }], projects_attached: [] };
  let choices = defaultChoices("delete", preview);
  assert.equal(deleteAvailable(preview), true);
  assert.equal(titleWords(choices.thing, "Ship the report"), "Delete Ship the report?");
  assert.equal(confirmWords("goal", choices, preview), "Delete goal, keep 1 project");
  assert.equal(destroys(choices), true);
  assert.deepEqual(fits("GoalPlan", planOf("goal", choices)), []);

  // The person thinks again inside the dialog: the title is no longer the door's.
  choices = { ...choices, thing: "archive", projects: "archive" };
  assert.equal(titleWords(choices.thing, "Ship the report"), "Archive Ship the report?");
  assert.equal(confirmWords("goal", choices, preview), "Archive goal and 1 project");
  assert.equal(destroys(choices), false);
  assert.deepEqual(planOf("goal", choices), { goal: "archive", projects: "archive", tree: false });
  assert.deepEqual(fits("GoalPlan", planOf("goal", choices)), []);
  assert.ok(retireSections("goal", preview, choices, { harnesses: 0, shells: 0, agents: 0 }).some((s) => s.id === "born"), "what it made is named before anything goes");

  // A goal whose design another goal still uses is archived, whatever door was pressed.
  const refused = { ...preview, refusal: "goal g cannot be deleted: its design d is still used by goal g2" };
  assert.equal(titleWords(defaultChoices("delete", refused).thing, "Ship the report"), "Archive Ship the report?");
});
