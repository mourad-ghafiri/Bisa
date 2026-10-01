/**
 * Where I was, in the threads, the Project IDE, Notes and Draw
 * (`crates/desktop.md` §Per-viewer state): a thread comes back where it was
 * being read, a document where it was scrolled and in the mode it was read
 * in, the Git panel, the rail, the Files tree and the Board as they stood —
 * across leaving the screen and across a restart — and what is gone takes
 * its memory with it. The journeys run over the models; the wiring is read
 * from the sources, as `readAsShown.test.mjs` reads it — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/whereIWasIde.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { KeptMemory } from "../shell/keptMemoryModel.mjs";
import { joinNewest, keptThread } from "../views/_studio/threadCacheModel.mjs";
import { parseThreadPlace, placeFrom, restoreStep, RESTORE_PAGES } from "../views/_studio/threadPlaceModel.mjs";
import { EMPTY_SESSION, gitViewOf, withGitView } from "../views/_work/gitPanelModel.mjs";
import { DOC_MODE, EDITOR_VIEW, editorViewValue, placeValue, scrollName } from "../views/_workbench/docViewModel.mjs";
import { docModeKey } from "../views/_workbench/fileDocModel.mjs";
import { docsPrefix, gitPlace, idePlace, placesOfRoot } from "../views/_workbench/idePlacesModel.mjs";
import { listedSelection, parseSelection } from "../views/_workbench/railSelectionModel.mjs";

const read = (rel) => readFileSync(new URL(`../${rel}`, import.meta.url), "utf8");

/** Whether `facts` stand in `text` in that order. */
function inOrder(text, facts, why) {
  let at = -1;
  for (const fact of facts) {
    const next = text.indexOf(fact, at + 1);
    assert.ok(next > at, `${why}: ${fact}`);
    at = next;
  }
}

/** A memory over a storage that is a map, and a clock that never fires: `flush` writes. */
function memoryOver(storage, key = "bisa.test.view") {
  const hands = {
    storage: () => ({ getItem: (k) => storage.get(k) ?? null, setItem: (k, v) => void storage.set(k, v), removeItem: (k) => void storage.delete(k) }),
    now: () => 0,
    later: () => 0,
    cancel: () => {},
  };
  return new KeptMemory({ key, version: 1, caps: { places: 16, valueBytes: 8 * 1024, totalBytes: 64 * 1024 }, hands });
}

const message = (n) => ({ id: `m${n}`, created_at: n, retracted: false });
const page = (from, to) => Array.from({ length: to - from + 1 }, (_, i) => message(from + i));

test("a thread left above its bottom comes back on the message it was on — in the window from what it had, after a restart by paging to it", () => {
  const storage = new Map();
  const places = memoryOver(storage);
  // Reading back in a long thread: the third page up, the edge inside m41.
  const rows = page(1, 120).map((m, i) => ({ id: m.id, top: i * 50, height: 50 }));
  places.keepQuietly("channel:c1", "at", placeFrom(rows, 40 * 50 + 12));
  // Leaving the screen: the thread's messages are the window's, and the newest page joins them.
  const held = keptThread({ messages: page(1, 120), reactions: [], hasOlder: true });
  const back = joinNewest({ shown: held.messages, page: page(63, 122), pageSize: 60, hasOlder: held.hasOlder });
  const kept = parseThreadPlace(places.read("channel:c1", "at"));
  assert.deepEqual(kept, { message: "m41", offset: 12 });
  assert.deepEqual(restoreStep({ kept, loading: false, ids: back.messages.map((m) => m.id), hasOlder: back.hasOlder, pagesLoaded: 0 }), { do: "scroll", message: "m41", offset: 12 }, "the message never left the page");
  // A restart: the place is read back from storage, the messages from the node — the newest page first.
  places.flush();
  const again = parseThreadPlace(memoryOver(storage).read("channel:c1", "at"));
  assert.deepEqual(again, kept);
  let shown = page(63, 122);
  let pages = 0;
  assert.deepEqual(restoreStep({ kept: again, loading: true, ids: [], hasOlder: false, pagesLoaded: 0 }), { do: "wait" }, "nothing moves under a skeleton");
  while (restoreStep({ kept: again, loading: false, ids: shown.map((m) => m.id), hasOlder: true, pagesLoaded: pages }).do === "older") {
    pages += 1;
    shown = [...page(Math.max(1, shown[0].created_at - 60), shown[0].created_at - 1), ...shown];
  }
  assert.equal(pages, 1, "one page up reaches it");
  assert.equal(restoreStep({ kept: again, loading: false, ids: shown.map((m) => m.id), hasOlder: true, pagesLoaded: pages }).do, "scroll");
  // A message further back than the bound: the thread opens at its bottom and the place is forgotten.
  const far = { message: "m0", offset: 0 };
  assert.deepEqual(restoreStep({ kept: far, loading: false, ids: ["m900"], hasOlder: true, pagesLoaded: RESTORE_PAGES }), { do: "bottom", forget: true });
});

