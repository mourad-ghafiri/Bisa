/**
 * The Draw overlay's facts, where they can be tested without a DOM (19 —
 * Drawings). Each is here because getting it wrong produces a wrong *fact*
 * rather than a wrong pixel: a tab that lists another kind's drawings, a new
 * drawing filed somewhere you were not, a search that drops a drawing that
 * says the words, a scene saved with a window's own state in it.
 *
 * The scope vocabulary, the tabs, the targets and the words for a scope are
 * the notes overlay's (`notes/notesModel.mjs`): a drawing hangs off the same
 * six things a note does, and one rule holds for both.
 *
 * Plain `.mjs` with a `.d.mts` beside it — `node --test` imports it with no
 * build step, and TypeScript reads the declarations.
 */

import { t as tr } from "../i18n/l10n.mjs";
import { NOTE_TABS, NOTE_TAB_LABEL, noteTab, noteTargets, routeTarget, scopeOfRow, scopeWords, tabAdmits, tabKind, tabQuery } from "../notes/notesModel.mjs";

/** The tabs, in the order the strip draws them; *All* is first and the default. */
export const DRAW_TABS = NOTE_TABS;
export const DRAW_TAB_LABEL = NOTE_TAB_LABEL;

/** Which tab a stored preference selects; anything unknown is *All*. */
export const drawTab = noteTab;
/** The query string a tab's listing sends to `GET /drawings`. */
export const drawTabQuery = tabQuery;
/** Whether a drawing of `kind` belongs on `tab`. */
export const drawTabAdmits = tabAdmits;
/** The scope kind a tab lists, or `null` for every kind. */
export const drawTabKind = tabKind;
/** The scopes a new drawing may take under a tab, the route's own first. */
export const drawTargets = noteTargets;
export { routeTarget, scopeOfRow, scopeWords };

/**
 * The drawings a search keeps: a case-insensitive match on the title, order
 * kept. Every word of the query has to appear. A drawing's scene is not
 * searched — its texts are the canvas's, and a list carries no scene.
 * @template {{title?: string}} T
 * @param {T[]} rows
 * @param {string | null | undefined} query
 */
export function filterDrawings(rows, query) {
  const words = (query ?? "").toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return rows;
  return rows.filter((r) => {
    const hay = (r.title ?? "").toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/* ---------------------------------------------------------------------------
   The scene a drawing keeps
--------------------------------------------------------------------------- */

/** The ground a new canvas is drawn on. */
const DEFAULT_BACKGROUND = "#ffffff";
/** The grid's pitch when it shows — the canvas's own default. */
const GRID_SIZE = 20;

/**
 * The part of the canvas's state a drawing keeps: the ground and whether the
 * grid shows — nothing else. The selection, the zoom, the open menu, the
 * collaborators are one window's, and saving them would make every viewer's
 * canvas jump to the last saver's.
 * @param {{viewBackgroundColor?: unknown, gridModeEnabled?: unknown} | null | undefined} appState
 * @returns {{view_background_color: string, grid: boolean}}
 */
export function persistedAppState(appState) {
  const colour = typeof appState?.viewBackgroundColor === "string" && appState.viewBackgroundColor.trim() !== "" ? appState.viewBackgroundColor : DEFAULT_BACKGROUND;
  return { view_background_color: colour, grid: appState?.gridModeEnabled === true };
}

/**
 * The canvas's state from a saved scene: the two facts, in the canvas's
 * words. `gridModeEnabled` and `gridSize` together, since the canvas reads both.
 * @param {{view_background_color?: string, grid?: boolean} | null | undefined} saved
 */
export function canvasAppState(saved) {
  return {
    viewBackgroundColor: saved?.view_background_color ?? DEFAULT_BACKGROUND,
    gridModeEnabled: saved?.grid === true,
    gridSize: GRID_SIZE,
  };
}

/**
 * The elements a drawing keeps: the drawn ones. The canvas keeps a deleted
 * element for its undo; a record does not, and the store drops them too —
 * dropping them here keeps the hash the desktop reads equal to the one it
 * would get back.
 * @template {{isDeleted?: boolean}} E
 * @param {readonly E[]} elements
 */
export function drawnElements(elements) {
  return elements.filter((e) => e.isDeleted !== true);
}

/**
 * Whether a scene the canvas reports differs from what was last saved, by
 * the canvas's own version numbers — cheap, and what `onChange` fires for
 * (a hover changes nothing here).
 * @param {readonly {id: string, version?: number, isDeleted?: boolean}[]} shown
 * @param {readonly {id: string, version?: number, isDeleted?: boolean}[]} saved
 */
export function sceneMoved(shown, saved) {
  const a = drawnElements(shown);
  const b = drawnElements(saved);
  if (a.length !== b.length) return true;
  const versions = new Map(b.map((e) => [e.id, e.version ?? 0]));
  return a.some((e) => versions.get(e.id) !== (e.version ?? 0));
}

/* ---------------------------------------------------------------------------
   Preferences
--------------------------------------------------------------------------- */

/** How long after the last stroke the canvas saves. A scene is heavier than a note, so it waits a little longer. */
export const SAVE_MIN_MS = 300;
export const SAVE_MAX_MS = 5000;
export const SAVE_STEP_MS = 100;
export const SAVE_DEFAULT_MS = 1000;

/**
 * A stored or typed autosave delay, made safe to hand to `setTimeout`: the
 * default for anything that is not a number, the bounds for one that is.
 * @param {unknown} raw
 */
export function clampSaveDelay(raw) {
  const n = typeof raw === "string" ? Number(raw) : raw;
  if (typeof n !== "number" || !Number.isFinite(n)) return SAVE_DEFAULT_MS;
  return Math.min(SAVE_MAX_MS, Math.max(SAVE_MIN_MS, Math.round(n / SAVE_STEP_MS) * SAVE_STEP_MS));
}

/** What the header's status says, in one word or none. */
export function drawStatusWords({ conflict, error, saving, dirty }) {
  if (conflict) return conflict;
  if (error) return error;
  if (saving) return tr("draw-status-saving");
  if (dirty) return tr("draw-status-unsaved");
  return "";
}

/**
 * Whether a drawing that could not be read goes without a word. The open
 * drawing is kept across a restart, and what it names may have gone while
 * the app was closed: the one that came back from the last window and that
 * the node no longer has opens nothing, and says nothing — the person asked
 * for nothing. Any other failure is said.
 * @param {string | null} id the drawing that could not be read
 * @param {string | null} restored the drawing that came back from the last window, until it was read
 * @param {number | null | undefined} status the answer's status
 */
export function goneQuietly(id, restored, status) {
  return id !== null && id === restored && status === 404;
}
