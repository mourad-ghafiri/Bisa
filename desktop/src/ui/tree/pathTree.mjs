/**
 * A flat list of paths as the tree a person recognises — the project's own
 * folders, nested as they are on disk, in the explorer's order (ide/03
 * §The tree): directories before files at every level, `sortEntries`' rule,
 * no compaction. The one builder every tree drawn from paths uses; Git ›
 * Changes is its first caller (ide/04).
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { sortEntries } from "../fileTreeModel.mjs";

/**
 * @template T
 * @typedef {{name: string, path: string, dirs: Map<string, PathNode<T>>, files: T[]}} PathNode
 */

function basename(path) {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
}

/** @template T @param {string} name @param {string} path @returns {PathNode<T>} */
function node(name, path) {
  return { name, path, dirs: new Map(), files: [] };
}

/**
 * The tree of `items`, each placed under the folders its path names. The
 * root is a nameless node whose folders and files are the top level.
 * @template T
 * @param {readonly T[]} items
 * @param {(item: T) => string} pathOf
 * @returns {PathNode<T>}
 */
export function pathTree(items, pathOf) {
  const root = node("", "");
  for (const item of items) {
    const path = pathOf(item);
    if (typeof path !== "string" || path === "") continue;
    const parts = path.split("/");
    let at = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const name = parts[i];
      const dirPath = parts.slice(0, i + 1).join("/");
      let next = at.dirs.get(name);
      if (!next) {
        next = node(name, dirPath);
        at.dirs.set(name, next);
      }
      at = next;
    }
    at.files.push(item);
  }
  return root;
}

/**
 * A node's children in the order the tree draws them — the explorer's:
 * folders first, then files, each by name.
 * @template T
 * @param {PathNode<T>} dir
 * @param {(item: T) => string} pathOf
 * @returns {({kind: "dir", node: PathNode<T>} | {kind: "file", item: T})[]}
 */
export function orderedChildren(dir, pathOf) {
  const entries = [
    ...[...dir.dirs.values()].map((n) => ({ name: n.name, path: n.path, dir: true, node: n })),
    ...dir.files.map((item) => ({ name: basename(pathOf(item)), path: pathOf(item), dir: false, item })),
  ];
  return sortEntries(entries).map((e) => (e.dir ? { kind: "dir", node: e.node } : { kind: "file", item: e.item }));
}

/**
 * Every item under a folder, in the tree's order — what a folder's verb
 * sends: the files it lists, never the folder as a pathspec.
 * @template T
 * @param {PathNode<T>} dir
 * @param {(item: T) => string} pathOf
 * @returns {T[]}
 */
export function filesUnder(dir, pathOf) {
  const out = [];
  const walk = (d) => {
    for (const child of orderedChildren(d, pathOf)) {
      if (child.kind === "dir") walk(child.node);
      else out.push(child.item);
    }
  };
  walk(dir);
  return out;
}
