/**
 * The list and detail screens remember how they stood, as a person meets
 * it: a feed read far down comes back on the row it stood on, a run's rows
 * stand as they were left, a drawing in progress outlives the window, a goal
 * opens its Details pane once — and a dialog starts empty. The journeys run
 * over the models; the wiring is read from the sources, as every scenario
 * here reads it — no DOM. Run with
 * `node --test desktop/src/scenarios/whereIWasScreens.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { href } from "../routeModel.mjs";
import { KeptMemory } from "../shell/keptMemoryModel.mjs";
import { wordsOf } from "../shell/viewValuesModel.mjs";
import { anchorOf, parseAnchor, scrollFor } from "../ui/anchorModel.mjs";
import { parseTagFilter } from "../ui/tagSearchModel.mjs";
import { rowOpen, rowPressed } from "../views/_goal/progressModel.mjs";
import { PAGE, depthOf, parseDepth, wantsDepth } from "../views/_pulse/pulseModel.mjs";
import { KeptReads, readKey } from "../views/_work/keptReadsModel.mjs";
import { parseOpened, toggled } from "../views/_work/openedModel.mjs";
import { draftBody } from "../views/_workflow/designDraftModel.mjs";
import { keptSelection, parseMemory } from "../views/_workflow/designerMemoryModel.mjs";
import { caughtUp, dirty, open, present } from "../views/_workflow/designerSession.mjs";
import { canUndo, create, push } from "../views/_workflow/history.mjs";
import { rowPlace } from "../views/inboxPlaceModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

/** The text from one marker up to the next, both of which must be there. */
const between = (text, from, to) => {
  const start = text.indexOf(from);
  assert.ok(start >= 0, `the source says \`${from}\``);
  const end = text.indexOf(to, start + from.length);
  assert.ok(end > start, `and \`${to}\` after it`);
  return text.slice(start, end);
};

/** Every marker is there, each after the one before it. */
const inOrder = (text, markers, why) => {
  let at = -1;
  for (const marker of markers) {
    const next = text.indexOf(marker, at + 1);
    assert.ok(next > at, `${why}: \`${marker}\``);
    at = next;
  }
};

/** A storage in memory: what one window writes, the next reads. */
function disk() {
  const map = new Map();
  return { map, getItem: (k) => (map.has(k) ? map.get(k) : null), setItem: (k, v) => void map.set(k, String(v)), removeItem: (k) => void map.delete(k) };
}

/** A memory over a disk, as a window opens it: the screens' values, or the documents'. */
function memoryOver(store, key = "bisa.view.state", valueBytes = 8 * 1024) {
  return new KeptMemory({
    key,
    version: 1,
    caps: { places: 96, valueBytes, totalBytes: 1024 * 1024 },
    hands: { storage: () => store, now: () => 1_000, later: () => 0, cancel: () => {} },
  });
}

/** The place a route is, as `viewMemoryStore.placeOf` says it. */
const placeOf = (route) => href(route).slice(1);

/** A feed's rows as the list lays them out: 36 pixels each, newest first. */
const laidOut = (keys) => keys.map((key, i) => ({ key, top: i * 36, height: 36 }));
const span = (from, to) => Array.from({ length: from - to + 1 }, (_, i) => `seq-${from - i}`);

