# 05 — The commit graph

No maintained Rust *library* solves graph layout — `git-graph` is a command-line tool — so the
layout is built here. The algorithm is
small and well understood; this document is it. Rows are cached in memory per process
(`GraphRegistry`, at most `LAID_ROOTS` roots at once) and fingerprint-checked per request; there is
no on-disk row cache ([feature status](../../feature-status.md)).

---

## Input

`git log --topo-order --date-order --parents --decorate=full --format=<US-separated>` over the
requested refs (all local branches and tags by default; a selectable set). `parse.rs` yields:

```rust
pub struct GraphCommit {
    pub id: CommitId,
    pub parents: Vec<CommitId>,       // first parent first
    pub refs: Vec<RefName>,           // branches, tags, HEAD
    pub author: String, pub email: String, pub at: u64,
    pub subject: String,
}
```

---

## Lane assignment — one pass, streaming

```
lanes: Vec<Option<CommitId>>          # lane i is reserved for the commit expected there next

for C in stream:
    claimed = indices where lanes[i] == Some(C)
    my_lane = claimed.first() or first_free(lanes)
    for other in claimed[1..]:        # several children converge on C
        emit Edge { from: other, to: my_lane, kind: Merge }
        lanes[other] = None
    lanes[my_lane] = C.parents.first()            # the first parent continues the lane
    for p in C.parents[1..]:
        lane = index where lanes[i] == Some(p) or first_free(lanes)
        lanes[lane] = Some(p)
        emit Edge { from: my_lane, to: lane, kind: Fork }
    emit Row { commit: C, lane: my_lane, active: lanes.count_some(), edges }
```

Properties, each with a test:

- **total** — every commit gets exactly one row and one lane;
- **every edge terminates on a real row** (property test over random DAGs, including octopus
  merges, which are the case a hand-written layout gets wrong);
- **first-parent continuity** — following first parents never changes lane, which is what makes a
  branch read as a line;
- **bounded width** — the active-lane count equals the number of concurrent open branches at that
  row, never more.

Lane colour is `lane_index % palette.len()` over a palette drawn from the theme's token roles, so it
tracks the user's accent and the contrast rules the themes already enforce.

---

## Why it does not collapse on a big repository

Layout is stateful from the top, so it cannot be computed for a scroll window alone. Instead:

1. `engine::ide::graph::layout` runs over the whole log in a background task; the first request on a
   root lays out the **first 1,000 rows inline** (`FIRST_SCREEN`) so the first screen paints while
   the tail is computed, and the header says *laying out … N so far* until `done`.
2. The rows are held **in memory per root** in `GraphRegistry` (`Laid { rows, done, fingerprint }`),
   fingerprint-checked against the refs on every request. A moved ref on a log that fits the first
   screen is laid out again inline — the fresh rows answer the request; a bigger one relays in the
   background while the stale rows stay on screen (`stale: true`) — a graph that blanks on every
   commit is a graph nobody leaves open. There is no on-disk row cache
   ([feature status](../../feature-status.md)).
