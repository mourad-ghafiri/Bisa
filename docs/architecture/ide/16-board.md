# 16 — The Board

Every workstream a card in five columns — **Backlog · Todo · Doing · Done · Archived** — with a
due date, drag and drop, and one click into the workstream or its project. A *view* over
workstreams: nothing about how a project or a checkout works changes because a card moved.

---

## A column is not the state

A workstream's lifecycle — `open`, `dirty`, `committed`, `pushed`, `pr_open`, `merged`, `closed` —
moves only through a `WorkstreamTransition` (invariant I42). A card's column is where a person put
it. The two live side by side on the record and never write each other:

```rust
pub struct WorkstreamBoard {           // core/board.rs — on `Workstream.board`, absent until set
    pub column: Option<BoardColumn>,   // the person's choice; None follows the lifecycle
    pub rank: Option<u32>,             // the order within the column
    pub due: Option<DueDate>,          // a calendar day, YYYY-MM-DD
}
```

**An unplaced card follows the lifecycle** by one rule, `BoardColumn::for_state`, mirrored on the
desktop in `views/_board/boardModel.mjs` and held by a test on both sides: `open → Backlog`,
`dirty · committed · pushed · pr_open → Doing`, `merged → Done`, `closed → Archived`. Once a person
places a card the choice stands — a commit does not pull it back to Doing — except a **closed**
workstream, which is Archived whatever was chosen: it is terminal and its checkout may be gone.

The slot is the fourth thing a person may change about a workstream, beside the name, the note and
the pin: the store's one edit gate (`update_workstream`) admits it and refuses everything else by
name, and it rides the same record, so it is local by construction — a workstream never syncs, and
neither does its place on a board.

## Placing a card

The desktop never does rank arithmetic; it sends an index — the card's place **among the column's
placed cards as the node counts them**, not among the cards the screen shows: `boardModel.placeIndex`
takes the workstreams, the columns as drawn (narrowed, filtered, Archived folded) and the slot the
card was dropped on, and answers the index before the next placed card shown, else after the
previous placed one, else the column's end; `placeBody` clamps it to the route's `u32` — the menu's
*Move to* once sent `Number.MAX_SAFE_INTEGER` as "last", which the node refused every time
(`boardModel.test.mjs`, `scenarios/board.test.mjs`). `PUT /workstreams/{wid}/board/place`
`{column, index}` → `projects::place_workstream_card`: the column's other *placed* cards, by rank,
are the neighbours; `board::rank_at` answers a rank between them (`RANK_STEP` = 1024 apart when
numbered afresh, the midpoint otherwise) or, when the neighbours touch, the whole column renumbered.
Every record the move rewrote is written through the gate, announced as `WorkstreamEdited`, and
returned. A card in a column by its lifecycle alone has no rank and no say in the order; it sorts
after the placed cards, oldest first.

The due date is an ordinary edit: `PATCH /workstreams/{wid}` `{due}` — a `DueDate` is a real day
of the calendar or a 400 naming the shape — and `null` clears it. The card reads it against the
machine's own day: *overdue* (danger), *due today* and *due soon* (warn, within
`workstreams.board.due_soon_days`), *due* (quiet).

## The card

What the rail's row for the same workstream says, with room: the title (`workstreamCardModel`'s
rule — the name, else the branch, else *primary*), the project as a door of its own, the branch,
the chips the rail wears (`WorkstreamStatusChips`: state, `PR #n`, ahead/behind, dirty counts,
*no checkout*), the due chip, the pin, the note's first line; then **who stands in it** — the
harness marks and session marks of `workstreamSessionRows`, the shells' count and the pulse's line
(`pulseOf`); the ports (`PortChips`); when it last moved (`boardModel.lastActivity`). The facts come
from the four feeds the rail assembles — `useWorkspace().workstreams`, **the rail's own status
store** (`shell/workstreamStatusStore.ts`, `useWorkstreamStatuses` — the Board runs no poll and no
frame listener of its own; the store reads again on the facts `shell/workstreamFramesModel.mjs`
lists and on a reconnect), the sessions and terminals, the ports — and nothing per card, so a card
and the rail's row cannot disagree. The title is `workstreamCardModel.cardTitle`, the one rule the
rail's row reads; the column's count, the Doing limit's warning, the *Archived* answer to a closed
card's move and the project door's words are the model's (`countWords`, `wipState`, `closeWords`,
`projectWords`, `canMoveTo` — held by `boardModel.test.mjs` and `scenarios/workstreams.test.mjs`).

The verbs, on `⋮` and the right click: the workstream's own from `railMenuModel.workstreamMenuSpec`
(*Open · Rename · New shell · Diff against base · Close this workstream…*), then the Board's —
*Open the project · Set a due date… · Clear the due date · Pin · Move to ▸ (the five)*. *Move to*
is the same act as a drag without a pointer.

## Drag and drop