test("the thread's wiring: the place is the chat's own, a restore never fights the hand, and the messages are the window's", () => {
  const chat = read("views/_studio/Chat.tsx");
  inOrder(chat, ["const place = useThreadPlace(chatKey(kind, scope));", "const restoring = useRef<ThreadPlace | null>(place.kept);", "useState(() => place.kept === null);"], "read once, before the thread decides where it starts");
  assert.ok(chat.includes("for (const hand of HANDS) el.addEventListener(hand, endRestore, true);") && chat.includes("window.setTimeout(endRestore, RESTORE_GRACE_MS)"), "the hand ends it, and the grace");
  assert.ok(chat.includes("if (restoring.current) scrollToPlace(el, restoring.current);"), "what grows above is followed while the restore is on");
  const messages = read("views/_studio/useScopeMessages.ts");
  assert.ok(messages.includes("useState(() => !threadOf(key))"), "a thread with something kept is not loading");
  assert.ok(messages.includes("if (current.current !== key) return;"), "an answer that lands after the scope changed is dropped");
  assert.ok(read("views/_work/keptReadsStore.ts").includes("forgetThreads();"), "forgotten with the other reads");
});

test("a document comes back where it was and in the mode it was read in, and a closed tab keeps nothing", () => {
  const storage = new Map();
  const docs = memoryOver(storage, "bisa.test.docs");
  const key = docModeKey("workstream:01W", "docs/guide.md");
  assert.ok(key.startsWith(docsPrefix("workstream:01W")), "a file's mode is kept under the document's own key");
  docs.keepQuietly(key, EDITOR_VIEW, { viewState: { scrollTop: 480 }, cursorState: [] });
  docs.keepQuietly(key, scrollName("rendered"), { top: 300, left: 0 });
  docs.keep(key, DOC_MODE, "split");
  docs.flush();
  const back = memoryOver(storage, "bisa.test.docs");
  assert.deepEqual(editorViewValue(back.read(key, EDITOR_VIEW)), { viewState: { scrollTop: 480 }, cursorState: [] });
  assert.deepEqual(placeValue(back.read(key, scrollName("rendered"))), { top: 300, left: 0 });
  assert.equal(back.read(key, DOC_MODE), "split");
  // The file is renamed: the view follows. The root is closed: every document of it goes.
  back.move(key, docModeKey("workstream:01W", "docs/manual.md"));
  assert.equal(back.read(docModeKey("workstream:01W", "docs/manual.md"), DOC_MODE), "split");
  back.keep(docModeKey("workstream:01W2", "a.md"), DOC_MODE, "source");
  assert.equal(back.forgetUnder(docsPrefix("workstream:01W")), 1);
  assert.equal(back.read(docModeKey("workstream:01W2", "a.md"), DOC_MODE), "source", "another root's documents stay");

  const store = read("views/_workbench/docViewStore.ts");
  assert.ok(store.includes("docViews.keepQuietly(key, name, value)") && store.includes("docViews.keep(key, DOC_MODE, mode)"), "a scroll is kept quietly; the mode aloud, so a mounted document follows");
  inOrder(store.slice(store.indexOf("export function forgetViews(")), ["docViews.forget(key);", "closed.set(key, true);"], "the unmount's last word is refused");
  assert.ok(store.includes("if (!closed.has(key)) docViews.keepQuietly("), "a closed tab keeps nothing");
  assert.ok(read("views/_workbench/EditorDoc.tsx").includes("useDocMode<DocMode>(registryKey, modes, defaultMode(docKind))"), "the mode's words are the document's own");
  const editor = read("ui/CodeEditor.tsx");
  assert.ok(editor.includes("ed.onDidScrollChange(keepView)") && editor.includes("VIEW_BEAT_MS"), "the editor hands its view over while it is on screen: a closing window unmounts nothing");
  const pane = read("views/_workbench/useConversationPane.tsx");
  assert.ok(pane.includes("keepDocMode(drafts.modeKey, drafts.mode);"), "Review forces Source under the same key");
});

