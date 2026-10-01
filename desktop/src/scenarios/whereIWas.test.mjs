/**
 * Where you were, as a person meets it: a screen left comes back as it was
 * left — by the sidebar, the palette and a card — a launch opens where the
 * app closed, what is gone is forgotten, and *Forget where I was* starts
 * every screen as new. The journey runs over the models; the wiring is read
 * from the sources, as every scenario here reads it — no DOM. Run with
 * `node --test desktop/src/scenarios/whereIWas.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { href, indexRouteOf, parse } from "../routeModel.mjs";
import { closeFlow } from "../shell/closeFlowModel.mjs";
import { gonePaths } from "../shell/gonePlacesModel.mjs";
import { KeptMemory } from "../shell/keptMemoryModel.mjs";
import { arrive, emptyPlaces, forgetPath, knowsPath, launchHash, parsePlaces, pathOf, sectionHash } from "../shell/placeMemoryModel.mjs";
import { forgetFlow, forgottenBy } from "../shell/whereIWasModel.mjs";
import { parsePlace } from "../ui/keptScrollModel.mjs";

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

/** A window: the place memory and the view memory over one disk, and the hash it stands on. */
function openWindow(store) {
  const timers = new Map();
  let next = 1;
  const view = new KeptMemory({
    key: "bisa.view.state",
    version: 1,
    caps: { places: 96, valueBytes: 8 * 1024, totalBytes: 256 * 1024 },
    hands: { storage: () => store, now: () => 1_000, later: (fn) => (timers.set(next, fn), next++), cancel: (id) => void timers.delete(id) },
  });
  const kept = store.getItem("bisa.view.places");
  const w = {
    view,
    places: (kept === null ? undefined : parsePlaces(JSON.parse(kept))) ?? emptyPlaces(),
    at: "",
    known: false,
    /** Follow a link, a door or a card: an arrival. */
    go(hash) {
      const out = arrive(w.places, hash, {});
      const path = pathOf(hash);
      if (path !== pathOf(w.at)) w.known = out.known;
      w.places = out.places;
      w.at = out.land;
      return w.at;
    },
    /** Change the screen in place — a tab, a filter, the pane: an exact entry. */
    set(hash) {
      const out = arrive(w.places, hash, { exact: true });
      w.places = out.places;
      w.at = out.land;
      return w.at;
    },
    /** A section's door — the sidebar's row, the rail, the palette's *Go to*. */
    door(sectionKey) {
      return w.go(sectionHash(w.places, sectionKey, w.at));
    },
    /** The way out's `keep`: both memories written. */
    close() {
      view.flush();
      store.setItem("bisa.view.places", JSON.stringify(w.places));
    },
    /** The launch: no hash, so the place the app closed on — else the home. */
    launch(home) {
      return w.go(launchHash(w.places) ?? home);
    },
  };
  return w;
}

test("a screen left comes back as it was left — by the sidebar, the palette and a card", () => {
  const w = openWindow(disk());
  // The Goals list, filtered.
  w.go("#/inbox");
  w.door("goals");
  assert.equal(w.at, "#/goals", "a section never been in opens on its list");
  w.set("#/goals?holder=you&q=ship");
  // A goal, from its card: a first visit, so the page may open its Details pane.
  w.go("#/goals/01G");
  assert.equal(w.known, false, "never been on: the page opens its pane");
  w.set("#/goals/01G?aux=inspector");
  // The Workflow tab, the pane closed by hand, a step picked, half-way down the page.
  w.set("#/goals/01G?tab=workflow&aux=inspector");
  w.set("#/goals/01G?tab=workflow");
  w.view.keep("/goals/01G", "workflow:step", "review");
  w.view.keepQuietly("/goals/01G", "scroll:tab:workflow", { top: 640, left: 0 });
  // A channel, read half-way up.
  w.door("channels");
  w.go("#/channels/general");
  // Away, to the Inbox.
  w.door("inbox");
  assert.equal(w.at, "#/inbox");

  // Back by the sidebar.
  assert.equal(w.door("goals"), "#/goals/01G?tab=workflow", "the goal, on its tab, the pane still closed");
  assert.equal(w.known, true, "been on: the page leaves its pane as it was left");
  assert.equal(w.view.read("/goals/01G", "workflow:step"), "review");
  assert.deepEqual(parsePlace(w.view.read("/goals/01G", "scroll:tab:workflow")), { top: 640, left: 0 });
  // Pressed again from inside the goal: the list, as it was left.
  assert.equal(w.door("goals"), "#/goals?holder=you&q=ship");
  // And again from the list: nowhere.
  assert.equal(w.door("goals"), "#/goals?holder=you&q=ship");

  // Back by a card on another screen — a bare address.
  w.door("inbox");
  assert.equal(w.go("#/goals/01G"), "#/goals/01G?tab=workflow");
  // A link that names its own tab is taken at its word.
  w.door("inbox");
  assert.equal(w.go("#/goals/01G?tab=progress"), "#/goals/01G?tab=progress");
  // The channel is where Channels leads.
  assert.equal(w.door("channels"), "#/channels/general");
});