3. `GET /ide/graph/{scope}/{id}?from=&count=&refs=all|head` returns one window (400 by default,
   2,000 at most). `refs` is the one filter: `all` — every ref of the person's, the default — or
   `head` — only what the current branch reaches (`RefScope`, `bisa-vcs`'s `graph_log` runs
   `--exclude=refs/bisa/* --all` or `HEAD`); anything else is 400. **The platform's own refs are
   never the history's**: a recovery point under `refs/bisa/safety/` ([ide/04](04-git.md)) is
   left out of the walk and out of the decorations (`parse::decorations`), so a pinned tip no
   branch names any more is not a commit of the history and a recovery ref reads as no chip —
   Safety is where they show (`vcs interactive::a_recovery_point_is_in_safety_and_nowhere_in_the_history`). The engine lays out and caches **per scope**
   (`GraphRegistry`'s key is `scope/id/refs`, each with its own fingerprint), so switching the
   filter is a second layout kept beside the first, not a relayout of it. The client keeps the
   choice per checkout in the Git session (`gitPanelStore`, `graphRefs`), so a view switch keeps it.
   The client keeps the rows **sparse**: an array of `total` slots that windows land in
   (`graphModel.emptyRows`, `mergeWindow`); `ui/VirtualList.tsx` renders the window, reports the
   range on screen (`onRange`), and the view asks for exactly the pages with a hole in them
   (`holesIn`). A jump to row 87,000 is one window away, not two hundred pages of scrolling; a
   slot no window has reached draws as a skeleton row.
4. A relayout empties the sparse rows, so the holes refill from the new layout as they scroll into
   view — never half old and half new.

**Search** is the node's, over the whole laid-out log: `GET /ide/graph/{scope}/{id}/search?q=&from=&limit=&refs=`
(the same `refs` as the window, so a search under *only the current branch* looks through that
layout alone) scans the rows in memory (`graph::matches` — subject, author, id prefix, ref name, case-insensitive;
500 indexes by default, 5,000 at most, `truncated` when the cap stopped it, `done` false while the
tail is still laying out) and answers **row indexes**. The client highlights the rows on screen by
the same rule (`graphModel.matches`, so what dims and what was found agree), and `Enter`, `n` and
`N` jump through the indexes (`nextIndex`), scrolling the list to the row and fetching its window
when it is not there yet; the header says *3 of 41 matches*, or *… in the first 1,000 rows — still
laying out* (`searchStatus`). The box is not on screen at rest: *Search…* in the header's `⋮`, or
`⌘⇧H` from anywhere in the root, opens History with a **search line** under the header — the box
at full width, the status, previous and next as glyphs, a close — and Esc or the close hides it
again and clears the query. The topology stays exactly where it was while you look through it,
which is the point of a graph over a list.

**Click to inspect** opens the commit **in the centre** as a document (`{kind: "commit", sha}`,
`CommitDocument` — a preview, the next row's click replaces it; the row stays pressed): the header
with the short id (copyable), the ref chips, author and date, *merge of n*, the **View** control
and the toolbar's actions; the message; the file list against the first parent with `+n −m` —
renames detected (`-M` on both `diff-tree` runs, so a moved file is one row, `old → new`) — as a
**selector**; and, under it, what the view chosen reads. **The three views are the patch
document's** (ide/04 §The Changes view; `patchViewModel.mjs`, the one choice for every patch under
`bisa.ide.views` as `patch` — a commit's file reads the way a changed file does): on **Hunks** the
whole patch, read-only (`ReadOnlyDiff`), until a row is chosen, and the chosen file's patch alone
after (`GET /workstreams/{wid}/git/commit/{sha}/diff?path=`; a click on the chosen row lets go); on
**Side by side** and **Inline** one file's two versions compared in the code editor
(`Comparison.tsx`, shared with the patch document) — the first parent's, at the file's old path for
a rename, nothing for a root commit or a new file, against the commit's, nothing for a deleted one
(`GET /workstreams/{wid}/git/commit/{sha}/sides?path=`) — the chosen row, else the first, else
*Nothing to compare*. What is read is `commitFocusModel.focusOf(files, chosen, view)`'s word
(`chooseFile` the click's, `emptyPatchWords` the sentence over a patch with no lines — a move
without changes says so); the hunks' tooltip says *read-only*, since the acts do not apply to
history (`patchViewHint(view, { readOnly })`). Below the desktop, one plan serves both shapes
(`bisa_engine::ide::git::commit_file_plan` — the first parent, the left side's path, the pathspecs
a rename needs both of — read by `commit_file` and `commit_file_sides` over
`bisa_vcs::git::commit_diff(repo, sha, paths)` and `blob(repo, BlobRev::Rev(id), path)`), so the
hunks and the comparison never disagree; a path the commit did not touch is a 400 — what a commit
is and what it changed are read together, and the graph beside stays the graph. The actions run through the same hook the
rows use (`useCommitActions`) with the same dialogs; the document's action context is the commit's
own — it is HEAD when it wears the `head` ref. The ref chip and the action button are one file the
row and the document share (`CommitActionControls.tsx`). The header is one line: a stable count (`commitCount` — *1,234 commits*, or
*1,234 so far* while the tail lays out), *· this branch* when the filter is on, one status word
while something happens (`layoutWord`), and a `⋮` at the right whose items are the model's
(`graphModel.historyMenu`): **Search…** (or *Hide the search*), **Show all branches and tags** /
**Show only the current branch** — the one in force wears a check (`refScopeWords`) — and, under a
rule, **Lay out again**. The view **fills its column**: the right panel hands the Git occupant its
height (`RightPanel` → `GitTab` → `GitViewBody`, each `min-h-0` and `flex-1`), the graph's root is
`h-full`, and the `VirtualList` is the one thing that scrolls — never a `max-h` in viewport units,
which capped the list at a fraction of the window and scrolled it inside the scrolling panel.

**One action list.** What can be done to a commit is said once, in `commitActionsModel.mjs`
(`COMMIT_ACTIONS`: cherry-pick · revert · checkout · branch here · tag here · inspect · copy SHA ·
copy short SHA · attach to the Agent tab — one label, one glyph, one hint, one consent rule each),
and every door reads it: a row — its lanes, its id, its refs, the subject with the row's width, the
author bounded to six rems, the time — keeps every verb behind one `⋮` at its end (`ui/MoreMenu`,
revealed on hover, focus and while it is the open row — the `row-actions` rule), the same menu a
right-click or Shift+F10 opens, in four sections (`MENU_SECTIONS`); nothing else sits between the
subject and the row's edge, so the subject has the row. The commit document's toolbar draws the
same list with icons and labels. Whether an action is off, and why, is one pure rule (`disabledReason`): a consented
one waits while another runs or while an operation is half-done in the checkout (*a rebase is
half-done — settle or abort it under Changes first*), *checkout* is pointless where HEAD already is
and *cherry-pick* where the commit is the tip (`headOf` reads HEAD from the rows in hand); the
reason is the button's tooltip and the menu item's suffix. A **ref chip** is a menu of its own
(`refActions`): a local branch offers *Switch to it…* (consented), *Open a workstream on it…* (a
door to the New workstream dialog preset to the branch — never off, nothing moves here) and *Copy
name*, a tag *Open a workstream at it…*, *Copy name* and *Delete tag…* (consented, red, pinned in
Safety first), a remote branch *Copy name*;
`HEAD` is a fact, drawn as a plain label so it never reads as a menu; a chip that is one wears a
chevron. A branch or a tag name is refused inline by `refNameProblem` — git's `check-ref-format`
rules a person hits, each a sentence — before the round trip, in the one `TagDialog` the graph and
the Branches view share; a message makes a tag annotated. One hook runs every action (`useCommitActions`) and one set of dialogs asks
(`CommitActionDialogs`): the consented ones confirm once with `consentWords` — a cherry-pick offers
*Record where it came from* (`-x`), and a merge commit picked or reverted asks which parent to keep
(`mainline`) — write a recovery ref first and toast `doneWords`. Every act runs through `gitOps`
against the checkout's session ([04 §What survives a switch](04-git.md#what-survives-a-switch)),
so it is one of the Git tab's verbs: off while another runs, and a conflict is a 409 read by its
code that writes the session's `operation` and puts the **Resolve card** above the view, where
*Continue*, *Skip* and *Abort* are. A branch or a tag is a ref: nothing moves. Every one of them
bumps the session's `stale`, on which the graph relayouts.

---

## Budgets

From [14](14-performance.md): first paint < 150 ms on the 100k-commit fixture; full layout < 2 s in
the background. Both measured with `criterion` in `bisa-engine` over the synthesised fixture,
reported, not gated ([14](14-performance.md)).
