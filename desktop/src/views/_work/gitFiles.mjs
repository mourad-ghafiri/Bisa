/**
 * What git says about a working tree, as data — no React in it.
 *
 * The project git panel is a surface where a wrong grouping is a wrong
 * *fact*: a file that is staged and edited again since belongs in two lists,
 * a conflicted one belongs in neither of the usual three, and a suggestion
 * that failed must leave the box empty rather than filled with something
 * nobody wrote. None of that needs a DOM, so it lives here where
 * `node --test` can reach it, following `projectForm.mjs`.
 *
 * # Two letters, not one verdict
 *
 * `GitFileRow` carries git's `index` and `worktree` letters separately
 * because a file can be staged *and* modified since. Folding them into a
 * single badge throws away the one thing a person needs to see before
 * committing — that what they are about to commit is not what is on disk.
 * {@link standingOf} therefore says **both** of a row — the Changes tree
 * draws the file once, in its folder, wearing a chip per side — and
 * {@link groupGitFiles} counts such a row on both sides for the summary and
 * the bulk scopes rather than picking a winner.
 *
 * # Conflicted is a fourth group, not a missing one
 *
 * `is_staged`/`is_unstaged` in `bisa-vcs` are both false for an unmerged
 * path — its two letters are the sides of the conflict, not an index/worktree
 * pair. A three-way split on `staged`/`unstaged`/`untracked` therefore drops
 * conflicted files off the screen entirely, which is the worst possible
 * outcome for the one state that most needs a person's attention.
 */

import { identityBlockedReason } from "./gitIdentityModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The glyph a change wears, everywhere in the app.
 *
 * Shared with the workstream panel on purpose: a modification is the same
 * concept in a checkout and in a project folder, and two surfaces each
 * picking their own mark is how a reader ends up learning the vocabulary
 * twice.
 */
export const KIND_MARK = {
  added: "+",
  modified: "~",
  deleted: "−",
  renamed: "→",
  copied: "»",
  "type-changed": "±",
  unmerged: "!",
  /**
   * Not one of `ChangedFile.kind`'s values — a workstream diff cannot contain
   * an untracked file at all. It is here because the project panel *can* show
   * one, and `?` is already what the workstream panel writes beside its
   * separate untracked list.
   */
  untracked: "?",
};

/** git's porcelain letters, in the vocabulary {@link KIND_MARK} is keyed by. */
const LETTER_KIND = {
  M: "modified",
  A: "added",
  D: "deleted",
  R: "renamed",
  C: "copied",
  T: "type-changed",
  U: "unmerged",
  "?": "untracked",
};

/** What one of git's two letters means, or `null` for `.` — unmodified. */
export function kindForLetter(letter) {
  if (typeof letter !== "string" || letter === "" || letter === ".") return null;
  return LETTER_KIND[letter] ?? null;
}

/**
 * A file's **standing**: which sides it has. `staged` — the index holds
 * something HEAD does not; `unstaged` — the working tree differs from the
 * index; both, when both; `untracked` and `conflicted` as their own, neither
 * side (an unmerged path's letters are the sides of the conflict, not an
 * index/worktree pair). `primary` is the side a click on the row selects and
 * Space acts on: the unstaged side when the file has one — the working tree
 * is what the person is looking at — else the staged.
 * @param {{staged?: boolean, unstaged?: boolean, untracked?: boolean, conflicted?: boolean} | null | undefined} row
 * @returns {{staged: boolean, unstaged: boolean, untracked: boolean, conflicted: boolean, primary: "staged" | "unstaged"}}
 */
export function standingOf(row) {
  const conflicted = !!row?.conflicted;
  const untracked = !conflicted && !!row?.untracked;
  const staged = !conflicted && !untracked && !!row?.staged;
  const unstaged = !conflicted && !untracked && !!row?.unstaged;
  return { staged, unstaged, untracked, conflicted, primary: staged && !unstaged ? "staged" : "unstaged" };
}

/**
 * What the explorer paints: every changed path as one standing — the kind
 * from git's letters (`kindForLetter`) and whether it is the index's. A
 * file both staged and edited since reports the working tree's kind, which
 * is what the person is looking at; a conflict and an untracked file are
 * their own kinds on either side. The fold up the folders and the tones are
 * the kit's (`ui/fileStandingModel.mjs`): this is the git side of the line.
 * @param {readonly {path: string, index?: string | null, worktree?: string | null, untracked?: boolean, conflicted?: boolean} [] | null | undefined} files
 * @returns {{path: string, kind: "conflict" | "deleted" | "renamed" | "modified" | "added" | "untracked", staged: boolean}[]}
 */
