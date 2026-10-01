/**
 * The places the Project IDE keeps its view under (`crates/desktop.md`
 * §Per-viewer state; `shell/viewMemoryStore.ts`). A routed screen keeps its
 * memory under its route's path; the IDE's panels are not routes — one root
 * is shown by many addresses, a tab and a document among them — so each is
 * a **named place**, and the names have one home:
 *
 * - `ide:<scope>:<id>` — a root: its Files tree's open folders, its search
 *   box, its conversations list.
 * - `board` — the Board: its search and where it was scrolled. One board,
 *   whatever root it is opened from — its scope is the rail's selection.
 * - `git:<scope>:<id>` — a checkout's Git panel: the file selected, the
 *   rows folded, the history's filter and search.
 * - `rail` — the project rail: its filter, its scroll, its selection.
 *
 * A document's own view — the editor's, its scroll, its mode — is kept
 * under the document's key (`editorKey`), in the documents' memory.
 *
 * Facts only. Plain `.mjs`, so `node --test` reads it.
 */

/** The project rail's place: one rail, whatever root is open. */
export const RAIL_PLACE = "rail";

/** The Board's place: one board, whatever root it is opened from. */
export const BOARD_PLACE = "board";

/**
 * The place a root keeps its view under.
 * @param {string} root the root's key — `rootKey(scope, id)`, `workstream:01W`
 */
export function idePlace(root) {
  return `ide:${root}`;
}

/**
 * The place a checkout's Git panel keeps its view under.
 * @param {string} scope the Git session's scope key — `workstream:01W`
 */
export function gitPlace(scope) {
  return `git:${scope}`;
}

/**
 * The places that go with a root that is forgotten — closed, retired,
 * deleted out from under the workbench.
 * @param {string} root the root's key
 * @returns {string[]}
 */
export function placesOfRoot(root) {
  return [idePlace(root), gitPlace(root)];
}

/**
 * The root a path of the Project IDE shows — `/projects/workstream/01W` is
 * `workstream:01W` — or `null` for a path that is no root's: what a fact
 * about a thing that is gone names is a path (`shell/gonePlacesModel.mjs`),
 * and a root's memory is kept under its key.
 * @param {string} path
 * @returns {string | null} the root's key
 */
export function rootOfPath(path) {
  const parts = typeof path === "string" ? path.split("/") : [];
  if (parts.length !== 4 || parts[0] !== "" || parts[1] !== "projects") return null;
  const [scope, id] = [parts[2], parts[3]];
  if (!/^[a-z_]+$/.test(scope) || id === "" || /[?#]/.test(id)) return null;
  return `${scope}:${id}`;
}

/**
 * The prefix every document of a root is kept under in the documents'
 * memory: `editorKey(root, …)` begins with it.
 * @param {string} root the root's key
 */
export function docsPrefix(root) {
  return `${root}|`;
}
