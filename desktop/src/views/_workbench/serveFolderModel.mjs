/**
 * *From folder…* (ide/18): what the Browser button's picker offers, in what
 * order, and the words it says — no React and no DOM, so every rule is a
 * test's to reach.
 *
 * The picker is the checkout's own tree with the files left out: **Root**
 * first — the checkout whole — then every folder under it, listed the way
 * the explorer lists them (`ui/fileTreeModel.mjs`, so an ignored `dist/` is
 * there to be served). A filter finds a folder by a few letters of its path
 * over the folders the path index knows and the ones already listed; a path
 * typed in full that matches nothing is still offered as typed, so a folder
 * a build has not made yet can be named.
 *
 * A folder is its path relative to the checkout, `/`-separated, no leading
 * or trailing slash; the checkout itself is {@link ROOT_FOLDER}, the empty
 * string — what the node reads an absent `folder` as.
 */

import { visibleRows } from "../../ui/fileTreeModel.mjs";
import { rankPaths } from "../../shell/quickOpenScore.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The checkout itself. */
export const ROOT_FOLDER = "";
/** Root's row id: a folder's id is its path, and the root's path is empty. */
export const ROOT_ID = "::root";
/** The row that offers a path as typed. */
export const TYPED_ID = "::typed";
/** How many folders a filter lists. */
export const FILTER_LIMIT = 50;
/** Where the folder last served in each checkout is kept, and for how many checkouts. */
export const MEMORY_KEY = "bisa.ide.serve.folder";
export const MEMORY_CAP = 50;

/** A row's id for a folder. @param {string} folder */
export function idOfFolder(folder) {
  return folder === ROOT_FOLDER ? ROOT_ID : folder;
}

/**
 * The refusal a folder that cannot be served reads as — the node's own rule,
 * said before the node has to: relative to the checkout, never above it. The
 * empty path is no problem: it is the root.
 * @param {string} folder
 */
export function folderProblem(folder) {
  const f = String(folder ?? "").trim();
  if (f.startsWith("/") || /^[A-Za-z]:[\\/]/.test(f) || f.split(/[\\/]/).some((seg) => seg === "..")) return t("workbench-serve-folder-folder-checkout-relative-never-above");
  return null;
}

/** A path as a person types one, as a folder: `./docs/build/` is `docs/build`. @param {string} typed */
export function normalizeFolder(typed) {
  return String(typed ?? "")
    .trim()
    .replace(/\\/g, "/")
    .replace(/^(\.\/)+/, "")
    .replace(/\/+$/, "")
    .replace(/^\.$/, "");
}

/**
 * Every folder the file paths name, each once, in path order — `a/b/c.txt`
 * names `a` and `a/b`.
 * @param {readonly string[]} paths
 */
export function foldersOf(paths) {
  const seen = new Set();
  for (const path of paths ?? []) {
    let at = path.lastIndexOf("/");
    while (at > 0) {
      const folder = path.slice(0, at);
      if (seen.has(folder)) break;
      seen.add(folder);
      at = folder.lastIndexOf("/");
    }
  }
  return [...seen].sort((a, b) => a.localeCompare(b));
}

/**
 * The folders that hold an `index.html` — what the server answers a
 * directory with, so the ones a page is most likely to open from.
 * @param {readonly string[]} paths @returns {Set<string>}
 */
export function indexFolders(paths) {
  const out = new Set();
  for (const path of paths ?? []) {
    if (path === "index.html") out.add(ROOT_FOLDER);
    else if (path.endsWith("/index.html")) out.add(path.slice(0, -"/index.html".length));
  }
  return out;
}

/**
 * The server already up on a folder of this checkout, or null.
 * @param {readonly {owner: {kind: string, folder?: string}, port: number}[]} servers this checkout's
 * @param {string} folder
 */
export function serverFor(servers, folder) {
  return (servers ?? []).find((s) => s.owner?.kind === "workstream" && (s.owner.folder ?? ROOT_FOLDER) === folder) ?? null;
}

/**
 * The tree's rows: Root, then the folders under it as the listing has them —
 * files left out, a folder the listing shows to hold no folder drawn without
 * a chevron, a listing still in flight or failed said in place.
 * @param {object} state a `fileTreeModel` state
 * @param {string} rootLabel the project's name
 */
export function folderRows(state, rootLabel) {
  const rows = [{ kind: "root", id: ROOT_ID, folder: ROOT_FOLDER, depth: 0, label: rootLabel, expandable: false, expanded: true, ignored: false }];
  for (const row of visibleRows(state)) {
    if (row.kind === "entry") {
      if (!row.entry.dir) continue;
      const listed = state.dirs[row.path];
      const childless = listed?.status === "ready" && !listed.entries.some((e) => e.dir);
      rows.push({ kind: "folder", id: row.path, folder: row.path, depth: row.depth + 1, label: row.entry.name, expandable: !childless, expanded: row.expanded && !childless, ignored: Boolean(row.entry.ignored) });
    } else if (row.kind === "loading" || row.kind === "error") {
      rows.push({ kind: row.kind, id: row.id, folder: null, depth: row.depth + 1, label: "", expandable: false, expanded: false, ignored: false, focusable: false, error: row.error ?? null });
    }
  }
  return rows;
}