export function treeStandings(files) {
  const out = [];
  for (const row of files ?? []) {
    if (typeof row?.path !== "string" || !row.path) continue;
    if (row.conflicted) {
      out.push({ path: row.path, kind: "conflict", staged: false });
      continue;
    }
    if (row.untracked) {
      out.push({ path: row.path, kind: "untracked", staged: false });
      continue;
    }
    const worktree = kindForLetter(row.worktree);
    const index = kindForLetter(row.index);
    const kind = worktree ?? index;
    if (!kind) continue;
    out.push({ path: row.path, kind: TREE_KIND[kind] ?? "modified", staged: worktree === null });
  }
  return out;
}

/** The explorer's six kinds from the porcelain's eight: a copy is a new file, a type change a modification. */
const TREE_KIND = Object.freeze({
  added: "added",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
  copied: "added",
  "type-changed": "modified",
  unmerged: "conflict",
  untracked: "untracked",
});

/** The two letters as git prints them, for the row's tooltip and state code. */
export function letterPair(row) {
  const index = typeof row?.index === "string" && row.index ? row.index : ".";
  const worktree = typeof row?.worktree === "string" && row.worktree ? row.worktree : ".";
  return `${index}${worktree}`;
}

/**
 * `GET /projects/{pid}/git/files` split into the four lists the panel
 * counts — the summary line, the commit gate, the bulk scopes. The tree
 * itself is not drawn from them (`gitTreeModel.mjs` draws every file once,
 * in the project's shape); a file staged and edited since is in two of
 * these because it counts on both sides.
 *
 * Sorted by path within each group: git's own order is stable enough to work
 * with and unstable enough to reorder rows under a pointer when a neighbour
 * changes, which is how somebody stages the wrong file.
 */
