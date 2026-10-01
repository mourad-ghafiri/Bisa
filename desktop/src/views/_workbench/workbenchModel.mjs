/**
 * Which documents are open in the workbench, per root — the whole rule, with no
 * React in it.
 *
 * # What is here, and what is in the URL
 *
 * This holds **the list of open tabs**. It does not hold which one is active:
 * that lives in the URL (`?doc=file:src/main.rs`), and the component reads both.
 *
 * The split is deliberate and it is not a compromise. The rule of the app is
 * that *almost everything you can be looking at is in the URL* — and what you are
 * looking at is one document. The *set* of tabs you happen to have open is
 * furniture, like the sidebar's width or the tree's expanded folders, none of
 * which are in the URL either; putting it there would also make Back **close a
 * tab**, which Back does not mean anywhere else in this app.
 *
 * Having exactly one writer for "which document" is what removes a whole class
 * of drift by construction. And it works because a tab id fully describes its
 * own tab: a cold reload with an empty store can adopt a `?doc=` it has never
 * seen and rebuild a usable workbench from the URL alone.
 *
 * # Terminals are tabs here, but not *stored* here
 *
 * A terminal tab (`terminal:<key>`) is a session in the terminal store, which
 * is the single owner of every shell. {@link mergedTabs} lists them beside the
 * documents at render time; {@link openTab} refuses to store one, so the two
 * stores can never disagree about which shells exist. Only the *active* id in
 * the URL ever names a terminal.
 *
 * # One order for the strip
 *
 * The root's `strip` is the one order its strip draws — document ids and
 * terminal ids alike, in the order they were opened or dragged, newest last.
 * The pane tree keeps *membership* (which leaf a document sits in; terminals
 * ride the first), `strip` keeps *order*. A document opened appends itself
 * ({@link openTab}); a terminal the strip has not seen is recorded at the
 * end when it is drawn ({@link reconcileStrip}); a drag rewrites the order
 * ({@link moveTab}); a close drops the id. So a file opened after a shell is
 * after the shell, and a harness opened after that is last.
 *
 * # There is no About tab
 *
 * About lives in the right panel (layout). A root with nothing open
 * has an empty strip and nothing active, and the centre shows a landing —
 * `null` is a real answer from {@link nextActiveAfterClose}.
 *
 * # Recently closed
 *
 * Every root keeps the documents closed in it, most recent first and capped,
 * so ⌘⇧T can bring one back. Terminals are not in it: a closed shell is a
 * terminated process, and nothing here can bring that back.
 *
 * # Panes and pins
 *
 * A root's documents sit in a pane tree (`shell/paneTreeModel.mjs`, the same
 * one the terminal layer uses): one leaf until somebody splits, then one tab
 * strip per leaf. Every document tab is in exactly one leaf; closing a pane
 * moves its tabs next door. Terminals are not in the tree — they stay tabs of
 * the root's first strip and are drawn by their own layer. A **pinned** tab
 * sits first in its strip, has no ×, is left alone by *Close others* and
 * *Close all*, and is asked about before it closes.
 *
 * # Preview tabs
 *
 * A single click in the explorer, or a search hit, opens a **preview**: a tab
 * like any other, drawn in italics, of which a pane holds at most one — the
 * next preview takes its place, and the replaced one is not remembered as
 * closed (nobody closed it). It becomes a kept tab when it is edited, saved,
 * pinned, moved, double-clicked, or asked to *Keep open*. Previews are never
 * saved in the layout: what a restart brings back is what you meant to keep.
 * The record is `preview: { [leafId]: tabId }` on the root's entry.
 *
 * # Untitled documents
 *
 * ⌘N opens an **untitled** tab (`untitled:<seq>`, *Untitled-1*): a buffer
 * with no file behind it yet. It is the one tab whose id does not describe
 * its content, so it is never adopted from a `?doc=` the store lacks, never
 * saved in the layout, and never a preview — a document you are about to
 * type into must not be replaced by the next glance. Saving names it: the
 * tab is replaced in place by the file's ({@link replaceTab}), keeping its
 * pane, its position and its pin. `seq` is the lowest number no untitled
 * tab of the root holds ({@link nextUntitledSeq}), so closing *Untitled-2*
 * and pressing ⌘N again gives *Untitled-2*.
 *
 * # Loose files
 *
 * A **loose** tab (`loose:<absolute path>`) is a file from anywhere on this
 * machine — dropped from the file manager or picked in the open dialog —
 * under no project, goal or group (ide/03 §Loose files). Its id is its
 * absolute path, so it survives a reload like a file tab and is saved in the
 * layout; it is kept, never a preview. The path is the shell's canonical one.
 */

import { addTab, closeLeaf, findLeaf, leafOfTab, leaves, normalize, removeTab, setActiveTab, setRatio, singleLeaf, splitLeaf } from "../../shell/paneTreeModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/** What a workbench can be rooted at. Mirrors `WorkbenchScope` in the router. */
export const WORKBENCH_SCOPES = ["workstream", "work_item", "goal"];

