import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { STATUS_TONE, assigneeSummary, cardMenu, compactChips, listeningChip, needsYouCount, projectsOf, queuedChip, rowVerbs, secondLine, statusTone, statusWord, workflowWord, designingRow } from "./goalCardModel.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../crates/bisa-core/src");
const TOKENS = readFileSync(join(HERE, "../../theme/tokens.css"), "utf8");

function variantsOf(file, name) {
  const src = readFileSync(join(CORE, file), "utf8");
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in ${file}`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) => m[1].toLowerCase());
}

test("every core goal status has a tone, and every tone names a theme role", () => {
  for (const s of variantsOf("goal.rs", "GoalStatus")) {
    assert.ok(STATUS_TONE[s], `${s} has a tone`);
    const t = STATUS_TONE[s];
    assert.ok(t === "accent" || TOKENS.includes(`--color-${t}`), `${t} is a role`);
  }
  assert.equal(statusTone("nonsense"), "text-dim");
});

test("every core goal status has its word in the catalog, and the card spells neither a status nor a mode itself", () => {
  const statuses = variantsOf("goal.rs", "GoalStatus");
  assert.deepEqual(statuses.map(statusWord), statuses, "in English the word is the status");
  assert.equal(statusWord("paused-by-a-newer-node"), "paused-by-a-newer-node", "a status this build has no word for is said as the node said it");
  assert.equal(statusWord(null), "");
  const model = readFileSync(join(HERE, "goalCardModel.mjs"), "utf8");
  for (const s of statuses) assert.ok(model.includes(`t("goals-goal-card-status-${s}")`), `${s} reads the catalog`);
  const card = readFileSync(join(HERE, "GoalCard.tsx"), "utf8");
  assert.ok(card.includes("<Tooltip label={statusWord(row.status)}>") && !card.includes("label={row.status}"));
  assert.ok(card.includes("{MODE_LABEL[mode]}") && !card.includes(">{mode}<"), "the mode is the catalog's word, as on the goal's page");
});

test("a card that left while its verb's read was in flight is told nothing: no toast over another screen", () => {
  const card = readFileSync(join(HERE, "GoalCard.tsx"), "utf8");
  const read = card.slice(card.indexOf(".goal(row.id, ctl.signal)"), card.indexOf("return () => ctl.abort();"));
  assert.ok(read.indexOf("if (ctl.signal.aborted) return;") > 0, "the read's own abort is no failure");
  assert.ok(read.indexOf("if (ctl.signal.aborted) return;") < read.indexOf("toast.error("), "and it is asked before anything is said");
  const popover = readFileSync(join(HERE, "GoalActPopover.tsx"), "utf8");
  assert.ok(popover.includes("if (ctl.signal.aborted) return;"), "the popover's read keeps the same rule");
});

test("the second line is the statement unless the title already is it", () => {
  assert.equal(secondLine({ title: "Ship v2", statement: "Ship version two of the app." }), "Ship version two of the app.");
  assert.equal(secondLine({ title: null, statement: "Untitled goal statement" }), null, "shown as the title instead");
  assert.equal(secondLine({ title: "Same", statement: "Same" }), null);
  assert.equal(secondLine({ title: "T", statement: "   " }), null);
});

test("a long strip folds to one line and never hides the current step", () => {
  const steps = Array.from({ length: 20 }, (_, i) => ({ id: `s${i}`, kind: "agent", state: { state: i < 15 ? "done" : "pending" } }));
  const short = compactChips(steps.slice(0, 5), ["s2"], 12);
  assert.equal(short.hidden, 0);
  assert.equal(short.shown.length, 5);
  const folded = compactChips(steps, ["s15"], 12);
  assert.equal(folded.shown.length, 12);
  assert.equal(folded.hidden, 8);
  assert.ok(folded.shown.some((s) => s.id === "s15"), "the current step is kept past the fold");
  assert.deepEqual(folded.shown.slice(0, 3).map((s) => s.id), ["s0", "s1", "s2"], "the first steps stay, in order");
  assert.equal(compactChips([], [], 12).hidden, 0);
  assert.equal(compactChips(null, null).shown.length, 0);
});

test("projects and needs-you come from the workspace's own lists", () => {
  const projects = [{ project: { id: "p1" }, goals: ["g1", "g2"] }, { project: { id: "p2" }, goals: ["g2"] }, { project: { id: "p3" }, goals: [] }];
  assert.equal(projectsOf("g1", projects).length, 1);
  assert.equal(projectsOf("g2", projects).length, 2);
  assert.equal(projectsOf("g9", projects).length, 0);
  const inbox = [{ key: "g1", needs_action: [{}, {}] }, { key: "c1", needs_action: [] }];
  assert.equal(needsYouCount("g1", inbox), 2);
  assert.equal(needsYouCount("g2", inbox), 0);
  assert.equal(needsYouCount("g1", null), 0);
});

test("assignees summarise to short words, at most three, then a count", () => {
  const a = assigneeSummary([{ agent: "dev" }, { team: "core" }, { human: "abcdef0123456789" }, { agent: "qa" }]);
  assert.deepEqual(a.shown.map((w) => w.word), ["dev", "core", "abcdef01"]);
  assert.equal(a.more, 1);
  assert.deepEqual(assigneeSummary([]), { shown: [], more: 0 });
});

test("the workflow word is the strip's name or nothing", () => {
  assert.equal(workflowWord({ workflow_name: "Ship it" }), "Ship it");
  assert.equal(workflowWord({ workflow_name: null }), null);
  assert.equal(workflowWord(null), null);
});

test("a row's run verbs come from the row alone: live stops, queued counts, a goal that ran restarts, designing blocks a start", () => {
  const row = (over = {}) => ({ id: "g", status: "running", holder: "agents", workflow: "wf", run: "r1", run_status: "running", queued: 0, ...over });
  assert.deepEqual(rowVerbs(row()), { start: { label: "New run…", queues: true, adopt: false }, stop: { label: "Stop", live: true, queued: 0, sessions: 0 }, restart: { label: "Restart" } });
  assert.deepEqual(rowVerbs(row({ status: "waiting", run_status: "waiting", queued: 2 })).stop, { label: "Stop", live: true, queued: 2, sessions: 0 });
  assert.deepEqual(rowVerbs(row({ status: "done", run_status: "done", holder: "finished" })), { start: { label: "New run…", queues: false, adopt: false }, stop: null, restart: { label: "Restart" } });
  assert.deepEqual(rowVerbs(row({ status: "draft", run_status: "cancelled", holder: "you", queued: 1 })).stop, { label: "Stop", live: false, queued: 1, sessions: 0 }, "a queue alone is stopped");
  assert.deepEqual(rowVerbs(row({ status: "draft", run: null, run_status: null, holder: "you" })), { start: null, stop: null, restart: null }, "a first start is the page's: an adoption may be owed");
  assert.deepEqual(rowVerbs(row({ status: "failed", run_status: "failed", holder: "design" })), { start: null, stop: null, restart: null }, "the Workflow Agent repairs; nobody starts over its head");
  assert.deepEqual(rowVerbs(row({ status: "failed", run_status: "failed", holder: "design", queued: 1 })).stop, { label: "Stop", live: false, queued: 1, sessions: 0 }, "a stop is never blocked");
  assert.deepEqual(rowVerbs(row({ status: "closed", closed: { reason: "abandoned" } })), { start: null, stop: null, restart: null });
  assert.deepEqual(rowVerbs(null), { start: null, stop: null, restart: null });
  // A session working on the goal with no run going — a design wake, a turn in its thread — is a stop from the card too.
  assert.deepEqual(rowVerbs(row({ status: "draft", run_status: "cancelled", holder: "you" }), 1).stop, { label: "Stop", live: false, queued: 0, sessions: 1 });
  assert.deepEqual(rowVerbs(row({ status: "closed", closed: { reason: "abandoned" } }), 1).stop, null, "closed: nothing of it runs");
  assert.equal(queuedChip(row()), null);
  assert.equal(queuedChip(row({ queued: 3 })), "queued 3");
  assert.ok(readFileSync(join(HERE, "goalCardModel.mjs"), "utf8").includes('t("goals-goal-card-queued", { n })'), "the chip's word is the catalog's, never built in code");
});

test("the ⋮: Open and Delete… on every card, the run's verbs between them — the first opening its group, Stop and Delete… in danger", () => {
  const row = (over = {}) => ({ id: "g", status: "running", holder: "agent", workflow: "wf", run: "r1", run_status: "running", queued: 0, ...over });
  const ids = (r) => cardMenu(rowVerbs(r)).map((i) => i.id);
  assert.deepEqual(ids(row()), ["open", "start", "restart", "stop", "delete"]);
  assert.deepEqual(ids(row({ status: "draft", run: null, run_status: null, holder: "you" })), ["open", "delete"], "a draft that never ran: nothing to run, still opens and can go");
  assert.deepEqual(ids(row({ status: "closed", closed: { reason: "abandoned" } })), ["open", "delete"], "closed: nothing runs; the retirement dialog is still the way out");
  assert.deepEqual(ids(row({ status: "done", run_status: "done", holder: "finished" })), ["open", "start", "restart", "delete"]);
  assert.deepEqual(cardMenu(null).map((i) => i.id), ["open", "delete"], "no verbs at all: the two every card has");
  const items = cardMenu(rowVerbs(row()));
  assert.deepEqual(items.map((i) => [i.id, !!i.danger, !!i.separatorBefore]), [
    ["open", false, false],
    ["start", false, true],
    ["restart", false, false],
    ["stop", true, false],
    ["delete", true, true],
  ]);
  assert.deepEqual(items.map((i) => i.label), ["Open", "New run…", "Restart", "Stop", "Delete…"]);
  // The card draws the model's menu on every row, vertical as the workflow card's, and Delete… is the goal page's retirement dialog.
  const card = readFileSync(join(HERE, "GoalCard.tsx"), "utf8");
  assert.ok(card.includes("<MoreMenu vertical") && card.includes("cardMenu(verbs)"), "one menu, the model's, always drawn");
  assert.ok(!card.includes("runMenu.length > 0 &&"), "no card is left without its menu");
  assert.ok(card.includes("<RetireDialog") && card.includes('kind="goal"') && card.includes('wanted="delete"'), "Delete… opens the retirement dialog");
});

test("a listening goal wears its chip — paused once a failure stopped it — and its row offers no new run: its run by hand is the page's", () => {
  assert.equal(listeningChip({ listening: null }), null);
  assert.deepEqual(listeningChip({ listening: { since: 1 } }), { words: "listening", tone: "accent", paused: false });
  assert.deepEqual(listeningChip({ listening: { since: 1, paused: { reason: { reason: "run_failed", run: "01R" }, at: 2 } } }), { words: "paused", tone: "warn", paused: true });
  assert.equal(listeningChip(null), null);
  const row = (over = {}) => ({ id: "g", status: "waiting", holder: "world", workflow: "wf", run: "r1", run_status: "done", queued: 0, listening: { since: 1 }, ...over });
  assert.deepEqual(rowVerbs(row()), { start: null, stop: null, restart: { label: "Restart" } });
  assert.equal(rowVerbs(row({ listening: null })).start.label, "New run…", "a goal that does not listen starts again from its row");
});

test("a card says designing only for a goal the design holds, in a mode that designs", () => {
  assert.equal(designingRow({ holder: "design" }, true), true);
  assert.equal(designingRow({ holder: "design" }, false), false, "a manual goal is nobody's to design, whatever holds it");
  for (const holder of ["person", "agent", "nobody", undefined]) assert.equal(designingRow({ holder }, true), false, String(holder));
  assert.equal(designingRow(null, true), false);
});