test("the Pulse, read three pages down, comes back on the row it stood on — in the window, and after a restart", () => {
  const store = disk();
  const first = memoryOver(store);
  const place = placeOf({ name: "pulse" });
  // Three pages of the Goals tab, a row opened, scrolled to the middle of row 150.
  const rows = span(240, 1);
  const opened = toggled(new Set(), "seq-150");
  first.keep(place, "opened", wordsOf(opened));
  first.keepQuietly(place, "depth:goals", depthOf(3 * PAGE));
  const stood = anchorOf(laidOut(rows), laidOut(rows).find((r) => r.key === "seq-150").top + 10);
  assert.deepEqual(stood, { key: "seq-150", offset: 10 });
  first.keepQuietly(place, "anchor:goals", stood);
  // The window keeps the feed itself, a concept at a time.
  const reads = new KeptReads();
  reads.keep(readKey("pulse", "goals"), { items: rows, next: { at: 1, seq: 1 } });
  assert.equal(reads.read(readKey("pulse", "all")), undefined, "another tab's feed is its own");

  // Away and back in the same window: the feed is drawn at once, and twelve
  // rows landed at its head meanwhile — the row is 432 pixels further down.
  const grown = [...span(252, 241), ...reads.read(readKey("pulse", "goals")).items];
  assert.equal(scrollFor(parseAnchor(first.read(place, "anchor:goals")), laidOut(grown)), (12 + 90) * 36 + 10);
  assert.ok(parseOpened(first.read(place, "opened")).has("seq-150"), "and the row is still open");

  // A restart: nothing of the feed is kept, so it is read again as deep as it was left.
  first.flush();
  const second = memoryOver(store);
  const wanted = parseDepth(second.read(place, "depth:goals"));
  assert.equal(wanted, 3 * PAGE);
  const anchor = parseAnchor(second.read(place, "anchor:goals"));
  let loaded = span(252, 252 - PAGE + 1);
  const next = { at: 1, seq: 1 };
  assert.equal(scrollFor(anchor, laidOut(loaded)), null, "the head page does not hold the row: no place yet");
  let pages = 1;
  while (wantsDepth({ loaded: loaded.length, wanted, next, inFlight: false })) {
    loaded = [...loaded, ...span(252 - loaded.length, 252 - loaded.length - PAGE + 1)];
    pages += 1;
  }
  assert.equal(pages, 3, "read back as far as it was, no further");
  assert.equal(scrollFor(anchor, laidOut(loaded)), (12 + 90) * 36 + 10, "and the list is put back on the row");
  // The row went while the app was closed: the list stays at its head.
  assert.equal(scrollFor(anchor, laidOut(loaded.filter((key) => key !== "seq-150"))), null);
});

test("a run's rows stand as they were left, through a tab switch and a restart", () => {
  const store = disk();
  const first = memoryOver(store);
  const place = placeOf({ name: "run", id: "01RUN" });
  assert.equal(place, "/runs/01RUN");
  // `review` is live and open by itself; `build` finished and is opened by hand; `review` is then closed by hand.
  let said = new Set();
  said = rowPressed(said, "build", false);
  said = rowPressed(said, "review", true);
  first.keep(place, "steps:opened", wordsOf(said));
  first.flush();

  const second = memoryOver(store);
  const back = parseOpened(second.read(place, "steps:opened"));
  assert.equal(rowOpen(back, "build", false), true, "opened by hand: still open");
  assert.equal(rowOpen(back, "review", true), false, "closed by hand: still closed");
  assert.equal(rowOpen(back, "ship", true), true, "a step nothing was said of opens while it is live");
  // Another run of the same workflow has the same steps, and its own rows.
  assert.equal(second.read(placeOf({ name: "run", id: "01OTHER" }), "steps:opened"), undefined);
  const rows = read("views/_goal/RunSteps.tsx");
  assert.ok(rows.includes('useViewState<ReadonlySet<string>>(place, "steps:opened", NOTHING_SAID, parseOpened, wordsOf)'), "the rows are the screen's memory, under the place the caller names");
  assert.ok(rows.includes("rowOpen(said, r.id, live)") && rows.includes("rowPressed(prev, r.id, live)"), "how a row stands is the model's to say");
  assert.ok(read("views/_goal/ProgressTab.tsx").includes('place={run ? placeOf({ name: "run", id: run.id }) : placeOf({ name: "goal", id: goal.id })}'), "the goal page keeps them under the run's place");
  assert.ok(read("views/WorkflowRun.tsx").includes("place={place}"), "and the run's own page under the same one");
});