test("a launch opens where the app closed, with what every screen kept", () => {
  const store = disk();
  const first = openWindow(store);
  first.go("#/goals/01G");
  first.set("#/goals/01G?tab=workflow&aux=inspector&insp=files");
  first.view.keep("/goals/01G", "opened", ["step-2", "step-5"]);
  first.view.keepQuietly("/goals/01G", "scroll:tab:workflow", { top: 640, left: 0 });
  first.close();

  const second = openWindow(store);
  assert.equal(second.launch("#/inbox"), "#/goals/01G?tab=workflow&aux=inspector&insp=files", "the same screen, tab and pane");
  assert.equal(second.known, true, "a place the app closed on is a place been on");
  assert.deepEqual(second.view.read("/goals/01G", "opened"), ["step-2", "step-5"]);
  assert.deepEqual(parsePlace(second.view.read("/goals/01G", "scroll:tab:workflow")), { top: 640, left: 0 });
  // The doors remember too.
  second.go("#/inbox");
  assert.equal(second.door("goals"), "#/goals/01G?tab=workflow&aux=inspector&insp=files");

  // A machine that never closed the app opens on the home.
  assert.equal(openWindow(disk()).launch("#/inbox"), "#/inbox");
  // What a link hands over once is never where a launch opens.
  const invited = openWindow(disk());
  invited.go("#/settings?tab=people&join=SECRETCODE");
  invited.close();
  assert.ok(![...invited.view.keptPlaces(), JSON.stringify(invited.places)].join("").includes("SECRETCODE"), "an invitation code never reaches the memory");
});

test("what is gone is forgotten: no door returns there, and the section falls to its list", () => {
  const store = disk();
  const w = openWindow(store);
  w.go("#/goals");
  w.set("#/goals?holder=you");
  w.go("#/goals/01G");
  w.set("#/goals/01G?tab=workflow");
  w.view.keep("/goals/01G", "opened", ["step-2"]);
  w.go("#/inbox");

  // The goal is deleted: the bus says so, and the shell forgets its places.
  for (const path of gonePaths({ type: "goal_deleted", goal: "01G" })) {
    w.view.forget(path);
    w.view.forgetUnder(`${path}/`);
    w.places = forgetPath(w.places, path);
  }
  assert.equal(knowsPath(w.places, "/goals/01G"), false);
  assert.equal(w.view.read("/goals/01G", "opened"), undefined);
  assert.equal(w.door("goals"), "#/goals?holder=you", "the door leads to the list, as it was left");

  // Deleted while the app was closed: the launch lands there, the read says
  // *not found*, and the screen leaves for its section's list.
  const closed = openWindow(disk());
  closed.go("#/workflows/01W");
  closed.close();
  assert.equal(launchHash(closed.places), "#/workflows/01W");
  assert.deepEqual(indexRouteOf(parse("#/workflows/01W", { name: "inbox" })), { name: "workflows" });
  assert.equal(href(indexRouteOf(parse("#/channels/general", { name: "inbox" }))), "#/channels");
  // An archived thing is still there.
  assert.deepEqual(gonePaths({ type: "goal_archived", goal: "01G", archived: true }), []);
});