/** How many roots keep their tabs. Beyond this the least recently opened in is
 * forgotten — a bound on memory, not a bound on how many projects you have —
 * unless it holds unsaved work (`capRoots`). */
export const MAX_ROOTS = 8;

/**
 * The roots that hold unsaved work, from the keys of the buffers that do
 * (`editorRegistry.editorKey`: `<root>|<tab id>` — a root's key holds no bar).
 * @param {Iterable<string>} unsaved the unsaved buffers' keys
 * @returns {string[]} root keys, none twice
 */
export function heldRoots(unsaved) {
  const roots = new Set();
  for (const key of unsaved ?? []) {
    const at = String(key).indexOf("|");
    if (at > 0) roots.add(String(key).slice(0, at));
  }
  return [...roots];
}

/**
 * The roots kept, most recently opened in first: the first `MAX_ROOTS`, and
 * past them **every root that holds unsaved work**. A root that goes takes
 * its tabs, and a tab that goes takes its buffer (`workbenchStore`'s `set`):
 * the cap is a bound on memory, and what a person typed and did not save is
 * not memory to reclaim — it once went, unasked, with the ninth project
 * opened.
 * @param {readonly {key: string}[]} roots newest first
 * @param {Iterable<string> | null | undefined} held the keys of the roots holding unsaved work
 */
export function capRoots(roots, held) {
  if (roots.length <= MAX_ROOTS) return roots;
  const keep = new Set(held ?? []);
  return roots.filter((root, i) => i < MAX_ROOTS || keep.has(root.key));
}

/** How many closed documents a root remembers for *Reopen the last closed tab*. */
export const MAX_RECENTLY_CLOSED = 10;

export function emptyWorkbench() {
  return { roots: [] };
}

/** The store's key for one rooted thing. Same shape as the URL's path. */
export function rootKey(scope, id) {
  return `${scope}:${id}`;
}

/**
 * A tab's id — self-describing, never opaque, because it goes in the URL and
 * has to survive a reload into a store that has never seen it.
 */
export function tabId(tab) {
  if (!tab) return "";
  if (tab.kind === "file") return `file:${tab.path}`;
  // The side before the path: the path is last because it may hold colons.
  if (tab.kind === "patch") return `patch:${tab.staged ? "staged" : "worktree"}:${tab.path}`;
  if (tab.kind === "commit") return `commit:${tab.sha}`;
  if (tab.kind === "terminal") return `terminal:${tab.key}`;
  if (tab.kind === "browser") return `browser:${tab.key}`;
  if (tab.kind === "device") return `device:${tab.id}`;
  if (tab.kind === "artifact") return `artifact:${tab.message}:${tab.ordinal}`;
  if (tab.kind === "transcript") return `transcript:${tab.session}`;
  if (tab.kind === "untitled") return `untitled:${tab.seq}`;
  if (tab.kind === "loose") return `loose:${tab.path}`;
  return tab.kind;
}

/** What an untitled document is called until it is saved. */
export function untitledName(seq) {
  return `Untitled-${seq}`;
}

/**
 * The number the next untitled document takes: the lowest positive integer
 * no untitled tab in `tabs` holds, so a closed one's number is reused.
 */
export function nextUntitledSeq(tabs) {
  const held = new Set((tabs ?? []).filter((t) => t?.kind === "untitled").map((t) => t.seq));
  let seq = 1;
  while (held.has(seq)) seq += 1;
  return seq;
}

/**
 * The inverse, or `null` for anything this workbench cannot show.
 *
 * `indexOf` rather than `split(":")`: a path may itself contain a colon
 * (`src/a:b.txt` is a legal filename), and splitting would silently truncate it
 * to `src/a`. One character, and it reaches a URL eventually.
 */
export function parseTabId(id) {
  if (id === "diff") return { kind: id };
  if (typeof id !== "string") return null;
  if (id.startsWith("terminal:")) {
    const key = id.slice(9);
    return /^t\d+$/.test(key) ? { kind: "terminal", key } : null;
  }
  if (id.startsWith("browser:")) {
    const key = id.slice(8);
    return /^b\d+$/.test(key) ? { kind: "browser", key } : null;
  }
  if (id.startsWith("device:")) {
    // A device's id: a simulator's UDID, an emulator's image name or serial,
    // a phone's serial — never a path.
    const device = id.slice(7);
    return device && !device.includes("/") ? { kind: "device", id: device } : null;
  }
  if (id.startsWith("artifact:")) {
    // `artifact:<message id>:<ordinal>` — the message id is hex, so the
    // last colon is the one that matters.
    const at = id.lastIndexOf(":");
    const message = id.slice(9, at);
    const ordinal = Number(id.slice(at + 1));
    return message && Number.isInteger(ordinal) && ordinal >= 0
      ? { kind: "artifact", message, ordinal, title: "" }
      : null;
  }
  if (id.startsWith("transcript:")) {
    const session = id.slice(11);
    return session ? { kind: "transcript", session, title: "" } : null;
  }
  if (id.startsWith("patch:")) {
    // `patch:<staged|worktree>:<path>` — one file's patch on one side.
    const rest = id.slice(6);
    const at = rest.indexOf(":");
    if (at === -1) return null;
    const side = rest.slice(0, at);
    const path = rest.slice(at + 1);
    if (!path || (side !== "staged" && side !== "worktree")) return null;
    return { kind: "patch", path, staged: side === "staged" };
  }
  if (id.startsWith("commit:")) {
    const sha = id.slice(7);
    return /^[0-9a-f]{7,64}$/.test(sha) ? { kind: "commit", sha } : null;
  }
  if (id.startsWith("untitled:")) {
    const seq = Number(id.slice(9));
    return Number.isInteger(seq) && seq >= 1 ? { kind: "untitled", seq } : null;
  }
  if (id.startsWith("loose:")) {
    // An absolute path, as the shell canonicalised it; anything else is not a loose file.
    const path = id.slice(6);
    return path.startsWith("/") && path.length > 1 && !path.endsWith("/") ? { kind: "loose", path } : null;
  }
  if (!id.startsWith("file:")) return null;
  const path = id.slice(5);
  return path ? { kind: "file", path } : null;
}