One typed payload, `workstream-card {id, column, label}` (`ui/dnd/dragData.mjs`). Each column is a
`DropZone` around a vertical `SortableList`, and the columns are one sortable **family**
(`family="board"`), so a card keeps its id whichever column holds it. **The drag is shown as it
goes.** The ghost under the pointer is the card itself (`useDragGhost`: the same `BoardCard`, from
the same `cardProps` as the column's, drawn `ghost` — no verbs — and lifted, `.board-card-lift`, the family's floating shadow with the accent's one-pixel edge);
the card left on the board is a placeholder — its footprint, dashed and faded
(`.board-card-placeholder`). A card hovering another column is moved there at once, at the slot it
would take (`onHover` from the list or the zone's well → `optimisticMove` as a *preview*), so the
room it will take is real while it is still in the air: the column's cards part, an empty column's
*Nothing here* line gives way, and along a column the sortable's own transforms slide the
neighbours. A drop over a card lands at that card's slot, a drop on the well lands last; the ghost
then **settles** — travels into the placeholder's place over the slow token — and the card wears
the accent's wash for a beat (`.board-card-landed`). A drag that ends anywhere else takes its
preview with it. Space lifts and drops from the keyboard (the kit's sensor). The screen keeps the
move as *pending* while the node writes it (`optimisticMove`, identity-stable) and the workspace's
next refresh — `workstream_edited` — replaces it with the node's answer; a refused move is put back
with the reason. Every duration is a motion token, so reduced motion collapses it all.

## The mode and its doors

The Board is the Project IDE's third centre — **Board Mode**, beside Project Mode and Agent Mode
(ide/09 §Agent Mode): `views/_board/BoardCenter.tsx`, drawn by the workbench between the project
rail and the right panel where the documents or the conversation would be. Not a screen of its own
and not a right-panel occupant, which is per root and *never a list*: the Board stays
workspace-wide — every project's workstreams, narrowed to **what the rail selected** — while the
rail and the right panel stay the root's. The rail publishes what a person selected
(`railSelectionStore.ts`, kept in the desktop's view memory under the rail's place and so across a restart — `parseSelection` making what is read back safe and `listedSelection` dropping a project the workspace no longer lists; `railSelectionModel.selectionOf`: a group or goal heading
is its projects, a project row is that project, a workstream, shell or agent row is the project it
stands in) on a click or the keyboard's cursor and never on the reveal that follows the route, so
the Board opens on every workstream; `boardScope` turns the selection into the projects the rows
are narrowed to and the bar's words — *All workstreams*, *Bisa*, *Shop · 3 projects* — and
the chip's × is the door back to *All*. Which root is in Board Mode is remembered per root like the other two
(`ideModeStore.ts`), and the switch offers it on the roots the switch shows for — a workstream with
a project. Doors: **Board** on the header's switch, *Project · Agent · Board* (`modeSegments`,
`MODE_WORDS.board`, the glyph `ICON.board`); the cycle `toggle_ide_mode` (`Mod+Alt+A`, Project →
Agent → Board → Project); the `board` command (`Mod+Shift+B`, global — `shortcuts.openBoard`: the
workstream you are in goes to Board Mode; from a goal's or a work item's root, or another screen,
the workstream you were last in does and `#/projects` takes you back to it, `lastRootStore.ts`);
the palette's *Board Mode: every workstream as a card*. The centre's one bar shows the scope chip,
filters — a search over title, branch, project and note; *Archived* — says how many cards the
scope and the filters leave, and holds *New workstream…*, which is the workbench's own dialog
(`NEW_WORKSTREAM`, on the one project selected, else the root's) rather than a second one. There is
one layout, the five columns: lanes by project were a wall past a few projects and shared one fold
and one Doing limit across lanes. Column folds and the Archived toggle are per-viewer furniture
(`bisa.board.*`, `views/_board/boardStore.ts`); the selection is not furniture.

## Settings

Four keys in the `workstreams` group, shown under Settings › Project IDE › **Board** and omitted from
the Workstreams panel: `workstreams.board.enabled` (off takes Board off the switch, the cycle, the
palette and the chord — `availableModes(false)` — and a root that remembered Board Mode opens in the
default, `modeFor` clamping it), `workstreams.board.due_soon_days`,
`workstreams.board.wip_limit` (the Doing column's header warns over it — never a refusal),
`workstreams.board.show_archived` (what the Board opens with; the toolbar switch is this window's).

## What the Board is not

Not a second rail: the rail is *where work is*; the Board is *how it stands*. Not a screen: a mode
of the centre, so the rail, the right panel and the header's chips stay the root's while it shows.
Not a workflow: a
column has no gate, no transition, no agent watching it. Not synced: a workstream names a path on
one machine, and so does its card. Not a place to delete: *Close* moves the record to Archived and
leaves the checkout; removing a checkout is the IDE's, consented, with a recovery ref first.
