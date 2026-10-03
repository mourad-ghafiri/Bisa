# The Project IDE

**Projects** in the sidebar is a development environment: an editor, a file tree that writes, a git
client with a commit graph, terminals with your harnesses in them, workstreams you switch between,
pull requests you review and merge, and the workspace's agents reachable inside the project they are
working in. This is the tour, task by task, with the CLI verb beside each surface and the design
document each rests on. The collaboration surfaces — Inbox, Goals, Workflows, channels — are
[`the-desktop.md`](the-desktop.md); the model behind projects and workstreams is
[`projects.md`](projects.md).

## Where it opens

`#/projects` opens on the workstream you were last in, else the first project's own root, and — with
no project yet — on a landing whose one job is a door: **New project**, which creates, clones or
imports. Nothing about goals: a person who wants an editor with agents in it gets one, and never
has to learn what a goal is to use it — a goal is optional on every way in, now or later. The one index of cards is the **Board** (below) — every workstream in five
columns, the centre's third mode beside Project and Agent.

A home is always a live project's open workstream: an archived project's checkouts are nobody's,
remembered or not. **Archive or remove the project you are standing in** — from the rail's menu, from
About, from `bisa project rm` in a terminal or from another window — and the IDE leaves the same
moment for the first project's own root (by name), or for the landing when no project is left;
Back never returns to the root that went. A checkout deleted while the app was closed, reopened on
its address, says the place is no longer here and lands the same way.

