/**
 * What a `file_changed` frame means to git — no React (ide/04 §Live). The
 * watcher reports the working tree and a closed list of `.git` paths
 * (`ide/watch.rs`: `HEAD`, `index`, `packed-refs`, `FETCH_HEAD`, the
 * operation markers, `refs/`, `rebase-merge/`, `rebase-apply/`,
 * `sequencer/`); this file says which kind of change each is, and which of
 * the Git tab's views re-read on which kinds, so every view listens through
 * one rule and a fetch that only moved refs never re-lists the tree.
 */

/** The kinds of change, in no particular order. */
export const GIT_CHANGES = Object.freeze(["worktree", "index", "head", "refs", "stash", "operation"]);

/** How long a burst of frames is gathered before one re-read per view. */
export const GIT_COALESCE_MS = 300;

/**
 * The frames after which every root somebody shows is read again:
 * something may have written into a tree with no `file_changed` of its
 * own — a run's end, a result taken in, a commit the node made.
 */
export const WRITES_TREE = Object.freeze(["execution_ended", "result_accepted", "workstream_committed"]);

/** How many roots nobody shows keep their last rows for the return; the ones left longest ago past it forget theirs. */
export const MAX_IDLE_ROOTS = 8;

/**
 * The roots to let go of: among those nobody shows and nothing is reading
 * or about to (`inFlight`, a `flush` pending), the ones left longest ago
 * past `max` — by `leftAt`, the count at which each was last left, so a
 * root visited first, left, shown again and left last is the last to go,
 * never the first because it was read first. A root never shown counts as
 * left before every one that was.
 * @param {Iterable<readonly [string, {subscribers: number, inFlight: boolean, flush: unknown, leftAt: number}]>} entries
 * @param {number} max
 * @returns {string[]}
 */
export function idleRootsToDrop(entries, max) {
  const idle = [...entries].filter(([, e]) => e.subscribers === 0 && !e.inFlight && e.flush === null).sort((a, b) => a[1].leftAt - b[1].leftAt);
  return idle.slice(0, Math.max(0, idle.length - Math.max(0, max))).map(([scope]) => scope);
}

/**
 * What a frame re-reads of the changed files: every root shown, when the
 * frame means something wrote into a tree with no frame of its own
 * (`WRITES_TREE`); one workstream's rows, coalesced, when a `file_changed`
 * of its touched the tree or the index (`readsFor`); nothing otherwise — a
 * fetch that only moved refs re-lists nothing, and a goal's folder has no
 * git rows.
 * @param {{type?: string, scope?: string, id?: string, path?: string, kind?: string} | null | undefined} frame
 * @returns {{kind: "shown"} | {kind: "root", id: string} | {kind: "none"}}
 */
export function rereadsFor(frame) {
  const type = frame?.type;
  if (typeof type !== "string") return { kind: "none" };
  if (WRITES_TREE.includes(type)) return { kind: "shown" };
  if (type !== "file_changed" || frame.scope !== "workstream" || typeof frame.id !== "string" || frame.id === "") return { kind: "none" };
  if (!readsFor(kindsOf({ path: frame.path ?? "", kind: frame.kind ?? "" })).files) return { kind: "none" };
  return { kind: "root", id: frame.id };
}

const OPERATION_FILES = new Set(["ORIG_HEAD", "MERGE_HEAD", "REBASE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"]);
const OPERATION_DIRS = new Set(["rebase-merge", "rebase-apply", "sequencer"]);
const REF_FILES = new Set(["packed-refs", "FETCH_HEAD"]);

/**
 * The kind of change a root-relative path is — `null` for a `.git` path the
 * panel has nothing to show for.
 * @param {string} path
 * @returns {"worktree" | "index" | "head" | "refs" | "stash" | "operation" | null}
 */
export function gitChangeOf(path) {
  const parts = String(path ?? "").split("/").filter((p) => p.length > 0);
  if (parts[0] !== ".git") return "worktree";
  const [, first, second] = parts;
  if (first === undefined) return null;
  if (first === "index") return "index";
  if (first === "HEAD") return "head";
  if (first === "refs") return second === "stash" ? "stash" : "refs";
  if (REF_FILES.has(first)) return "refs";
  if (OPERATION_FILES.has(first) || OPERATION_DIRS.has(first)) return "operation";
  return null;
}

/** The kinds a frame carries: one for a path, every kind for a `rescan`. */
export function kindsOf(frame) {
  if (frame.kind === "rescan") return new Set(GIT_CHANGES);
  const kind = gitChangeOf(frame.path);
  return kind === null ? new Set() : new Set([kind]);
}

/**
 * Which views re-read for a set of kinds: the status on anything (the
 * branch, ahead/behind, the operation in progress); the files on what
 * touches the tree or the index; the history on a commit or a ref moving;
 * the branches on a ref or HEAD; the stashes on the stash ref.
 * @param {Iterable<string>} kinds
 */
export function readsFor(kinds) {
  const has = new Set(kinds);
  const any = (...ks) => ks.some((k) => has.has(k));
  return {
    status: has.size > 0,
    files: any("worktree", "index", "head", "operation", "stash"),
    history: any("head", "refs", "operation"),
    branches: any("refs", "head", "operation"),
    stashes: any("stash"),
  };
}