test("a drawing in progress outlives the window as it stands, and is forgotten when it is saved, cancelled or dropped", () => {
  const store = disk();
  const docs = memoryOver(store, "bisa.view.docs", 64 * 1024);
  const place = placeOf({ name: "goal", id: "01G" });
  let drawing = create({ name: "Ship it", description: "", inputs: [], steps: [{ id: "start", name: "Start", kind: "start" }], tags: [] });
  drawing = push(drawing, { ...drawing.present, steps: [...drawing.present.steps, { id: "build", name: "Build", kind: "agent" }] });
  docs.keepQuietly(place, "workflow:draft", drawing.present);
  docs.flush();

  const next = memoryOver(store, "bisa.view.docs", 64 * 1024);
  const restored = create(draftBody(next.read(place, "workflow:draft")));
  assert.deepEqual(restored.present.steps.map((s) => s.id), ["start", "build"]);
  assert.equal(canUndo(restored), false, "a history of one entry: the way back through it was the window's");
  // Saved: nothing of it is left.
  next.keepQuietly(place, "workflow:draft", null);
  assert.equal(draftBody(next.read(place, "workflow:draft")), null);

  const draft = read("views/_workflow/designDraftStore.ts");
  assert.ok(draft.includes("docViews.read(placeOfGoal(goal), DRAWING)") && draft.includes("create(body)"), "restored from the documents' memory, as a history of one entry");
  assert.ok(draft.includes("docViews.keepQuietly(placeOfGoal(goal), DRAWING, next ? next.present : null)"), "its present is kept, and a draft dropped is forgotten");
  assert.ok(!draft.includes(".past") && !draft.includes(".future"), "never its undo history");
  for (const door of ["export function draftOf(goal: string): Draft", "export function setDraft(goal: string, next: Draft): void", "export function useDesignDraft(goal: string)"]) assert.ok(draft.includes(door), door);
  const tab = read("views/_workflow/GoalWorkflowTab.tsx");
  assert.ok((tab.match(/setDraft\(null\)/g) ?? []).length >= 3, "saved, cancelled, and dropped when the goal starts a run");
});

test("the goal page opens its Details pane only for a goal never been on, and leaves a goal that is gone", () => {
  const page = read("views/GoalDetail.tsx");
  const arrival = between(page, "// The details pane is the goal's home state", "}, [id]);");
  inOrder(arrival, ['if (placeWasKnown(placeOf({ name: "goal", id }))) return;', 'if (!aux.kind || aux.kind === "session") aux.open("inspector");'], "a goal come back to stands as it was left");
  assert.equal((page.match(/aux\.open\("inspector"\)/g) ?? []).length, 1, "one door opens the pane on arrival");
  assert.ok(page.includes('useAsync((s) => api.goal(id, s), [id], { keep: goalRead })') && page.includes('const goalRead = readKey("goal", id);'), "the last answer is drawn at once");
  inOrder(page, ["if (missing) forgetRead(goalRead);", 'useGonePlace(missing, { name: "goal", id });'], "what the window kept of a goal that is gone goes with it");
  assert.ok(page.includes('leave(tr("screens-goal-detail-goal-deleted"));'), "and a goal deleted under the page still leaves at once");
  assert.ok(page.includes('ref={tab === "conversation" ? undefined : scrolls}') && page.includes('data-scroll-keep={tab === "progress" ? "tab:progress" : undefined}'), "each tab's scroll under its own name; the thread's place is the thread's");
});