export function groupGitFiles(files) {
  const staged = [];
  const unstaged = [];
  const untracked = [];
  const conflicted = [];
  for (const f of Array.isArray(files) ? files : []) {
    if (!f || typeof f.path !== "string" || f.path === "") continue;
    if (f.conflicted) {
      conflicted.push(f);
      continue;
    }
    if (f.untracked) {
      untracked.push(f);
      continue;
    }
    // Both, when both are true. See the note at the top of the file.
    if (f.staged) staged.push(f);
    if (f.unstaged) unstaged.push(f);
  }
  const byPath = (a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  return {
    staged: staged.sort(byPath),
    unstaged: unstaged.sort(byPath),
    untracked: untracked.sort(byPath),
    conflicted: conflicted.sort(byPath),
  };
}

/** Nothing for git to report at all. */
export function isClean(groups) {
  return (
    groups.staged.length === 0 &&
    groups.unstaged.length === 0 &&
    groups.untracked.length === 0 &&
    groups.conflicted.length === 0
  );
}

/**
 * The icon actions a row reveals on hover or focus (ide/04), from its
 * standing, in order: *Stage* when the file has an unstaged, untracked or
 * unmerged side, *Unstage* when a staged one — a file staged and edited
 * since shows both — then the throw-away: *Discard changes…* for a
 * working-tree change (an unmerged path included, which a discard puts back
 * to HEAD) and *Delete file…* for a file git has never seen. A file staged
 * alone carries only *Unstage*: the index is not discarded from — unstage
 * first, as its menu says. Every action is asked about before anything
 * moves except the toggles, which are the index alone. The tooltip
 * sentences live here so the row and its tests read one text.
 * @param {ReturnType<typeof standingOf>} standing
 * @returns {{id: "stage" | "unstage" | "discard" | "delete", icon: "stage" | "unstage" | "discard" | "delete", label: (path: string) => string, tone: "quiet" | "danger", hint: string}[]}
 */
export function rowActions(standing) {
  const actions = [];
  if (standing.unstaged || standing.untracked || standing.conflicted) {
    actions.push({ id: "stage", icon: "stage", label: (path) => t("work-git-files-stage-2", { path }), tone: "quiet", hint: standing.conflicted ? t("work-git-files-put-index-resolved-ready-commit") : t("work-git-files-put-index-ready-commit") });
  }
  if (standing.staged) {
    actions.push({ id: "unstage", icon: "unstage", label: (path) => t("work-git-files-unstage-2", { path }), tone: "quiet", hint: t("work-git-files-take-back-out-index-file-itself") });
  }
  if (standing.unstaged || standing.conflicted) {
    actions.push({
      id: "discard",
      icon: "discard",
      label: (path) => t("work-git-files-discard-changes", { path }),
      tone: "danger",
      hint: standing.conflicted ? t("work-git-files-discard-changes-back-head-recovery-ref") : t("work-git-files-discard-changes-back-index-recovery-ref"),
    });
  }
  if (standing.untracked) {
    actions.push({
      id: "delete",
      icon: "delete",
      label: (path) => t("work-git-files-delete", { path }),
      tone: "danger",
      hint: t("work-git-files-delete-file-git-has-never-seen"),
    });
  }
  return actions;
}

/**
 * What every bulk verb takes, from one place: **all** — everything git has
 * not got yet, the modified tracked files and the untracked ones together;
 * **tracked** — the modified tracked files alone, `git add -u`'s meaning;
 * **untracked** — the files git has never seen; **staged** — what *Unstage
 * all* takes back; **discardable** — what *Discard all changes…* puts back,
 * the working-tree changes and the unmerged paths. A conflicted path is in
 * no stage scope: staging it would mark it resolved. A file in both Staged
 * and Unstaged counts once on each side.
 * @param {{staged: readonly {path: string}[], unstaged: readonly {path: string}[], untracked: readonly {path: string}[], conflicted: readonly {path: string}[]} | null | undefined} groups
 * @returns {{all: {paths: string[], count: number}, tracked: {paths: string[], count: number}, untracked: {paths: string[], count: number}, staged: {paths: string[], count: number}, discardable: {paths: string[], count: number}}}
 */
export function stageScopes(groups) {
  const paths = (g) => (groups?.[g] ?? []).map((r) => r.path);
  const tracked = paths("unstaged");
  const untracked = paths("untracked");
  const scope = (list) => {
    const unique = [...new Set(list)];
    return { paths: unique, count: unique.length };
  };
  return {
    all: scope([...tracked, ...untracked]),
    tracked: scope(tracked),
    untracked: scope(untracked),
    staged: scope(paths("staged")),
    discardable: scope([...tracked, ...paths("conflicted")]),
  };
}

/**
 * A changed file's context menu, from its standing: open it as a document,
 * stage and/or unstage it, throw its change away or delete it, its paths,
 * and where it is in Files. Each item names the keymap command that means
 * the same thing, when one does.
 *
 * **Discard** is offered for a working-tree side only — unstaged or unmerged
 * — because that is what it does: the working tree goes back to the index,
 * and the index is not touched (a staged change is kept; unstage first). An
 * untracked file has nothing in the index to go back to, so its verb is
 * **Delete**, through the IDE's disposal — the Trash when the root says so —
 * never through git. Stashing is the Stashes view's, whole tree or *Only
 * these files* (ide/04 §Stash), so no row offers it.
 * @param {{standing: ReturnType<typeof standingOf>, desktop: boolean, canOpen: boolean}} ctx
 * @returns {{id: string, label: string, command?: string, separatorBefore?: boolean, disabled?: boolean, danger?: boolean}[]}
 */
export function gitFileMenu({ standing, desktop, canOpen }) {
  const items = [
    { id: "open", label: t("work-git-files-open-file"), disabled: !canOpen },
    // The change reaches the agent only through a door like this one (ide/09).
    { id: "attach-agent", label: t("work-git-files-attach-agent") },
  ];
  if (standing.unstaged || standing.untracked || standing.conflicted) items.push({ id: "stage", label: t("work-git-files-stage"), separatorBefore: true });
  if (standing.staged) items.push({ id: "unstage", label: t("work-git-files-unstage"), separatorBefore: !items.some((i) => i.id === "stage") });
  if (standing.untracked) items.push({ id: "delete", label: t("work-git-files-delete-file"), separatorBefore: true, danger: true });
  else if (standing.unstaged || standing.conflicted) items.push({ id: "discard", label: t("work-git-files-discard-changes-2"), separatorBefore: true, danger: true });
  items.push({ id: "copy-path", label: t("work-git-files-copy-relative-path"), separatorBefore: true });
  if (desktop) items.push({ id: "copy-absolute", label: t("work-git-files-copy-absolute-path") });
  items.push({ id: "reveal-files", label: t("work-git-files-reveal-files"), command: "reveal_in_files", disabled: !canOpen });
  return items;
}

/**
 * A selection is a path *and which side of it* is being read, because those
 * are two different patches for the same file: `staged: true` is the index
 * against HEAD, `false` is the working tree against the index. It is exactly
 * the query `GET /projects/{pid}/git/diff` takes, so the panel never has to
 * translate between what is highlighted and what it asked for.
 * @param {"staged" | "unstaged"} side
 */
export function selectionOf(row, side) {
  if (!row || typeof row.path !== "string") return null;
  return { path: row.path, staged: side === "staged" };
}

/** Whether this row's `side` is the patch open — the file's row lights on either, the side's chip on its own. */
export function isSelected(selection, row, side) {
  return (
    selection !== null &&
    selection !== undefined &&
    !!row &&
    selection.path === row.path &&
    selection.staged === (side === "staged")
  );
}

/**
 * The selection after the file list changed under it.
 *
 * Staging a file empties its worktree diff and fills its index one, so the
 * pane a person is reading would otherwise go blank the moment they staged
 * what they were looking at. Following the file to the side that still has
 * content keeps the patch on screen; a path that has left the list entirely
 * (committed, or reverted) drops the selection rather than leaving a
 * highlight pointing at nothing.
 */
export function keepSelection(selection, files) {
  if (!selection) return null;
  const row = (Array.isArray(files) ? files : []).find((f) => f && f.path === selection.path);
  if (!row) return null;
  if (selection.staged) {
    if (row.staged) return selection;
    return row.unstaged || row.untracked ? { path: row.path, staged: false } : null;
  }
  if (row.unstaged || row.untracked || row.conflicted) return selection;
  return row.staged ? { path: row.path, staged: true } : null;
}

/**
 * Why Commit is off, or `null` when it is on. The reason is the tooltip.
 *
 * **Staged, not "everything".** `POST /projects/{pid}/git/commit` treats an
 * empty `paths` as "commit what is already staged" and never as "commit
 * everything", because a project's folder is the user's own working tree and
 * may hold edits that have nothing to do with this commit. The panel sends no
 * paths, so this predicate asks about the index rather than about the screen.
 *
 * **Amend** (ide/04 §Amend) rewrites the last commit: it needs one (`head`),
 * and nothing staged is fine — a reword — while who commits, an unmerged
 * index and a blank message block it as they block a commit.
 */
export function commitBlockedReason({ message, groups, exists, git, identity, head = null, amend = false }) {
  if (exists === false) return t("work-project-detail-folder-not-disk");
  if (git === false) return t("work-git-files-plain-folder-there-no-repository-commit");
  // Who commits comes before what: git refuses the commit outright with no
  // identity, and the fix is a different control than staging a file.
  const nobody = identityBlockedReason(identity);
  if (nobody) return nobody;
  if (amend && !head) return t("work-git-files-no-commit-amend-yet");
  if (groups && groups.conflicted.length > 0)
    return t("work-git-files-resolve-conflicted-files-first-git-will");
  if (!amend && (!groups || groups.staged.length === 0))
    return t("work-git-files-nothing-staged-stage-file-commit-records");
  if (typeof message !== "string" || message.trim() === "") return t("work-git-files-commit-needs-message");
  return null;
}

/**
 * What `POST /projects/{pid}/git/message` left you with.
 *
 * The route always answers 200, and `suggested: false` with an `error` is the
 * no-harness / timeout case. `message: null` here means **do not touch the
 * box**: the one thing this must never do is put words in a commit that no
 * agent wrote, and a caller that treated a failure as an empty string would
 * silently clear whatever the person had already typed.
 */
export function suggestionOutcome(response) {
  if (!response || response.suggested !== true) {
    const why = typeof response?.error === "string" ? response.error.trim() : "";
    return {
      message: null,
      note: why
        ? t("work-git-files-no-message-suggested", { why })
        : t("work-git-files-no-message-suggested-node-did-not"),
    };
  }
  const message = typeof response.message === "string" ? response.message.trim() : "";
  if (message === "") {
    return {
      message: null,
      note: t("work-git-files-agent-answered-empty-message-write-one"),
    };
  }
  return { message, note: null };
}

/**
 * `+n −m` for one patch, counted from the patch itself.
 *
 * `GitFileRow` carries no line counts — unlike `ChangedFile`, which a workstream
 * diff assembles from `git diff --numstat`. Rather than one request per row to
 * manufacture them, the panel shows them for the file actually open, where the
 * patch is already in hand.
 */
export function diffStat(patch) {
  let insertions = 0;
  let deletions = 0;
  for (const line of (typeof patch === "string" ? patch : "").split("\n")) {
    // `+++`/`---` are the file headers, not content.
    if (line.startsWith("+") && !line.startsWith("+++")) insertions += 1;
    else if (line.startsWith("-") && !line.startsWith("---")) deletions += 1;
  }
  return { insertions, deletions };
}

/**
 * A repository nobody has committed to yet.
 *
 * `branch` is *named* on an unborn HEAD — `git status --porcelain=v2` reports
 * `# branch.oid (initial)` beside a real `# branch.head main` — so the branch
 * name says nothing about whether there is a history. The absent `head` is
 * what says it.
 *
 * This is the ordinary state of a fresh managed project: creating one runs
 * `git init`, and until something commits, work items on it run **in the
 * primary workstream** — the root itself — because there is no commit to
 * branch a worktree from.
 */
export function noCommitsYet(status) {
  return !!status && status.git === true && status.detached !== true && !status.head;
}
