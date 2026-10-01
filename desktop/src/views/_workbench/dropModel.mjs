/**
 * A file dropped from this machine onto the Project IDE (ide/03 §Loose files),
 * as facts: whether a drag is a file drag, which absolute paths a drop
 * carried, and which workbench root a loose file opens in.
 *
 * The window takes HTML5 drops (`dragDropEnabled: false`), so the DOM's
 * `File`s have names and bytes and no path; the shell reads the drag
 * pasteboard for the paths. The two lists meet here, by name.
 */

/** Whether a drag carries files — the DOM's `dataTransfer.types` names `Files`. */
export function isFileDrop(types) {
  return Array.from(types ?? []).includes("Files");
}

function basename(path) {
  const s = String(path ?? "");
  const at = s.lastIndexOf("/");
  return at === -1 ? s : s.slice(at + 1);
}

/**
 * The absolute paths the drop carried, in the DOM's order: each dropped
 * name paired with the pasteboard path of that name, once. A name the
 * pasteboard does not know is left out; a drop that pairs nothing is
 * `[]` — the workbench then points at *Open file…* rather than guessing.
 * @param {readonly string[]} names the DOM's file names
 * @param {readonly string[]} paths the shell's absolute paths
 * @returns {string[]}
 */
export function pathsForDrop(names, paths) {
  const pool = new Map();
  for (const p of paths ?? []) {
    if (typeof p !== "string" || !p.startsWith("/")) continue;
    const key = basename(p);
    const list = pool.get(key) ?? [];
    list.push(p);
    pool.set(key, list);
  }
  const out = [];
  for (const name of names ?? []) {
    const list = pool.get(String(name));
    const path = list?.shift();
    if (path) out.push(path);
  }
  return out;
}

/**
 * The root a loose file opens in: the workbench on screen, else the root the
 * person used most recently (the store lists them newest first), else none —
 * a loose file is a document, and a document needs a workbench to sit in.
 * @param {{name: string, scope?: string, id?: string} | null | undefined} route
 * @param {readonly string[]} roots root keys, most recently used first
 * @returns {string | null} a root key (`<scope>:<id>`)
 */
export function looseTargetRoot(route, roots) {
  if (route && route.name === "workbench" && route.scope && route.id) return `${route.scope}:${route.id}`;
  return (roots ?? []).find((r) => typeof r === "string" && r.includes(":")) ?? null;
}