/** Every folder the listing has shown so far. @param {object} state */
export function listedFolders(state) {
  const out = [];
  for (const dir of Object.values(state?.dirs ?? {})) for (const e of dir.entries ?? []) if (e.dir) out.push(e.path);
  return out;
}

/**
 * The filter's rows, best first: the folders whose path the letters run
 * through, Root when its name or *root* matches, and last the path as typed
 * when it names a folder none of them is.
 * @param {string} query
 * @param {readonly string[]} folders every folder known — the index's and the listing's
 * @param {string} rootLabel
 */
export function filterRows(query, folders, rootLabel) {
  const q = String(query ?? "").trim();
  if (!q) return [];
  const rows = [];
  const needle = q.toLowerCase();
  if ("root".startsWith(needle) || rootLabel.toLowerCase().includes(needle)) {
    rows.push({ kind: "root", id: ROOT_ID, folder: ROOT_FOLDER, depth: 0, label: rootLabel, expandable: false, expanded: false, ignored: false });
  }
  const unique = [...new Set(folders ?? [])];
  for (const hit of rankPaths(normalizeFolder(q) || q, unique, FILTER_LIMIT)) {
    rows.push({ kind: "match", id: hit.path, folder: hit.path, depth: 0, label: hit.path, expandable: false, expanded: false, ignored: false });
  }
  const typed = normalizeFolder(q);
  if (typed && !folderProblem(typed) && !unique.includes(typed)) {
    rows.push({ kind: "typed", id: TYPED_ID, folder: typed, depth: 0, label: typed, expandable: false, expanded: false, ignored: false });
  }
  return rows;
}

/** What the filter says when it lists nothing. @param {string} query */
export function filterProblem(query) {
  return folderProblem(normalizeFolder(query)) ?? t("workbench-serve-folder-no-folder-checkout-matches");
}

/**
 * The row after or before the cursor among the rows a cursor can stand on —
 * the filter field's arrows, which leave Left and Right to the text.
 * @param {readonly {id: string, focusable?: boolean}[]} rows
 * @param {string | null} cursor
 * @param {1 | -1} step
 */
export function stepCursor(rows, cursor, step) {
  const ids = (rows ?? []).filter((r) => r.focusable !== false).map((r) => r.id);
  if (ids.length === 0) return null;
  const at = ids.indexOf(cursor);
  if (at === -1) return step === 1 ? ids[0] : ids[ids.length - 1];
  return ids[Math.min(ids.length - 1, Math.max(0, at + step))];
}

/** The folders to open so `folder` shows: its ancestors, outermost first. @param {string} folder */
export function ancestorsOf(folder) {
  const parts = String(folder ?? "").split("/").filter(Boolean);
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** The button: *Open :4173* for a folder already served, else *Serve and open*. @param {{port: number} | null} server */
export function submitWords(server) {
  return server ? t("workbench-serve-folder-open", { port: server.port }) : t("workbench-serve-folder-serve-open");
}

/** The line under the list: what the choice does. @param {string} folder @param {{port: number} | null} server */
export function choiceWords(folder, server) {
  const what = folder === ROOT_FOLDER ? t("workbench-serve-folder-whole-checkout") : `\`${folder}/\``;
  if (server) return t("workbench-serve-folder-already-serving-opens-browser-tab", { what, port: server.port });
  return t("workbench-serve-folder-serves-on-port", { what });
}

/**
 * The remembered folders as kept: checkout id → folder, newest last. Anything
 * that is not that shape reads as nothing remembered.
 * @param {string | null} raw
 * @returns {Record<string, string>}
 */
export function parseRemembered(raw) {
  try {
    const value = JSON.parse(raw ?? "null");
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const out = {};
    for (const [wid, folder] of Object.entries(value)) {
      if (typeof folder === "string" && !folderProblem(folder)) out[wid] = normalizeFolder(folder);
    }
    return out;
  } catch {
    return {};
  }
}

/** The folder last served in a checkout; the root when none was. */
export function rememberedFolder(byCheckout, wid) {
  return byCheckout?.[wid] ?? ROOT_FOLDER;
}

/** `byCheckout` with `wid`'s folder set and moved to the newest end, the oldest dropped past `cap`. */
export function rememberFolder(byCheckout, wid, folder, cap = MEMORY_CAP) {
  const next = { ...(byCheckout ?? {}) };
  delete next[wid];
  next[wid] = normalizeFolder(folder);
  const keys = Object.keys(next);
  for (const key of keys.slice(0, Math.max(0, keys.length - cap))) delete next[key];
  return next;
}
