# The desktop

Bisa's collaboration surface. Every part of it hangs off goals and the workflows that do the work:
conversations spawn goals, workflows run them — or run on their own in the workspace — projects are
attached to them, the inbox is what they need from you. Humans
and agents use the *same* surfaces through different doors — a person through the app or the CLI,
an agent through its MCP tools. A message from your Developer and a message from your teammate are
the same kind of event, signed by different keys.

This is a tour, screen by screen, with the CLI equivalent beside each surface. The **Projects**
destination — the IDE — has a tour of its own, [`the-ide.md`](the-ide.md).
[`desktop/README.md`](../../desktop/README.md) is how the app is built and the rules its code keeps.

## The shape of the window

| Region | What it holds |
|---|---|
| **Top chrome** | the sidebar's collapse / expand toggle · back / forward · the screen's title · a live-status chip · **Search or jump to…** (`⌘K`) · **you**, at the far right — your identicon, whose menu opens Identity, Settings and About Bisa |
| **Sidebar** | seven destinations **in the order you put them** — drag a row to a new place, or an icon when the sidebar is collapsed (from the keyboard: Space lifts a focused row, ↑ ↓ move it, Space drops it); the order is remembered on this machine, the palette's *Go to* follows it, a machine that has not closed the app yet opens on the first row, and *Reset order* is under Settings › Appearance — then live sections for channels, direct messages and the workspaces you are a guest of; a *Node unreachable* card at the bottom only while the node is. The **Inbox** row wears a badge — everything not yet dealt with there, accented while any of it needs you. **Collapsed** (the top chrome's toggle), the sidebar is a rail of icons: the same seven doors, then Channels and Messages with their unread summed, each named on hover, the Inbox count in its icon's corner — never out of sight, *9+* past nine, the exact numbers (*12 need you · 3 unread*) on hover; the same toggle expands it again at the width you left it |
| **Footer** | the window's read-outs — a harness account's usage on the left; the open terminals and running harnesses, the ports, the node, the machine's CPU, GPU, memory and disk, the network, and the two overlays' switches on the right — each a door to its overlay ([§The footer](#the-footer)) |
| **Main** | a conversation, or a screen |
| **Aux pane** | one slot on the right: a thread, an agent session's transcript, a profile, the goal inspector, or an artifact with the conversation's gallery under it ([Artifacts](artifacts.md)) |
| **Terminal layer** | every open shell and harness, drawn over the workbench's centre while a terminal tab is active there; mounted outside the routed screen, so a build survives navigation |
| **Notes overlay**, **pet**, **addons** | float over the app; an addon's window ([Addons](addons.md)) is a sandboxed page in a box you drag, at the pet's height. The notes panel is a solid sheet on every theme — on Glass it is frosted and nearly opaque, so the screen behind it never reads through what you are writing |

### The footer

One row of read-outs, every one a fact a model decides and a test holds (`desktop/src/shell/*Model.mjs`).

**Usage** (left) — what a harness's account has left, on one compact line: the harness's mark and name, then each window as its word, a small bar and a percentage, the first window's reset right after it — *5h ▰▰ 24% · resets in 2 h 10 min · Weekly ▰▱ 41% · Fable ▱▱ 9%* for Claude Code (its five-hour window, its week across every model, and the per-model week the plan carries: *Fable* on Max and premium seats, where up to half the week may go to Fable; on Pro, Fable runs on usage credits and there is no Fable meter); *5h* and *Weekly* for Codex; every provider Oh My Pi is signed into; OpenCode's Claude Pro/Max sign-in. Every other reset on hover, the credit balance in the overlay rather than on the line, and a refresh; a harness that reports none — pi, GitHub Copilot CLI, Grok Build, Gemini CLI — says so in its own words. A re-read that fails keeps the last numbers for an hour, dimmed, with the reason on hover. Click it for every installed harness with its usage and pick the one that stays here — remembered on this machine; nothing when no harness is installed (`harnessUsageModel`, `footerUsageModel`). It appears on its own when the app opens and reads again by itself — when the node comes back, and shortly after a read that failed — and *Refresh* also re-checks which harnesses are installed, so one that was slow to answer at launch shows up without a restart.

**Sessions and ports** — the caret and language of the editor you are in; the **open terminals** and **running harnesses** — the same rows the Project IDE's rail draws (a harness you opened in a terminal is one row, the harness's, once it reports; a restored tab counts before its terminal has mounted; an exited tab is listed dim and not counted), each saying where it stands under its name — the project and the workstream, *Bisa › feat/a*, *Bisa › primary*, or *this machine* for a sign-in shell — and each a door: a terminal row opens its tab in the place it is rooted at, a harness row its tab or the Agents pane on its workstream. Every open port opens in the browser or stops, from whichever goal, project, workflow, terminal or harness started it; the ports list opens with a line saying how many and where they were started, each port with its pid; when the machine's ports cannot be scanned the list says so with the reason and keeps the last answer (`footerSessionsModel`, `statusBarModel`, `portsModel`).

**The node** — its glyph with a dot in the connection's tone and the sentence on hover; on a click what it is: connected or not and at which address, run by this app (its pid, how long up, how many restarts) or by you, its version beside the desktop's with a caution when they differ, whether the engine is paused, the sessions live, the socket and the address it listens on, the data and logs folders, its relays — one row each with its state, *off* while the switch is off, **Check relays** trying every one once, a door to Relays & sync — the wire's counts (people hosted, hosts joined, published, ingested, direct sessions, the last catch-up) and its own CPU and memory, with a door to Settings › Node (`nodeStatModel`).

**Resources** — the machine's **CPU**, **GPU** (on a machine that reports one), **memory** and the platform's **disk** footprint, each a glyph with its value and a sentence on hover — *CPU 34% · 10 cores · load 2.1 1.8 1.5 · Bisa 12%*. A read that fails keeps the last reading and the sentence says so — *the last reading is kept*, with the reason. Click one for its breakdown: the host's figure, a bar read left to right — what Bisa takes (the desktop app, the node, every harness session, every terminal, each with its whole process tree, each its own colour), then what the rest of the machine takes, then what is free — with a legend beneath saying each in the metric's words, *Bisa 2.6 GB · 16% · Machine 6.8 GB · 43% · Free 6.6 GB · 41%* — and rows by the dimension you pick from a row of tabs, each its glyph and its whole word, the panel wide enough for all of them, remembered per read-out on this machine: **platform**, **goals**, **projects**, **workflows** (a goal's run's, or a run in the workspace's), **harnesses** (by kind) and **terminals** (by tab) for CPU and memory; **workspace** (the data directory by area), **goals**, **projects** (with their checkouts), **harnesses** (their session records — a transcript is the harness's own file), **terminals** (scrollback) and **activity** (bytes read and written over the last few seconds) for disk. A goal, a project, a workflow, a run in the workspace or a tab in a row is a door. The desktop row counts this window's process alone: the renderer runs under WebKit and is counted there. The GPU has no breakdown — no reader gives one per process without elevated access (`resourceModel`).

**Network** — one word with a dot: *UP* in green while the internet is reachable, *VPN* in green while a tunnel is up and the internet is reachable through it, *DOWN* in red when this Mac cannot reach the internet (a dash before the first read) — with the sentence on hover (how long the probe took, your public IP, the tunnel, the proxy the platform follows); on a click the internet (reached in how long, the public IP, whether DNS answers), the VPN's standing and its tunnel (who runs it, its addresses, all traffic or a split, its DNS, the public IP when all traffic leaves by it), every interface that is up (Wi-Fi, Ethernet, a tunnel — its addresses, its gateway, which carries the default route), this Mac's default route and resolver, the proxy System Settings names, what the platform leaves through and anything that keeps your settings from being in force — read every half minute, again when a network setting changes and the moment the window loses or regains its network, with a door to Settings › Network (`networkStatModel`, `networkModel`).

**Overlays** — the pet and the notes overlays, on or off; and the **Addons** popover: the layer's switch (`⌘⇧X`), every installed addon with its state, its on/off switch and an eye that shows or puts its window away, *Reset positions*, a door to Settings › Addons ([Addons](addons.md)).


The app opens **where it was closed** ([§Where you were](#where-you-were)); on a machine that has
not closed it yet it opens on the **first destination of your sidebar order** — the Inbox until you
move something above it — and a link that names nothing lands there too. Almost everything you can be
looking at is in the URL, so back and
forward restore it and `Esc` closes the pane rather than the screen. The live-status chip is ranked,
never stacked: *node unreachable* outranks *n lists could not be read* (a read of the workspace that
failed — the last values stay up, the chip's title names the lists and the diagnostic log has the
detail), which outranks *paused*, which outranks *N agents writing*. Every command's chord
comes from one model, `keymap.preset` chooses the set, and the whole list is
[`reference/keymap.md`](../reference/keymap.md); `Ctrl` works everywhere `⌘` does. A few keys are
a surface's own and stay outside the keymap — the notes' `Alt+N`, the Inbox's `j` · `k`, the
designer canvas's.

## Where you were

Leave a screen and come back, and it is as you left it. Close the app and open it, and it is where
you closed it. Nothing to turn on: the app remembers, on this machine, for the workspace it is open
on.

**A screen comes back as it was left.** Its tab, its filters and its search, the row you had
selected, the rows you had opened, the step you had picked, whether the Details pane was open and on
which tab, and how far you had scrolled — for the Inbox, the Pulse, Goals and each goal, Workflows,
each workflow's designer and each run, Agents, Teams, Settings, every channel and direct message, the
Project IDE, Notes and Draw. A list draws what it last showed at once and reads again behind it, so
you never come back to an empty screen.

**A section's door returns to where you were in it.** *Goals* in the sidebar — or its icon on the
rail, or *Go to Goals* in the palette — opens the goal you were on, on its tab; pressed again from
inside that goal it opens the list, filtered as you left it. The same for Workflows, Channels,
Messages, Agents and the Project IDE, which has no list and opens on the workstream you were in. A
card or a link that names only the thing opens it as you left it; one that names a tab or a pane is
taken at its word.

**A conversation comes back where you were reading.** A channel, a direct message or an agent's
thread you had scrolled up in opens on the message that was at the top of your view, not at its
newest — with **Jump to newest** at the foot while there is more below. Reading is as it was: a
thread counts as read only once you reach its bottom, so one that comes back above new replies is
still unread.

**The app opens where it closed.** The same screen, tab, selection and scroll; the window at the
size and place it had, on the screen it was on, maximized or fullscreen if it was; the panels, the
open documents with their cursor and folds, the terminals with their scrollback and the browser tabs.
A window whose screen is no longer there opens centred, at the size it had. A design in progress on
a goal's canvas comes back as it stood — without its undo history.

**What is gone is forgotten.** A goal, a workflow, a project, a channel or a conversation that was
deleted, a workstream that was closed, a workspace you left: no door returns there. If it went while the app
was closed, the app opens on its list and says *It is no longer here — back to the list.* Something
archived is still there, and so is its place.

**What starts empty, on purpose.** A dialog or a form opens empty every time, and a secret field is
never kept. A message you were writing is kept until you send it, as are a note and a document; the
files attached to a message not yet sent, the search palette, undo histories, banners and the *Back*
history of the last time the app ran are not.

**Forget where I was**, under [Settings › Capabilities › Desktop](#settings), starts every screen as new: it asks
first, saves what is unsaved, forgets every place and what every screen kept, and opens the window
again on the first destination. Your documents stay, and so does your layout — sizes, docks, the
sidebar's order, the theme, the window's size and place. The messages you were writing stay too,
unless you tick *Also discard the messages I was writing*.

All of it is this machine's: nothing is sent to the node, nothing is synced, and another workspace
opened on the same machine starts with a memory of its own.

## The menu bar icon

Bisa lives in the menu bar, not only in its window. Its mark is there from launch to quit, and it is
alive: **the mark alone means calm** — connected, nothing running, nothing owed. When there is
something to say, **one dot** appears at the mark's lower right, in the system's own colours: **amber**
— something needs you; **green** — agents are working; **red** — the node cannot be reached; **grey**
— connecting, or the engine is paused. Red is a node that answered once and is gone: the window says
*waiting for the node…* and asks again on its own, and when the node is back everything is read
again. Beside the mark, **the number** of things that need you: an
ask or a gate, a harness waiting at its prompt, a person waiting to be admitted — the Inbox's rows that
need you, never the unread and never a notice — gone at zero. The same number is the app's badge in the
Dock and the Inbox's accented count in the sidebar: one number in three places. Hover for the
sentence — *Bisa — Working — 2 agents · 3 need you*. The mark is drawn in the menu bar's own ink,
black or white with the system's appearance, whatever theme the window wears. The menu's lines and
the Edit menu's verbs speak the window's language: the app pushes their words to the menu bar once
it has started.

A **left click** brings the window back — shown, un-minimised and focused, whatever it was doing. A
**right click** opens the menu: what the platform is doing, in one line; *3 need you* — a door to the
Inbox while something is owed; **Open Bisa**; **Show in Dock**; **Quit Bisa**.

**Closing the window keeps Bisa running.** The red button puts Bisa away the way `⌘H` does — the app
hides, and nothing else happens: agents, terminals and the node keep going, nothing is asked and
nothing is saved, because nothing is lost. The window comes back from a left click on the icon, from
*Open Bisa*, from the Dock, from `⌘Tab` — the same doors that bring back any hidden app — and a
browser tab open in the IDE changes none of that. Quitting is deliberate — *Quit Bisa* in the icon's
menu or `⌘Q` — and asks and saves as [Settings › Capabilities › Desktop](#settings) says. Turn the switch off and the
red button quits, as it always did.

**Show in Dock** is macOS's. Off, the Dock icon goes, Bisa leaves `⌘Tab` and the application menu, and
the menu bar icon is the app's only door — the window still comes back from it. The choice is a
setting (`desktop.dock_icon`), so the menu and Settings › Capabilities › Desktop always agree; the app launches with
a Dock icon and loses it a moment later while the switch is off. On other platforms the icon and its
menu are the same and the Dock line is absent; a left click may do nothing where the OS emits none
(Linux), and *Open Bisa* is the way in.

## Where things live

| Destination | What it is |
|---|---|
| **Inbox** | what concerns *you*: what is owed — questions, gates, held steps — and what happened to what you asked for — a run that finished or failed, a script that failed, a listener that could not start its run, a pull request opened — plus mentions and direct channels |
| **Agents** | the roster you have, and what is running right now |
| **Teams** | the teams you have and their members, with the catalog's teams one click away |
| **Projects** | the IDE: every project, the workstreams open on them, and what runs there ([`the-ide.md`](the-ide.md)) |
| **Workflows** | the library — yours and the catalog's templates, each a card with a thumbnail of its graph, searched and filtered in one bar; a goal's designs stay on their goal — the designer, each workflow's **On/Off** switch for the events it starts on, and its runs in the workspace |
| **Goals** | everything the workspace is carrying, one flat list narrowed by filters — the goals put away only when the *Archived* switch is on |
| **Pulse** | everything happening across the platform, newest first, by concept — the whole feed, whether or not it concerns you |

Below those, two live sections: **Channels** — `general` first, then in the order they were made, a
message never reshuffling them — and **Direct messages** — the one that moved last first — each
collapsible and remembering it; their doors open the same lists as pages, searched
([Channels and messages](#channels-and-messages)). There is no goal section, on purpose: **Goals** is the one live list of goals — a
workflow's runs in the workspace are on its **Runs** tab — and a mirror of it in the nav would be a
lifecycle by another name. Skills, MCP servers and the **Catalog** are not destinations: they are where staff
and procedures come from, so they live in **Settings → Library** and **Capabilities**.

## Inbox

The one place for what concerns you: what is owed to you, and what happened to what you asked for.
A responsibility surface, not a firehose: **a row is a thing, never an event** — a goal, a channel,
a direct channel, a conversation, a workstream, a project, a workflow — updated in place, and **a row you read
stays.** A row carries two kinds of fact. An **ask** — a gate, a question, a held `wait` step — is
something owed to you, answered here. A **notice** is something that happened to the thing: a run
that finished or failed, a step that blocked, a budget spent, a design that stalled, a listener
that could not start its run or holds something for you to read, a committer wanted, a workstream script that failed, a pull request opened or
merged, a command the guard refused, a folder a workflow step made, a workflow the Workflow Agent
designed or proposed, a workflow put away — read here, opened there, in the
same words the Pulse gives the same fact. Your own save of a workflow is no notice: news is what
another hand did. That short list is the whole of what a notice can be; the
Pulse keeps everything else. A row is earned by an ask, a decision, a mention, a direct channel, an
unread message or a notice, and kept. One row is a wait rather than a thing kept: **a harness you
opened in the Project IDE's terminal that is waiting on you** — a permission prompt, a question —
has a row under *Waiting on you* (and under *Projects*) for as long as it waits, titled by the
harness and the checkout it stands in, with the same words the rail's raised hand says; **Open the
terminal** lands on that workstream with the tab in front, where the prompt is answered, and the
row goes when it is, or when the tab closes. *Read* is yours — one local watermark per row, never synced,
covering its messages and its notices alike. *Handled* is the workspace's — a signed fact. **The
sidebar's badge** counts everything not yet dealt with: the rows you have not read and the rows that
need you, one number, accented while any needs you and neutral when it is only unread (hover it:
*1 needs you · 2 unread*); reading a row lowers it, answering an ask lowers it. What is
**owed** is the asks alone: a notice never moves a goal card's count or who
holds the ball.

The strip says what each choice would show, one row across the top of the screen as the Pulse's: the source tabs — *Any · Messages · Projects · Workflows · Goals ·
People* — each with its glyph and its count (a row sits under what it is **about**: a conversation
about a goal is under *Goals*, its row saying *about goal …* with the goal's glyph, one about a
checkout under *Projects*; *Messages* is channels, direct channels and conversations about nothing
in particular), and the three buckets — *Needs you · Unread · All* — with
theirs; the words are always drawn (only a window too narrow for them folds the tabs to their glyphs, the word on hover); both stay in the address, so a link to *Unread · Projects* is that
list. *Mark all read* takes what is in view — a few rows at a time, and the list is read again
whatever happened, so a row whose mark failed still reads as new. The list is drawn in three groups — **Waiting on
you**, **New**, **Kept** — newest first inside each. A row wears its state's mark — the accent for
something owed, a plain dot for something new, an opened envelope once read — the glyph of the kind
of thing it is, its title, and one line that says what it wants: the ask in the accent, the newest
notice you have not read in its own tone (a failure reads as a failure), or the last thing said.

The detail pane is the thing itself. Its header names the kind, the holder on a goal, how many
things wait and how many are new, and the door — *Open goal*, *Open in the IDE*,
*Open workflow*. A workflow's row also carries its runs in the workspace — what they finished,
failed or spent, and the questions and approvals they wait on — and while one of them asks, the
door opens that run's page.
Under it, the asks first, as the very cards the goal page's *Your move* band mounts, so answering
here or there is one act; then **What happened** — the notices newest first, each the Pulse's own
row with its door, the unread ones marked, folded past five; then the conversation when the thing
has one — a goal's thread, a channel, a direct channel, a conversation with agents. A workflow has no conversation of its own here: its notices and its door are the pane.
Selecting a row with no conversation of its own reads it after a beat. **A conversation reads
itself as it is shown** — here, in the Project IDE's Agent pane, on a goal, on a workflow, in a
channel, a direct message or a hosted channel alike: the moment you can see its newest message —
the window in front of you, the thread at its bottom — and a beat has passed, the row is read and
the badge drops, so an answer you read where it landed never waits for you here. Scrolled up to
read back, it waits until you come down; a reply that lands while the window is hidden is read
when you return; rows passed with `j` and `k` read nothing.

Anything needing a decision comes in three shapes. A **gate** is Approve, Decline and an optional
reason — an `approval` step, a proposed workflow or amendment, a publish are binary; the *Adopt this
workflow?* gate adds the run's inputs beside Approve, because adopting is starting. A **question**
gets three doors, and only the first belongs to the agent: the options it offered, a free-text box
*beside* them, and **I'm not sure**, which asks for a narrower question rather than stopping the
work. A **held step** — a `wait` step waiting for you to release it — has one verb, *Release*.
Gates opened by another process are reconstructed from the run — a waiting `human` step, a running
`approval` step, a held `wait` step, an un-adopted proposal — so nothing waiting on you can hide
behind a restarted daemon. A pending push or pull request lands here as a `publish` gate. The
screen's own chords: `j` · `k` move, `Enter` · `o` open, `a` focuses the action control, `e` marks
read, `u` marks unread, `Esc` clears the selection — nothing while you type or inside an answer form.

```sh
bisa inbox                       # the same rows, each ask's command and each unread notice under it
bisa answer <goal-or-run> "…" | -o <option> | --unsure
bisa approve <goal-or-run> [--no] --rationale "why"
bisa step release <goal-or-run> <step>    # release a held step
```

## Goals

One flat, full-width list of **goal cards**, newest activity first. Each card is three compact
lines: the status glyph, the title, a one-word **holder** (*you · agents · world · finished · design*),
a *guided* chip while the Workflow Agent designs, its tags and when it last moved; the statement,
dimmed; then the run — every step as a filled chip toned by its state, the current one larger,
ringed and named, a long workflow folded to one line with a *+n* pill — *n of m steps*, the
workflow's name, who is assigned, how many projects it carries, how many things wait on you, a
*queued n* chip while runs wait behind the live one, and,
when the move is yours, an **Act** button that answers, marks done or releases without leaving the
list; a `⋮` beside it, on every card, holds *Open*, the run's verbs — *New run…*, *Restart*, *Stop* —
drawn from the row alone, and *Delete…*, the same retirement dialog as the goal's page (it says what
runs and stops it first). A chip opens the goal's Workflow tab on that step. An archived goal wears its mark and joins the list only while the **Archived** switch is on (`?archived=1`). No sections and no lifecycle buckets: filters narrow
— who holds it, which workflow, tags, text — and they live in the URL, so Back restores a view and a
link carries it. The holder is the same rule the inbox and `bisa status` use
([03 — Workflows](../architecture/03-workflows.md#the-goal-and-its-status)), so the three cannot
disagree. Empty states say *Nothing in flight yet*, *Nothing matches*, or — with the *You* filter on —
*Nothing waits on you*.

Opening a goal is reading its run downward. The **header** names the goal by its words — the title
with the statement under it, or the sentence alone when it has no title, never its id (a long
sentence folds to two lines, whole in its tooltip) — with an arrow back to *Goals* at its left, as
the workflow's header has; then the holder, *n of m steps* and the workflow's name (the step chips stay on the Goals list; the Progress
tab is the run), the run's verbs — *Start
run…*, *Adopt and start…* with the run's inputs, or *New run…* (queued behind a live run), and
*Stop* while a run is live or queued — a Details toggle and the menu
(*Choose workflow…*, *Promote to library* — on a goal with its own design, a copy into the library — *Assign…*, *Projects…*, *Restart* on a goal that ran, *Close goal…*, *Archive goal…* — or *Unarchive* — and *Delete goal…*; *Archive goal…* and *Delete goal…* open the retirement dialog, offered while a run is going too — it says what runs and what stops, and stops it first, [Goals](goals.md#archiving-and-deleting)). Under it, **Your move**:
everything owed to you on this goal — every open question and gate as the very card the Inbox shows,
including one rebuilt after a restart — so answering here or in the Inbox is one act; it folds to a
line past two, and hides when nothing is owed. Then three tabs (`?tab=`). **Progress**, the default:
one row per step — kind, name, state, holder, started and how long — the live ones open, each showing
its output, its error, its work item and the verbs it admits (answer, mark done, release; *decide*
scrolls to the band); while the Workflow Agent is designing, Progress and Workflow show where it
stands, and *Choose workflow…* is not offered — you pick a workflow before it starts (a manual
goal, or a capture with a workflow) or after it stops, never over its head; a manual goal's card
opens the designer instead. **Conversation** is the goal's thread, where the Workflow Agent asks
and proposes — one line per proposal, saying whether it started the run (auto), waits for your
adoption (guided) or left a draft on the canvas (manual). **Workflow** is its run drawn on the
designer's canvas — each step ringed and chipped with its state, taken flows solid and skipped ones
dimmed — with the actions the current step admits on the side; the *Designed for this goal* banner
(proposed, not yet adopted, or adopted); before a run, *Edit the steps* opens the design as a draft
— while a run goes it is not offered: a running workflow is not edited from here. The goal's
conversation, where `@Workflow Agent` asks for a draft, a step or a fix, is the Conversation tab. The
**Details** pane has four tabs — Details (the run's card, projects, the goal's documents with *Add…*, spend, assignees, origin with the
goal's mode), Work (one dense row per item — its state, the first line of its instructions, its step; who was asked and
who runs it beneath, only when there is someone to name), Projects (what the run's steps made — a goal's projects are born of its run; nothing is created or attached here),
Files — and opening a work item replaces the pane with its transcript, tailed live. **While a run is going the pane is
read-only**: a banner says so, and the assignees, documents, files and the workflow itself wait for the run to finish.
A goal that **listens** says so under its title — *Listening · every Monday 09:00*, or *Paused: …* with
the reason — with *Stop listening* or *Listen again*; its start verb is *Start listening…*, and
*Run now…* is a run by hand ([Goals](goals.md#a-goal-that-listens)).

```sh
bisa status <goal> | run <goal> --input k=v | amend <goal> --from … | workflow use <goal> <id>
bisa step answer|release|done <goal-or-run> <step>
```

## Workflows

The library, then the designer — the canvas with a right panel whose rail picks **Properties**,
**Agent** (the conversation about the workflow, the Workflow Agent one `@` away) or **Runs** (its runs
in the workspace), and an arrow back to *Workflows* at the header's left. **Workflows** opens on *Yours · Templates* (remembered), every
workflow and every template a card with a **thumbnail of its graph** — the picture the designer
opens on, a template's before it is installed — in sections by domain tag, and one bar over both
views: a **search** over the name, the description, the slug, a step's name or kind and the tags; a
**status** (*All · Runs · Has problems · In use*; on templates *All · Installed · Not yet installed*);
the tag facets; an **Archived** switch on yours for the ones put away; the filters in the address, so
a link carries a search. A workflow's card leads with one state line — *ready to run*, *n problems*,
*running n runs*, *running in n goals*, *runs on a goal*, *archived* — then its facts (*steps · inputs
· yours or the template*), its holders and its tags, and wears a `⋮` beside the door: *Open*; *Run…*
when it can run in the workspace (the inputs, then the run starts — no goal — and its page opens);
*Stop every run* and *Restart every run* while a run of it in the workspace goes; *Delete…* (the
retire dialog, Archive while anything uses it). A workflow whose steps read the goal it serves says
*runs on a goal* and offers no *Run…*. A template's card says *installed* or *not installed yet* and carries *Use template* —
which installs the template into your library and opens it. *New workflow* creates a workflow
on the node at once — one step, *Start · by hand* — and opens it. A card opens the **designer**: the
palette of the eighteen step kinds on the left, under four headings — *Events*, *Gateways*, *Loops*,
*Tasks* — dragged onto the canvas; the graph in the middle; the
right panel's **Properties** pane with one form per kind — what a `start` begins on, in words (*By hand ·
On a schedule · When called · When a message arrives · When a signal is raised · When a project
changes · When a run finishes · When the platform says · When an outside platform lists something
new · When a check starts failing*), the inputs it fills from the event and how many of its runs
may go at once; instructions and references for `agent`, the
question and its options for `human`, the rules and `otherwise` for `decide`, one condition for
`if` (with `all` · `any` · `one` · `not` groups that nest), the subject and its cases for `switch`,
the state, the question, its options and `otherwise` for `judge` — the Decision-Making Agent's, asked
either way the moment the step exists — the items and the bound for `for_each`, the condition and the bound for `while`, the connector, the
operation, the account and one field per parameter for `connector` (an operation that writes says
so, and asks for an approval before it), what a `wait` holds for — a delay, a moment, a schedule, a
signal, a message, a project's change, a run's end, a platform topic, a person's release — the
signal an `emit` raises, what an `end` ends (*End this path · Finish the run · Fail the run*), how a
`decide` picks (*the first rule that holds · every rule that holds*), and the common `then` · `join` (`all` ·
`any` · `one`) · `on_fail` · `retries` · `max_visits` — with **boundary events** on a step that can be
stopped: *＋ Timeout · Reminder · Message · Signal*, each *Divert to a path* or *Post* / *Emit*; the workflow's own properties add *Let the
Decision-Making Agent decide in runs of this workflow*. Connect steps by dragging a handle; a step that
branches has one handle per branch — a `decide`'s rules plus `otherwise`, `yes`/`no`, one per case,
one per `judge` option plus `otherwise`, `each`/`done`, `loop`/`done` — and one per boundary event that diverts, drawn from its chip on the card's lower edge. A card stays where you put it — its position is saved with the
workflow — and **Tidy** lays everything out again from the flows when you want it to.

Validation runs as you type and the problems list names the step and the rule; a step whose
choice you have not made yet — a connector, an operation, a condition — is a row in it, not a
refusal. A save is a new revision after `workflow.autosave.delay_ms` of quiet (or ten such delays
after the first unsaved edit), and a save with problems is kept and cannot start. Leaving the
designer sends the last edits on the way out; quitting with unsaved edits asks first. `Delete` removes the selection, `⌘D` duplicates, `⌘Z` · `⌘⇧Z` undo and redo, `Esc` clears
— on the focused canvas only. Snapping, the grid and the minimap are `workflow.designer.snap`,
`workflow.designer.grid` and `workflow.designer.minimap` under **Settings → Automation → Workflows**.
The inspector's width and the library's tab are remembered per viewer; the step you picked, where
the canvas looked and the Agent pane's conversation are remembered per workflow, so leaving for the
Inbox and coming back by any door — or closing the app and opening it — finds the designer as you
left it.

**Runs** (`⌘⇧R`) lists the workflow's runs in the workspace — the ones going first, then the newest —
each with its number (*Nightly report #3*), its status, when it started or ended and who started it
(*by you*, *by schedule*, *by hook*, *by signal report.ready*, …, *test*), and *Open*, *Restart* and,
while it goes, *Stop*; *Run…* sits on top, offering *By hand* or *Test: as if … happened* when the
workflow begins on events.
Several go at once and none queues, and none freezes the canvas: each runs its own copy. *Open* is
the run's page (`#/runs/<id>`): the header with its status, who it waits on, when it started, who
started it, *Stop*, *Restart* and *Open workflow*; **Your move** for what it owes you; **Progress**,
its steps read downward with the verbs each admits, as on a goal; and **Canvas**, its frozen workflow
drawn read-only with the run on it. A goal's run opened by its id lands on the goal's Workflow tab
instead. What the canvas promises —
one picture for a design and a run, a step's position its own — is the designer contract in
[03 — Workflows](../architecture/03-workflows.md#the-designer); what to put in a step is
[`workflows.md`](workflows.md).

```sh
bisa workflow list [--templates] | show <id|slug> | new --from … | edit <id> --from … | validate --from … | rm <id>
bisa workflow run <id> [--input k=v] [--watch] | runs <id> | stop <id> [--run <run>] | restart <id> [--run <run>]
```

## Projects — the IDE

`#/projects` **is the IDE**: it opens on the workstream you were last in, and with no project yet on
a landing with one door — *New project*, which opens the dialog where a project is created, cloned or adopted from a folder. Archive or remove the
project under you — from the rail, About, the command line or another window — and the IDE leaves at
once for the first project's own root, or for that landing when none is left. Its rail, its documents and terminals,
its right panel, the editor, git, review notes, the commit graph, pull requests, language servers
and the agents that work inside a project are [`the-ide.md`](the-ide.md).

## Channels and messages

**The Channels page** (`#/channels`, the sidebar's door with no channel picked) lists every standing
channel as a row — the name, the topic, when it last moved and who said what last, the roster's
faces when nothing was said yet, its tags, a working dot, the live unread count and a `⋮` with
*Open* and *Edit…* — in the sidebar's order, `general` first then creation, under one toolbar: a
search over name, topic and tags, *n of m*, *New channel* and the tag facets; the words and the
tags are remembered with the page. **The Messages page** (`#/messages`) lists every direct channel
by who is in it — their names, the first one's face, an agent's harness beside it — with the last
words said and when, a working dot and the live unread count, the one that moved last first, under
a search by name and *New message*. A direct channel's stored name is never shown: it is who is in
it. Both pages fill the window like the Goals list.

A standing channel's header carries its topic, its tags, a working dot, and an `@handle` chip that
opens the roster — the agents and teams that belong here, and the people on other nodes listed on
it. **Edit** changes topic, roster (agents, teams and, under **People**, the hosted members a guest
reaches this channel through) and tags; the audience and the kind are fixed. Below your own
channels and direct messages the sidebar has one section per workspace you are a guest of —
**Hosted by …** — with the channels and direct channels the host says you reach; a hosted channel
is the same surface, read and written through the host, text, mentions and replies only
([Collaboration](collaboration.md)). A direct channel with one agent shows that agent, its harness
and model plan, and whether it answers you only or any member.

One conversation component renders a goal's thread, a channel, a direct channel and a conversation
with agents: day
dividers, an unread rule placed once on entry, collapsed runs by one author, one level of reply
indent, an accent rail on your own messages, retractions that collapse to a line, reactions that say
who. Typing `@` opens a picker over people, agents, teams and — in a channel — its own handle; the
**address tray** above the composer names who a message will reach and is remembered per
conversation. The General Agent and the Workflow Agent are in neither picker: an unaddressed question reaches
the General Agent anyway. Your message gets a 👀 from the agent that took it; *"{name} is writing…"* shows while a
turn runs. Attach files by paperclip, drag or paste. A pasted **picture** — a screenshot, a copy
from a picture app — asks for its name first: the suggestion is `pasted-image-<stamp>.png` with
the stem selected, so `Enter` keeps it, and the conversation and the agents see the picture under
the name you give (a name typed without an extension gets the picture's); a file pasted or dropped
with a name of its own keeps it. The New Goal dialog takes a paste the same way, into its
Documents. A collaborator sees a file *not on this machine* with a **Request** control. What an agent made for you to look at — a page, a chart, a sheet, a
deck — is an **artifact**: a card under its message, live where it can be, opened beside the
conversation with *Save as…*, *Reveal in Finder* and *Open with the default app*
([Artifacts](artifacts.md)); the *file* word on your own attachment's chip turns it into one.

A path in a message — `src/main.rs:42`, `./docs/plan.md`, `~/Projects/app/README.md` — is a door:
click it for *Open src/main.rs:42* in the Project IDE, *Reveal in Finder* (the file manager's own
name on your platform) and *Copy the path*; when more than one checkout holds the file, one *Open
in …* per checkout; ⌘-click opens at once. A URL shows a small card with the site's name and the
whole address, and opening it — in Bisa's browser or the machine's — is your act: nothing opens on
a plain click, and the app never leaves its window. A redacted secret is never a link.

## Conversations

A **conversation** is a saved exchange you start with one or more agents, about something — its
**origin**: this node, the workspace, a goal, a workflow, a project, a workstream, a drawing or a note.
You can have as many as you like about the same thing, and each is kept: come back to it, search it,
name it, put it away. Each thing lists its own, **on the same surface everywhere**: a goal's
**Conversation** tab, the designer's **Agent** pane (its `…` menu, *Conversations about this
workflow*, opens it on the list), the drawer *Ask an agent* opens beside a note or a drawing, and the
IDE's Agent panel and Agent mode each carry the thing's name and a **Conversations** door with the
count of those about it — the IDE's is the project's, every checkout's included. The door unfolds the
rows, newest activity first, a search box that finds one by a word said in it, an *Archived* switch,
and **New conversation**. With no conversation yet the surface says so — *No conversation about this
note yet* — and offers the same one button. Either way **New conversation** starts one in one click,
untitled — the thread opens at once; name it later from its menu. Open a drawer or a pane over
conversations you already have and you see them listed, one click from any; come back and you are on
the one you left. There is no list of every conversation: a conversation is reached where it is about.

A conversation picked from the list opens in place — in the drawer, under the goal's bar, in the
workflow's pane — with the messages, the agents that took part, its origin as a link to the thing, the
title you can rename in place, and *Archive* and *Delete* in its menu; the goal's own thread is the
first row of the goal's list, to come back to. Searching or flipping *Archived* narrows the list and
never moves you off the conversation you are on. One about a project or a workstream opens **in the
IDE**, on that checkout's Agent panel — that is where the agent runs and where its edits land
([the IDE](the-ide.md#agents-in-the-workstream)). One about the workspace or the node has no screen
to live in: a link to it — from ⌘K, the Inbox or the Pulse — opens it on a page of its own. The
agent you talk to runs where the
origin says: in the checkout for a workstream or a project, in its own scratch folder otherwise,
and it is told what the conversation is about. The Workflow Agent can be reached in a conversation
about a goal, a workflow, the workspace or the node — never in one about a checkout, which has no
workflow for it to design.

**The reply streams.** An agent's answer arrives as it is written, at the foot of the conversation
under its name, with its **thinking** — the reasoning the harness wrote before the words — folded
above it as *Thinking · 1.2k chars*. Open a block to read along, or press **Show thinking** under the
timeline to open every one; **Hide thinking** folds them again, and the choice is kept. When the
answer is complete it is a message like any other, its thinking kept above it. A harness that does
not reason in the open shows its words alone. A conversation that has grown long is the harness's
to remember: it keeps and compacts its own context, and every later turn carries on from what it
knows; the platform writes no summary and asks nothing of you.

A conversation is not a channel — it has no roster and no handle — and not a goal's thread, where
the Workflow Agent designs and gates are asked; and the agent's session that answers you is shown
on the conversation, never among a checkout's harnesses and terminals.

## The Browser pane

The platform has a browser of its own — the one the IDE's Browser tab uses — and it is never
further than the Details pane. `⌘⇧L` and *Browser* in the search show the pane beside whatever
screen you are on and hide it again, and close a pane opened on a tab at once — and with no tab in
sight they open one for you, at home in the goal, workflow, channel, message or conversation you
are on; the pane by itself never does: land on a screen that remembers it, or follow an addon
there, and it says *No browser tab here yet* with *New tab*. The **Browser** button is the Project
IDE's alone ([the IDE](the-ide.md#terminals)); no other screen's header carries one. So a pane
closed by mistake — the ✕, Escape, Back — loses nothing: the tabs stay, the footer's count says
how many, and `⌘⇧L` brings them back. The footer's **Browser** count is one number for every tab wherever you are — the ones you
can look at and the ones kept out of sight — and only ever moves because somebody opened or closed
one: you, an agent, or a page asking for a window. It is tinted while a tab is on screen, in the pane or the
IDE, its tooltip saying what is showing and how many agents opened (*3 browser tabs — 2 opened by
agents, 1 out of sight*); a click opens it the way the memory and disk read-outs
open: a bar of the tabs in sight against the ones out of sight, then **Tabs** — every tab with
where it was opened (*Goal · Ship the storefront*, *Project IDE · Bisa › feat/a*, *Channel ·
#general*) and who opened it when that was not you (*opened by Reviewer*, *opened by a page*), the one on screen marked, *unseen* on one out of sight, a ✕ beside each — **Origins**
(how many tabs at each place — *Project IDE*, *Goal*, *Workflow*, *Channel*, *Message*,
*Conversation*, *Workspace*) or **Unseen** (the tabs agents keep out of sight, one click showing
each), the choice remembered, with *New tab* at home wherever you are and the door to **Settings › Capabilities › Browser**
at the top; the Pulse, the workspace's home, carries the button too. Each of these opens the pane beside whatever you are on: a strip of
every open tab, a browser's bar, and the page under it. Drag the pane's edge to make it as wide as
seven tenths of the room beside the sidebar — never narrower than 300 px — and double-click the
edge for the default; the width is remembered on this machine, and a window made narrower draws
the pane narrower until the room is back. The bar is whole on every tab — back,
forward, reload (*Stop* while the page loads), the address field, the wand, the camera, *Open in
the Project IDE* for a tab that belongs to a checkout or a goal, *Open in the machine's browser* —
and what a tab lacks is greyed with its reason: a new tab holds everything but the address, back
and forward follow the page's own history. Type an address and press Enter — `localhost:5173`,
`example.com`, a whole URL; `⌘L` brings the field back with the address selected, `⌘[` and `⌘]`
go back and forward, `⌘R` reloads, `⌘T` opens a tab beside, in the page or in the bar alike. A link
that wants a window opens a tab beside; the page's title names the tab; a page that never comes
stops saying *loading* after a while and says so. A page an agent opens from a conversation
appears here, beside that conversation, so you see what it reads; a tab that belongs to a checkout
shows in the IDE's own strip when you are there in Project Mode, and in the pane otherwise — Agent
Mode and Board Mode included, a switch carrying it across. A URL in a message
opens here too — its card's first verb, *Open in Bisa's browser* — and an address typed into the
search (`⌘K`) is one row that opens it. The **camera** is two buttons: one copies a screenshot of
the page to the clipboard, the other saves one where you choose — the same picture an agent gets
when it asks for one, and the page never blanks for it. Right-click menus, dialogs and the
palette are drawn over the page, which steps aside while they are open. Notes, drawings, their
round buttons, the pet and addon windows float over the page instead: it stays live around them —
you keep browsing — and they take their own clicks (on macOS). An
html page or a figure an agent posted as an artifact opens here too, from its `…`, *Open in the
browser*, with an origin of its own so its scripts and assets load.

**Annotate a page for an agent — in the Project IDE.** The wand sits on the bar while you are in
the Project IDE, for a tab in its centre or in this pane beside it, and works on every page a tab
shows — a page the project serves, your dev server, the web — never on a page an agent posted as
an artifact, which is its own. Press it, point at an element, click it and say what should change
in the box that opens beside it — *Enter* adds the note, *Esc* closes the box; each becomes a
numbered badge on the page and a line in the tray under it. For a tab that belongs to a checkout
the tray is the IDE's — **Send** to an agent as an edit, file chips when the page is a file of the
checkout — and for a goal's tab it is the screen's: **Attach to the message** puts the elements as
chips in the composer beside it. Beside any other screen the pane only shows the page: there is no
wand, and annotations you started in the IDE wait there for you.

**Agents browse here, the way you do, and out of sight when nobody is watching.** Every agent —
the platform's own and every one from the catalog, on whatever harness it runs — reads and drives
pages through this browser and no other: the guard refuses the machine's browser and headless
browsers and tells the agent which tools to use instead; only a project's own end-to-end suite is
put to you. An agent opens a page, reads its outline the way you see it — every heading, link,
button and field — clicks, types a keystroke at a time, presses Enter, chooses an option, hovers,
scrolls, waits for the page to settle, reads the console and takes a screenshot; a dialog the page
raises is answered for it and it reads what the page said. A tab an agent opens beside you — in a
channel, a direct message, a guided goal, the checkout's IDE — is at home where you are talking to
it and shows there. In a goal in **auto mode**, where nobody
is watching, the tab is **kept out of sight**: it renders and answers the agent's every tool, the
screenshot too, and nothing opens beside you. The footer's Browser count includes it, its overlay
lists it under **Unseen** — and *unseen* under **Tabs**, dim, its row's title *an agent browses
here out of sight* — and one click there shows it; the pane's strip and the IDE's Browser button list
it the same way, dim with the hidden glyph. An agent may ask
for one tab either way, and says so.

**Settings › Capabilities › Browser** is where the browser is yours to set: whether it is on at
all on this machine (off, nothing opens and agents are told so), which agents may drive it —
*Everyone*, the default; *Assigned*: agents carrying the *Embedded Browser* skill, the platform's
own always; *Nobody* — how far (any page, or only pages served on this machine), when an agent's
tab is kept out of sight — *Unattended*, the default: in a goal in auto mode; *Always*; *Never* —
whether an agent may run a script in a page to read its state — *Allow*, the default; *Refuse* —
the home page a new tab opens on, whether a restart brings the tabs back, and how wide a
screenshot is.

**Settings › Capabilities › Mobile Development** is where Flutter development is set up — off
until you turn it on. The card holds the switch and which platforms this Mac develops for (*iOS and
Android*, *iOS*, *Android*; iOS is held on a machine that is not a Mac, with the reason). **The
setup** examines the machine: one row per component — Flutter, Xcode, an iOS simulator runtime,
CocoaPods, the Android SDK with `adb` and the emulator, a virtual device, Java — each *found* with
what was found, or *missing* with the official guide and the command you run yourself, or *held*
when its side is off; then Flutter's own doctor lines; *Check again* examines now. **Devices**
lists the simulators, emulators and phones the machine can reach with *Boot*, *Shut down* and
*Show*, and *Create a simulator…* from a device type and a runtime Xcode has. **Which agents may
use the devices** is the browser's three-way switch. Where Flutter and the Android SDK are, when
the search does not find them, are two rows under it. On, a Flutter checkout in the Project IDE
gets a **Devices** button beside Browser ([the IDE](the-ide.md#mobile-development)).

## Agents, Teams, Catalog

**Agents** has *Definitions* and *Running*. The three core agents are pinned above the filters: the General Agent and the Workflow Agent each as an agent's card, then the Decision-Making Agent — its name, what it is, whether it is on and who answers for it — which opens Settings › Decision Settings › Decision Making, since it has no page of its own and nothing messages it. A card
shows origin, harness, model plan with live health badges, skills, MCP servers; the pane carries
identity, respond policy, and **Recall**. *Running* is the live roster with **Abort** on each session that is running or waiting on you. **Teams**
lists members (the General Agent and the Workflow Agent marked *implicit*). Both screens carry
**Catalog** in their top bar, beside *New agent* and *New team*, which opens the catalog on that kind
— Settings › Library › Agents or Teams. The **Catalog** (Settings → Library) shows every entry by kind with its install
plan — what it brings, marked *already here* or *will be created* — and confirms before a
multi-object install.

## Events and listening

What starts a workflow is drawn in the workflow: there is no screen of its own for it
([Events and gateways](events.md)).

- **In the designer**, a `start` step says what begins a run; a step's boundary events are chips on
  its lower edge; and the header carries the workflow's **On/Off** switch once it begins on an
  event — *Off*, *On — listening for every Monday 09:00 · next Mon 09:00*, *Can't turn on: 2
  problems*, *Paused: …*. Turning it on asks the inputs no event supplies and a budget per run, and
  shows a public hook's secret with its path, **once**.
- **On the library card**, a workflow that listens wears *On* beside its state, and its `⋮` carries
  *Turn on…* and *Turn off*.
- **On a goal**, the header says *Listening* and for what; a goal whose workflow begins on events
  is started with *Start listening…*.
- **In the Inbox**, a listener that could not be armed, could not start its run, or holds a payload
  from outside for you to read is a notice on its host's row — the workflow's, under *Workflows*,
  or the goal's.
- **In Settings**, *Automation › Events* holds this machine's switches: whether it listens at all,
  how often it looks, whether public hooks are answered, the chain depth, the rate and the backlog.

## Pulse

Everything that happens across the platform, newest first, as one feed the node keeps as it
happens — every goal's journal, every message, and the engine's own facts: a project made, a
workstream opened or committed, an event heard and the run it started, a setting changed, the node paused. A row of tabs
narrows it to one concept — **All · Workspace · Goals · Workflows · Projects · Channels · Agents ·
Node**, each tab with its concept's glyph, folding to the glyphs alone on a narrow window — and the choice stays in the address, so a link to *Workflows* is a link to
workflows. The node ships the event, not a sentence: a row carries the payload verbatim and the
desktop renders it, so a step's failure, a decision's rationale and per-turn cost all appear, and
a row reads the same the moment it happens and after a reload. Scroll and the feed keeps coming —
a page at a time, as far back as the workspace goes; something that happens while you look
arrives within a second. A Pulse left open holds ten pages and lets the oldest rows go past that,
never the ones you are reading; scrolling down brings them back. A row opens what it is about — the goal, the channel, the workstream in
the Project IDE, the workflow, the agent — and opens rather than truncates its detail, where every
path and URL is a door: the file in the Project IDE or the file manager, the URL after a card
asks, looked up across every checkout on this machine. `bisa pulse --concept <name>` renders
the same facts in the terminal.

## Notes

`Alt+N` opens a markdown scratchpad over whatever you are looking at. A note is about one of six
things — the workspace, a project, a goal, a workflow, a channel, or this node (the machine itself:
its harnesses, its setup) — and the panel lists them under seven tabs: **All · Workspace · Projects
· Goals · Workflows · Channels · Node**. *All* is the default and shows every note with a chip
naming what it is about; a plural tab is every note of that kind. **The panel keeps its place**:
navigating from a goal to a project changes neither the tab nor the open note — only your hand on
the strip or a click on a row does — because reading a note about one thing while looking at another
is the point. The route has one say: **New** files a note where you stand (this goal's, this
project's, this workflow's, this channel's, else the workspace's), and on a tab with more than one
place it is a menu with that place first, marked *here*. **Ask an agent** (the agent glyph) in a
note's header opens a conversation beside it — the same thread and composer as everywhere else, started in one click and
named later from its menu, in a drawer you drag wider or narrower by its edge; the agent reads the
note first and writes into it only when you ask, with `note_append`.

A search box above the list narrows it as you type — every word, in a title or a body — and `⌘F`
over the list lands in it. Inside a note, `⌘F` opens find and `⌘R` find-and-replace, the same bar
every document uses: regex and match-case toggles, Enter steps and selects the match in the text,
*Replace* and *Replace all* write through the editor so the save sees them like typing; in Read the
bar searches the rendering and has nothing to replace into. Write, Split and Read views; saves as
you stop typing — and **Save** in the header (or ⌘S) saves now; two writers (you, and an agent in the
conversation beside the note, whose only write is to append when asked). Leaving a note with unsaved
changes — Back, the panel's ×, `Alt+N`, the dock, another note opened over it — asks first: **Save**,
**Don't save** or **Cancel**, the same question the IDE's tabs ask, and quitting the app counts and saves
an unsaved note like a document. **Delete** asks first too, and says that the note's conversations go
with it; it is also on every row of the list (the trash that shows when you point at a row or tab to
it), so a note goes without being opened. Nothing in
the workspace depends on a note, and it stays on this machine unless you push. The notes icon floats
over the app: drag it anywhere, and it keeps its distance from the edges it is nearest when the
window is resized or maximized — a restore puts it back exactly; **Settings → Notes → Reset
position** returns it to its corner. The number on it is every note there is, whatever tab the panel
is on — live whether the panel is open or closed (a note an agent writes into, one made from a
conversation, one deleted, a project gone with its notes), and still there when you hide the button
from the footer and show it again; **Settings → Notes** turns the number off, and the button's name
still says it. A long note needs room: **Maximize** in the panel's header
fills the window between the header, the footer and the sidebar — the same frame the Draw panel
takes — and Escape puts it back in its corner (the find bar and the search box get Escape first);
**Settings → Notes → Open maximized** makes that the panel's default.

**Your notes are a git repository.** Every note is a Markdown file under `notes/` in the data
directory, and the first note makes that folder a repository. A strip at the foot of the notes list
reads it in one line — *3 changes · ↑1 · committed 2h ago* — and holds the few verbs that matter:

- **Commit…** unfolds a message box in place. **Suggest** asks the General Agent for a message from
  what changed (read-only; it cannot commit), an empty box commits as *Notes, 7 Sep 2026*, and
  `⌘Enter` is *Commit*. A commit takes every change: notes are not staged by hand.
- The **⋮** menu: **Push**, **Fetch**, **Pull** (fast-forward only, a safety ref first — anything
  else says so and leaves the terminal to you), **Set origin…** (a code host URL or a path on this
  machine), and **Who commits…** (a name and email set on the notes repository alone, prefilled
  from your global identity). A verb that is off says why when you choose it.

Notes belong to no project and no goal, so the Publish gate does not stand here: a push is your own
act from a button, and nothing pushes on its own. Nothing autosaves a commit either — the strip is a
line, not a nag, and the editor screen has no git chrome at all. Deleting a goal, a project, a
workflow or a channel takes its notes out of the tree; the next commit records that.

## The pet

An optional companion in Codex's pet format — `pet.json` plus a 1536×1872 sheet of 192×208 cells,
eight columns by nine rows, one row per state. Nine states driven by what the app already knows:
*waiting* when something owes you, *review* when an approval is open, *running* while an agent is
mid-turn, *idle*; *running right* and *running left* while you drag it, so it faces the way it
travels; *waving*, *jumping* and *failed* play once. A pack drawn for the platform says in
its manifest how each state plays — its frames and each frame's duration — and that is what plays;
a pet that says nothing has its frames read from the sheet. Under Reduce Motion a single frame is
held.

**Nine ship with the platform** — the *midnight-shipping* pack: **Bracket** (a shelf fungus that
grows on what you delete), **Buffer** (a snail that takes its time), **Fathom** (a squid, the
patient archivist), **Jolt** (a gremlin on its second wind), **Kernel** (an immovable tabby),
**Loop** (a tireless mechanical metronome), **Lumen** (a dreamy moth), **Moonrice** (a rice-dumpling
ghost that brought you something warm) and **Nocturn** (an insomniac owl). They are in the binary:
nothing installs them and nothing removes them. **Moonrice is the one shown when you turn the pet
on and none was chosen.**

The footer's pet icon opens the **Pet** overlay, the way the memory and CPU read-outs open theirs:
a *Show the pet* switch, a size slider (the pet on screen is the preview), the pets as small tiles —
pick one and it shows — *Reset position* and a door to *Settings › You › Pet*. **Settings › You › Pet** shows
every pet as a tile — its idle frame, its name, its tagline, its archetype and mood — with the one
showing ringed; *Import a folder* brings in a pack of your own in Codex's format, and only your own
has *Remove*. Drag the pet anywhere: like the notes icon it keeps its distance from the edges it
is nearest when the window changes size, and *Reset position* returns it to its corner.

**In the Project IDE the pet follows one harness** — the session the workstream in view is about.
That is the one you chose last: a click on a row in the Agent panel (the row highlights), an agent
row on the Workstreams rail, or a harness tab brought to the centre. With nothing chosen it is the
workstream's loudest live session, the same one the Agent panel's status line names. The pet then
says what that harness is doing — *running* while it starts, thinks or runs a tool, *waiting* when
it waits on you, *failed* when it failed or was aborted, *idle* between turns — and plays once on
the edges: a jump when the session finishes, *failed* when it fails. The workspace's other events
leave it alone while it follows. A click on a following pet opens that workstream's Agent panel.
Everywhere else — the Inbox, a goal, a workstream with nothing running — it stands for the whole
workspace as above.

## Draw

**⌘⌥D** opens a canvas over whatever you are looking at — the notes panel's twin for pictures: a
system's parts and how they talk, a flow, a board, a map. A drawing is about one of the same six
things a note is, listed under the same seven tabs, made by **New**, and searched by title. Two things are not like a note. A drawing is a **record of the workspace**: it travels to
everyone in it, and an agent on any member's machine can read it and draw into it. And a drawing
needs room: **Maximize** in its header fills the window between the header, the footer and the
sidebar; Escape puts it back in its corner once the canvas has nothing of its own to cancel.

**New** opens a dialog: a gallery of *Empty* and ten templates — a system architecture, a flowchart,
swimlanes, a mind map, a board, a user journey, entities and relations, a retrospective, a wireframe,
a timeline — each a tile with a preview of the picture and a line on when to reach for it, a name the
template proposes and you may change, and, when the tab offers several places, *Where* it is filed
(where you stand, first). *Create* opens it. **Save** in the header (or ⌘S) saves now, beside the
autosave; leaving a drawing with unsaved strokes asks **Save · Don't save · Cancel**, as a note does, and
**Delete** asks first, naming the snapshot and the conversations that go with it — from the open
drawing's header, or from the trash on its row in the list. The canvas is Excalidraw, with the platform's own shape libraries in its sidebar: software-engineering
shapes (a service, a database, a queue, a cache, a client, an API gateway, a load balancer, an
external system, a person), stickers (sticky notes, callouts, badges, a checkmark, a warning) and
drawing elements (a title banner, a section frame, a legend, a note card). Everything is vector: the
image tool is off, since a drawing must reach every member whole. The canvas saves as you draw, and
nothing about the window — the zoom, the selection — is saved with it.

**Ask an agent** (the agent glyph) opens a conversation beside the canvas whose subject is this drawing — started in one
click, named later from its menu, in a drawer you resize by its edge, with the composer the IDE's
has: who a bare message reaches, and *Stop* while an agent works. Every agent
carries the Drawing skill and seven tools: it reads the drawing first (every element with its id),
then draws — boxes with labels, arrows bound between them, frames, a Mermaid flowchart laid out into
real shapes — while you watch the shapes land, erases by id, and takes a snapshot to look at what it
drew. Who may draw is the workspace's word (**Settings › You › Draw**: everyone, agents carrying the
skill, or nobody), and the canvas must be open on some desktop: with none, the tool says so at once.

**Your drawings are a git repository** too, `drawings/` in the data directory, with the same strip
at the foot of the list — *Commit…*, *Suggest*, *Push*, *Fetch*, *Set origin…*, *Who commits…* —
and one verb fewer: there is no *Pull*. A drawing's record arrives by sync, and its file here is an
export the next change rewrites. The footer's pen switch shows or hides the floating drawings
button, above the notes button; **Settings › You › Draw** holds the dock, its count, opening maximized, the
save delay, and the workspace's word on who may draw and how wide an agent's snapshot is.

## Addons

Small windows that float over the app — a clock, the machine's load, the weather, a game, a sticky
note — each a folder of HTML its developer shaped and you installed. Thirteen ship in the catalog;
**Settings › Library › Addons** installs one, imports an addon of your own, switches each on or off
and grants each exactly what it asks for, line by line; the footer's **Addons** popover lists every
installed addon with its switch, shows or puts away each window and every window (`⌘⇧X`). An addon runs behind three walls and reaches the platform
only through what you granted; the whole story, for the person and for the developer, is
[Addons](addons.md).

## Settings

Settings opens from **your profile** at the top right of the chrome — your face, once you set
one, your identicon until then (its menu: *Identity* lands on
your keys, *Settings* on the panel you were on last), from `⌘,`, or from *Go to Settings* in the
search. The same menu's **About Bisa** opens on the platform's mark — a blue glass squircle holding the
split B; the same file is the app's icon in the dock — and says which version
this desktop is and which the node is running, warns when the two differ, and shows the data
directory and your npub with a copy for each — and, above them, **your profile**: your name and your photo, the ones every message you write and every people list draws, here and on every workspace you are a member of; the photo is scaled to a small square before it is kept, and both travel the moment you save. The mark is our own; the harness marks are their
owners'. A workspace with no project yet wears the mark on its landing, above *Add a project*.

A rail of groups: **You** (Identity, Appearance, Pet, Notes, Draw), **Workspace** (People — the people on
other nodes this workspace hosts, each at a role, the invitations as a link, a code and a QR, the
claims waiting on you, joining another workspace and the ones you are in ([Collaboration](collaboration.md));
Relays & sync — the switch that turns the wire on (off by default), each relay with its state, *Check* on every row and *Check all*, a check before adding, *Reset to the defaults*, reconnect, the wire's counts;
Governance — who decides each gate, and the role matrix), **Capabilities** (Skills, MCP servers, Connectors, Harnesses, System, Desktop, Network, Browser, Mobile Development), **Library** (one panel per kind — Agents, Skills, Teams, Channels, Connectors, Workflows, Addons — the catalog a workspace installs from), **Git & code hosts** (Identity, SSH keys, GitHub, GitLab,
Bitbucket), **Security** (Redactor, Guard, Classifier — the classifier's own panel now picks who reads: an agent, a bare harness, or the Decision-Making Agent), **Decision Settings** (Decision Making — [the Decision-Making Agent](decisions.md)'s one panel: the agent and its readiness, who answers and only the fields that provider takes, an API key sent once and never displayed, `decisions.enabled` then every decision point with its own switch, the deadline and the two confidence thresholds, *Try it*, and the newest judgements), **Automation** (Events — whether this machine listens, its tick, a check's timeout, a file scan's bounds, whether public hooks are answered, how often pull requests are asked about, the chain depth, the rate and the backlog; what starts a workflow is a step of it, and whether it listens is its own switch — Goals — how a new goal moves, auto · guided · manual, what an auto goal does above a step's ceiling — the classifier reads it, or you are asked — and how many failed runs an auto goal repairs alone — Workflows — Budgets, the workspace's default
ceiling for a goal or a run in the workspace made without a budget of its own), **Project IDE** (IDE, Editor, Terminal, Workstreams, Board, Diagrams, Artifacts, Agents, Keymap, Language servers — the workspace-scope defaults a project's About › Settings can override), **Performance** (Cache — every named cache's hits, misses and entries from `GET /cache/stats`, and *Clear caches*), **Node** (pause/resume,
restart the sidecar — which also restarts itself: a node that exits on its own is started again on the same port after a short delay, and the lists are read again with a toast saying so; **Logging** — the diagnostic log on this machine: the newest crash first — one sentence, *Show details* for
where it was, on which thread, its backtrace, what the node last said and the last lines before it, the
file manager's reveal on the report — then where the files are, one block per process with how big each file
is and when it was written, the crash reports, the file manager's reveal on the folder, then the four
`logging.*` dials — the switch, the level, the rotation, the files kept — errors only by default and never
sent anywhere). A screen that hits an error says so in place, that the details are in the diagnostic log,
and offers *Try again*, *Reload* and *Reveal the log*. **Connectors**, under Capabilities, is where an outside platform gets an account on this
machine: one card per installed connector, its accounts by label with which secret fields are set and where they
live, *Connect* for an OAuth2 platform (the browser opens; a code can be pasted instead), *Set secrets…* (a PEM
key pasted whole for a `jwt` platform), *Check*,
*Make default*, *Forget…*, and *Add a connector* for a custom definition validated before it is saved
([`connectors.md`](connectors.md)). Every panel is drawn the moment it opens and says what it is
still reading — *reading the GitHub CLI…*, *reading the harnesses…* — where the answer will go, after a
beat, so a read that lands at once never flashes a placeholder — the rule of every screen, not only Settings: a placeholder's box is held from the first frame and filled only if the read is still out after the beat (a dialog or a popover that opens onto a
read says so at once — never an empty panel); the settings themselves are kept across
panels, so switching between them draws the controls at once;
a panel that reads again says *checking again…* beside the last answer and keeps it when the
re-read fails, with the reason. The GitHub panel draws its title, chip and token form at once,
and the connection fills in as `gh` answers.
The **language** is a setting of this machine, `appearance.language`, with no dial in the panel
(`bisa settings set machine appearance.language en`): *System* follows this machine's languages to the first the
platform has; *English* is the one shipped today, and every word — the desktop's, the node's
refusals, the CLI's — follows the choice. Appearance is six
independent dials — five theme families (*Glass*, the one the app opens in, then *Harbor*, *Orchard*,
*Dune*, *Suede*), each with a light and a dark side, plus *System* — Glass on whichever side the
machine is; four accents; two densities; a type-size multiplier (a 14px body at 100%, up to
150%); and two faces, one for the interface (System, Inter, Atkinson Hyperlegible) and one for code
(System, JetBrains Mono, IBM Plex Mono), bundled so they look the same on every machine. Every
palette is built for hours in front of it — text never on pure white or on black, secondary text
kept readable, each pairing held to a contrast floor by a test — and **the accent means *your
attention***: whatever is waiting on you wears it. Two families are a material as well as a palette.
**Glass** frosts its panes — the sidebar, the chrome, the panels, every dialog and menu — over
whatever is behind the window, which the desktop app makes transparent for it with the OS's own
under-window blur behind (macOS; Mica on Windows; elsewhere the panes frost the page's own ground,
still glass with less behind it); its contrast floors are measured over the composite, on a white
wallpaper and a black one alike, which is why its dark side is nearly solid. **Suede** is matte:
undyed paper in the light and warm charcoal in the dark, no drop shadows, a card told by its tone
and its border, status colours pressed into the surface rather than lit. In every family the editor
and the terminal stay solid — a blurred desktop is not a surface to read code over. Each dial previews itself, and the Omnibox reaches
them all by typing (`Theme:`, `Accent:`, `Font:`, `Text: larger`). The **Project IDE** group and
the **Workflows** panel are generated from the settings registry, one control per key with its origin
badge; every key, its scopes and its default are in [`reference/settings-keys.md`](../reference/settings-keys.md),
and how a value resolves is [ide/13 — Settings](../architecture/ide/13-settings.md). **Nothing here
is per project.** Settings is the workspace's and this machine's screen; a project's own values —
how its branches merge and pull, how its workstreams are made, its editor's tab size, the harness a
terminal opens with — are set on the project itself, in the Project IDE: About › Settings' *Git*, *Workstreams* and
*Editor & terminal* cards, each with *Inherit* to put the workspace's value back — drafted until the view's one Save at the top writes them.

**System**, under Capabilities, is what this Mac lets the desktop app do. First, **Notifications**:
which of Bisa's system notifications reach you, by what happened. At the top, what macOS says
(*allowed*, *not allowed*, *not asked yet* — *Allow* asks once; after a refusal only System Settings
› Notifications can change the answer, and the card goes on saying *not allowed* each time it is
opened) beside a master switch, **System notifications**: off, nothing
reaches the notification centre — the menu bar icon, the Dock badge and the Inbox still say what needs
you. Under it, one switch per category: **Asks — waiting on you** (an agent's session held at a
permission, an approval gate or a sign-in; a step's gate; a question; a project with nobody set to
commit after an agent's work), **Failures** (a session, a listener that could not start its run, a workstream
script that exited badly — each said once), **Finished work** (a session finished — off by default,
since finished work is not an interruption), **Workflows** (a workflow proposed to adopt, a design
the Workflow Agent could not finish) and **Addons** (the notices addons you granted `notify` send).
Every one is this machine's, never synced. The card also says the one thing about the icon a
notification wears: it is the icon of the app it comes from — a development build posts as Terminal
and wears Terminal's; the bundled Bisa wears its own, and macOS keeps the icon it first saw for the
app until it is re-registered ([recipes §19](../contributing/recipes.md#19-change-the-platforms-mark)).
Then the two grants, **Full Disk Access** and
the **Microphone**, each with what macOS says (*granted*, *not granted*, *not asked yet*) beside a
switch of the platform's own, off until you turn it on. The switch is what the platform may rely on;
the grant is macOS's. Turn Full Disk Access on and System Settings opens on *Privacy & Security ›
Full Disk Access* — add Bisa there and come back; the panel re-reads the grant when the window
regains focus, and *Check again* does it on demand. It is recommended for productivity: without it
macOS fences the app off from Library, Documents on a managed Mac and other users' folders, and an
agent working there fails on the fence. The panel also says, in the same place, that the security
features protect your machine on a best-effort basis — they judge what a guarded harness asks and
redact what leaves; they are not a sandbox. Turn the microphone on and macOS asks once, with the
app's own reason; it is reserved for voice mode, which is not implemented — nothing listens, and nothing of ours
opens the microphone while the switch is off. Turning either switch off revokes nothing: only System
Settings can. In a browser session the switches are still yours to set; the grants can be read only
by the desktop app.

**Desktop**, beside it, is the app's own behaviour on this machine, three switches, each on by
default, and under them **Where you were** — one action, *Forget where I was*
([§Where you were](#where-you-were)). **Closing the window keeps Bisa running** — the red button hides Bisa as `⌘H` does and the app
lives on in the menu bar ([§The menu bar icon](#the-menu-bar-icon)); off, closing the window quits.
**Show in the Dock** — the Dock icon, wearing the number of things that need you; off, the menu bar
icon is the only door. **Confirm before quitting** — `⌘Q`, the icon's *Quit Bisa*, or the red
button while the first switch is off, asks first: the dialog names the shells and harnesses running
and the documents unsaved — an open note or drawing with unsaved changes counts as one — and only after
you confirm are the documents saved and the app quit;
*Cancel* leaves everything as it was. Off, the app quits at once, still saving what is unsaved. A
document that cannot be saved keeps the app open: every other document is still saved, the one that
failed stays on screen saying why, and a system notification — *Bisa — still open* — says so, since
you were on your way out (under the **System notifications** master alone: it has no category to
switch off). The two terminal confirmations — closing a live shell, terminating a
running harness — are under **Project IDE › Terminal**, each on by default, beside **Start a
resumed harness** (on by default: a harness that continues its latest session is nudged into
motion once its prompt is drawn; off, it waits for you).

**Network**, after it, is two things. *This Mac* is read, never changed: whether the **internet** is
reachable — *up*, with how long the probe took and this Mac's **public IP** as the echo service
answers it; *down*, with the reason — and the **Public IP service** asked for that address
(`https://api.ipify.org` unless you set another; empty asks none, and the internet is then judged by
the connect alone); whether a **VPN** is up — *up*, with the tunnel it runs on, who runs it
(Tailscale, WireGuard, a corporate client, or the protocol System Settings knows it by — IKEv2, IPsec,
L2TP), its addresses, whether all your traffic leaves by it or only some routes, the DNS it answers
with, and your public IP when all traffic leaves by it; *not connected* when a VPN is set up but off;
*no VPN* otherwise; every **interface** that is up — Wi-Fi, Ethernet or a tunnel, its addresses, its
gateway, which one carries the default route — beside your default route and the resolver this Mac
asks, read on the footer's cadence and again when you come back to the window or its network changes
(*Check again* reads once); and the **proxy System Settings names**, with **Use in Bisa** when it
is one the platform can follow, which copies it into the settings below (a PAC file is a script, and
the panel says it cannot be followed). *How the
platform reaches the internet* is yours to set: **Proxy** — *Environment* (whatever the node was
started with; the desktop starts it with none), *None*, or *Manual* with an HTTPS proxy, an HTTP
proxy (a login goes in the URL and is never shown again) and a bypass list of names, suffixes,
addresses and CIDRs — and **Speak HTTP/1.1 only**, for a proxy or an inspection tool that does not
speak HTTP/2. The same proxy reaches everything the platform runs for you — harness sessions, git,
the GitHub and GitLab CLIs, workstream scripts — so nothing the platform does leaves this Mac another
way; an **in force** line says what the platform does now and why, and **Check** sends one request to
a URL of your choosing through the platform's own client and tells you whether it got through.

**Git & code hosts** is five panels for the things git and your code hosts need from you that are
not settings. *Identity*: your global git config as a form — the name and email every new
repository inherits — *Who commits in a new repository* (inherit it, pin it into each new repository,
or ask every time; the workspace's rule, `git.committer`) — and your **profiles by organization**: one per organization, workspace or
GitLab group (or per user you push to as somebody else), naming the author, the SSH key and the
code host account its repositories use. A profile is a small git config file the platform keeps
and your global git config includes for that organization's remotes, so `git config --show-origin`
in a terminal shows the same thing and nothing here can disagree with git; every repository outside
the profile keeps your global config. *SSH keys*: one card per public key in `~/.ssh` —
its fingerprint, whether ssh-agent holds it, the last check you ran on it, and the one next step
with the button that does it. The path a key walks is the card's: *Generate key* (an ed25519 pair,
no passphrase — add one with `ssh-keygen -p` in a terminal, and the dialog says so), *Copy public
key* — or *Add to a host*, which copies it and opens the host's own SSH-keys page in your browser
(GitHub, GitLab, Bitbucket, Codeberg; the platform registers nothing for you) — *Check on
github.com* (one handshake that changes nothing on either side; the host's answer stays pinned to
the card, *github.com knows this key as ada · checked 2 min ago*), then *Use in a profile*, the
door to Identity where the key is bound to an organization. *Load into ssh-agent* sits beside the
path, not on it. Above the cards, ssh-agent's own line — and when it is not running, the command
to start it, ready to copy. Below them, the git hosts your `~/.ssh/config` names, *Copy a Host
block…* to paste into that file yourself (the platform never writes it), and *What would ssh
offer* a host — answered offline, by `ssh -G`. *GitHub*, *GitLab*, *Bitbucket*: one panel each, and the first thing it says is where
you already stand — **the CLI**: *GitHub CLI 2.63 · signed in to github.com as @you* (nothing more
to do: pull requests, reviews and merges go through it), or *not signed in* with **Authenticate**,
which opens a terminal tab running the CLI's own browser sign-in and updates the panel when you are
back, or *not installed* with the install commands for your machine to copy and a link to the
install page. Bitbucket Cloud has no CLI, so its panel starts at the token. Then your stored
accounts — as many as you have — each with *Check* (who the host says you are, the scopes, the
organizations, groups or workspaces it can see), *Make default* and *Forget*; one sentence naming
who requests would go as and from where (*the GitHub CLI*, *git's credential helper*, a stored
account); what git itself does for pushes over HTTPS; and *Add a token* (kept only once the host
accepts it, under the login the host answers; your username beside it for Bitbucket) with the
scopes the host needs and a link to the token page. The panels look at your machine and never at a
repository or a remote; the one place a push or a pull request is affected is a checkout's
About › Settings, which says which of all this it will use ([the IDE](the-ide.md#repository)).

**Security** is three panels, and a panel whose feature is switched off on this node says so in a
banner over its rules, which draw idle. *Redactor*: the shipped rules with a switch each (a GitHub
token, an AWS key, a private-key block, a secret-looking assignment, …), *This node's environment*
— how many of the node's own variables are recognised by name, each a switch, never a value — your
own rules — a pattern, or the name of an environment variable whose value must never reach an agent
— and a *Try it* box that shows what a pasted text would become. *Guard* (the Tool & Commands
Guard): the shipped refusals with their switches, your own rules in the order they are tried (a
command pattern, a path glob, a tool name or every call, each `allow` · `deny` · `ask` ·
`classify`), a *Try it* box that judges a command or a path by the rules alone, which harnesses the
guard can stop (*judged before it runs* or *observed only*), the last decisions (a *you ·
remembered* row is an earlier answer standing in for a new question — the Inbox card says under
its verbs that your answer is kept for the goal, or for the run in the workspace), and the two host
lists the platform's own calls
obey. *Classifier*:
whether it is on, which agent reads a doubtful command and how long it may take, what a harmful
verdict does, and a readiness line that says whether the agent can be launched. Rules are settings:
the workspace's and this machine's both apply, in that order, and the panel says which you are
editing. What all of it means is [11 — Security](../architecture/11-security.md).