/**
 * The tab an id names, **as the store holds it**: the very object in `docs`
 * whose id is `id`, so a renderer that keys effects on the tab sees one
 * identity across renders. An id the store has not adopted yet — a terminal,
 * a `?doc=` from a pasted link — is parsed instead; nothing is `null`.
 * @param {readonly WorkbenchTab[]} docs
 * @param {string | null | undefined} id
 * @returns {WorkbenchTab | null}
 */
export function tabIn(docs, id) {
  if (!id) return null;
  return (docs ?? []).find((t) => tabId(t) === id) ?? parseTabId(id);
}

function rootAt(state, key) {
  return state.roots.find((r) => r.key === key) ?? null;
}

/** A fresh root's entry: one pane, nothing pinned, nothing remembered, no preview. */
function newEntry(key) {
  return { key, tabs: [], recentlyClosed: [], panes: singleLeaf("d1"), paneSeq: 1, focusedPane: "d1", pinned: [], preview: {}, strip: [] };
}

/** The preview record without the entries `drop` names; the same object when none do. */
function withoutPreviews(preview, drop) {
  const out = {};
  let changed = false;
  for (const [leaf, id] of Object.entries(preview)) {
    if (drop(leaf, id)) changed = true;
    else out[leaf] = id;
  }
  return changed ? out : preview;
}

/**
 * The entry with `ids` gone from everywhere they are named — the tabs, the
 * leaves, the pins, the preview records. The one way a tab leaves, so nothing
 * can be forgotten by one path and kept by another.
 */
function dropTabs(entry, ids) {
  const gone = new Set(ids);
  if (gone.size === 0) return entry;
  let panes = entry.panes;
  for (const id of gone) panes = removeTab(panes, id);
  return {
    ...entry,
    tabs: entry.tabs.filter((t) => !gone.has(tabId(t))),
    panes,
    pinned: entry.pinned.filter((id) => !gone.has(id)),
    preview: withoutPreviews(entry.preview, (_, id) => gone.has(id)),
    strip: entry.strip.filter((id) => !gone.has(id)),
  };
}

/** The entry with `id` no longer a preview — a kept tab from here on. */
function kept(entry, id) {
  return { ...entry, preview: withoutPreviews(entry.preview, (_, p) => p === id) };
}

function replaceRoot(state, entry) {
  return { roots: state.roots.map((r) => (r.key === entry.key ? entry : r)) };
}

/** `closed` (most recent first) pushed onto a root's memory, deduplicated and capped. */
function remember(recentlyClosed, closed) {
  const ids = new Set(closed.map(tabId));
  return [...closed, ...(recentlyClosed ?? []).filter((t) => !ids.has(tabId(t)))].slice(0, MAX_RECENTLY_CLOSED);
}

/** A root's entry after `closed` were closed on purpose: gone everywhere, and remembered. */
function closing(entry, closed) {
  return { ...dropTabs(entry, closed.map(tabId)), recentlyClosed: remember(entry.recentlyClosed, closed) };
}

/**
 * Every stored tab for a root — the documents.
 *
 * A pure read: it never records that the root was visited, so rendering cannot
 * reorder the LRU. Only {@link openTab} promotes a root.
 */
export function tabsFor(state, key) {
  const entry = rootAt(state, key);
  return entry ? entry.tabs : [];
}

/** `ids` in the order `strip` gives them; the ones it does not know after, as they came. */
function inStripOrder(strip, ids) {
  const at = new Map((strip ?? []).map((id, i) => [id, i]));
  const known = ids.filter((id) => at.has(id)).sort((a, b) => at.get(a) - at.get(b));
  const fresh = ids.filter((id) => !at.has(id));
  return [...known, ...fresh];
}

/**
 * The strip: the documents and the terminal sessions rooted here as tabs, in
 * the root's one order (`strip`) — newest last. A tab the order has not seen
 * yet (a terminal that just opened) comes after, as it came, until
 * {@link reconcileStrip} records it. Stable under focus, so nothing
 * reshuffles under the pointer.
 * @param {object[]} docs
 * @param {{key: string}[]} terminalSessions
 * @param {readonly string[]} strip
 */