test("the Git panel comes back as it stood, and nothing an operation left comes back with it", () => {
  const storage = new Map();
  const view = memoryOver(storage);
  const stood = { ...EMPTY_SESSION, busy: "push", failure: { kind: "commit", error: "refused", paths: [] }, selection: { path: "src/a.ts", staged: false }, changesFolds: ["dir:src"], graphRefs: "head" };
  view.keepQuietly(gitPlace("workstream:01W"), "view", gitViewOf(stood));
  view.flush();
  const back = withGitView(EMPTY_SESSION, memoryOver(storage).read(gitPlace("workstream:01W"), "view"));
  assert.deepEqual(gitViewOf(back), gitViewOf(stood));
  assert.deepEqual([back.busy, back.failure, back.pending, back.operation], [null, null, null, null], "a restart starts clean");
  const store = read("views/_work/gitPanelStore.ts");
  assert.ok(store.includes("withGitView(EMPTY_SESSION, viewState.read(gitPlace(scope), VIEW))") && store.includes("if (!sameGitView(current, next))"), "read once a scope, written when the view moved");
  for (const [file, fact] of [
    ["views/_work/BranchesPanel.tsx", 'useViewState(gitPlace(scope), "branches.filter", "", textValue)'],
    ["views/_work/CommitGraph.tsx", 'useViewState(place, "history.q", "", textValue)'],
    ["views/_work/CommitGraph.tsx", 'useViewState<string | null>(place, "history.open", null, idValue)'],
  ]) assert.ok(read(file).includes(fact), `${file}: ${fact}`);
});

test("the rail, the Files tree, the search box and the Board keep how they stood, each under its place", () => {
  assert.equal(idePlace("workstream:01W"), "ide:workstream:01W");
  for (const [file, fact] of [
    ["views/_workbench/ProjectRail.tsx", 'useViewState(RAIL_PLACE, "filter", "", textValue)'],
    ["views/_workbench/ProjectRail.tsx", "useViewScroll(rail, RAIL_PLACE);"],
    ["views/_workbench/RightPanel.tsx", 'useViewState(idePlace(key), "files.open", NO_FOLDERS, wordsValue)'],
    ["views/_work/GoalInspector.tsx", 'useViewState(place, "files.open", NO_FOLDERS, wordsValue)'],
    ["views/_workbench/FilesSearch.tsx", 'useViewState(place, "search.q", "", textValue)'],
    ["views/_board/BoardCenter.tsx", 'useViewState(place, "board.q", "", textValue)'],
    ["views/_board/BoardCenter.tsx", "useViewScroll(board, place);"],
    ["views/_studio/useConversationSurface.ts", 'useViewState(place, "conversations.q", "", textValue)'],
    ["views/_workbench/useConversationPane.tsx", "place: idePlace(scopeKey)"],
  ]) assert.ok(read(file).includes(fact), `${file}: ${fact}`);
  // The hits are the disk's: read again, never kept.
  assert.ok(read("views/_workbench/FilesSearch.tsx").includes("const [hits, setHits] = useState<SearchHit[]>([]);"));
  // The rail follows the route: a scroll being put back gives way to the reveal.
  inOrder(read("views/_workbench/ProjectRail.tsx"), ["setCursor(id);", "yieldKeptScroll(rail.current);", "setScrollTo((prev) => ({ id, nonce: prev.nonce + 1 }));"], "the reveal is the last word");
  const hook = read("ui/useKeptScroll.ts");
  inOrder(hook.slice(hook.indexOf("const giveWay = () => {")), ["step();", "finish();"], "what is kept is put back once, then the restore ends");
  // The tree is handed its folders when it starts and when its root changes, and hands them out as they change.
  const tree = read("ui/FileTree.tsx");
  assert.ok(tree.includes("useReducer(reduce, openFolders, initialState)") && tree.includes('dispatch({ type: "reset", open: folders.current.kept });') && tree.includes("folders.current.tell?.(openFolderPaths(open));"));
});

