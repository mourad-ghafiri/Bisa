/**
 * What a drag carries (ide/03): one closed union of typed payloads, built here
 * and read here, so no surface invents a MIME string of its own and a drop
 * target can ask "is this a path?" without parsing anything. Every payload
 * has a `label` — the words the drag ghost shows.
 *
 * Plain `.mjs` so `node --test` can pin the shapes; the components only
 * build and match.
 */

import { t as tr } from "../../i18n/l10n.mjs";

function basename(path) {
  const s = String(path ?? "");
  const at = s.lastIndexOf("/");
  return at === -1 ? s : s.slice(at + 1);
}

/**
 * A file or folder of a root — or several, when a selection moves together —
 * within its tree or onto a pane that takes files. `path` is the row under
 * the pointer; `paths` is everything on the move, `path` included.
 * @param {{scope: string, id: string, path: string, dir: boolean, paths?: readonly string[]}} p
 */
export function pathDrag({ scope, id, path, dir, paths }) {
  const all = [...new Set([path, ...(paths ?? [])])];
  const label = all.length > 1 ? tr("ui-drag-data-items", { all: all.length }) : basename(path) || path;
  return { type: "path", scope, id, path, dir: Boolean(dir), paths: all, label };
}

/**
 * A diff hunk, for the agent pane's context tray.
 * @param {{path: string, staged: boolean, text: string, id: string}} p
 */
export function hunkDrag({ path, staged, text, id }) {
  return { type: "hunk", path, staged: Boolean(staged), text, id, label: tr("ui-drag-data-hunk", { basename: basename(path) }) };
}

/**
 * A document tab, along its strip or to another pane.
 * @param {string} id the tab id
 * @param {string} pane the leaf it sits in
 * @param {string} label
 */
export function docTabDrag(id, pane, label) {
  return { type: "doc-tab", id, pane, label };
}

/**
 * A terminal tab, along its strip or to another pane.
 * @param {string} key the mount key
 * @param {string} pane the leaf it sits in
 * @param {string} label
 */
export function terminalTabDrag(key, pane, label) {
  return { type: "terminal-tab", key, pane, label };
}

/**
 * A row of the project rail. `ctx` scopes its siblings — the project a
 * workstream belongs to, the workstream a shell stands in, `""` for a group.
 * @param {string} kind
 * @param {string} id
 * @param {string} ctx
 * @param {string} label
 */
export function railRowDrag(kind, id, ctx, label) {
  return { type: "rail-row", kind, id, ctx, label };
}

/**
 * A destination of the sidebar's primary nav, along its list — the expanded
 * rows or the collapsed rail — to a new place in the person's order.
 * `glyph` is the `ICON` name the destination wears, so the ghost can show it.
 * @param {string} key the destination's key
 * @param {string} label
 * @param {string} glyph
 */
export function navRowDrag(key, label, glyph) {
  return { type: "nav-row", key, label, glyph };
}

/**
 * A workstream's card on the Board (ide/16), from the column it sits in to
 * another, or along its own column.
 * @param {string} id the workstream id
 * @param {string} column the column it is leaving
 * @param {string} label
 */
export function workstreamCardDrag(id, column, label) {
  return { type: "workstream-card", id, column, label };
}

/** The payload's type, or null for something that is not a drag payload. */
export function dragType(data) {
  return data && typeof data === "object" && typeof data.type === "string" ? data.type : null;
}

/**
 * Whether a payload is one of the given types.
 * @param {unknown} data
 * @param {...string} types
 */
export function isDragOf(data, ...types) {
  const t = dragType(data);
  return t !== null && types.includes(t);
}

/** The rail's row kinds, by the glyph the ghost wears for each. */
const RAIL_GLYPH = Object.freeze({ group: "folder", project: "project", workstream: "workstream", terminal: "shell" });

/**
 * The glyph the drag ghost wears — an `ICON` name, never a component, so the
 * fact is pinned without a DOM — or null for a payload with no picture.
 * @param {unknown} data
 */
export function dragGlyph(data) {
  switch (dragType(data)) {
    case "rail-row":
      return RAIL_GLYPH[data.kind] ?? null;
    case "path":
      return data.dir ? "folder" : "file";
    case "doc-tab":
    case "hunk":
      return "document";
    case "terminal-tab":
      return "shell";
    case "workstream-card":
      return "workstream";
    case "nav-row":
      return typeof data.glyph === "string" ? data.glyph : null;
    default:
      return null;
  }
}

/** How many things a drag moves: a selection of paths counts, everything else is one. */
export function dragCount(data) {
  return dragType(data) === "path" && Array.isArray(data.paths) ? Math.max(1, data.paths.length) : 1;
}