export function mergedTabs(docs, terminalSessions, strip = [], browserSessions = []) {
  const terminals = (terminalSessions ?? []).map((s) => ({ kind: "terminal", key: s.key }));
  const browsers = (browserSessions ?? []).map((s) => ({ kind: "browser", key: s.key }));
  const all = [...docs, ...terminals, ...browsers];
  const byId = new Map(all.map((t) => [tabId(t), t]));
  return inStripOrder(strip, all.map(tabId)).map((id) => byId.get(id));
}

/**
 * One pane's rendered strip: the leaf's documents and, for the first pane,
 * the terminals rooted here, in the root's order, pinned first.
 * @param {readonly string[]} strip
 * @param {readonly string[]} leafTabs the leaf's document ids
 * @param {readonly string[]} terminalIds `terminal:<key>` ids, or none
 * @param {readonly string[] | null | undefined} pinned
 */
export function leafStrip(strip, leafTabs, terminalIds, pinned) {
  return stripOrder(inStripOrder(strip, [...leafTabs, ...(terminalIds ?? [])]), pinned);
}

/**
 * The strip learns what it drew: ids it has not seen are recorded at the
 * end, in the order they came — a terminal opened now is last, and a
 * document opened after it lands after it — and ids that are gone are
 * forgotten. The same state when nothing moved; a root with no entry yet
 * (terminals alone) gets one, at the cold end.
 * @param {readonly string[]} ids the merged strip as drawn
 */
export function reconcileStrip(state, key, ids, held = null) {
  const entry = rootAt(state, key);
  const strip = entry?.strip ?? [];
  const live = new Set(ids);
  const known = strip.filter((id) => live.has(id));
  const seen = new Set(known);
  const next = [...known, ...ids.filter((id) => !seen.has(id))];
  if (next.length === strip.length && next.every((id, i) => id === strip[i])) return state;
  if (entry) return replaceRoot(state, { ...entry, strip: next });
  // A root seen now is the most recent, as `openTab` keeps them.
  return { roots: capRoots([{ ...newEntry(key), strip: next }, ...state.roots], held) };
}

/**
 * Open a document — kept, or as the focused pane's **preview**.
 *
 * A document already open is left where it is: deliberately **not** moved to
 * the end, because re-clicking a file in the tree is the ordinary way this is
 * reached, and a strip that reshuffled under the pointer would put a different
 * tab where the one you just aimed at used to be. A kept tab stays kept
 * whatever is asked; a preview asked for as kept is promoted.
 *
 * A new preview takes the pane's one preview slot: whatever held it leaves,
 * and is not remembered as closed — nobody closed it.
 *
 * `held` names the roots holding unsaved work: the cap never takes one.
 */
export function openTab(state, key, tab, { preview = false, held = null } = {}) {
  const id = tabId(tab);
  if (!id || !parseTabId(id)) return state;
  // A terminal is the terminal store's and a browser tab the browser
  // store's; listing either here would be a second owner of a process.
  if (tab.kind === "terminal" || tab.kind === "browser") return state;
  // An untitled document is kept whatever was asked: a preview is replaced by
  // the next glance, and this is a buffer somebody is about to type into. A
  // loose file was dropped or picked on purpose — kept as well.
  if (tab.kind === "untitled" || tab.kind === "loose") preview = false;

  const entry = rootAt(state, key);
  // Already open: glanced at again, it is shown again where it is; opened
  // on purpose, a preview is promoted to a kept tab.
  if (entry?.tabs.some((t) => tabId(t) === id)) return preview ? activateDoc(state, key, id) : keepTab(state, key, id);

  const base = entry ?? newEntry(key);
  const others = state.roots.filter((r) => r.key !== key);
  // Into the focused pane, shown there — added before the old preview leaves,
  // so a pane whose only tab was that preview is never emptied on the way.
  const leaf = findLeaf(base.panes, base.focusedPane) ?? leaves(base.panes)[0];
  // Newest last: the strip's order is the order things were opened in.
  const strip = base.strip.includes(id) ? base.strip : [...base.strip, id];
  const added = { ...base, tabs: [...base.tabs, tab], panes: addTab(base.panes, leaf.id, id), strip };
  const replaced = preview && base.preview[leaf.id] ? dropTabs(added, [base.preview[leaf.id]]) : added;
  const next = preview ? { ...replaced, preview: { ...replaced.preview, [leaf.id]: id } } : replaced;
  // Most recently opened-in first, so the eviction below drops the coldest — never one holding unsaved work.
  return { roots: capRoots([next, ...others], held) };
}

/**
 * Open a document **beside** what is open (ide/19): a root with one pane and
 * something in it is split `dir`-wise and the document goes into the new
 * half, so the code stays where it is and the device appears next to it; a
 * root already split, or with nothing open yet, takes it like any document
 * into the focused pane. Already open, it is shown where it is and kept.
 */