test("forget where I was: every screen starts as new, the documents and the furniture stay", async () => {
  const store = disk();
  store.setItem("bisa.theme", "dark");
  store.setItem("bisa.sidebar.order", '["goals","inbox"]');
  const w = openWindow(store);
  w.go("#/goals/01G");
  w.set("#/goals/01G?tab=workflow");
  w.view.keep("/goals/01G", "opened", ["step-2"]);
  w.close();

  const said = [];
  const flow = forgetFlow({
    dirty: () => true,
    save: async () => (said.push("save"), true),
    forget: {
      memories: () => {
        w.view.clear();
        w.places = emptyPlaces(w.places.owner);
        store.setItem("bisa.view.places", JSON.stringify(w.places));
        said.push("memories");
      },
      root: () => void said.push("root"),
      conversations: () => void said.push("conversations"),
      drafts: () => void said.push("drafts"),
    },
    seal: () => {
      w.view.seal();
      said.push("seal");
    },
    leave: () => {
      // The window on its way out: the screen it stood on hands its last scroll over, and the window flushes as it goes.
      w.view.keepQuietly("/settings", "scroll:main", { top: 240, left: 0 });
      w.view.keep("/goals/01G", "opened", ["step-2"]);
      w.view.flush();
      said.push("leave");
    },
  });
  assert.equal(await flow.run({ drafts: false }), "forgotten");
  assert.deepEqual(said, ["save", "memories", "root", "conversations", "seal", "leave"], "saved first; the messages being written were not asked for; sealed before the window goes");
  assert.deepEqual(forgottenBy({ drafts: true }).at(-1), "drafts");
  assert.equal(store.getItem("bisa.view.state"), null, "what the window handed over on its way out reached no storage");

  const next = openWindow(store);
  assert.equal(next.launch("#/inbox"), "#/inbox", "the home");
  assert.equal(next.go("#/goals/01G"), "#/goals/01G");
  assert.equal(next.known, false, "as new");
  assert.equal(next.view.read("/goals/01G", "opened"), undefined);
  assert.equal(store.getItem("bisa.theme"), "dark");
  assert.equal(store.getItem("bisa.sidebar.order"), '["goals","inbox"]');
});

test("what is remembered is kept on every way out", async () => {
  // The quit: asked, saved, kept, gone.
  const said = [];
  const flow = closeFlow({
    confirms: () => true,
    ask: async () => (said.push("ask"), true),
    dirty: () => true,
    save: async () => (said.push("save"), true),
    unsaved: () => void said.push("unsaved"),
    keep: async () => void said.push("keep"),
  });
  assert.equal(await flow.run(async () => void said.push("finish")), "closed");
  assert.deepEqual(said, ["ask", "save", "keep", "finish"]);

  const guard = read("shell/useCloseGuard.ts");
  assert.ok(guard.includes("keep: settleMemories,"), "the quit flow's keep is the memories'");
  inOrder(between(guard, "function hide(): void {", "\n}\n"), ["flushMemories();", 'invoke("hide_window")'], "the window put away writes first");
  const locale = read("i18n/localeStore.ts");
  inOrder(between(locale, "function setLocale(locale: string): void {", "\n}\n"), ["flushMemories();", "window.location.reload();"], "a language change writes before the window opens again");
  const memories = read("shell/viewMemoryStore.ts");
  assert.ok(memories.includes('window.addEventListener("pagehide", flushMemories)') && memories.includes('document.visibilityState === "hidden"'), "and so does a window that is put away by the OS");
});

