/**
 * Types for `pathTree.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step.
 */

/** A folder of the tree: its name, its full path, its folders and the items in it. */
export interface PathNode<T> {
  name: string;
  path: string;
  dirs: Map<string, PathNode<T>>;
  files: T[];
}

export type PathChild<T> = { kind: "dir"; node: PathNode<T> } | { kind: "file"; item: T };

export declare function pathTree<T>(items: readonly T[], pathOf: (item: T) => string): PathNode<T>;
/** The explorer's order: folders first, then files, each by name. */
export declare function orderedChildren<T>(dir: PathNode<T>, pathOf: (item: T) => string): PathChild<T>[];
/** Every item under a folder, in the tree's order. */
export declare function filesUnder<T>(dir: PathNode<T>, pathOf: (item: T) => string): T[];