export function openTabBeside(state, key, tab, dir = "row", held = null) {
  const id = tabId(tab);
  if (!id || !parseTabId(id)) return state;
  if (tab.kind === "terminal" || tab.kind === "browser") return state;
  const entry = rootAt(state, key);
  if (entry?.tabs.some((t) => tabId(t) === id)) return keepTab(activateDoc(state, key, id), key, id);
  const base = entry ?? newEntry(key);
  const all = leaves(base.panes);
  if (all.length !== 1 || all[0].tabs.length === 0) return openTab(state, key, tab, { held });
  const others = state.roots.filter((r) => r.key !== key);
  const leaf = all[0];
  const splitId = `ds${base.paneSeq + 1}`;
  const newLeaf = `d${base.paneSeq + 2}`;
  const split = splitLeaf(base.panes, leaf.id, dir, splitId, newLeaf, false);
  const strip = base.strip.includes(id) ? base.strip : [...base.strip, id];
  const next = { ...base, tabs: [...base.tabs, tab], panes: addTab(split, newLeaf, id), strip, paneSeq: base.paneSeq + 2, focusedPane: newLeaf };
  return { roots: capRoots([next, ...others], held) };
}

/** A preview becomes a kept tab. The same state when it was not a preview — safe to call from an effect. */
export function keepTab(state, key, id) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const next = kept(entry, id);
  return next.preview === entry.preview ? state : replaceRoot(state, next);
}

/** Whether a document is its pane's preview. */
export function isPreview(state, key, id) {
  return Object.values(rootAt(state, key)?.preview ?? {}).includes(id);
}

/** Every preview tab id in a root. */
export function previewIds(state, key) {
  return Object.values(rootAt(state, key)?.preview ?? {});
}

/** The one tree a root nobody split reads as — shared, never written to (every change makes a new tree). */
const EMPTY_PANES = Object.freeze(singleLeaf("d1"));

/** The pane tree of a root (one leaf for a root nobody split). */
export function panesFor(state, key) {
  return rootAt(state, key)?.panes ?? EMPTY_PANES;
}



/**
 * Split the focused pane (or `leafId`), moving the active document into the
 * new half so the split shows two things, not one and an empty pane. A pane
 * with fewer than two documents has nothing to split with and stays.
 */
export function splitDocPane(state, key, dir, leafId = null) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const leaf = findLeaf(entry.panes, leafId ?? entry.focusedPane) ?? leaves(entry.panes)[0];
  // A split takes the active document across; the pane it leaves must keep
  // two, or one of the two panes shows a single document beside an empty
  // choice — no split when only one document would be left showing.
  if (leaf.tabs.length < 3) return state;
  const splitId = `ds${entry.paneSeq + 1}`;
  const newLeaf = `d${entry.paneSeq + 2}`;
  const panes = splitLeaf(entry.panes, leaf.id, dir, splitId, newLeaf, true);
  if (panes === entry.panes) return state;
  // The active tab went across on purpose: a preview that travels is kept.
  const moved = leaf.active === null ? entry : kept(entry, leaf.active);
  return replaceRoot(state, { ...moved, panes, paneSeq: entry.paneSeq + 2, focusedPane: newLeaf });
}

/** Close a pane; its documents move next door — its preview as a kept tab, since the pane next door may have its own. The only pane stays. */
export function closeDocPane(state, key, leafId) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const panes = closeLeaf(entry.panes, leafId);
  if (panes === entry.panes) return state;
  const focusedPane = findLeaf(panes, entry.focusedPane) ? entry.focusedPane : leaves(panes)[0].id;
  const preview = withoutPreviews(entry.preview, (leaf) => leaf === leafId);
  return replaceRoot(state, { ...entry, panes, focusedPane, preview });
}

/** Move a document into another pane and show it there; a preview moved on purpose is kept. */
export function moveDocToPane(state, key, id, leafId) {
  const entry = rootAt(state, key);
  if (!entry || !entry.tabs.some((t) => tabId(t) === id) || !findLeaf(entry.panes, leafId)) return state;
  const panes = addTab(entry.panes, leafId, id);
  if (panes === entry.panes) return state;
  return replaceRoot(state, { ...kept(entry, id), panes, focusedPane: leafId });
}

export function focusDocPane(state, key, leafId) {
  const entry = rootAt(state, key);
  if (!entry || !findLeaf(entry.panes, leafId) || entry.focusedPane === leafId) return state;
  return replaceRoot(state, { ...entry, focusedPane: leafId });
}

/** Show `id` in its pane, and focus that pane. */
export function activateDoc(state, key, id) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const leaf = leafOfTab(entry.panes, id);
  if (!leaf) return state;
  const panes = setActiveTab(entry.panes, leaf.id, id);
  if (panes === entry.panes && entry.focusedPane === leaf.id) return state;
  return replaceRoot(state, { ...entry, panes, focusedPane: leaf.id });
}

export function setDocPaneRatio(state, key, splitId, ratio) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const panes = setRatio(entry.panes, splitId, ratio);
  return panes === entry.panes ? state : replaceRoot(state, { ...entry, panes });
}

/** Whether a document is pinned in its root. */
export function isPinned(state, key, id) {
  return (rootAt(state, key)?.pinned ?? []).includes(id);
}

