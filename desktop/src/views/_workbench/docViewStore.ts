/**
 * Where a person was in each open document (ide/03 §Tabs): the code editor's
 * view — scroll, cursor, selection, folds — a rendering's scroll offsets, a
 * page's scroll inside its frame, and the mode the document is read in.
 *
 * A pane draws one tab at a time, so a document that leaves the screen is
 * unmounted and comes back as a new component: without this it came back at
 * the top, every time. A place is the **tab's**, beside its buffer
 * (`docBuffersStore.ts`) and under the same key (`editorKey`: `<root>|<tab
 * id>`): kept from the first scroll, carried across a rename on disk,
 * forgotten when the tab closes — in the one place tabs change,
 * `workbenchStore`'s `set` — and with its root when the root is forgotten.
 *
 * **Kept across a restart.** The open documents come back with the node's
 * layout, and each comes back where it was: the views live in the documents'
 * memory (`docViews`, `shell/viewMemoryStore.ts`), window furniture written
 * on a beat. A scroll and an editor's view are kept quietly — read once at
 * mount, so writing them re-renders nobody; the mode is what a document
 * draws, so it is kept aloud and a mounted document follows a write from
 * outside it. What is read back is made safe first (`docViewModel.mjs`).
 *
 * **A closed tab keeps nothing.** A document is still drawn when its tab
 * closes, and its unmount hands its view over one last time — after the
 * place was forgotten. That last word is refused (`closed`), or every
 * closed document would leave its view behind in a memory that outlives the
 * window; the document keeps its place again the moment it is drawn again.
 *
 * The editor's slot is opaque here: Monaco's view state is Monaco's own format.
 */

import { useCallback, useSyncExternalStore } from "react";
import { Lru } from "../../shell/lru.mjs";
import { docViews } from "../../shell/viewMemoryStore";
import { wordOf } from "../../shell/viewValuesModel.mjs";
import type { Place } from "../../ui/keptScrollModel.mjs";
import { DOC_MODE, EDITOR_VIEW, PAGE_SCROLL, editorViewValue, placeValue, scrollName } from "./docViewModel.mjs";
import { docsPrefix } from "./idePlacesModel.mjs";

/** The documents whose tab closed: what their unmount hands over is not kept. Bounded — a key is a few words, and only the newest closes can still be unmounting. */
const closed = new Lru<true>(256);

/** A document reads its place, so it is drawn: from here on it keeps it again. */
function read(key: string, name: string): unknown {
  closed.delete(key);
  return docViews.read(key, name);
}

/** Keep quietly, unless the document's tab has closed. */
function keepOf(key: string, name: string, value: unknown): void {
  if (!closed.has(key)) docViews.keepQuietly(key, name, value);
}

export function editorViewOf(key: string): unknown {
  return editorViewValue(read(key, EDITOR_VIEW));
}
export function keepEditorView(key: string, state: unknown): void {
  keepOf(key, EDITOR_VIEW, editorViewValue(state));
}

export function scrollOf(key: string, name: string): Place | null {
  return placeValue(read(key, scrollName(name)));
}
export function keepScroll(key: string, name: string, place: Place | null): void {
  keepOf(key, scrollName(name), place);
}

export function pageScrollOf(key: string): Place | null {
  return placeValue(read(key, PAGE_SCROLL));
}
export function keepPageScroll(key: string, place: Place | null): void {
  keepOf(key, PAGE_SCROLL, place);
}

/**
 * A document's mode written from outside its render — the door that opens a
 * file *for review* forcing Source before the document mounts
 * (`reviewLensModel.reviewOpenDrafts`). A mounted document follows at once.
 */
export function keepDocMode(key: string, mode: string): void {
  closed.delete(key);
  docViews.keep(key, DOC_MODE, mode);
}

/**
 * The mode a document is read in, kept under the document's key: a tab
 * switch keeps it, and so does a restart. A word the document does not
 * offer — another version's, a file that changed its kind — is `initial`.
 */
export function useDocMode<M extends string>(key: string, offered: readonly M[], initial: M): [M, (next: M) => void] {
  const subscribe = useCallback((cb: () => void) => docViews.subscribe(key, DOC_MODE, cb), [key]);
  const raw = useSyncExternalStore(
    subscribe,
    () => docViews.read(key, DOC_MODE),
    () => undefined,
  );
  // A word, so what is drawn is the same between renders without a memo.
  const mode = wordOf(offered)(raw) ?? initial;
  const set = useCallback((next: M) => keepDocMode(key, next), [key]);
  return [mode, set];
}

/** Files renamed on disk: each place follows its tab to the new key. */
export function moveViews(moves: readonly { from: string; to: string }[]): void {
  for (const { from, to } of moves) docViews.move(from, to);
}

/** The tabs closed: their places go with them. */
export function forgetViews(keys: readonly string[]): void {
  for (const key of keys) {
    docViews.forget(key);
    closed.set(key, true);
  }
}

/** A root forgotten — closed, retired, deleted out from under the workbench: every document of it goes, open or not. */
export function forgetRootViews(root: string): void {
  docViews.forgetUnder(docsPrefix(root));
}