The IDE comes back as you left it, after a restart too: the open documents, each on its cursor,
its folds, its scroll and its mode; the folders open in Files; the search box with its text and
switches; the rail's filter, scroll and selection; in Git, the file selected, the folds of the
Changes tree, the Branches filter, the History's search and the commit it had open; the Board's
search and scroll; the Agent pane's conversation and where you were reading it. What was
*happening* is not kept — a banner, a failure to retry, a preview, an untitled document, the tabs
you had closed — and a workstream that is closed or a project that is deleted takes its memory
with it ([The desktop §Where you were](the-desktop.md#where-you-were)).

The workbench (`#/projects/{scope}/{id}`, scope one of `workstream`, `work_item`, `goal`; a
project's own root is its **primary workstream**, under the project's id) is three regions:

- **The rail**, left, under three tabs that are **three origins** — a project sits in exactly one,
  the one that says where it was born, and each tab, its glyph beside its word, badges how many it
  holds — one row at every width: a rail too narrow folds *Workflows* to its glyph first, then
  *Goals*, then *Workspace*, the word in the tooltip and the badge kept. *Workspace* is the
  projects you made with no goal in hand, grouped by the group you gave each; *Goals* is the projects
  made from a goal, under the goal that made them; *Workflows* is the projects an agent made running
  a step, under the workflow whose step it was. A goal or workflow that no longer exists still heads
  its section, marked *(removed)*. What a project is *attached* to is a different thing — shown on
  the project, in About, never as a tab. An archived project is out of the tree until the **Archived** switch beside the filter box is on, then dim with its mark. The tree is one geometry: a guide line hangs from every
  open row's chevron down to its children, every row's glyph sits in one column, a section header
  wears its count as a badge with space above it, the project is the one tall card, and the row you
  stand in wears a wash and an accent pill on its left edge. Each project opens to its workstreams — the primary first, badged — and each workstream to
  the work standing in it: the agents the engine runs there on a step and the harnesses you opened in
  a terminal, which report what they are doing themselves — an agent answering you in a conversation
  is on the Agent panel, never a row here — each row with its harness's own mark,
  the model it runs on after its name once the harness has said, and the effort it was launched at (*general-agent · claude-opus-5-5 · high*,
  and it follows a switch, Claude Code's fallback or your own `/model`), its state (*running Edit*,
  *waiting on you — permission: Bash*, *failed: …*), how long, its sub-agents nested under it, and
  *Terminate* (a harness) or *Abort* / *Answer* (an engine session) on hover — the stop mark only
  while the session runs or waits on you, never while it sits idle between turns; a harness's row opens its terminal tab,
  and a *waiting on you* there is answered in the terminal. What a harness's **account** has left
  is not a session row's fact: it lives in the window's footer ([The desktop](the-desktop.md) —
  one harness at a time, every installed one a click away) and, folded under each harness, in the
  Workstreams panel and Settings › Harnesses. A plain shell is a terminal row that
  reads *open* or *exited* with *Restart* — it never guesses a state from its output. A mark on every row says the loudest state among
  what is under it (waiting › failed › done › working › idle) — a raised hand that nudges while something
  waits on you, a loader turning while a tool runs, a sparkle breathing while an agent thinks, a check
  that pops once when it is done, a cross that shakes once when it failed, a moon when parked; the
  motion stops under your system's reduced-motion setting. On a workstream row the mark reads first, on
  the left, and the row then shows each harness's own mark — Claude's, Codex's, pi's, Oh My Pi's,
  OpenCode's, GitHub Copilot's, Grok's, Goose's, Cursor's — for each harness working in it, and a terminal glyph for each open shell. Each
  tab carries one badge, the number of projects it holds; attention is the mark and the row's wash,
  not a count. A listening port a shell or harness opens shows as a chip on the workstream's row:
  hover for its number and process, click to open `http://localhost:<port>`, right-click to copy the
  URL or stop the process (the shell stays). A live line counts up its total running time, a finished
one reads how long it took ("done · 2m 14s"), and a shell says how long it has been open; the
current-state timer is on hover. The order never moves. Hover a project
  for `+` — a new workstream — and the toolbar's `+` offers a project or a workstream on the one you
  are in. Right-click a project for *Rename*, *Set photo*, *Move to a group*, *Attach to a goal*,
  *New goal with this project*, *New workstream*, *Archive or remove project…* (or *Unarchive*); a group or a goal heading for *New goal
  from these N projects*; a workstream for *Rename*, *New shell here*, a harness, *Diff against
  base*, *Close*; a harness for *Terminate* — the same as closing its tab: the process is ended, the row goes with the tab;
  a shell to focus, restart, close (the others here, the exited ones); an engine session to show
  in the Agent panel, answer in the Inbox, or — while it runs — abort. Fold a workstream and its harness rows fold
  with it — the row keeps a mark per harness and shell, its state mark and its port chips. **A collapsed
  project shows its loudest workstream's line beside its name** (the name is never cut for it; the line is) — *Claude Code · running Edit ·
  src/cart.rs · 12s*, *↳ explore · waiting on you — permission: Bash · 2m*, *failed: the reason*,
  *done · 3m* — with a glyph for the tool's tier (reading, writing, executing), `+2 working` when
  more sessions run there, `↳ 3` for the sub-agents out, `1 port` — hover for who and the detail;
  the project and workstream you are in sit on the accent wash with a bar at the left edge and their names in the accent's ink; a waiting workstream row wears the bar alone, a failed one a red one. **The rail follows the route**: open a
  project from a goal's Projects panel, a step's row on Progress, a workflow's card or a pasted
  address, and the rail switches to its tab, unfolds its section and scrolls to its row; a
  project's menu offers the way back — *Open the goal it was born of*, *Open the workflow whose step
  made it*. A project made by a step of the goal's **own** design sits under the goal in *Goals*;
  only a library workflow's steps fill *Workflows*. The rail is a
  list you can arrow through: ↑↓ move, ←→ collapse and expand, `*` opens every sibling, typing a
  name jumps to it, Enter opens, `F2` renames. Double-click, or `F2` on the root you are
  in, renames in place. **Drag a row to reorder it** — a group among the groups, a project among
  its group's projects (or anywhere in the list while nobody has made a group), a workstream within
  its project, a shell within its workstream — the row lifts as a chip, a bar slides to where it
  lands and the neighbours make room, a heading it would join lights up, and a place that will not
  take it says so; **drag a project onto another group's heading, or between its projects, to
  move it there**. From the keyboard, Space lifts the row, ↑↓ step it, ←→ move it a level out or
  in, Space drops it, Escape puts it back. `⌘B` hides the rail; the filter box narrows everything. Beside the `+`, an
  **Import** button (`⌘⇧O`) brings in a folder from this machine or clones a repository; it sits in
  *Workspace* whatever tab you are on, and the dialog's one sentence says so — the dialog asks
  nothing about goals (a workflow's own projects are made by its steps, so an imported one never
  sits in *Workflows*). A goal heading's menu has *Import into this goal…*, the one door that fixes
  a goal: the project is attached as it is made and sits under that goal in *Goals*. Attaching a
  project you already have is *Attach to a goal…* — on its row's menu here, or under About › Goals;
  the same dialog either way.
- **The centre**: one strip of the documents you opened *and the terminals rooted here*. A terminal
  is a tab like a file; `⌘W` closes the active tab (a dirty document asks first; so does a live shell or a running harness, unless you switch that off under Settings › Project IDE › Terminal — the question itself names the switch and links there; a shell and a harness each have their own) and `⌘⇧T` reopens
  the last document closed here. *Split right* and *Split down* on the strip put the active document
  in a pane of its own — each pane with its own strip, tabs dragged between them, *Close pane*
  folding its tabs into the neighbour; the pin on the strip keeps a tab first and out of *Close all*.
  **Drag a tab along its strip** to reorder it — its neighbours slide aside as you go, and Space
  on a focused tab lifts it for the arrows (a pinned one stays among the pinned); `⌃Tab` and
  `⌃⇧Tab` cycle the strip, terminals included. The strip is **one order, newest last**: a file
  opened while a shell is open lands after the shell, a harness opened after that is last, and a
  drag moves any tab anywhere along it — a shell between two files included. **Right-click a tab**: a document for *Close · Close
  others · Close to the right · Close all · Pin · Split right · Split down · Copy relative path ·
  Copy absolute path · Reveal in Files · Reveal in Finder*; a terminal or a harness shell for
  *Focus · New shell here · Restart · Send the last lines to the agent · Show in the Agent panel ·
  Close · Close the other tabs · Close N exited*. Every item shows its chord.
  Closing the window, or quitting, asks first when *Confirm before quitting* is on (Settings › Desktop — the question links there) and then saves what is dirty; the shells themselves are drawn by the terminal layer over this area, which
  is why a build survives you leaving. With nothing open the centre is a landing: open a file (`⌘P`),
  a terminal (`⌃\``), the agents (`⌘⌥A`). That is the centre in **Project Mode**. The header's
  switch — **Project · Agent · Board**, `⌘⌥A` round the three — puts the centre in **Agent Mode** instead: the workstream's
  conversation with the agents, at reading width, with the rail and the right panel unchanged (see
  *Agent Mode* below); a browser tab open here moves to the Details pane on the right, and comes
  back to the strip when you return. The mode is remembered per workstream; a workstream you have never switched
  opens in the mode **Settings › Project IDE › IDE** names. A document always wins the centre: click
  a file in Files, a search hit or a changed file in Git while the centre is the conversation or the
  Board, and the centre is that file — the switch reads *Project* again.
- **The right panel** and its **rail**: a vertical strip of icons on the panel's right edge, evenly
  spaced, the one showing marked on the outer edge — **Files** (the tree, `⌘⇧E`) and **Git** (Changes · Branches · History · **Stashes**,
  `⌘⇧G` — what you do to the tree, and nothing to set up); then **Workstreams** (`⌘⇧U`) — the
  checkout you stand in, with the live status of its harnesses, sub-agents and terminals and, on a
  branch, its whole way to its base. Two more occupants open from the
  **header** instead, as icon buttons beside *Terminal*: **Agents** (`⌘⇧M`) and **About** (`⌘⇧D` —
  *Project*, *Checkout* and *Settings*, the last two the one place a repository is set up). Hover any of them for its name
  and chord. Pressing one shows it in the panel's column; pressing the one showing closes the
  column, and the rail stays — every occupant is one click away at every moment. Two tabs wear a
  **mark** while their column is closed, so you know without opening it: **Git** carries an accent
  dot when the checkout has something to commit or discard (the tooltip counts it — *3 changes to
  commit or discard*; a red one for conflicts), and **Workstreams**, on a branch, carries a dot while
  a step of its lifecycle is in progress — a pull request open with its checks, review and merge in
  hand, or a pulsing one while an agent is reviewing or fixing, or a push, pull request or merge is
  in flight. The rail reads, top to bottom, **Files · Git · Workstreams · Agent · About**, and it
  never loses a tab: one this root cannot show — Git on a goal's folder, Agent while
  the conversation is already the centre — is dimmed, its tooltip saying why, and pressing it does
  what the chord would. The header's first button, before the project's name, hides and shows the
  **project rail** on the left (`⌘B`); its last hides and shows the **right panel** (`⌘⌥B`) — each
  toggle on the side it moves. Between the name and that last button sit the centre's switch —
  **Project · Agent · Board** — *Quick open* and *Terminal*. Which occupant a root last showed is
  remembered per root; which view Git and About are on, and whether Changes draws folders or a
  flat list, is remembered once.

Design: [ide/01 — Trust boundary](../architecture/ide/01-trust-boundary.md),
[ide/02 — Component model](../architecture/ide/02-component-model.md).

## About, and the goals a project belongs to

**About** (`⌘⇧D`, the header's button beside Terminal) is three views. **Project** is the project's identity and relations, and nothing
else: its path, an **Origin** line — *made here*, *from goal …*, or *by a step of …*, each a link,
recorded once and never editable — its assignees, and a **Goals** section: each attached goal with a
*Detach* beside it, and *Attach to a goal…* (the rail's project menu opens the same dialog). Not the
tab strip, not the toolbar, not a banner: a person who came here to edit a file did not come here
to think about goals. *Edit project…* takes the name, the tags, the **group** and a **photo** — any
picture: it is scaled to a small square on this machine before it is kept, so a camera's file costs
nothing to sync or to draw, and a picture that is not one is refused in words.
**Checkout** and **Settings** are where the repository is set up — [below](#repository):
Checkout is how this checkout reaches its remote, what git thinks of it, its remotes and who
commits here; Settings is the project's
publishing policy, its git, workstream, script, editor and terminal settings. Its workstreams are
an occupant of their own. About is the **project's** on every checkout of it:
standing in a branch's workstream, About still shows the project, and the branch's own facts are
under Workstreams.

What reacts to a project — a commit on a branch, a push, a pull request that changed state, files
that changed — is a workflow's `project` start or wait, which names the project and is drawn in the
designer ([Events and gateways](events.md#project)); it looks at the repository itself, whether or
not the project is open here. **Workstreams** (`⌘⇧U`) is **the checkout you stand
in** — its name (rename it in place), and on a branch beside the primary its whole way to its base
right under the name — where it becomes a pull request, is checked, reviewed, merged and tidied
away ([below](#pull-requests-and-the-code-host)) — who is standing in it, *Close* (its path is never printed; the menu copies it) —
on every root, the primary included; it never lists the project's other workstreams and never
opens one: that is the rail's, on the left, where every workstream has a row and a `+`. Opening
or picking a workstream lands there.

**Attaching asks nothing** — it is additive, reversible and moves nothing; a toast names both ends:
*"web-app is attached to Dark mode."* **Detaching is confirmed**, and the dialog's whole job is to be
believed:

> **Detach web-app from Dark mode?**
>
> Nothing is deleted. The folder, its branches, its checkouts and its history stay exactly where
> they are, at `~/.bisa/projects/web-app`.
>
> Steps that already ran keep their record of this project, and their workstreams are untouched.
>
> Dark mode's agents will stop seeing this project as part of the goal.
>
> [ Cancel ]  [ Detach ]

The last line is the only real consequence, stated last because it is the one to weigh. Deleting a
goal that has projects says the same thing as a promise, in its retirement dialog: *"2 projects are attached and will not be
deleted — web-app and marketing-site stay in your workspace, along with every branch and checkout
under them."* The rules the copy is allowed to promise are the model's:
[04 — Workspace, Project, Goal](../architecture/04-workspace-project-goal.md).

```sh
bisa project list | show <id> | attach <id> <goal> | detach <id> <goal>
```

## The editor

A file opened from the explorer opens **in an editor** — Monaco, wrapped once, themed from the same
token roles as the rest of the app, with multi-cursor, bracket matching and find (`⌘F`) and
find-and-replace (`⌘R`) in the file — the same two chords open a find bar over a **rendered** file
too: a README's page, a csv's grid (it scrolls to the cell), an HTML page inside its frame; replacing
there lands in the source, so the page follows and the tab dirties. An svg is a picture — its bar
takes you to the source
built in. Its type is yours: `editor.font_size` (14 by default), `editor.line_height`,
`editor.word_wrap` and `editor.font_family` under Settings › Project IDE › Editor apply to an open
editor as you change them, and an empty family means the code face chosen in Appearance. The
terminal's `terminal.font_size` and `terminal.font_family` work the same way. Markdown opens
rendered, with *Source* one click away; a relative link in it opens the document it names, a
relative picture in it draws, and every path and URL in it is a door (see *Links and paths* below).
The view control on such a file is three glyphs — the **eye** for the rendering, the **brackets**
for the source, the **two columns** for both side by side — with the word as the tooltip, and the
IDE remembers which you chose for each file while the app is open.

**Not everything is text, and the IDE knows.** A **PDF** opens page by page with a page counter and
a zoom; a **picture** — PNG, JPEG, GIF, WebP, HEIC, an SVG — opens as the picture (an SVG with
*Source* one click away); a **video** or a **recording** opens with controls and seeks; a
**spreadsheet** (`.xlsx`, `.xls`, `.ods`) opens as a grid with lettered columns and a tab per sheet,
and a `.csv` or `.tsv` as the same grid with its *Source* beside it; a **Word document** (`.docx`)
reads as prose; a **deck** (`.pptx`) as an outline of its slides — title, text, pictures, notes; an
HTML page renders in the same sandbox an agent's page does, opening on its *Source*, with the
rendering and the two side by side one glyph away — and, in a workstream, annotatable for an agent
(see *From a rendered page* under *Agents in the workstream*). Each wears its
own glyph in the tree and on its tab, and its toolbar says what it is, how big, and what the viewer
found — *12 pages*, *120 rows × 6 columns* — with **Reveal in Finder** at the right. A file too
large to render here says so — its size and this machine's limit — and reveals; a file that turns
out not to be text does the same.
Nothing here is a URL the page loads: the bytes are fetched as you are, and drawn on this machine.

**Saving is compare-and-swap, never last-write-wins.** The editor holds the hash of what it read;
`⌘S` — or autosave, `editor.autosave.mode` and `editor.autosave.delay_ms` from Settings — sends it
with the text. If the file changed underneath (an agent, a terminal, another window), the save is
refused and a banner says *changed on disk since you read it*; your text is untouched. **Review**
opens theirs beside yours in a diff you can edit; *Keep mine* makes yours the next save; *Take
theirs* reloads. The same banner appears when the watcher sees the file change under a dirty
buffer; a clean buffer reloads silently.

**A single click on a file in the explorer opens a preview** — an italic tab, one per pane, replaced
by the next file you glance at, so browsing does not leave a row of tabs behind. The moment you
type in it, save it, double-click the row, pin it, drag it, or pick *Keep open* (`⌘⌥⏎`), it is a
tab like any other. Quick open (`⌘P`) and a search hit you press `Enter` on open kept; a hit you
click or arrow to previews. *Close saved* (`⌘⌥⇧W`) closes every tab with nothing unsaved; `⌘1` …
`⌘8` go straight to a tab and `⌘9` to the last; `⌘G` asks for a line; `Alt+Z` wraps long lines; a double-click on a tab
pins it. Above the editor the path is crumbs — a folder reveals itself in Files, the file copies its
path — and the footer says where the caret is and what the document is.

**A file from anywhere on this machine opens here too.** Drag it from the file manager onto the
Project IDE, press `⌘O` to pick one or more, or choose *Open in the IDE* on a path outside every
project that a message or a terminal printed. It opens as its own tab under no project, goal or
group — the crumbs are its whole path, and the file's name reveals it in the file manager — and
`⌘S` saves it back where it lives, compare-and-swap like any file: changed underneath, and the
same *changed on disk* banner and merge appear. *Save to…* moves it: *Into this project…* asks
for a path under the root and the tab becomes that file; *Elsewhere on this machine…* is the OS's
save dialog. Drop a picture on the conversation box and it is still an attachment — the box takes
its own drops.

**`⌘N` opens a new file before it has a name.** The tab is *Untitled-1* (the next free number
— close it and the next `⌘N` is *Untitled-1* again), plain text, kept, ready to type into; nothing
is on disk yet, so autosave stays out of the way. `⌘S`, or *Save to… › Into this project…* in the bar, asks
for a path relative to the root — folders are made on the way — refuses a name that is already
taken under the field, and on *Save* the tab becomes that file where it stands, with Files
selecting it; *Save to… › Elsewhere on this machine…* is the OS's save dialog instead. Cancel
keeps the tab, unsaved. *New file* and *Open a file from this machine* are also in the palette
(`⌘K`).

**A tab keeps its place.** Switch to another file and come back, and the document is where you left
it: the source with its scroll, cursor, selection and folds; a rendered Markdown, a sheet (its row
and its column), a PDF, a document or a picture at the same scroll; an HTML page too — which also
stays put while you edit it in *Split*, instead of jumping to the top on every keystroke. In a
rendering you can select any text, paths and links included: dragging across a path, or
double-clicking a word of one, selects it; a plain click still opens it.

A tab with unsaved work wears a dot, whether or not it is the one on screen, and keeps what you
typed while you look at another file. Closing it asks — **Save**, **Don't save**, **Cancel** — naming
the document; for a `⌘N` document that has no name yet, *Save* asks for one first. Closing several
asks once, for all of them, and so does closing the window or quitting. `⌘A` selects the whole file
in the editor, the rows in Files, the text in a field. Two bounds are this machine's, set under
Settings › Project IDE › Editor: above the editable size (2 MiB unless you moved it) a file opens read-only with
tokenisation off and says why; above the refuse size (20 MiB unless you moved it) it is refused,
the message naming its size and the limit, with **Reveal in Finder** to hand. A save that would
put a file over the bound is refused the same way, and nothing you typed is lost. The open documents and the active one are saved per root under
`ide/layout/` and come back on the next visit — furniture, local to this machine.

Design: [ide/03 — Files and editing](../architecture/ide/03-files-and-editing.md).

## Files

A tree over the root, in the workbench explorer and — read-only — in a goal's inspector. Entries
say what they are — work, note, result, journal, state, directory, file — derived from where they
sit; under a project or workstream root nothing is classified, because that is somebody's source
tree. It expands one level at a time, shows its bounds rather than hiding them, and refreshes on the
watcher's `file_changed` frames while a document is open.

**Select many** as in any file manager: Cmd-click toggles a row, Shift-click takes a range,
Shift+arrows extend it, `⌘A` selects every row on screen, `Esc` clears; a plain click or arrow goes
back to one. Delete, Cut, Copy, Duplicate and a drag then act on all of them — one confirmation
naming the three you chose, one request per entry, stopping at the first refusal and saying how far
it got. Above the tree, *Collapse all* folds every folder; every file wears the glyph its name
earns; a row with unsaved changes wears the dot; **a changed file's name is coloured** — green for
a new or untracked file, amber for one modified, renamed or deleted, red for a conflict — with git's
mark at its right (`~` modified, `+` added, `?` untracked, `!` conflict), and a folder holding a
change is coloured with the strongest one inside it, so you can see where the work is from the
top; and the tree follows the tab you click.

In the workbench the tree **writes**: right-click a row for *New file…*, *New folder…*, *Rename*,
*Duplicate*, *Cut*, *Copy*, *Paste*, *Copy relative path*, *Copy absolute path*, *Reveal in Finder*
(*Reveal in File Explorer* on Windows, *Reveal in file manager* elsewhere — the same verb everywhere) and *Delete…*; right-click the tree's background for the root's
*New file…*, *New folder…*, *Paste*, *Reveal* and *Refresh*; the `+` above the tree is the same
menu; drag a row onto a folder — or onto the tree's background for the root — to move it; the
folder it would join lights up, and nothing lights for a move that changes nothing. **An empty
folder takes a new file too**: *New file…* on a root with nothing in it shows the name field where
the first row will be; `Escape` anywhere in the tree cancels a name you changed your mind about.
**A new file opens as you name it** — a kept tab in the middle panel, the row selected in the tree —
so the next thing you type is its first line; a new folder is selected and left closed.
**The keyboard does all of it**, and every chord is yours to change in Settings › Keymap: `Enter` renames
in place (the Finder's rule — a click already opens; the `vscode` preset makes `Enter` open and `F2`
rename), `Space` opens, `⌘⌫` deletes, `⌘C` / `⌘X` / `⌘V` copy, cut and paste — a cut row dims until
you paste it, a copy pasted beside itself becomes `foo copy.txt` — and **`⌘V` also pastes what you
copied in Finder**: files and folders land in the folder under the cursor (the root with nothing
selected), copied whole; a name already there becomes *name 2*, then *name 3*; a link and a folder's
own `.git` are left out and said, and the toast counts what landed. The menu says *Paste from the
file manager* while Finder is the source. With a **picture** on the clipboard — a screenshot, a
copy from a picture app — `⌘V` opens a draft row under the folder named `pasted-image-<stamp>.png`,
its stem selected: type the name, `Enter` writes the PNG there (a name without an extension gets
`.png`; a taken name, or one starting with a dot, is refused under the row), and the menu says
*Paste the picture*. A file copied in Finder wins over a picture: it lands under its own name.
`⌘D` duplicates, `⌘⌥N` and
`⌘⌥⇧N` make a file or a folder. A name that is empty, has a slash, or is a sibling's is refused under
the row before anything is asked of the disk. **Search the tree** with the magnifier above it or
`⌘⇧F`: *Names* finds files as you type over the same index quick open uses; *Contents* runs the
node's ripgrep and shows hits as they land, grouped by file, with regex, case and whole-word toggles
and `in:src/** -in:*.lock` in the box to narrow where; a hit opens the file as a preview at its
line and shows it in the tree; ↑↓ step through them in that one tab and `Enter` keeps the one you
are on. A document tab's *Reveal in Files* (`⌥⇧R`) does the same for
what you have open; *Reveal in Finder* (`⌘⌥R`) shows it to the OS. Every action is an engine call confined to the writable root (a project
tree, a checkout, a goal's or a run's scratch folder), so `..` and a symlink out are one refusal. A delete goes
to the **Trash** by default (`editor.delete.trash`), and the confirmation says which will happen
before you click — *Move to Trash*, or *Delete* with the words about what has no copy; a folder shows
what it holds. A duplicate is named the way the Finder names one (`foo copy.txt`, `foo copy 2.txt`).
The tree follows the disk: an agent's write appears without a click, rows your `.gitignore` covers
are dimmed rather than hidden, a rename moves the open document's tab with it (a dirty one is saved
first), and a delete closes the tabs under it — asking about unsaved text.

```sh
bisa files tree <scope> <id> [--path …] [--depth n]
bisa files show <scope> <id> --path <file>
bisa files search <scope> <id> <query> [--include <glob>] [--exclude <glob>]
bisa files replace <scope> <id> <query> <replacement> [--apply]   # a dry run without --apply
```

Design: [ide/03](../architecture/ide/03-files-and-editing.md),
[ide/12 — Search and quick open](../architecture/ide/12-search-and-quick-open.md).

## Workstreams

A workstream is one checkout of a project — where terminals, agents and editors work. Every project
has a **primary** workstream: its own root, ready to work in with nothing to create. The others are a
git worktree **and** a branch, one-to-one, where one piece of work happens. Switching between
workstreams is navigation, not a git operation: each is its own directory with its own index, so
nothing is stashed and nothing is disturbed. **Open a workstream** on a project starts with one
choice, **Start from**: a **new branch** (an optional label for a derived `work/<label>-<tail>` name,
or a branch name of your own — made safe, never refused; a name that already exists is refused and
the dialog points you at the branch itself — and *Start at*, a ref, the base unless you say
otherwise), a **branch** the repository has (checked out as it is; one another checkout already holds
is greyed with who holds it), a **remote branch** (as of the last fetch, with a *Fetch* button;
fetched, then a local branch of the same name tracking it), a **tag** (a new branch at it,
`from/<tag>` unless you name one — or a *new tag, made now* at a ref you pick), or an **open pull
request** on the code host (its branch, tracking the remote's; its base as the base; the lifecycle
from where the pull request stands — checks, review, merge). **Base** — the branch the work goes
back to — is its own field, apart from where the branch starts; a pull request brings its own. A
repository with no commits shows *Make the first commit* instead: the Git tab's Changes view makes
that commit, and the button wakes up after it. A branch row's `⋮` under Git › Branches, a tag row's,
and a branch or tag chip in History offer *Open a workstream on/at …*, which opens the same dialog
preset to that ref.

**Opening one is never a hunt**: the `+` on a project's row, the rail toolbar's `+`, the landing of
a project's primary, `⌘⇧W`, *Workstream: open one on this project* in the palette, or *New
workstream…* in the Workstreams panel's `⋯` menu all open the same dialog — rail shown or hidden;
the panel still shows the workstream you are in and nothing beside it. It knows whether the repository can take one — a
repository with no commit says so and points at *Git › Changes* instead of failing after the click —
offers the branches to start from with the default preselected, and shows the branch the engine will
make as you type (`work/<label>-······`, the tail being the id's). `⌘↵` opens it, and the rail lands
on the new row. In the rail a project opens to its workstreams — the primary first, then each branch,
each with its live facts beside its name — how far it stands ahead of or behind its base, who is
working in it; uncommitted changes are the Git tab's mark, not a word on the row — and clicking
one re-roots the workbench there. `⌘P`
reaches workstreams, work items, agents and every terminal by name too. A workstream can be **renamed** (double-click, `F2`,
or *Rename*) — the label is yours; the branch is still the branch — and carries a note, a pin, a
place on the Board and a due date.

### The Board

Press **Board** on the header's switch (`⌘⇧B` from anywhere, `⌘⌥A` round from Agent, or *Board
Mode* in the palette) and the centre shows every workstream as a card in
five columns — **Backlog · Todo · Doing · Done · Archived** — the rail and the right panel staying
where they are; switch back to **Project** or **Agent** and the tabs or the conversation are where you
left them. A card's column is yours: drag it where
it belongs — the card lifts as itself, the column you carry it over makes room where it will land,
and it settles into place when you let go — or *Move to* from its menu, and it stays there through
commits and pushes. A card you
never placed follows its lifecycle — opened and untouched in Backlog, anything with a change or a
pull request in Doing, merged in Done — and a closed workstream is Archived whatever you chose. Drag
along a column to order it; Space lifts and drops a card from the keyboard.

A card shows what the rail's row shows, with room: the name, the project (a door of its own), the
branch, the state and pull-request chips, a **due date** — *overdue*, *due today*, *due in 3 days*,
*due 2026-09-30* — the pin, the note's first line, the harness and session marks of who stands in
it with what they are doing, the ports, and when it last moved. Click it to open the workstream;
`⋮` or a right click holds the verbs: open, rename, a new shell, the diff against the base, close;
open the project, set or clear the due date, pin, move to any column.

The Board shows **every workstream** until you pick something in the rail: click a project and
the Board narrows to its workstreams, click a group heading and it narrows to the group's — the
chip at the toolbar's left says which, and its × is the way back to *All*. The toolbar also finds a
card by title, branch, project or note and shows or hides Archived. A Doing column over its limit
says so in its header — a warning, never a refusal. *New workstream…* opens one on the project you
selected, else the one you are in. **Settings › Project IDE › Board** switches the Board off — it
leaves the switch, the cycle, the palette and the chord, and a workstream you left on the Board
opens in the default — and sets when a due date reads as soon, the Doing limit, and whether Archived
shows.

Each workstream row is a card: its label or branch, then — each only when true — its state, its
**pull request** (`#12`, a link to the code host), the agents running in it, `+a −b` against its base,
`↑a ↓b` against its upstream or *not pushed*, `S n` staged, `M n` modified, `? n` untracked,
conflicts, and a merge or rebase left half-done; cached two seconds per workstream on the node.
The workstream's own panel — the *Workstreams* occupant when you stand in that checkout — shows the
same chips in its header and holds **the whole checkout**: double-click the title to rename it;
under it the sessions standing in it; then, on a branch, its way to its base — the seven steps,
the one action that is legal now, the pull request as a card, the review, the merge
([below](#pull-requests-and-the-code-host)); and a `⋯` menu with *New workstream…* (the same
dialog the rail opens, on this project), *Rename*, *Copy the checkout
path* (the path itself is never printed — a managed folder says nothing a person needs), *Open
the pull request on the code host*, *Diff against base*, *Close this workstream…* and *Delete the
checkout…*. **Force push with lease** lives in Git ›
Changes, last in the sync bar's `⋮`, and in Git › Branches on the current branch (`--force-with-lease`
for a rewritten branch, never the project's default branch, through the Publish gate, consented,
with a recovery ref written first). On the primary the same panel
wears a *primary* chip and offers *Rename* and the path alone. Closing a workstream **with its
checkout** is consented too: what the checkout held is saved under `refs/bisa/safety/` in the
project's repository — which every worktree shares, so the ref outlives the directory — and the
toast names it. A workspace that would rather never lose a tree by a click sets **Closing a dirty
workstream** to `refuse` (Settings › Project IDE › Workstreams, `workstreams.dirty_close`): a checkout
that holds uncommitted work then keeps its tree, the refusal says what it holds — *1 changed file, 2
untracked files* — and nothing in it is stopped; a clean checkout goes as before, and closing the
record alone is never refused. Closing, with or without the checkout, ends what stands in the workstream: its
harnesses are terminated, its shells closed, the agents working there aborted — the dialog says how
many before you confirm, and that is the one question; the tabs close without asking again. In the
rail, a session row's verbs are icons you meet on hover — a stop mark to
*terminate* a harness or *abort* an engine session, shown only while it is running, a refresh mark to *restart* a shell — with the
word in the tooltip; and a row you point at or stand on is lit from its own indent, not from the
edge of the list.

```sh
bisa workstream open <project> [--label …] [--from <spec>] [--base <branch>]
bisa workstream show|diff|commit|push|pr|close <id> …
```

Design: [ide/07 — Workstreams](../architecture/ide/07-workstreams.md).

## Diff, staging and review notes

The **Git** tab's *Changes* view shows every path git has something to say about, **once, in
the project's own tree** — the same folders the Files tab draws, every changed file in its place
(fold a folder, and stage, unstage or discard what is under it from its row) or, with the switch
above the list, as a flat list of paths; the choice is remembered. Each file wears its **standing**
as a chip or two: *added*, *modified*, *renamed*… in the accent colour for what is **staged**, quiet
for what is **unstaged**, *untracked* for a file git has never seen, *conflict* for an unmerged one
— a file you staged and then edited again wears both, which is the fact. Beside the layout switch a compact **filter** — *All · Staged · Unstaged · Tracked · Untracked ·
Modified*, each with its count — narrows the tree to the files you are working on; *Unstaged* is a
tracked file's working-tree change and *Untracked* a new file, never both; *Modified* is a content
change, so an added or renamed file is not one. The choice is remembered, it narrows only what is
drawn — *Stage all* still stages everything — and a word that hides every row offers *Show all*.
Above them, **Stage all**
stages everything git has not got yet, and its `▾` offers *Stage tracked* (the modified tracked
files alone), *Stage untracked*, *Unstage all* and, apart, *Discard all changes…* and *Delete all
untracked files…* — every file git has never seen, to the Trash or gone as the root says, after a
confirmation that counts and names them — each with its count. Select a row for the file's patch — the working-tree side when it has one — or click a chip
for that side's patch; for the primary that is the project's own tree. **The patch opens in the
middle of the screen**, as a document at full width and height — a glance, so clicking down the list keeps one
tab; double-click a row to keep it — with its header: the path, which side, `+n −m`, the **View**
control, **History** (the commits that touched the file — click one and it opens in the middle too)
and **Open the file**. **View** is three glyphs — **Hunks**, **Side by side**, **Inline** — and the
one you pick holds for every patch — a changed file's here, a commit's file in History — across a
restart. *Side by side* puts the two versions in two columns and *Inline* interleaves the removed
and added lines in one, both read-only with the unchanged stretches folded; on the working-tree side that is the index against the file, on the
staged side HEAD against the index; a new file has an empty left, a deleted one an empty right, a
binary file says so. A file git has never seen is drawn whole on *Hunks*, every line an addition,
so what will land in the commit is what you read; it is staged whole from the list. On **Hunks**
the patch is drawn hunk by hunk, and each hunk has three acts:

- **Stage hunk** (or **Unstage hunk** on the staged side) puts exactly that hunk in the index with
  `git apply --cached`. Nothing here writes to the working tree.
- **Pick lines.** Click a `+` or `-` row to pick it; *Stage N lines* stages only those. Unpicked
  deletions stay as context, unpicked additions stay in the tree.
- **Annotate** opens a composer under the hunk. The note is pinned to the hunk's text (its hash is
  the note's identity), names the lines and whether the change was staged, and is stored under the
  project's `review/` directory in the workspace — never in the repository, so it survives a detach
  and never lands in somebody's clone.

**Commit** records what is staged and stays on this machine; **Suggest** asks the General Agent for a
message and never writes one you did not accept. **Amend**, the switch beside Commit, rewrites the
last commit instead: the box fills with its message, what is staged folds in (nothing staged is a
reword), the button reads *Amend* and asks first — naming the commit and, when it is already on
its upstream, warning in red that the branch then needs *Force push with lease…* from the `⋮`. The
old commit is saved in Safety before anything moves, so *Restore* brings it back. The platform's own git never gets in a commit's
way — its reads take no lock and its writes queue per checkout — so if Commit says *another git
process is using this checkout*, it is one that is not the platform's: an editor waiting on a
message, an agent's own `git` in a terminal; let it finish and try again. Parking changes is the **Stashes** view's, below —
nothing in Changes stashes. The view reads top to bottom in the order of the work: the sync
bar, one line saying how much changed and who will sign the commit, the files, the commit box —
pinned at the bottom while the files scroll, so nothing is
scrolled past to reach it; ⌘Enter commits — and, last, the review notes. Hover a file's row, or
arrow onto it, and its verbs appear over the end of its name — at rest the name has the whole row: **stage** and/or **unstage** as its standing
allows, **discard** for a working-tree or conflicted change, **delete** for one git has never
seen, and a `⋮` with everything else; a folder's row has the same verbs, each over the files under
it that it applies to, with the count — *Stage 3 under src/*, *Discard changes under src/?* asks
with the folder's name, and deleting the untracked files under a folder leaves the folder and
anything tracked in it. The arrows move and fold, a typed letter finds a name, Space stages what
the cursor is on — a file, or a folder's files — and Delete discards or deletes it after asking.
Wherever something is thrown away the dialog is one question with the verb as
its button and one line — *Saved to Safety first* — instead of a paragraph. The Git tab's **Stashes** view — the fourth, beside History — shows every
entry — `stash@{n}`, the message or *WIP on main*, *on* the branch it was made on, *untracked* when
it carries such files, when — with *Show* (the patch), *Apply* (onto the tree, the entry kept),
*Pop* (apply, then drop — only when it applies cleanly; a conflict keeps the entry) and *Drop*, the
last two asked about first, and **Stash changes…** at its head — the one place a stash is made. It
parks the working tree's changes as an entry and brings the tree back to HEAD: a message if you want
one, *Include untracked* (files git has never seen; ignored files never), *Keep the index* (what is
staged stays staged), and **Only these files** — tick the tracked changes to stash, and the rest of
the tree stays as it is. The button is off, with the reason written under the header, when there is
nothing git would save. The list is the repository's, so a stash made in another workstream of the
same project is here too.
Like everything else that moves the tree, a stash saves what was here under Safety first — and a
popped or dropped entry is itself pinned there, so *Restore* in the Branches view puts it back on the
list. A stash that stops on a conflict leaves nothing to abort: settle the files — first in the
Changes list, under *Conflicted* — or *Discard changes…* on them, which puts an unmerged path back to HEAD. **History** on the patch header lists the commits
that touched the file, following renames. In the editor, **Blame** draws the last commit and author
beside each line number; lines the tree has that HEAD does not say *not committed*.

**Nothing here is lost to a tab switch.** What the Git tab is doing and has typed is kept per checkout, not per screen: press *Suggest*, switch to Branches or to another workstream and come back — the message and the note are in the box; a pull that stopped on a conflict still shows its banner; a merge waiting on the Publish gate still says so; a half-typed branch name, a merged file in the conflict document, the lines you picked in a hunk, the config and scripts forms and a review note's edit are where you left them. The commit message survives a restart too; the rest is this run's and starts clean next time. One rule keeps a stale draft from biting: a file that changed on disk since is a fresh start.

Notes gather in **Review notes**, the view's last section under the commit box: *not sent* until you hand them over, *sent* with
the time once you have, *resolved* when the agent (or you) marks them done. Editing a note clears
*sent*. **Send to agents** posts one message per goal the project is attached to, the notes as
diff-hunk chips — what the agent sees is what you see. A project attached to no goal has no thread
to post in; the notes are still sent, and the toast says so. An agent working in the project reads
them as a tool result with `review_notes_list` and closes each with `review_note_resolve`.

```sh
bisa project files|stage|unstage|diff|commit|message <id> …
```

Design: [ide/04 — Git](../architecture/ide/04-git.md).

## Branches, tags, remotes — and Safety

The Git tab follows the checkout: a commit in a terminal, an agent's git, a fetch from another tool reaches the sync bar, the list, History, Branches and Stashes on their own — the watcher runs while the tab shows — and Refresh is still there for a read you want now. The Git tab's **Changes** view opens with the **sync bar**: where the branch stands against its
upstream (`origin/main · ↑2 ↓1`, *up to date*, *no upstream yet*), then **Push** and a `⋮` with
**Refresh** first, the three pulls — the project's *Pull* setting first (fast-forward only by
default), *Pull with rebase* and *Pull and merge* — then **Fetch**, and **Force push with lease…**
last, in red: it overwrites the upstream with this branch only if the upstream still points where
it did when you last fetched, never on the project's default branch, through the publishing policy,
after a confirmation that says so. Each pull and the force push are confirmed and saved to
Safety first, and an item that cannot run now says why in its label. A pull
that cannot fast-forward says how far apart the two are and offers the other two modes; one that
stops on conflicts lists the files — click one and the conflict document opens below — and points at
the **Resolve card** above, where *Continue*, *Skip* and *Abort* live while anything is half-done.
Before a pull or a merge runs, the dialog **looks ahead**: *No conflicts expected*, or *2 files would
conflict: a.rs, b.rs* — nothing has moved yet, so you decide knowing.
Push goes through the project's publishing policy exactly as it does from a workstream, and
publishes a branch that has no upstream yet.

The **Branches** view is four sections, all open with their counts. **Branches** lists local
branches — each with where it stands against its upstream (`↑2 ↓1`, or the upstream's name when
in step) and a *merged* chip once the default branch has all of it; the current branch first, the
default second, and a filter box once the list is long. Hover one for **Switch** and a `⋮` with
everything else: **Merge into <current>…** opens one dialog — fast-forward when possible, always a
merge commit, fast-forward only, or squash into one staged change, and a message for the merge
commit; **Rebase <current> onto it…** offers to stash your local changes first and bring them back,
and *Only the commits since…* to move just the tail of the branch; **Cherry-pick from it…** lists
the commits that branch has and yours lacks — tick the ones you want, *record where each came
from*, *stop before committing*; **Set upstream…**; **Open a workstream…**; *Rename…* and
*Delete…* — which, when the branch follows a remote one, offers *Also delete it on origin*, an
outward act that goes through the project's publishing policy like a push. On the current branch
the menu adds **Rebase interactively…** — the commits since a target you pick, each with *pick ·
reword · squash · fixup · drop*, ↑ ↓ to reorder, a message box for a reword or a squash, a summary
line (*5 commits → 3*), and the *Rebase* button off with the first thing wrong — no terminal, no
editor — and *Force push with lease…*. *New branch…* in the header asks for a name (checked as you type),
where to start — HEAD, or any branch, tag or commit — with *Switch to it* on by default. **Tags**
lists them with *Delete* on hover and *Tag HEAD…* in the header — the same dialog the graph's *Tag
here* opens. **Remotes** lists each remote by its name alone — `origin`, with how many branches it
holds — with **Fetch** on hover and a `⋮` for *Edit the URL…*, *Copy the URL*, *Open on github.com*
and *Delete…*; hover the name for where it lives, how it is reached and its URL. **Fetch all**,
**Add remote…** and a tree/list switch sit in the header, and a filter field appears once there are
many branches. Fold a remote open (`origin` starts open) and its **remote branches** are under it as
a tree in the shape of their names — `feature/x` under `feature`, folders you can fold — or, with
the switch, as a flat list newest first; a row is the branch's name without the `origin/` prefix,
*default* on the project's default branch, *tracked by main* when one of your branches follows it,
when it last moved (hover for the commit), and **Check out** on hover — a local branch of the same name, following it, switched to — with a `⋮`
that merges it into your branch, rebases yours onto it, cherry-picks from it, fetches just that
branch, opens a workstream on it, deletes it on the remote (through the publishing policy, its tip
saved under Safety first) or copies its name. A remote that has not been fetched says so. Deleting a remote asks first and tells the truth: nothing is pinned in Safety
for a remote, so the dialog shows the URL with a copy button. Every action that moves the working tree is
**consented** — it confirms first — and does one thing before anything else: it saves what is here.
`git stash create` writes the index and the working tree as a commit object without touching either,
and a ref under `refs/bisa/safety/` pins it (or HEAD, or the tip of a branch or tag about to be
deleted). The toast after each action names that ref.

**Safety**, the last section — open, never folded away — lists those recovery points — the operation, the branch it was on, what
was saved (*commit pinned*, *index + tree saved*, *stash entry saved*), when — with *Restore* on
each: HEAD goes back to its branch (or the commit), and the saved index and tree land on top; a
saved stash entry goes back on the stash list and the tree is left alone. Restoring is itself
recorded, so an undo can be undone. Nothing
prunes these refs but a person: the command sits under the list with a copy button.

Git refusing to overwrite a local change, or a rebase, merge, cherry-pick or revert hitting a
**conflict**, is a state the panel shows, not an error it hides — and one it walks you through,
whatever you know of git. A **Resolve card** appears above whichever Git view you are on. It names
what is happening by branch — *Merging feature/login into main*, *Rebasing feature onto main —
commit 3 of 7* — in one sentence says what that means, and shows the two sides as swatches: ● *main
— mine* and ● *feature/login — theirs* (a rebase says ● *your commit "Add login" — mine* and ● *main
— theirs, already there*: git's own words swap there, yours never do). *What is a conflict?* opens
three sentences for anyone meeting one for the first time, and the promise that nothing is lost.
Under it, a **checklist** of the conflicted files with what kind each is — *both changed*, *deleted
on feature/login*, *added by both* — ticked as each settles, *2 / 5* on a bar, and **Continue**
(off, with the count, until every file is settled; for a merge it names the commit it will make),
**Skip** (past the commit a rebase, cherry-pick or revert stopped on) and **Abort** (the branch goes
back exactly to where it was; every file you settled is put back too), each asked about first. A
rebase, merge or pick started in a terminal shows the same card, with the same names.

Click a file in the checklist — they are also first in the Changes list, under *Conflicted*, and
the *Conflicted* filter keeps them alone — and the **conflict document** opens in the centre: the
legend again, then **one card per conflict**, *Conflict 2 of 3*, the two versions side by side under
their names, the base on *Show base*, and the choices: **Keep mine**, **Keep theirs**, **Keep both**
(mine first, or theirs first) and **Edit…**, which opens the lines in the editor started from both
sides. Unchanged code between conflicts is folded (*… 42 unchanged lines …*, a click opens it). A
settled card folds to the lines it kept — *kept theirs — feature/login* — with *Undo*; the arrows and
`Alt+↑` / `Alt+↓` move between the ones still open, `⌘⌥1` / `⌘⌥2` / `⌘⌥3` keep mine, theirs or both
on the current one, *Keep all mine* / *Keep all theirs* settle the whole file at once. **Review**
shows the whole file as it will be saved against either side or the base — editable, and an edit
there is the result. **Mark resolved** lights only when every conflict is settled and no marker is
left; it saves the result and stages the path, and the document moves to the next conflicted file;
settle the last and *Continue* is the one thing lit. **Ask an agent** on a card hands that conflict
to the Agent pane with both sides named and a drafted question — the agent explains what each side
changed and suggests a merged version you can paste into *Edit…*; staging and Continue stay yours.
A file one side deleted and the other changed, one added on both sides, or a binary is a choice, not
a merge: its document is the two outcomes in words — *Keep mine — main* / *Delete it, as
feature/login did* — and git makes the one you pick. Changes
can be thrown away as well as staged: a row's menu on an unstaged or conflicted file has **Discard
changes…** (the working tree goes back to the index; a staged change is kept — unstage it first if
it should go too) — as an icon beside the row's stage toggle, red on hover, and in its menu — the toolbar has a **Discard all…** icon over the unstaged files, an untracked row has a **Delete file…** icon, a folder's row the same over the files under it, and the hunk view has *Discard* on
the unstaged side for a hunk or for the lines you picked — every one asked about first, and every
one saving what was there under Safety before it moves. A file git has never seen has **Delete
file…** instead: it goes to the Trash when the root says so (the *Delete to Trash* editor setting),
else it is gone, and the dialog says which. The commit graph's row menu and the commit document offer
*Checkout* (detached), *Cherry-pick*, *Revert*, *Create branch here* and *Create tag here*.

Agents cannot reach any of this: the type the consented tier requires is minted in one place in the
node from the request's bearer token; the engine takes it by value; the MCP intake has no field for
it.

Design: [ide/04 — Git](../architecture/ide/04-git.md).

## The commit graph

The Git tab's **History** view draws every commit reachable from a ref, newest first, with
lanes and edges laid out by the engine: the first parent continues a lane, a second parent forks
out of the node, converging lanes merge back in. Lane colours come from the theme's own roles. The
first thousand rows are laid out inline and the rest in the background — the header says *laying
out… N so far* until it is done — and a commit that lands while you look makes the header say *the
repository moved on* while the stale rows stay on screen and the relayout runs. The rows you have
not scrolled to are fetched as you reach them, so a hundred-thousand-commit history opens as fast as
a hundred. The graph fills the panel top to bottom, with one scrollbar. The header is one line —
*1,234 commits*, a word while it lays out, and a `⋮` at the right with **Search…**, **Show all
branches and tags** or **Show only the current branch** (the one in force wears a check; the
count and the rows follow it, and the choice is remembered per checkout), and **Lay out again**.
**Search…** (or `⌘⇧H`) opens a line under the header and asks the node to look through the
**whole** history — author, subject, id, ref — saying *3 of 41 matches*; rows on screen that
match stay bright and the rest dim; `Enter` / `n` / `N` jump to the next match wherever it is,
fetching its window on the way, without changing the topology; Esc hides the line again. Click a
row and the commit opens in the middle of the screen: message, refs, author, the files it
changed against its first parent, and its patch in the same three views a changed file has —
**Hunks**, **Side by side**, **Inline**, the one *View* choice you made in Changes. The file list
is a picker: on *Hunks* you read the whole patch until you click a file, then that file alone (click
it again to let go); *Side by side* and *Inline* compare one file's two versions — the parent's and
the commit's, a new file with an empty left, a deleted one with an empty right, a moved file's old
text on the left — the file you clicked, else the first. A glance, so the next row you click takes
its place; the row you clicked stays marked. Each row reads its id, its refs, the commit's subject,
who made it and when; **hover a row** and a `⋮` at its right edge opens everything — *Copy SHA*,
*Copy short SHA*, *Checkout (detached)…*, *Cherry-pick onto the current branch…*, *Revert…*,
*Create branch here…*, *Create tag here…*, *Open the commit*, *Attach to the Agent tab* — the same
menu a right-click opens, an item that is off saying why (*HEAD is already here*, *a rebase is
half-done — settle or abort it under Changes first*); and the commit document's toolbar draws the
same list with icons and labels. The **ref chips** on a row are
menus of their own: a branch offers *Switch to it…* and *Copy name*, a tag *Copy name* and
*Delete tag…*, a remote branch *Copy name*; `HEAD` is a fact. A branch or a tag name is checked
as you type, with git's own rules said in a sentence (*no spaces*, *no two dots in a row*, *a
part cannot end in .lock*); a message makes a tag annotated. Checkout, cherry-pick, revert,
switching to a branch and deleting a tag confirm first and save what is here under Safety — one
question, whichever door you came through; a cherry-pick offers *Record where it came from*, and
picking or reverting a merge commit asks which parent to keep; a conflict names the files and
puts the Resolve card above the view, where *Continue*, *Skip* and *Abort* are.

Design: [ide/05 — Commit graph](../architecture/ide/05-commit-graph.md).

## Repository

**About › Checkout** and **About › Settings** are the repository itself, as distinct from what
changed in it — the two short views a repository is set up in, so the Git tab is only what you do
to the tree. *Checkout* is this checkout: its connection, its facts, its remotes, its git config;
*Settings* is what the project saves for every checkout: publishing, git, workstreams, scripts,
editor and terminal. On a plain folder Checkout shows one card instead — **Initialise a
repository** — the same card the Git panel and the Workstreams panel show; it runs `git init`, asks
who commits as a new project does, and makes the first empty commit when someone can (see *Import or
adopt* in [Projects](projects.md)). Every door points at the right one — the header's *no committer* chip, *Set
origin* on the sync bar and *Set who commits here* under the commit box land on Checkout; a refused
push's *Change the policy* on Settings. **Nothing on either view saves as you change it.** A bar
stays at the top as you scroll: *All saved*, or *3 unsaved changes* with **Discard** and one
**Save** that writes them all — the policy, the settings, the git config, the scripts (approved on
this machine in the same act), the account pin. A change you did not save waits in the panel, tab
switch or not, until you save or discard it; a value the node refuses stays drafted with the
refusal in a toast, and what came before it is saved. Checkout opens with **Connection** — what
this checkout will use the next time it talks to its remote, so you know
*before* a push whose name and whose key are going out: the remote as `github.com · acme/web` with
its protocol (SSH, HTTPS, local); the **profile** it falls under (*Acme*, or *none — your global
config applies*); **who commits**, and where that comes from — *set for this repository*, *your
Acme profile*, *your global git config*; the **transport** — over SSH, the key `ssh` would offer
and whether ssh-agent holds it, over HTTPS, git's credential helper and the username it holds; and
the **account** pull requests will be opened as, with where that comes from — pinned here, your
Acme profile's, your default for GitHub, **the GitHub CLI signed in on this machine**, **git's
credential helper** — and a way to pin a different one to this repository alone. If `gh` is
already signed in, or `git push` over HTTPS already works, the card says so and asks nothing of
you: the platform reads what your machine already has before it sends you anywhere. When
something is off, a caution says so in one sentence — *the author set here is not the Acme
profile's*, *the key for this remote is not loaded in ssh-agent*, *no GitLab account answers for
this remote*, *@you is not a member of acme as far as GitHub can see* — with a *Fix in Settings*
link to the panel for that host; the same cautions put a dot beside the Git tab's views and a chip
in the header beside *no committer*. **Check the connection** asks three read-only questions — the
code host, as that account, whether the repository is there and you may push to it; the SSH host,
one handshake; git, `ls-remote` — and changes nothing anywhere. `bisa git connection
<workstream> --check` prints the same.

**Repository** is what git thinks of the checkout — branch, upstream, ahead/behind, remote, default branch.
**Remotes** is `origin` as a card — `github.com · owner/repo` — with *Edit* to point it elsewhere in
place and *Add origin* when the project has none, the other remotes under it; changing `origin` is
what the code host detection and the project record read (the working view of remotes, with their
branches and *Fetch*, is Git › Branches). **Who
commits here** is the `user.name`/`user.email` every commit made in this repository will carry —
yours from the Changes view, an agent's on settlement, in the primary or in any worktree — with where
it comes from: *set for this repository*, *inherited from your Acme profile*, *inherited from your
global git config*, or *nobody is set to commit here*. Editing writes the pair into the repository's
**local** config and nowhere else; a repository nobody commits in whose remote resolves to a
connected account — your `gh` sign-in included — offers **Commit as …** with that account's name
and the address the host attributes commits to, one click and it is set; the card saves on its own
even while the rest of the project is still being read, and refuses half a pair;
**Git config** is the repository's local layer as a form — name, email, *use config only*, signing,
rebase-on-pull, line endings — each showing what it inherits from your global git config, with
*Inherit* to clear a value, and *Pin the global pair here* on an inherited identity: one click that
writes your global name and email into this repository's own config, so it keeps its author. Your global git config is never written from a project. A repository nobody is set to commit in blocks the commit button with *Set who commits here*,
shows *no committer* in the workbench header, and refuses a commit by name from the CLI and the API —
before anything is staged. The idea is one author per repository, pinned: a person who sets who
commits in each repository never pushes to one with another's name.

**A new repository inherits your global git config, and nothing asks.** When a project is created — by
you, by a goal, by a workflow step, by an agent — its repository takes your global name and email
unless you say otherwise. The project dialog says so in one line under the form, *Commits as Name
<email> — your global git config*, with *Change…* to set local values (name, email, `useConfigOnly`,
and under *More git config…* signing, rebase-on-pull, line endings) for this repository alone — for a
new, cloned, imported or linked-in-place repository alike, and whatever you type there is written to
the repository's own git config at creation and read back under About › Checkout. **Who commits in a
new repository** under Settings › Git & code hosts › Identity changes the rule for the workspace:
*inherit* (the default), *pin* — the global pair is written into each new repository's own config, so
it keeps its author even if your global config changes — or *ask*, which opens the identity fields in
the dialog every time and asks for every project a goal or a workflow makes. When your global config
has no identity, the dialog opens the identity fields from
the start whatever the rule; and a project created without one — from wherever — raises the *Who commits in …?* dialog,
which writes the repository's local config and can *also save the name and email as your global git
config* so the next project stops asking. Settings › Git & code hosts › Identity shows your global git
config and sets it, once, for every project — and holds your **profiles by organization**, so a
repository under `github.com/acme` commits as your Acme author, pushes with the Acme key and opens
pull requests as the Acme account without a local setting in any of them ([the desktop
guide](the-desktop.md#settings)). A commit refused for want of an identity asks too, and an agent's settlement
that was refused is committed by itself once you answer. *Not now* puts the question off until the
next launch.

**About › Settings** is what is saved on the **project** and so is the same in every checkout: **Publishing** — the policy a push or a pull request passes, first because a refused one sends you here — then **Git** and **Workstreams**, the project's own values for how its branches merge and pull and how its checkouts are cleaned up — the default branch, the merge strategy, the pull mode, deleting the branch after a merge; the clean-up and where you land after a merge — **Agents & decisions** — the mode a conversation here starts in, the **effort** a model works at here when nothing more specific says ([Agents and teams](agents-and-teams.md#effort)), and whether the Decision-Making Agent decides for this project — and **Editor & terminal** — tab size, spaces, wrapping, format on save, the harness a terminal here opens with — each with *Inherit* to put the workspace's value back on save, and a *changed* chip on every row you touched until you do. Settings never holds a project's value; the project does, here.

**Workstream scripts** are the last card: three shell scripts the project runs around every
workstream — **pre-create**, in the project root before the checkout exists (a non-zero exit refuses
the workstream, and the dialog shows what the script printed); **post-create**, in the new checkout
once it exists (`npm install`, `cp .env.example .env`; a failure is toasted and the workstream
stays); **clean**, in the checkout before it is deleted (`docker compose down`; a non-zero exit keeps
the checkout and the delete dialog shows why). A script that failed is a notice on the workstream's
Inbox row too, so a failure nobody was looking at is read later rather than lost. Each is told where it is —
`BISA_PROJECT_PATH`, `BISA_WORKSTREAM_PATH`, `BISA_BRANCH`, `BISA_BASE`,
`BISA_WORKSTREAM_ID`, `BISA_SCRIPT_PHASE` — and terminated at the timeout you set. The
scripts travel with the project; the **trust** to run them is this machine's: the view's *Save*
writes a script you typed and approves it here in the same act, and a script that arrived from a
collaborator reads *changed since anyone here approved it* and will not run until you read it and
press *Approve on this machine* on the same bar. They run for a
workstream you open, one an agent opens, and one the clean-up deletes alike.

```sh
bisa project identity <project>                                  # who commits here, and where that comes from
bisa project identity <project> --name "Ada Lovelace" --email ada@example.org   # set it, locally
bisa project new storefront --committer "Ada Lovelace <ada@example.org>"       # born with one
bisa project new storefront --git-config user.useConfigOnly=true                # any schema key, locally
bisa project git-config storefront --set pull.rebase=true --unset core.autocrlf # the local layer, later
bisa project git-config --global --set user.name="Ada Lovelace"                 # your global config, at your request
bisa git connection <workstream> --check      # what this checkout will use to reach its remote, and the three probes
bisa git profile list                         # your profiles by organization
bisa git ssh keys                             # your public keys, and which ssh-agent holds
bisa git account list --host gitlab           # your accounts for a host and the default
bisa git health --host github                 # the CLI, the accounts, who answers
bisa git login --host github                  # how to sign in, printed
```

## Pull requests and the code host

The **Workstreams** panel (`⌘⇧U`), on any workstream with a branch beside the primary, walks the
branch to its base in seven steps right under its name — **Commit · Push · Open pull request · Checks · Review · Merge ·
Clean up** — and offers exactly one action at a time, under the step it belongs to: *Commit in Git › Changes* when nothing is beyond the base yet, **Push and open pull
request** when commits are not on the remote, **Open pull request** once they are, **Merge** once
the pull request is open, **Clean up branch** once it has landed. In the pull request's dialog,
**Suggest** asks the General Agent for the title and the body from the branch's commits and its
changes, as the commit box asks for a message: the draft lands in the fields for you to read and
edit, a field you typed in while it was asked keeps your words, **Undo** puts back what was there,
and nothing goes to the code host until you open it. The step with the action wears
the accent dot, and only that one; a step merely in flight — checks still running, the code host
still checking — wears a ring that pulses instead, so the dot always means *here is what you can
do*. Each step keeps its own surface
under its row — the pull request and its checks under *Open pull request*, the reviews under
*Review*, the merge under *Merge* — so reading down the panel is reading the work in order; the
review reads above the merge but never holds it, and the Merge button is under *Merge* and nowhere
else, whatever the checks are doing. A step that cannot go says why in its own words — *2 checks
failing*, *conflicts with main*, *a draft*, *commits not pushed* — and a commit you made in
a terminal counts: the panel reads the branch, not only the record. The pull request opens on the code host behind the project's `origin` — **GitHub, GitLab or
Bitbucket**, public or your own instance — and the panel speaks that host's words: on GitLab it is
a *merge request*, the button says **Open merge request**, and *Open on GitLab* opens it in your
browser. The form is rendered from what that code host **can do**: a *Draft* toggle, *Reviewers* and
*Labels* appear only when the code host has them, and are absent — never greyed — otherwise; the title
starts as your last commit's subject. A pull request is opened on pushed work, so a branch that is
not on the remote yet is **pushed first by the same click** — the button says so — under one
decision. Opening passes the
project's publishing policy as a push does: `auto` proceeds, `gated` opens a gate in the Inbox and
nothing exists on the code host until it is approved, `manual` refuses so a person does it. A refusal
says which it was — the manual policy, a gated project whose workstream has no goal to ask on, a
declined gate, a branch with nothing to publish yet — and the first two carry **Change the policy**,
which opens About › Settings, where the policy is set — the one place it lives; Settings › Git &
code hosts holds your global git config, your profiles, your SSH keys and your GitHub, GitLab and
Bitbucket accounts, nothing per project. When a gate was **approved and the act then did not go
out** — the remote refused the push, the code host the pull request or the merge, the network was
gone — the panel says so under the step: *Approved, and it did not go out*, what was approved and
the reason in the node's words, with the button there to try again; the Git tab's sync bar says the
same of a push, and the Inbox carries the notice on the workstream's row. The pull request is opened **as the account the
checkout's Connection card names** — the organization's profile's, the host's default, one you
pinned there, the CLI's, or git's — so who opens it is a fact you read before the click, never a
surprise after.

Once open, the **pull request card** under *Open pull request* shows its number and title, who
opened it, `head → base`, and — on a code host that reports check runs — the checks' one-line
summary as a chip. The runs themselves sit under **Checks**, the step's own surface: each with its
conclusion, its name as the door to the log, and what the code host said. A run that **failed**
carries **Fix with ▾** on hover: pick an agent — the last one you handed a check to comes first —
and it is told what failed, where the log is, and to reproduce the failure in this checkout, fix
its cause and commit on the branch, never push or merge. You follow it right under the list, the
way you follow a review — *fixer is fixing check ci / lint…*, what it is doing, **Stop** — and when
it ends the line says what it committed, with *Open Git › Changes* one click away.

The **Review** step is **optional**, and it is there from the moment the branch exists — one place
to ask an agent, whether or not a pull request does. **Before a pull request**, it reviews the
branch against its base: pick the agent from the list, add a line about what to look at if you
like, and press *Review against main with <agent>*. The request goes into the workstream's
conversation and **you follow it right there**, under the button: the agent reads the diff against
the base in the checkout and answers in the conversation — its verdict in one line, then findings
citing files and lines — changing nothing; nothing reaches a code host; when it is done the line
says *reviewer reviewed the branch:* with its answer folded under it. A branch with no commit
beyond its base says *Nothing to review yet* and points at Git › Changes.

**Once a pull request exists**, the same step reviews the pull request, in the order things usually
happen. One sentence says where the review stands — *Optional: ask an agent to review, add your own,
or merge as it is.* — and under it, **Agent**: the agent from a plain list (the one you used last
first — no search), an optional line of words for it, and *Review with <agent>*. The request goes
into the workstream's conversation and **the panel stays where you are**: a line under the button
follows the agent — *general-agent is reviewing…* with the state mark and how long it has run, and
under it what it is doing this moment, *running Bash · cargo test*, *thinking…*, *running sub-agent · explore, plan* while its sub-agents are the work, *2 sub-agents*. A sub-agent sits under its harness while it works and leaves the moment it finishes; one that failed stays red until the harness's next turn. The workstream's and the project's marks stay the loader until the harness itself is done — a finished sub-agent is not a tick.
**Stop** ends that session — offered only while the agent is running or waiting on you, never while
its session sits idle; a small agent icon opens the Agent panel if you want the whole
conversation, and if the agent stops to ask you something the line says so and offers *Answer in
Agents*. An agent that has answered and gone quiet ends the run on its own — the line settles once
its reply is in the conversation. When the review lands on the pull request, the line gives way to the review itself: the
agent's name, when, and its words folded to a first sentence, with *Review again*. When a run ends
any other way the line says how — *general-agent was stopped before reviewing.*, *general-agent
failed — <reason>.* — with the agent's last words folded under it and × to dismiss. **One run at a
time**: once it ends, stops, or its review lands, you can ask the same agent or another. Then
**You**: one box, optional, and the buttons — **Approve**, **Submit review**,
**Request changes** — only the ones the code host takes. An approval goes out with or without
words; *Submit review* and *Request changes* wait for words and say so under the buttons. The
account that opened the pull request may comment on it but not approve it or request changes, and
since your token opens every pull request the platform opens, on your own pull request *Submit
review* is the one button and the row says why. Once given, the row shows your verdict and when.
The platform signs the agent's review with its name on the pull request itself, which is how the
panel tells the agent's review from yours — the same account posted both.

Then **Comments** — what reviewers left on the code, on the code host — grouped by file, open first,
the count on the header (*3 open · 2 resolved*), each folded to where it is and what was said first
(*line 12 · open · @alice This rounds twice.… · +2*) with its replies unfolding under it — an
agent's reply named by the agent, since the platform signs it. Hover an open comment and it has
**three hands**. **Fix with ▾** opens the agents this checkout can reach, the one you last handed a
comment to first: pick one and **that** comment goes to **that** agent — a different agent for the
next comment if you like. **Reply** opens a box for your own words, sent to the code host as
*Reply* or as *Reply and resolve*, which closes the thread with your answer on it. **Resolve** (or
**Reopen**) marks it on the code host without a word. The header's **Fix all 3 open ▾** hands every
open comment to one agent at once. An agent handed a comment answers on the thread itself and
resolves what it addressed — so what happened is on the pull request, not only in the thread here.
You follow a fix at the top of the Comments section, the same way as a review — *fixer is fixing
1 comment…*, what it is doing, **Stop** while it runs — and when it ends the line says *fixer fixed
1 comment — 2 new commits on the branch, to keep or discard in Git › Changes.* with its reply
folded under it and *Open Git › Changes* one click away. The comment reads *fixing…* while it
works, and the merge stays offered throughout. **One agent at a time in a checkout**: while any
agent works here, every other *Fix with* — on the comments and on the checks — says who is busy
instead (*alpha is fixing line 12*); replying and resolving stay yours whatever runs. A line for
each other account that reviewed closes the step. Reviews refresh on the
project's *Fetch interval* while the panel is open — and **while an agent works for you the whole
lifecycle follows it**: every fifteen seconds the pull request, its checks, its reviews and the
branch's own commits are read again, so a fix's commits move the Commit and Push rows as they land;
when the agent answers, and when it finishes, everything is read at once and every row settles
together. A small line under the steps says when the code host was last read — *code host read 12 s
ago*, or *following general-agent · read 5 s ago* — with a refresh button beside it to read it now.

A review **never holds the merge**. Until anyone reviews, the Review row reads *optional — an
agent's, yours, or none*; once anyone has — the agent alone, you alone, another account — it is done
and says so: *reviewed by general-agent and you*, *approved by @alice*, *changes requested by
@carol*. The Merge row is offered from the moment the pull request is open. Checks still running,
comments still open, and a request for changes still standing, are **cautions**: the Merge row wears
them, the button stays live, and the confirmation lists them before *Merge anyway*. Only what the
code host itself cannot merge blocks: a draft, conflicts, failing checks, unpushed commits; while the
host is still computing whether the branch merges cleanly the row waits with the ring and the button
is off with the reason. **Merge** is
the lifecycle's own button under the *Merge* step: the strategy is its caption
(a menu only when the code host offers more than one, starting on the project's *Merge strategy*
setting, `merge` by default), and the confirmation says in words how the branch lands and whether the
branch on the code host goes with it (on by default). A merge cannot be taken back, so it passes the
same Publish gate; when it goes through the workstream becomes *merged*, the card stays — it is the
record of how the work landed — and the last step, **Clean up**, asks what next: pull the default
branch, delete this workstream's checkout and branch, return to the default branch — every step
checked, each saving what is here first. Deleting the checkout ends what still stands in it — its
harnesses terminated, its shells closed, its agent sessions aborted — and the step says how many
before you confirm (*1 harness terminated and 2 shells closed*); the summary after says what went.
The settings *Cleanup after a merge* and *After a merge* make it silent or turn steps off — silent,
a toast says what was done — and while a session runs on the primary the pull and the return wait,
with the reason. A pull that stops on conflicts leaves you on the primary's Changes view with the
Resolve card and the conflict document.

**Your machine's sign-in comes first.** If the GitHub CLI (`gh`) or the GitLab CLI (`glab`) is
installed and signed in, the platform runs pull requests, reviews and merges *through it*, as the
account it is signed in as, and asks nothing else of you — the same way your terminal already
works. When the CLI cannot answer — not installed, not signed in, signed in as some other account,
or a thing it has no command for — the host's API is asked with a **token**: from
`BISA_GITHUB_TOKEN` (`_GITLAB_`, `_BITBUCKET_`), from the one you store under **Settings → Git
& code hosts → GitHub · GitLab · Bitbucket** (a 0600 file in the workspace's identity folder, or
the OS keyring when chosen), from the CLI's own token, or from **git's own credential helper**: if
`git push` over HTTPS already works on your machine, the same credential is asked of git
(`git credential fill`) and used here. Bitbucket Cloud has no CLI, so it is always the token — with
your username beside it, since that is how Bitbucket signs a request. Your SSH keys are never read
or used; nothing of the platform's opens the CLI's own files or your keychain — the CLI is *asked*,
once, for one command. No token is ever shown, logged, journaled or sent to the desktop. Each
host's Settings panel says where you stand, offline: *GitHub CLI 2.63 · signed in to github.com as
@you*, or *not signed in* with **Authenticate** — which opens a terminal tab running the CLI's own
browser sign-in, and the panel updates when you come back — or *not installed* with the install
commands to copy; then every account the platform holds, *connected as @you · repo, workflow* when
you press **Check**, what git itself does for pushes over HTTPS, and *Add a token* with the scopes
the host needs and a link to create one. Storing a token checks it first, so a token the host
refuses is never kept.

**Conflicts.** A merge, rebase or cherry-pick that stops on conflicting content is the Resolve
card's and the conflict document's (§Branches, tags, remotes — and Safety): the files first in the
Changes list under *Conflicted*, each resolved conflict by conflict, *Mark resolved* saving the
result with a compare-and-swap and staging the path, and *Abort* on the card restoring what the
recovery ref saved.

```sh
bisa workstream pr <id> --title … --body … | pr-view | pr-checks | pr-review | pr-merge
```

Design: [ide/08 — Code host](../architecture/ide/08-code-host.md).

## Agents in the workstream

The right panel's **Agents** tab is a **conversation** with the agents about this checkout — the one
the checkout is on; **Conversations** in the bar lists the project's — its own and every checkout's — live, or
put away with its *Archived* switch — to pick one up or start another, the way a goal's tab, a
workflow's Agent pane and the drawer beside a note or a drawing list theirs — the same surface everywhere
([the desktop](the-desktop.md#conversations))
— not a second chat, with a message's usual verbs: React, Reply and Copy on
hover, *Copy link* and *Retract* (your own) in the ⋮ menu and on right-click. What an agent makes
here to be looked at — a page, a chart, a report — renders live under its reply as an artifact,
opens beside the conversation or as a centre tab, and *Open in the IDE* opens the file it wrote in this
checkout ([Artifacts](artifacts.md)). Mention an agent and it answers; name nobody and the
**default agent** takes it — the chip at the head of the address tray says who (*→ Reviewer*), and
its menu picks another for this project (the setting `agents.default`; the General Agent unless you
say otherwise, and again whenever the one you named cannot answer); team handles work here as
everywhere. The one agent you cannot reach here is the **Workflow Agent**: it designs workflows for
goals, so it is offered nowhere on a checkout, a typed `@Workflow Agent` addresses nobody, and naming
it as the project's default reaches the General Agent instead — ask it in a goal's thread. The agent
runs **in the workstream's checkout**, and the pane's one bar shows exactly
what it is told about where it is — the project and the goals it is attached to (`↳ Dark mode`), or
the project alone — beside a chip that sums the turns up (*2 working · 1 waiting on you*). An attached project's
agent is told that no workflow step is judging this session and no result is owed here; the agent
of a project attached to no goal hears nothing about goals at all, and the bar says nothing either.
Attaching or detaching in *About* changes the frame on the next message.

**Context is chips, and chips are the whole contract.** Type `@` and a path — `@src/ma…` — and
pick the file: the words stay in your message and the file becomes a chip (delete the words and
the chip goes). Drag a file from the explorer or a hunk from a diff onto the pane, press `⌘⇧A` with
text selected in the editor, use the box's paperclip for the open file, an open tab, the selection,
the active terminal's last lines or files from disk, or the terminal strip's *Send to agent* — each
becomes a removable chip above the composer. Nothing is injected that is not a chip: what the agent
sees, you saw and could remove — on the first message and on every one after it. Chips belong to
the workstream you attached them in and wait there, with your draft, until you send or remove them.
Sent messages keep their chips in the transcript, and the bound is 64 KiB of chips per message,
said before you send rather than after — when you are over it only *Send* is closed, not the box.
`Enter` or `⌘Enter` sends, `Shift+Enter` breaks a line, and while an agent is working here a line
under the conversation says who is doing what — *Reviewer — running Bash* — and the one **Stop** sits
beside *Send*; an agent that is idle between turns is not working, so *Send* offers no Stop for it.
Once the agents have written to disk a second line says *3 files changed in this checkout* with
**Attach** — the changes land as hunk chips on your next message, each one readable and removable;
nothing about the tree reaches an agent until you ask — and **Review**, which opens the review bar
for this conversation: that is where you Keep or Undo an agent's edit, by hunk, by file or by the
whole turn ([Reviewing agent changes](#reviewing-agent-changes) below).

**The reply arrives as it is written.** Ask, and the agent's answer streams into the conversation
at a steady pace, at the foot of the timeline under its name, with a caret at the end of what is
still coming; a long answer never stutters as it grows. Above the words is its **thinking** — the
reasoning the harness wrote before it answered — *Thinking… 4s · 800 chars* while it thinks, with a
line of it moving even when folded, *Thinking · 1.2k chars* once done. While the agent reads a file
or runs a command, one dim line under its words says so — *running Read src/app.ts*. When the
answer is complete it becomes a message like any other, nothing blinking on the way, its thinking
kept above it. **Thinking: Auto ▾**, at the right of the status line under the timeline, is the
control over every block: **Auto** opens the thinking while it is all the agent has said and folds
it the moment the words begin; **Shown** keeps every thinking open; **Hidden** keeps every thinking
folded to one line. The choice is remembered; one block can still be opened or closed on its own. A
harness that reasons in the open — Claude Code, Codex, OpenCode, GitHub Copilot CLI, Grok Build,
anything speaking ACP — shows its
thinking; one that does not shows its words alone. Conversations grow long: the harness keeps and compacts
its own context, so nothing here asks you to.

While an agent is working here the line under the conversation says who is doing what — *Reviewer —
running Bash* — and the one **Stop** sits beside *Send*; the chip on the bar sums the turns up
(*2 working · 1 waiting on you*). The turns of a conversation are not a list you manage: a worker on
a step, or a harness you opened in a terminal, is on the rail instead, with its transcript and its
verbs; *waiting on you* raises an OS notification when `notifications.asks` is on (Settings › System ›
Notifications) and badges the dock with how many wait. A failure raises one too
(`notifications.failures`); *done* only when `notifications.done` is on. A session started from a goal is answered in the Inbox. A conversation
about a checkout has no goal to escalate to, so its agent asks in the conversation instead: an **ask
card** at the foot of the timeline, with *Allow once*, *Allow for this conversation* and *Deny*
([Reviewing agent changes](#reviewing-agent-changes) below). The same card asks about **what the
agent is about to read from outside**: a page it opened in the platform's browser, a review from a
code host — screened first, and held when it reads as instructions rather than content.

**Conversations** is the bar's one button. A checkout is on one conversation at a time — the one
you chose there last — or, with none chosen yet, the list of them, a click from any — and the button turns the
pane into the list of them: every conversation of the project, this checkout's, its siblings' and
the project's own, newest activity first, each row naming what it is about, a search over what was
said in them, an *Archived* switch, and *New conversation* to start another — and, with none at all, the
same empty state every surface has, one button, the same words. Pick
one and it opens in the pane in place of the list — you are on it now, and whatever you write
continues it, the agent picking up where that conversation left off, working in the checkout that
conversation is about; press the button again to go back without picking. Everything you hand to an
agent from the editor, a page or a terminal goes into the conversation you are on when it is about
this checkout or the project, and starts one about this checkout otherwise — a hand-off carries this
checkout's files, so it never lands in another checkout's conversation (the toast says so). The view you left — the list or the conversation — is where the pane opens
next time ([the desktop](the-desktop.md#conversations)).

**From the editor.** Select text and a small toolbar floats above it: **Ask** a question about the
selection, **Edit** it (the accented one — the agent writes the file, the document reloads and says
so when the agent finishes, and you Keep or Undo the change in the review), or **Attach**
it to Agents as a chip and write the message there;
the agent it posts to is on the toolbar, remembered. `⌘I` and `⌘⇧I` do the same from the keyboard.
Dismiss it and it stays away until you select something else.

**From a rendered page.** Press the **wand** on an HTML page's rendered or split view and the page
becomes an inspector: hover an element and it is outlined with its tag — an edge only, so you still
see the element — click it and a small box beside it —
in your theme, whichever the page's own colours — asks *What should change here?* The box shows where the element sits — *html › body › main › …* —
and each crumb is a door: click one to annotate that container, the body or the page itself
instead; a click on the page's margin picks `html`, on the body's own area `body`. While the box is
open the page stops picking: hovering outlines nothing and a click changes nothing, until you
**Add** the note or close the box — the outline stays on the element you chose, scrolling keeps the
box with it, and a crumb still works and keeps what you have typed. Each answer
becomes a numbered badge on the element and a line in the tray under the page — point at as many
as you like, drop one with its ×, click an annotated element again to change its note. Write a word
for the agent if you want, then **Send**: it goes to the General Agent unless the chip beside the
button names another — pick one there if you want — and it edits the file for every annotation, with each element and its change in front of it
as a chip; you Keep or Undo the change in the review, and the Agent pane opens so you can
follow. The page follows the agent: when it finishes, the page reloads and a toast says so — or
that the agent failed or was stopped. If you had edited the source meanwhile, the page keeps your
text and a banner offers *Reload the page*, *Keep mine* or *Review*. **Attach** puts the same chips
in the Agent pane's tray instead, for a message you want to write there. `Esc` leaves the
inspector; the badges stay until you send, attach or clear. Editing the source in *Split* keeps
them — an element the page no longer has says so in the tray, and its chip keeps the element as it
was. The same wand sits on the browser tab's bar for any page the tab shows — the checkout the
Browser menu serves, your dev server, the web — the same box beside the element, the same crumbs,
the same keys — and *Annotate the page for an agent…* in the tab's right-click menu and the Browser
button's menu turns it on too; a page served from a file of the checkout sends file chips the
agent edits, any other page sends the page's URL with each element. A page an agent posted as an
artifact is its own, and the wand is greyed on it with the reason. From the palette, *Attach: the open file / the selection / the terminal's
last lines / this work item* add chips without the pointer. `bisa agent-context`
prints, from the same source the MCP server serves, every tool each kind of session is handed and
the framing a conversation's turn reads.

Design: [ide/09 — Agents in the IDE](../architecture/ide/09-agents-in-the-ide.md).

### Reviewing agent changes

**Send, or stop.** The button beside the box is **Send** until you press it, then **Stop** while the
agent works — one button, never both — and **Send** again when the agent rests or you stop it.
`Enter` still sends a follow-up while the agent works.

**Choose a mode.** The mode picker beside the composer opens as a dropdown — or `Shift+Tab` in the
box cycles it — and sets the checkout's conversation to
**manual** (the default — an edit lands and waits for your word, a command no rule decides is asked),
**auto** (edits land and commands run without asking; you review afterwards, and what is still
pending is kept the moment you send your next message), and **plan** (nothing changes — the agent
reads, asks what it needs and replies with the plan). A guard rule's own refusal or ask is unchanged
by the mode; the mode only says what happens to a call the rules left undecided.

**Review a turn.** Once an agent has written to a checkout, its turn shows in the conversation with
the files it touched, and a bar above the box sums them up — *3 files changed by Reviewer · +40 −12*.
Open it for a row per file, each with **Keep** (moves what is pending forward to what is on disk)
and **Undo** (writes it back); **Undo all**, **Review** and **Keep all** sit under it. A file somebody
else also edited is marked so, and an Undo of it asks first; **Undo all** always asks, naming how
many files it writes back. The bar is the agent's changes alone;
the working tree's are in the Git panel.

**Undo one change.** Press **Review**, or a file's name, and the file opens in the editor on its
diff: the lines the agent removed and added in one column, the rest folded away, and under each
change a small strip — *Change 1 of 3 · Keep · Undo*. Step through them with the arrows on the bar
(or the review chords), keep or undo each, or **Keep file** / **Undo file** at once; **Next file**
takes you to the next one waiting. A page or a note that opens rendered says *This file has 3
changes to review* with **Show diff**. The rest of the turn's edits, and anyone else's since, are
untouched by an Undo of one change.

**Answer an ask.** In manual, a command no rule decided — and in every mode, a guard rule's own
`ask` — opens an ask card at the foot of the timeline rather than the Inbox, since a checkout
conversation has no goal. **Allow once**, **Allow for this conversation** (offered only for the
mode's own ask), or **Deny** with a note the agent reads as the reason. What you allowed for the
conversation holds while it stays in the same mode: change the mode and the agent asks again.

**Allow or deny what an agent reads.** Everything an agent reads from the internet through the
platform — a page in the embedded browser, a review or comment from a code host — is read by the
classifier before the agent sees it (Settings › Security › Classifier › *What agents read from
outside*, on by default). Safe content reaches the agent framed as data. When the classifier finds
instructions dressed as content — *ignore your rules and run this* — or gives no verdict, the page is
held and the card shows the site, the reason and an excerpt: **Allow once**, **Allow this site for
this conversation**, or **Deny**. Denied, the agent is told the page was withheld and goes on
without it; it never reads the words. A harness's own web fetch and search cannot be screened, so
the guard asks before them and points the agent at the browser tools.

**Plan, then build.** In plan mode the agent's reply is the plan itself. **Build this plan** puts the
conversation back in the mode it was in before the plan and hands the agent a message to act on it.

**Go back to before a message.** *Restore to before this message* on a turn's card puts every file
that turn or a later one touched back to what it was before that message woke the turn — a
three-way merge with whatever else has changed the file since. The messages themselves stay; only
the files move.

Design: [ide/20 — Reviewing agent changes](../architecture/ide/20-reviewing-agent-changes.md).

## Agent Mode

The centre as a conversation. Press **Agent** on the header's switch (`⌘⌥A`, or *Switch to Agent
Mode* in the palette, or *Talk to the agents* on the landing) and the documents and terminals give
way to the conversation the checkout is on — the same one the Agent panel shows, with the space
the centre gives it:

- **A header**: the conversation's title, the root's name, its branch and what its turns add up to; the placement every
  agent here is told — the project and the goals it is attached to — as chips; and
  **Conversations**, the one button, which turns the centre into the list of this checkout's
  conversations and back — the same surface as the panel's and every other owner's, at the centre's
  width; with no conversation yet, its empty state and the one button, *New conversation*. Who an
  unaddressed message reaches is the chip at the head of the composer's address tray, as in the panel.
- **The reply as it is written**, streaming into the timeline with its thinking folded above it,
  and the same *Show thinking* toggle under the timeline as in the panel.
- **The composer**: `@` offers every enabled agent and **every enabled team** — a team handle wakes
  its members — and the root's files, which arrive as chips. The paperclip attaches documents and
  photos; so does dropping them on the box or pasting a screenshot. The attach menu adds the open
  file, an open tab, the editor's selection, the checkout's changes (one chip per hunk, within the
  64 KiB budget) or the active terminal's last lines; a file dragged from the explorer or a hunk
  from a diff lands on the surface as a chip, and a row in Git › Changes has *Attach to the agent*.
  `⌘Enter` sends.

The rules are the conversation's: the Workflow Agent is not reachable here, a message that names nobody
reaches the project's default agent, and what you attach as chips is exactly what the agents see.
Switch back to **Project** and the tabs are where you left them; the draft and the chips stay with
the conversation either way. A browser tab open here follows the switch: in Agent Mode it shows in
the **Details pane** on the right — where a page an agent opens beside a goal or a workflow shows —
with the conversation in the centre; the Browser button, an agent's `browser_open` and a link put a
new tab there too while the centre is the conversation or the Board; back in **Project** the tab is
the strip's active tab again and the pane's Browser occupant closes. Close the pane by hand in Agent
Mode and it stays closed until you open it or return to Project. Opening a document from the conversation — a path in a message, a chip on a sent
message, *Open … in the IDE* on its card — switches the centre to **Project** with the file in
front of you: a document lives among the tabs, and one opened behind the conversation was one
nobody could see. While the centre is the conversation the rail's **Agent** tab is dimmed
— the conversation is already in front of you — and every door that asks for it (the tab, `⌘⇧M`, a
terminal tab's *Show in the Agent panel*, the pet) puts the caret in the composer instead.

## Language intelligence

Open a Rust, TypeScript, JavaScript, Python, Go or C/C++ file and the node starts that language's
server — `rust-analyzer`, `typescript-language-server`, `pyright-langserver`, `gopls`, `clangd` — if it
is on your login shell's `PATH`; nothing is installed for you, and the editor's status chip says
*rust server running*, or *no server installed* with the install hint. Your own servers go in
`lsp.servers` (Settings → Project IDE → Language servers) as `{language, command, args}` and win over
the presets. One process per project and language; it stops `lsp.idle_ttl_secs` after the last
document of its language closes, and a server that crashes three times in a minute stays down — with
the reason and a *restart* link — rather than being forked forever.

What you get: diagnostics in the gutter as you type (the server sees the buffer, not the saved file),
hover, go-to-definition (into another file too), find references, the document's symbols, `@` in the
palette for workspace symbols, and formatting on save when `editor.format_on_save` is on. The editor
only ever sees paths relative to the root; a location outside it (the standard library, a dependency)
is reported as not openable rather than as an absolute path.

Design: [ide/10 — Language intelligence](../architecture/ide/10-language-intelligence.md).

## Quick open

`⌘P` in a workbench is quick open: every file under the root, fuzzy-scored as you type — the
basename, word starts and runs count for more, a shorter path wins a tie — above the projects,
workstreams and work items the switcher always offered. Each section is capped so one cannot flood
the list. Prefixes narrow: `>` commands (also `⌘⇧P`), `:42` jumps to a line in the open document,
`#` work items, `@` symbols once a language server is up. The path index is fetched once per root
(`.gitignore` honoured, bounded), shared with the Files tab's name search, and patched from the
watcher's frames, so typing never waits on a walk. Every chord is in
[`reference/keymap.md`](../reference/keymap.md), and every chord can be changed in **Settings ›
Keymap** — grouped by where it is live (everywhere, the IDE, the tab strip, the Files tree, the
editor, a terminal), with a filter over names, ids and chords; *Record* takes the next chord you
press, `Enter`, `F2` and `Delete` included, and refuses one another command already holds there by
name. The preset and your changes are Machine settings (`keymap.preset`, `keymap.overrides`).

Design: [ide/12 — Search and quick open](../architecture/ide/12-search-and-quick-open.md),
[ide/15 — Keymap](../architecture/ide/15-keymap.md).

## Diagrams

Mermaid is the one diagram language. A `.mmd` or `.mermaid` file opens with **Preview · Split ·
Source** as three glyphs; a ` ```mermaid ` fence in a Markdown document renders inline in *Rendered* and *Split*,
and a fenced block in an agent's message renders through the same viewer. The preview follows the
buffer as you type, debounced, and the last good render stays while a new one parses. **An error
names the line in your file** — the fence's offset is added before it is shown — and clicking it
moves the cursor there. *Copy SVG*, *SVG* and *PNG* export through the desktop's save dialog.

Design: [ide/11 — Mermaid](../architecture/ide/11-mermaid.md).

## Terminals

A terminal is a **tab in the workbench's centre**, beside the files: the shells rooted in the
workstream you are looking at, in the order you opened them. The panes you can see draw on the GPU
(WebGL, eight at a time, the focused one first); a pane out of sight for a moment hands its slot
back. A URL or a path a build or a harness prints opens with ⌘-click — the path in the editor at its
line, the URL after a card asks. A declared chord a shell cannot mean —
`⌘\` to split, `⌘J`, `⌘⇧E` — works from inside the shell; copy, paste, find and `⌘R` stay the shell's. The shells themselves are drawn by a
layer mounted outside the routed screen and positioned over the centre, so a build survives
navigation. Every shell everywhere is one click away in the rail, under the workstream it stands in.
Beside it, **Browser** — one button for the platform's embedded browser and everything that puts
a page in it. One click opens a tab on the newest folder the IDE is serving, else on a port a
shell here opened, else blank with the address field ready (or on the home page Settings ›
Capabilities › Browser names). Its caret lists *New tab*; **From folder…** — a picker over the project's own tree: **Root**
first, the checkout whole, then every folder under it, the ones your `.gitignore` names included,
since a `dist/` is what one most often serves. Type a few letters to find a folder (`dbu` finds
a folder named *docs/build*), walk the rows with the arrows, open a folder with Right; a folder with an
*index.html* says so, and one already up says *serving :4173*. **Serve and open** — or Enter, or a
double-click — serves it on a port of this machine and opens it; a folder already served opens its
server instead of starting a second. The picker opens on the folder you served there last. Then every
server up with *Open :4173* and **Stop** (*Stop :4173* when several are up); the ports; the tabs
already open here; and *Annotate the page for an agent…* when the tab you are on shows a page an
agent can edit. Nothing in it runs a command: that is the Terminal button's, below. `⌘⇧R` opens the
newest server, else runs the project's run command, else opens that picker — so `⌘⇧R` then Enter
serves again what you served before — the item it means
shows the chord, in whichever menu it stands. A tab rides the centre's strip with the terminals in
Project Mode — in Agent Mode and Board Mode it rides the Details pane beside you, and a switch
carries it — and comes back after a restart (the tabs you can see; one an agent keeps out of sight ends with the window). The browser browses as the machine's does: any http(s) page, a bar whole on
every tab — back, forward, reload (*Stop* while a page loads), the address field, the wand, the
camera — with what the tab lacks greyed and saying why, back and forward following the page's own
history, the page's title on the tab, a link that wants a window opening a tab beside. `⌘L` is the
address field, `⌘[` and `⌘]` back and forward, `⌘R` reload, `⌘T` a tab beside — pressed in the bar
or inside the page. On any page the wand annotates elements for an agent exactly as on a rendered
file (*From a rendered page* below); the **camera** on the bar copies a screenshot of the page or
saves one where you choose; and every agent browses through this same browser, where you see
what it sees — the same tabs show in the **Browser pane** beside any other screen
([the desktop](the-desktop.md#the-browser-pane)). `Ctrl+Shift+\`` is a new browser tab here,
`⌘⇧L` the Browser pane. A folder in Files offers *Serve this folder* too.

The **Terminal** control on a workstream, work item or goal opens a login shell there; its caret
lists *Shell*; on a workstream the project's **run command** — *Run `npm run dev`*, set under About
› Settings › Workstream scripts beside the lifecycle scripts and approved on this machine like
them, opened in a terminal you watch (the tab reads *run · npm run dev*); a command this machine
has not approved opens that card instead, and a project that sets none shows no item — then every
harness the node found, with *resume* when that harness has run in that place before
and *fresh session* beside it — every harness continues its latest session in that directory with
its own form (Claude Code, Codex, OpenCode, pi, OMP, GitHub Copilot CLI, Grok Build, Goose and
Cursor alike), and a resumed
harness **starts**: once it has drawn its prompt the tab types *Continue where you left off.* for
you, unless you type first; **Start a resumed harness** under Settings › Project IDE › Terminal
turns that off, and the session then waits at its prompt. What the tab then tells the rail is the
harness's to give: Claude Code, Codex, OpenCode, pi, OMP and GitHub Copilot CLI each report what
they are doing — Copilot through a plugin the platform mounts for that one launch, which waits *in
Copilot's own words* when it asks you something — and Claude Code and Copilot CLI ask the guard
before a tool runs; **Grok Build opens as a plain terminal**: its terminal takes no hook for one
launch, so nothing is reported and its own prompt is the only one. A project's `terminal.default_harness`
setting makes the plain control open that harness instead. From the palette, `Terminal: Claude Code here` and friends open the same
things without a pointer.

**Splits.** With a terminal tab active the strip's controls cut the focused pane to the right or
downwards and open a shell in the new half. Once split, each pane gets its own tab strip; drag a tab
onto another pane to move it there — it moves, it is not remounted, so the shell keeps running.
`⌥←→↑↓` moves focus between panes. *Close pane* folds a pane's tabs into its neighbour; nothing is
terminated.

**Scrollback that survives a restart.** Two seconds after output stops, on exit, and when a tab goes
away, the buffer is serialised and kept by the desktop shell at `run/terminals/<key>.scrollback` in
the workspace — local state, never synced, capped at 4 MiB and by `terminal.scrollback_lines`. On
the next start every tab comes back in its pane, the checkpoint is replayed first, a divider line
says where the old run ended, and a fresh shell starts below it. `terminal.restore_scrollback` turns
the replay off.

**Type a harness into a shell and the tab becomes it.** Open a plain shell, run `claude` (or
`codex`, `omp`, any harness the catalog knows, an npm-installed one included) and within two
seconds the tab wears that harness's mark and name, the rail's row does too with *Terminate* as its
verb, the footer counts a harness, and closing asks as for a harness. Quit it and the tab is a shell
again. What the harness is *doing* stays unknown — a harness started by hand does not report, so it
has no state and no sub-agents; open it from the terminal menu for those.

**Liveness has three values.** A tab is *live*, *exited (n)* — the danger tone when non-zero — or
*unverifiable*, with the reason on hover: restored and not yet respawned, or a spawn that failed.
Loss of contact is never recorded as an exit. **Find in the buffer** is `⌘F` — the same bar a rendered file shows, with regex and match case. A dead shell keeps its
tab and its buffer; **Restart** spawns a fresh one in the same tab. Nothing respawns on its own.

Design: [ide/06 — Terminals](../architecture/ide/06-terminals.md).

## Mobile Development

A checkout that holds a Flutter app gets a **Devices** button beside Browser, once Settings ›
Capabilities › Mobile Development is on. Its click does the one thing most worth doing — shows the
device the app is running on, runs on the device that is up, boots a simulator or an emulator, or
opens the setup — and its menu lists every device this machine can reach with what it can do now:
*Run on*, *Show*, *Stop the app on*, *Boot*, *Shut down*; a phone that is offline is said, not
offered; *Check the setup…* opens Settings.

*Run on* opens a terminal labelled *flutter · <device>* with `flutter run` on that device — the
line is the node's, run in the checkout — and the device's **document** beside the code: a lone
pane is split and the device takes the new half, so the code stays where it was. The document
mirrors the device's screen a few times a second while the window is in front; its bar runs the
app, **hot reloads** (`r`), **restarts** (`R`) and **stops** it (`q`) through that terminal,
brings the **Simulator window** forward — the mirror is for watching; you touch the app in the
Simulator or emulator window — and captures the screen: *Copy*, *Save…*, *Attach to the Agent
panel*. A device that is shut down shows a boot door instead. The tab is saved with the layout.

The wand marks the screen for an agent: drag a rectangle over the mirror — or click it for the
whole screen — and say what should change there. The screen is captured at that moment, so the
picture is the one you marked; each capture is a numbered badge on the mirror and a line in the
tray, and the tray's doors are the annotation tray's — **Send** to the agent the chip names, as an
edit in this checkout's conversation, or **Attach** to the Agent panel and write the message there.
The agent gets the picture as a file, the rectangle in device pixels and your note, and answers with
a screenshot of the same spot once it has hot reloaded.

Agents have the same devices through tools of their own — `mobile_development_status`, `mobile_development_devices`,
`mobile_development_boot`, `mobile_development_screenshot` — and run the app themselves in a terminal; a build going to
the App Store or Google Play is asked of you by the guard first. The **Mobile Developer** in the
catalog knows Flutter, the devices and both stores ([agents and teams](agents-and-teams.md)).

## Links and paths

Every path and every URL you can see is a door. An agent's reply that names `src/main.rs:42`, a
compiler's error in a terminal, a rendered `README.md`, a step's output, a chip on a sent message,
a channel, the Pulse — click the path and a small card opens at the pointer:

- **Open src/main.rs:42** opens the file in the editor at that line. When more than one checkout
  holds the file, the card offers **Open in …** for each. ⌘-click skips the card and opens at once.
- **Reveal in Finder** (the file manager's own name on your platform) shows the file on disk.
- **Copy the path** copies it as written.

A path is looked up in the checkout you are in first, then the project's other checkouts — in a
channel or the Pulse, every checkout on this machine. `~/Projects/app/README.md` opens in the IDE
when that folder is a checkout here; when it is not, it opens as a loose document, or is revealed
in the file manager. A path nothing here holds can be copied.

A URL shows the site's name and the whole address, with **Open in Bisa's browser** (when the
embedded browser is on), **Open in the machine's browser** and **Copy the URL**. Nothing opens on a
plain click, the app never leaves its window, and only `http` and `https` addresses open; anything
else can be copied. A redacted secret is never a link.

In a terminal or a harness session the same doors answer to ⌘-click, since a plain click selects
text. A relative link inside a rendered document — `./notes.md` — opens the document it names
without a card.

Design: [ide/17 — Links and paths](../architecture/ide/17-links-and-paths.md).