/** Pin or unpin an open document. A pinned tab sits first in its strip; a pinned preview is a kept tab. */
export function togglePin(state, key, id) {
  const entry = rootAt(state, key);
  if (!entry || !entry.tabs.some((t) => tabId(t) === id)) return state;
  const pinned = entry.pinned.includes(id) ? entry.pinned.filter((p) => p !== id) : [...entry.pinned, id];
  return replaceRoot(state, { ...kept(entry, id), pinned });
}

/** The tabs of one leaf in strip order: pinned first, then as opened. */
export function stripOrder(tabIds, pinned) {
  const pins = new Set(pinned ?? []);
  return [...tabIds.filter((id) => pins.has(id)), ...tabIds.filter((id) => !pins.has(id))];
}

/**
 * A drag along the strip: put `id` at `index` among the tabs the strip
 * showed (`shown` — {@link leafStrip}'s answer, documents and terminals as
 * one run). A pinned tab stays among the pinned and an unpinned one among the
 * rest — the two groups are the strip's one fixed fact — so an index across
 * the boundary is answered `{state, refused}` rather than applied. The
 * root's `strip` is what a strip draws, and it is what moves; the root's
 * `tabs` list keeps the order things were opened in, the leaf its membership.
 * @param {readonly string[] | null | undefined} shown the strip as rendered; the pane's documents when absent
 * @returns {{state: object, refused: string | null}}
 */
export function moveTab(state, key, id, index, shown = null) {
  const entry = rootAt(state, key);
  if (!entry) return { state, refused: null };
  const leaf = id.startsWith("terminal:") ? null : leafOfTab(entry.panes, id);
  const rendered = shown ?? (leaf ? leafStrip(entry.strip, leaf.tabs, [], entry.pinned) : null);
  if (!rendered || !rendered.includes(id)) return { state, refused: null };
  const from = rendered.indexOf(id);
  const pinnedCount = rendered.filter((t) => entry.pinned.includes(t)).length;
  const pinned = entry.pinned.includes(id);
  const target = Math.max(0, Math.min(index, rendered.length - 1));
  if (pinned ? target >= pinnedCount : target < pinnedCount) {
    return { state, refused: pinned ? tr("workbench-workbench-pinned-tab-stays-among-pinned-ones") : tr("workbench-workbench-pinned-tabs-sit-first-pin-one") };
  }
  if (from === target) return { state, refused: null };
  const next = [...rendered];
  next.splice(from, 1);
  next.splice(target, 0, id);
  // The shown ids take their new relative order at the places they held in
  // the root's order; every other pane's tabs keep theirs.
  const moving = new Set(rendered);
  const queue = next.filter((t) => moving.has(t));
  const known = new Set(entry.strip);
  const strip = entry.strip.map((t) => (moving.has(t) ? queue.shift() : t));
  for (const t of queue) if (!known.has(t)) strip.push(t);
  // Placed by hand: a preview somebody arranged is a tab they mean to keep.
  return { state: replaceRoot(state, { ...kept(entry, id), strip }), refused: null };
}

/**
 * Close every document to the right of `id` in its pane's strip — the pinned
 * ones stay, as they do for *Close others*.
 */
export function closeTabsRight(state, key, id) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const leaf = leafOfTab(entry.panes, id);
  if (!leaf) return state;
  // The pane's documents in the strip's order: a terminal to the right is
  // left alone — closing a shell is a different verb.
  const shown = leafStrip(entry.strip, leaf.tabs, [], entry.pinned);
  const right = new Set(shown.slice(shown.indexOf(id) + 1).filter((t) => !entry.pinned.includes(t)));
  if (right.size === 0) return state;
  return replaceRoot(state, closing(entry, entry.tabs.filter((t) => right.has(tabId(t)))));
}

/** The ids to the right of `id` in its pane's strip, unpinned — what *Close to the right* would close. */
export function tabsRightOf(state, key, id) {
  const entry = rootAt(state, key);
  const leaf = entry ? leafOfTab(entry.panes, id) : null;
  if (!entry || !leaf) return [];
  const shown = leafStrip(entry.strip, leaf.tabs, [], entry.pinned);
  return shown.slice(shown.indexOf(id) + 1).filter((t) => !entry.pinned.includes(t));
}

export function closeTab(state, key, id) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const closed = entry.tabs.filter((t) => tabId(t) === id);
  if (closed.length === 0) return state;
  return replaceRoot(state, closing(entry, closed));
}

/** Close every document but `id` — and but the pinned ones, which stay. */
export function closeOtherTabs(state, key, id) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const closed = entry.tabs.filter((t) => tabId(t) !== id && !entry.pinned.includes(tabId(t)));
  if (closed.length === 0) return state;
  return replaceRoot(state, closing(entry, closed));
}

/** Close every document — the pinned ones stay. */
export function closeAllTabs(state, key) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const closed = entry.tabs.filter((t) => !entry.pinned.includes(tabId(t)));
  if (closed.length === 0) return state;
  return replaceRoot(state, closing(entry, closed));
}