test("the wiring: the router resolves before anything mounts, and every door is the section's", () => {
  const main = read("main.tsx");
  inOrder(main, ["installRouter();", "followWindowForMemories();", "ReactDOM.createRoot("], "before the first render");
  const router = read("router.ts");
  assert.ok(router.includes("launchPlace()") && router.includes("land("), "a launch with no hash opens on the last place; every hash lands through the memory");
  assert.ok(!/useSyncExternalStore\([^)]*window\.location\.hash/.test(router), "React reads the resolved hash, never the window's");
  for (const door of ["shell/Sidebar.tsx", "shell/SidebarRail.tsx"]) assert.ok(read(door).includes("useSectionHref("), `${door} reads the section's door`);
  assert.ok(read("shell/Omnibox.tsx").includes("goToSection("), "the palette goes through the same door");
  const doors = read("shell/sectionDoor.ts");
  assert.ok(doors.includes("sectionHash(places, key, at)") && doors.includes("sectionHash(placesNow(), section(route), address())"), "one rule behind the link and the chord");

  const app = read("App.tsx");
  assert.ok(app.includes("<GoalDetail key={route.id} id={route.id} />"), "a goal page is its goal's: another goal is another page");
  inOrder(between(app, "for (const path of gonePaths(e.payload)) {", "\n    }\n"), ["forgetPlaceMemory(path);", "const root = rootOfPath(path);", "if (root) forgetRootMemory(root);"], "the shell hears what is gone, once: the place, and the root of the IDE that stood on it");
  assert.ok(app.includes("adoptMemoryOwner(workspace.me)"), "the memory is one workspace's");
  // Every detail screen that reads its thing hands what is missing over: a channel deleted raises no fact on the bus at all, and a checkout deleted while the app was closed reaches the workbench as a read that says not-found.
  for (const [screen, route] of [
    ["views/Workbench.tsx", '{ name: "workbench", scope, id }'],
    ["views/GoalDetail.tsx", '{ name: "goal", id }'],
    ["views/WorkflowDesigner.tsx", '{ name: "workflow", id }'],
    ["views/WorkflowRun.tsx", '{ name: "run", id }'],
    ["views/Channels.tsx", '{ name: "channel", id }'],
    ["views/Messages.tsx", '{ name: "dm", id }'],
    ["views/ConversationDoor.tsx", '{ name: "conversation", id }'],
  ]) {
    const text = read(screen);
    assert.ok(/useGonePlace\((one\.)?missing, /.test(text) && text.includes(route), `${screen} leaves a place that is gone`);
  }
  for (const screen of ["views/Channels.tsx", "views/Messages.tsx"]) {
    inOrder(read(screen), ["if (error && !channel) {", "<ErrorNote error={error} retry={reload} />", "if (loading && !channel) {"], `${screen} says a channel it could not read, and never draws it as a conversation`);
  }
  const gone = read("shell/useGonePlace.ts");
  inOrder(gone, ["const known = placeWasKnown(path);", "forgetPlaceMemory(path);", "if (!known) return;", "replace(indexRouteOf(route));"], "forgotten either way; left only by a person who had been there");
});

test("the wiring: the memories are three over one class, and no store of them hears the bus", () => {
  const memories = read("shell/viewMemoryStore.ts");
  for (const key of ["bisa.view.state", "bisa.view.docs", "bisa.view.threads"]) assert.ok(memories.includes(`key: "${key}"`), key);
  assert.equal((memories.match(/new KeptMemory\(/g) ?? []).length, 3);
  for (const store of ["shell/viewMemoryStore.ts", "shell/placeMemoryStore.ts", "views/_work/keptReadsStore.ts"]) {
    const text = read(store);
    assert.ok(!text.includes("useEngineEvents") && !text.includes("onEngineEvent"), `${store} hears no bus: what is gone is told to it`);
    assert.ok(!/\blocalStorage\b\s*\./.test(text), `${store} reaches storage through storedPrefModel`);
  }
  const scroll = read("ui/useKeptScroll.ts");
  assert.ok(scroll.includes("let own = keptRef.current;"), "a scroll is written under the identity it was read for");
});

test("the wiring: Forget where I was is one card under Settings › Desktop, and it opens the window again", () => {
  const settings = read("views/Settings.tsx");
  inOrder(between(settings, 'panel.id === "desktop" && (', "</>"), ['<RegistryPanel group="desktop" />', "<WhereIWasCard />"], "under the switches");
  const card = read("views/_settings/WhereIWasCard.tsx");
  assert.ok(card.includes("<ConfirmDialog") && card.includes("danger\n"), "it asks first, in red");
  inOrder(between(card, "const ask = () => {", "};"), ["setDrafts(false);", "setAsking(true);"], "the confirmation starts as it is written: the box unchecked");
  assert.ok(card.includes("forgetWhereIWas({ drafts })"), "the box is the only thing it is told");
  const hands = read("shell/whereIWas.ts");
  inOrder(between(hands, "const flow = forgetFlow({", "});\n"), ["dirty: anyDirty,", "save: flushAll,", "forgetEveryMemory();", "forgetEveryRead();", "root: forgetLastRoot,", "forgetPref(webStorage(), PICK_KEY)", 'forgetEveryDraft([noteDraftKey("")])', "seal: sealMemories,", "replace(homeRoute());", "window.location.reload();"], "saved, forgotten, sealed, home, opened again");
  const memories = read("shell/viewMemoryStore.ts");
  inOrder(between(memories, "export function sealMemories(): void {", "\n}\n"), ["for (const memory of MEMORIES) memory.seal();", "sealPlaces();"], "the three memories and the places, sealed together");
  const places = read("shell/placeMemoryStore.ts");
  assert.ok(places.includes("if (sealed || next === places) return;") && places.includes("if (timer !== null && !sealed) write();"), "a sealed place memory records nothing and writes nothing, and a hash still lands");
  // The furniture that is no place is named nowhere in it, and neither is the window's file.
  for (const kept of ["bisa.theme", "bisa.sidebar", "bisa.aux", "bisa.ide.mode", "bisa.terminal", "bisa.browser", "window.json", "window_state"]) assert.ok(!hands.includes(kept), kept);
  assert.ok(!/localStorage\s*\.\s*clear\(\)/.test(hands) && !hands.includes(".clear()"), "the storage is never swept whole");
});