test("the goal's Workflow tab is read from the memory on every render: the step, the run viewed, where the canvas looked", () => {
  const tab = read("views/_workflow/GoalWorkflowTab.tsx");
  assert.ok(tab.includes('useViewState<string | null>(place, "workflow:step", null, idValue)') && tab.includes('useViewState<string | null>(place, "workflow:run", null, idValue)'), "the step and the run are the goal's memory");
  assert.ok(!/useState<string \| null>\((stepParam|runParam)/.test(tab), "never seeded once from the address");
  inOrder(between(tab, "if (!stepParam) return;", "}, [stepParam, setSelected]);"), ["setSelected(stepParam);", "setSearch({ step: null }, { replace: true });"], "a link's step is taken into the memory and off the address");
  inOrder(between(tab, "const liveRun = useRef(run?.id);", "}, [run?.id, setViewingRun]);"), ["if (liveRun.current === run?.id) return;", "setViewingRun(null);"], "the run viewed is let go when the live run changes, never when the tab opens");
  assert.ok(tab.includes("startViewport={startViewport}") && tab.includes("onViewport={keepViewport}") && tab.includes("canvasViewportAt(place)") && tab.includes("rememberViewportAt(place, v)"), "the canvas opens where it was left, as the designer's does");
  assert.ok(tab.includes("keptSelection(picked, value.steps)"), "a remembered step the picture no longer has is no pick");
  // What the memory gives back is what it is, or nothing.
  assert.deepEqual(parseMemory({ selected: "build", viewport: { x: -40, y: 12, zoom: 1.25 } }), { selected: "build", viewport: { x: -40, y: 12, zoom: 1.25 } });
  assert.equal(keptSelection("build", [{ id: "start" }, { id: "ship" }]), null);
});

test("a designer drawn from what the window kept stands on the node's own answer once it lands", () => {
  const stored = (revision, name) => ({ id: "01WF", name, description: "", inputs: [], steps: [{ id: "a", name: "A", kind: "approval", prompt: "?" }], origin: { origin: "workspace" }, author: "ab".repeat(32), tags: [], revision, created_at: 0 });
  // Drawn at once from the last visit's answer; an agent saved twice since.
  const drawn = open(stored(3, "As it was left"), []);
  const caught = caughtUp(drawn, stored(5, "As the agent left it"), []);
  assert.deepEqual([caught.base.revision, present(caught).name, dirty(caught)], [5, "As the agent left it", false]);
  const screen = read("views/WorkflowDesigner.tsx");
  assert.ok(screen.includes('useAsync((s) => api.workflow(id, s), [id], { keep: workflowRead })') && screen.includes('const workflowRead = readKey("workflow", id);'), "the last answer is drawn at once");
  inOrder(between(screen, 'const openedOn = useRef<"kept" | "read" | null>(null);', "}, [data, id, at]);"), ['openedOn.current = at === null ? "kept" : "read";', "open(data.workflow, data.problems)", "caughtUp(s, data.workflow, data.problems)"], "opened once, and caught up once when it was opened on a kept answer");
  inOrder(screen, ["if (missing) forgetRead(workflowRead);", 'useGonePlace(missing, { name: "workflow", id });'], "a workflow that is gone is left for the library");
  assert.ok(screen.includes("if (!session || !body || missing) {"), "and never drawn from what was kept of it");
  assert.ok(screen.includes('data-scroll-keep="palette"') && screen.includes("data-scroll-keep={`properties:${selected ?? \"\"}`}"), "the palette and each step's properties keep their scroll");
  assert.ok(!between(screen, "<WorkflowAgentPane", "/>").includes("scroll"), "the Agent pane is left to itself");
  const run = read("views/WorkflowRun.tsx");
  assert.ok(run.includes('useAsync((s) => api.run(id, s), [id], { keep: runRead })') && run.includes('useGonePlace(missing, { name: "run", id });'), "the run page too");
  assert.ok(run.includes('useViewState<RunTab>(place, "tab", "progress", tabValue)') && run.includes('useViewState<string | null>(place, "canvas:step", null, idValue)'), "with its tab and its step");
});

test("every list keeps the tags picked, its search and its scroll — and draws what it last read", () => {
  // A filter reads back as it was kept; what is no filter is none.
  const kept = JSON.parse(JSON.stringify({ selected: ["delivery", "design"], match: "all" }));
  assert.deepEqual(parseTagFilter(kept), { selected: ["delivery", "design"], match: "all" });
  assert.equal(parseTagFilter("delivery"), undefined);
  for (const [screen, place] of [
    ["views/Goals.tsx", 'placeOf({ name: "goals" })'],
    ["views/Workflows.tsx", 'placeOf({ name: "workflows" })'],
    ["views/Agents.tsx", 'placeOf({ name: "agents" })'],
    ["views/Teams.tsx", 'placeOf({ name: "teams" })'],
  ]) {
    const text = read(screen);
    assert.ok(text.includes(`const PLACE = ${place};`), `${screen} keeps its memory under its own place`);
    assert.ok(text.includes('useViewState<TagFilterState>(PLACE, "tags", NO_TAG_FILTER, parseTagFilter)'), `${screen}: the tags picked`);
    assert.ok(!text.includes("useState<TagFilterState>"), `${screen}: never in the screen's own state`);
    assert.ok(text.includes("useViewScroll(root, ") && text.includes("data-scroll-keep="), `${screen}: its scroll`);
  }
  for (const screen of ["views/Agents.tsx", "views/Teams.tsx"]) assert.ok(read(screen).includes('useViewState(PLACE, "search", "", textValue)'), `${screen}: its search`);
  const channels = read("views/Channels.tsx");
  assert.ok(channels.includes('useViewState<TagFilterState>(INDEX_PLACE, "tags", NO_TAG_FILTER, parseTagFilter)') && channels.includes('const INDEX_PLACE = placeOf({ name: "channels" });'), "the channels' index keeps its tags under the list's place");
  // The reads of a mount are kept for the window.
  const library = read("views/Workflows.tsx");
  assert.ok(library.includes('{ keep: readKey("workflows", archived ? "archived" : "library") }') && library.includes('{ keep: readKey("catalog", "workflow") }'), "the library and the catalog");
  assert.ok(read("views/Teams.tsx").includes('{ keep: readKey("teams") }') && read("views/Teams.tsx").includes('{ keep: readKey("team", team.id) }'), "the teams, and the team open beside them");
  // One path, several things: the name carries which.
  assert.ok(library.includes("data-scroll-keep={`list:${view}`}"));
  assert.ok(read("views/Agents.tsx").includes("data-scroll-keep={`tab:${tab}`}"));
  assert.ok(read("views/Teams.tsx").includes('data-scroll-keep={`page:${selectedId ?? ""}`}'));
  const settings = read("views/Settings.tsx");
  assert.ok(settings.includes('data-scroll-keep="nav"') && settings.includes("data-scroll-keep={`panel:${panel.id}`}") && settings.includes("useViewScroll(root, `${PLACE}#${panel.id}`, PLACE)"), "the settings' rail, and each panel under its own name");
});

test("the Inbox is never empty on the way back, and what is kept of a row is kept under the row's own place", () => {
  const inbox = read("views/Inbox.tsx");
  assert.ok(inbox.includes('const ROWS_READ = readKey("inbox");'), "one key for the rows");
  assert.ok(inbox.includes("useState<InboxRow[]>(() => keptRead<InboxRow[]>(ROWS_READ) ?? [])") && inbox.includes("useState(() => keptRead<InboxRow[]>(ROWS_READ) !== undefined)"), "drawn at once from what the window kept");
  inOrder(between(inbox, "// However the rows moved", "}, [loaded, rows]);"), ["if (loaded) keepRead(ROWS_READ, rows);"], "kept however they moved: a read, a frame, a mark");
  assert.ok(inbox.includes("useViewScroll(list, PLACE)") && inbox.includes('data-scroll-keep="list"'), "the list's scroll");
  inOrder(between(inbox, "const revealed = useRef(false);", "}, [loaded, selected]);"), ["if (revealed.current || !loaded || !selected) return;", "revealed.current = true;", 'scrollIntoView({ block: "nearest" })'], "the row picked is brought back in view, once");
  const happened = between(inbox, "function WhatHappened(", "if (notices.length === 0) return null;");
  inOrder(happened, ["const place = rowPlace(PLACE, row.key);", 'useViewState(place, "notices:all", false, flagValue)', 'useViewState<ReadonlySet<string>>(place, "notices:opened", NONE_OPENED, parseOpened, wordsOf)'], "what happened: shown whole or folded, and the notices opened");
  assert.ok(inbox.includes("useViewScroll(root, rowPlace(PLACE, row.key))") && inbox.includes('data-scroll-keep="detail"') && inbox.includes("<QuietDetail key={current.key}"), "a row's detail keeps its scroll under the row's place");
  assert.equal(rowPlace(placeOf({ name: "inbox" }), "01GOAL"), "/inbox/01GOAL");
  // The ask's own focus is left alone: nothing here scrolls the detail of a row that has a conversation.
  assert.ok(inbox.includes("autoFocus={i === 0}") && !between(inbox, "<Chat", "/>").includes("scroll"), "the pinned ask's focus wins");
});

test("the Pulse keeps its place as a row, and the kit's list never fights the hand", () => {
  const pulse = read("views/Pulse.tsx");
  assert.ok(pulse.includes('const feedKey = readKey("pulse", concept);') && pulse.includes("keepRead(feedKey, next);"), "the feed is kept for the window, a concept at a time");
  assert.ok(pulse.includes("const mine = state.concept === concept;"), "another tab's rows are never drawn under this one's name");
  assert.ok(pulse.includes('useViewState<ReadonlySet<string>>(PLACE, "opened", NONE_OPENED, parseOpened, wordsOf)'), "the rows opened");
  assert.ok(pulse.includes("parseDepth(viewState.read(PLACE, depthName(concept)))") && pulse.includes("viewState.keepQuietly(PLACE, depthName(concept), depthOf(items.length) || null)"), "how far it was read, read once and kept quietly");
  assert.ok(pulse.includes("wantsDepth({ loaded: items.length, wanted: depth.current, next, inFlight: flying })"), "and read back that far");
  assert.ok(pulse.includes("keepAnchor={anchor}") && pulse.includes("key={concept}") && pulse.includes("parseAnchor(viewState.read(PLACE, anchorName(concept)))"), "the list's place is a row, a tab at a time");
  assert.ok(!pulse.includes("data-scroll-keep") && !pulse.includes("useViewScroll"), "never pixels: the feed grows at its head");
  const list = read("ui/VirtualList.tsx");
  assert.ok(list.includes('const HANDS = ["wheel", "pointerdown", "keydown", "touchstart"] as const;') && read("ui/useKeptScroll.ts").includes('const HANDS = ["wheel", "pointerdown", "keydown", "touchstart"] as const;'), "the same hands end a restore as end a scroll's");
  assert.ok(list.includes("for (const hand of HANDS) node.addEventListener(hand, endRestore, true);") && list.includes("window.setTimeout(endRestore, RESTORE_GRACE_MS)"), "the hand or the grace ends it");
  inOrder(between(list, "const noteAnchor = useCallback(", "[flushAnchor],"), ["if (!hands || wanted.current !== null) return;", "anchorOf(rows, top)"], "a scroll the restore made is not the person's place");
  assert.ok(list.includes("if (wanted.current === undefined) wanted.current = hands.read();") && list.includes("scrollFor(anchor, rowsBetween.current(0, items.length))"), "read once, put back as the rows arrive");
});

test("a dialog and a form start empty: nothing typed into one is the screen's memory", () => {
  const teams = read("views/Teams.tsx");
  const dialog = between(teams, "function TeamDialog(", "function TeamCard(");
  assert.ok(dialog.includes('useState("")') && !dialog.includes("useViewState") && !dialog.includes("viewState"), "the team's dialog");
  const channels = read("views/Channels.tsx");
  const forms = between(channels, "function NewChannelDialog(", "/** Where the index keeps its memory");
  assert.ok(forms.includes('useState("")') && !forms.includes("useViewState") && !forms.includes("viewState"), "the channel's dialogs");
  const settings = read("views/Settings.tsx");
  assert.ok(!settings.includes("useViewState") && !settings.includes("keepRead"), "the settings keep their scroll, and nothing a panel's form holds");
  const goal = read("views/GoalDetail.tsx");
  for (const transient of ['useState<"archive" | "delete" | null>(null)', "const [confirmClose, setConfirmClose] = useState(false);", "const [choosing, setChoosing] = useState(false);", "useState<RunVerb | null>(null)"]) assert.ok(goal.includes(transient), `${transient}: a confirmation is the moment's`);
  const agents = read("views/Agents.tsx");
  for (const transient of ["useState<AgentDef | null>(null)", "const [creating, setCreating] = useState(false);", "const [deleting, setDeleting] = useState(false);"]) assert.ok(agents.includes(transient), transient);
  // No screen of this slice names a storage key or reaches storage by hand.
  for (const screen of ["views/Inbox.tsx", "views/Pulse.tsx", "views/Goals.tsx", "views/GoalDetail.tsx", "views/WorkflowRun.tsx", "views/Agents.tsx", "views/Teams.tsx", "views/Settings.tsx", "views/_goal/RunSteps.tsx", "views/_workflow/GoalWorkflowTab.tsx", "views/_workflow/designDraftStore.ts", "views/_workflow/designerMemoryStore.ts"]) {
    const text = read(screen);
    assert.ok(!/\blocalStorage\b/.test(text) && !text.includes("writePref(") && !/"bisa\.view\./.test(text), `${screen} keeps what it keeps through the view memory`);
  }
});