test("the rail's selection comes back, and a project that went while the app was closed selects nothing", () => {
  const storage = new Map();
  const view = memoryOver(storage);
  view.keepQuietly("rail", "selection", { kind: "group", label: "Shop", projects: ["p1", "p2"] });
  view.flush();
  const back = parseSelection(memoryOver(storage).read("rail", "selection"));
  assert.deepEqual(back, { kind: "group", label: "Shop", projects: ["p1", "p2"] });
  assert.deepEqual(listedSelection(back, ["p2", "p3"]), { kind: "group", label: "Shop", projects: ["p2"] });
  assert.equal(listedSelection(back, ["p3"]), null);
  const rail = read("views/_workbench/ProjectRail.tsx");
  assert.ok(rail.includes("if (ws.ready) keepListedSelection("), "only once the workspace has said what it lists");
});

test("a root that is gone for good takes what was kept of it, and leaving a root does not", () => {
  assert.deepEqual(placesOfRoot("workstream:01W"), ["ide:workstream:01W", "git:workstream:01W"]);
  const store = read("views/_workbench/workbenchStore.ts");
  const forget = store.slice(store.indexOf("export function forgetRootMemory("));
  inOrder(forget.slice(0, forget.indexOf("\n}")), ["forgetRootViews(key);", "for (const place of placesOfRoot(key)) viewState.forget(place);"], "its documents' views, its place, its Git panel's");
  const leave = store.slice(store.indexOf("export function forgetWorkbenchRoot("));
  assert.ok(!leave.slice(0, leave.indexOf("\n}")).includes("forgetRootMemory"), "Back to the project closes the documents and keeps how the root stood");
  assert.ok(read("views/_work/closeWorkstream.ts").includes('forgetRootMemory(rootKey("workstream", wid));'));
  assert.ok(read("views/_work/retire.ts").includes('forgetRootMemory(rootKey("workstream", wid));'));
});

test("Notes and Draw come back on what was open, keep no form and no drawer, and add no storage key", () => {
  for (const [file, settle] of [
    ["notes/notesStore.ts", "export function settleRestoredNote("],
    ["draw/drawStore.ts", "export function restoredDrawing("],
  ]) {
    const store = read(file);
    assert.ok(store.includes("active: idValue(viewState.read(PLACE, ACTIVE)) ?? null") && store.includes('query: textValue(viewState.read(PLACE, QUERY)) ?? ""'), `${file}: read once, when the store loads`);
    assert.ok(store.includes("viewState.keepQuietly(PLACE, ACTIVE, next.active)") && store.includes("viewState.keepQuietly(PLACE, QUERY, next.query || null)"), `${file}: kept quietly on a change`);
    assert.ok(store.includes("askOpen: false,") && !/keepQuietly\([^)]*askOpen/.test(store), `${file}: the Ask drawer is the moment's`);
    assert.ok(store.includes(settle), `${file}: what is gone opens nothing`);
  }
  assert.ok(read("notes/NoteOverlay.tsx").includes("settleRestoredNote(notes.map((n) => n.id));"), "the list is how a note is known to be there");
  assert.ok(read("draw/DrawOverlay.tsx").includes("goneQuietly(active, restoredDrawing(), e instanceof ApiError ? e.status : null)"), "a drawing is known by its read");
  // The memories are the three the shell made: nothing here writes a key of its own.
  for (const file of ["views/_studio/threadsStore.ts", "views/_studio/useThreadPlace.ts", "views/_workbench/docViewStore.ts", "views/_workbench/railSelectionStore.ts", "views/_workbench/idePlacesModel.mjs"]) {
    const text = read(file);
    assert.ok(!/"bisa\.[a-z0-9.]+"/.test(text) && !/\blocalStorage\b/.test(text) && !text.includes("writePref("), `${file}: no key, no storage of its own`);
  }
});