/** The ids *Close saved* would close: every document that is neither dirty nor pinned. */
export function savedTabs(state, key, dirtyIds) {
  const entry = rootAt(state, key);
  if (!entry) return [];
  const dirty = new Set(dirtyIds);
  return entry.tabs.map(tabId).filter((id) => !dirty.has(id) && !entry.pinned.includes(id));
}

/** Close every document that has nothing unsaved in it — the pinned ones stay. */
export function closeSavedTabs(state, key, dirtyIds) {
  const entry = rootAt(state, key);
  if (!entry) return state;
  const gone = new Set(savedTabs(state, key, dirtyIds));
  if (gone.size === 0) return state;
  return replaceRoot(state, closing(entry, entry.tabs.filter((t) => gone.has(tabId(t)))));
}

/**
 * Bring back the most recently closed document that is not open again already.
 * Returns the new state and the tab to make active, or the same state and
 * `null` when the root remembers nothing usable.
 */
export function reopenLastClosed(state, key) {
  const entry = rootAt(state, key);
  if (!entry) return { state, tab: null };
  const open = new Set(entry.tabs.map(tabId));
  const tab = (entry.recentlyClosed ?? []).find((t) => !open.has(tabId(t))) ?? null;
  if (!tab) return { state, tab: null };
  const remaining = entry.recentlyClosed.filter((t) => tabId(t) !== tabId(tab));
  const leaf = findLeaf(entry.panes, entry.focusedPane) ?? leaves(entry.panes)[0];
  const reopened = { ...entry, tabs: [...entry.tabs, tab], recentlyClosed: remaining, panes: addTab(entry.panes, leaf.id, tabId(tab)) };
  return { state: replaceRoot(state, reopened), tab };
}

/** The open document tabs at `path` or under it (a folder's). */
export function tabsUnder(tabs, path) {
  return (tabs ?? []).filter((t) => t.kind === "file" && (t.path === path || t.path.startsWith(`${path}/`)));
}

/**
 * A rename on disk: every document tab at `from` (or under it, for a folder)
 * follows to `to`, keeping its place in the strip. Returns the new state and
 * the pairs that moved, so the caller can move the active id too.
 */
/**
 * The tabs that were open and no longer are, as `[rootKey, tabId]` pairs —
 * what a close, a replace or a root falling out of the cap leaves behind. The
 * buffers of exactly these are forgotten (`docBuffersStore.ts`): a tab that
 * merely moved pane, or a root whose tabs were just restored, is not in it.
 * @param {{roots: {key: string, tabs: object[]}[]}} before
 * @param {{roots: {key: string, tabs: object[]}[]}} after
 * @returns {[string, string][]}
 */
export function closedTabs(before, after) {
  const open = new Map((after?.roots ?? []).map((r) => [r.key, new Set(r.tabs.map(tabId))]));
  const gone = [];
  for (const root of before?.roots ?? []) {
    const still = open.get(root.key);
    for (const tab of root.tabs) {
      const id = tabId(tab);
      if (!still?.has(id)) gone.push([root.key, id]);
    }
  }
  return gone;
}

export function retargetTabs(state, key, from, to) {
  const entry = rootAt(state, key);
  if (!entry) return { state, moved: [] };
  const moved = [];
  const tabs = entry.tabs.map((t) => {
    if (t.kind !== "file") return t;
    if (t.path === from) {
      moved.push([t.path, to]);
      return { kind: "file", path: to };
    }
    if (t.path.startsWith(`${from}/`)) {
      const next = `${to}${t.path.slice(from.length)}`;
      moved.push([t.path, next]);
      return { kind: "file", path: next };
    }
    return t;
  });
  if (moved.length === 0) return { state, moved };
  // The tree and the pins name tabs by id: rename them in place, keeping each
  // tab's pane and its position in the strip.
  const rename = new Map(moved.map(([was, now]) => [tabId({ kind: "file", path: was }), tabId({ kind: "file", path: now })]));
  const relabel = (n) =>
    n.kind === "leaf"
      ? { ...n, tabs: n.tabs.map((id) => rename.get(id) ?? id), active: n.active ? rename.get(n.active) ?? n.active : null }
      : { ...n, a: relabel(n.a), b: relabel(n.b) };
  const panes = normalize(relabel(entry.panes));
  const pinned = entry.pinned.map((id) => rename.get(id) ?? id);
  const preview = Object.fromEntries(Object.entries(entry.preview).map(([leaf, id]) => [leaf, rename.get(id) ?? id]));
  const strip = entry.strip.map((id) => rename.get(id) ?? id);
  return { state: replaceRoot(state, { ...entry, tabs, panes, pinned, preview, strip }), moved };
}

/**
 * One tab becomes another, in place: the same position in the strip, the
 * same pane, the same pin. What an untitled document does when it is saved
 * under a name — `untitled:1` becomes `file:<path>` and nothing else moves.
 * The same state when `fromId` is not open, or the replacement already is.
 */
export function replaceTab(state, key, fromId, tab) {
  const entry = rootAt(state, key);
  const toId = tabId(tab);
  if (!entry || !toId || !parseTabId(toId) || tab.kind === "terminal" || tab.kind === "browser") return state;
  if (!entry.tabs.some((t) => tabId(t) === fromId)) return state;
  if (entry.tabs.some((t) => tabId(t) === toId)) return state;
  const tabs = entry.tabs.map((t) => (tabId(t) === fromId ? tab : t));
  const relabel = (n) =>
    n.kind === "leaf"
      ? { ...n, tabs: n.tabs.map((id) => (id === fromId ? toId : id)), active: n.active === fromId ? toId : n.active }
      : { ...n, a: relabel(n.a), b: relabel(n.b) };
  const panes = normalize(relabel(entry.panes));
  const pinned = entry.pinned.map((id) => (id === fromId ? toId : id));
  const preview = Object.fromEntries(Object.entries(entry.preview).map(([leaf, id]) => [leaf, id === fromId ? toId : id]));
  const strip = entry.strip.map((id) => (id === fromId ? toId : id));
  return replaceRoot(state, { ...entry, tabs, panes, pinned, preview, strip });
}

/** For a root that has been deleted out from under the workbench. */
export function forgetRoot(state, key) {
  if (!rootAt(state, key)) return state;
  return { roots: state.roots.filter((r) => r.key !== key) };
}

/**
 * Which tab should be showing once `closingId` is gone — or `null` when the
 * strip is empty afterwards, which the centre renders as its landing.
 *
 * Pure, and separate from {@link closeTab}, because the model must not know the
 * URL exists — the caller closes the tab *and* navigates, and this is the one
 * fact both halves need to agree on. Operates on the rendered list.
 */
export function nextActiveAfterClose(tabs, closingId, activeId) {
  if (closingId !== activeId) return activeId;
  const ids = tabs.map(tabId);
  const at = ids.indexOf(closingId);
  if (at === -1) return activeId;
  // The tab to the right, then the one to the left — the rule every editor
  // uses, and the one that keeps your place while closing a run left to right.
  return ids[at + 1] ?? ids[at - 1] ?? null;
}

const KIND_LABEL = { diff: tr("workbench-workbench-diff"), terminal: tr("workbench-center-documents-terminal"), browser: tr("workbench-browser-doc-browser") };

/** What a patch's side is called on a tab and in a title. */
const SIDE_WORD = { staged: "staged", worktree: "tree" };
const SIDE_TITLE = { staged: "staged", worktree: tr("workbench-workbench-working-tree") };
const sideOf = (tab) => (tab.staged ? "staged" : "worktree");

/** What a tab calls itself in the strip. A file shows its basename; the full
 * path is the tooltip, because a strip of full paths is unreadable. A patch
 * is its file's basename with the side's word; a commit its short id. */
export function tabLabel(tab) {
  if (!tab) return "";
  if (tab.kind === "terminal") return tr("workbench-workbench-terminal", { key: tab.key });
  if (tab.kind === "browser") return tr("workbench-workbench-browser", { key: tab.key });
  if (tab.kind === "device") return tr("workbench-workbench-device", { tab: tab.id.slice(0, 8) });
  if (tab.kind === "patch") return `${basenameOf(tab.path)} · ${SIDE_WORD[sideOf(tab)]}`;
  if (tab.kind === "commit") return tab.sha.slice(0, 7);
  if (tab.kind === "artifact") return tab.title || tr("workbench-workbench-artifact-2");
  if (tab.kind === "transcript") return tab.title || tr("workbench-workbench-transcript-2");
  if (tab.kind === "untitled") return untitledName(tab.seq);
  if (tab.kind === "loose") return basenameOf(tab.path);
  if (tab.kind !== "file") return KIND_LABEL[tab.kind] ?? tab.kind;
  return basenameOf(tab.path);
}

function basenameOf(path) {
  const at = path.lastIndexOf("/");
  return at === -1 ? path : path.slice(at + 1);
}

/** The full thing a tab is about, for a tooltip and an aria-label. */
export function tabTitle(tab) {
  if (!tab) return "";
  if (tab.kind === "file") return tab.path;
  if (tab.kind === "patch") return tr("workbench-workbench-patch", { tab: tab.path, tab2: SIDE_TITLE[sideOf(tab)] });
  if (tab.kind === "commit") return tr("workbench-workbench-commit", { sha: tab.sha });
  if (tab.kind === "terminal") return tr("workbench-workbench-terminal", { key: tab.key });
  if (tab.kind === "browser") return tr("workbench-workbench-browser", { key: tab.key });
  if (tab.kind === "device") return tr("workbench-workbench-device-simulator-emulator-phone-mirrored-beside", { tab: tab.id });
  if (tab.kind === "artifact") return tab.title ? tr("workbench-workbench-artifact", { title: tab.title }) : tr("workbench-workbench-artifact-2");
  if (tab.kind === "transcript") return tab.title ? tr("workbench-workbench-transcript", { title: tab.title }) : tr("workbench-workbench-transcript-2");
  if (tab.kind === "untitled") return tr("workbench-workbench-not-saved-yet", { seq: untitledName(tab.seq) });
  if (tab.kind === "loose") return tr("workbench-workbench-machine-under-no-project", { tab: tab.path });
  return KIND_LABEL[tab.kind] ?? tab.kind;
}
