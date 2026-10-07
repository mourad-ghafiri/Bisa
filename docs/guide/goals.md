# Goals

A **goal** is the durable object: a stated want, the **workflow** that says how it becomes real, the
**run** that carries it, and a signed journal of everything that happened to it. Sessions come and
go; the goal is what you come back to. How to design a workflow — by hand, from a template, or with
the Workflow Agent — is [`workflows.md`](workflows.md); this guide is the goal's side. A workflow
needs no goal to run: *Run…* on it in the library, an event it listens for or `bisa workflow run` starts
a **run in the workspace**, which captures nothing and never appears in Goals
([Workflows](workflows.md#running-waiting-amending)).

## A goal has a status, not a state

A goal stores no state. What you see is a projection of whether it is closed and where its current
run has got to: `draft`, `running`, `waiting`, `done`, `failed` or `closed` — each defined in
[03 — Workflows](../architecture/03-workflows.md#the-goal-and-its-status).

Beside the status there is one more word every surface prints: **who holds the ball** — `you`,
`agents`, `world` (an event, the clock, a child goal), `finished` or `design` (no workflow yet). It is
one rule in the core, so `bisa status`, the Goals screen and the inbox cannot disagree
([03 — Workflows](../architecture/03-workflows.md#the-goal-and-its-status)).

A goal may have several runs over time — a second attempt, a different workflow — but only one that
is **live**. A run started while one is live is **queued**: it waits its turn and starts on its own
when the live run ends. **Stop** cancels the live run and withdraws the queue; the goal stays open,
a draft again, ready for a new run. **Restart** cancels the live run and starts a new run of the same
workflow with the same inputs at once, ahead of the queue. Closing is the one move outside the run:
it cancels the live run and the queued ones, every live step's work, and every gate the run holds
open. A closed goal is read, never resumed; its workstreams are released and the checkouts stay on
disk. Design: [03 — Workflows](../architecture/03-workflows.md).

## A goal that listens

A goal whose workflow begins on **events** — a schedule, a hook call, a message, a named signal, a
project's change — does not run once: it **listens while it is open**, and each occurrence is a run
on the goal. It is how standing work is said: *every Monday post the digest*, *whenever someone
posts in #support, triage it*.

- **Starting it arms it.** *Start listening…* on the goal (or `bisa run <goal>`) asks the inputs no
  event supplies and arms the workflow's start events; no run is made. A public hook's secret is
  shown then, once. Between runs the goal reads `waiting`, held by the `world`, and its header says
  what it listens for — *Listening · every Monday 09:00*.
- **Each occurrence is a run.** One at a time: an occurrence that arrives while a run is live
  queues one run behind it, or is dropped when its start's guard says `skip`. *Run now…* is a run
  by hand, at the workflow's start by hand, when it has one.
- **A failed run pauses it.** The goal reads `failed`, held by `you`, and hears nothing more — its
  queued event runs are withdrawn, so a repair has room — until a repair is adopted or you say
  *Listen again* (`bisa run <goal>` again). It listens with what it listened with before: you are
  asked for nothing you already gave, unless a repair redrew the inputs — what it then misses is
  asked for by name. A spent budget pauses it too. Failures count in a row:
  a run that ends `done` starts the count afresh, so a goal that ran well for a year is not one
  failure from its repair limit.
- **Stopping it stops the listening.** *Stop listening*, *Stop*, closing, archiving and deleting
  the goal all end it; what its events had queued settles as *not listening*.

A one-shot *wait for X, then do Y* is not a listening goal: it is a start by hand and a `wait`
step. The events, their fields and the guard are in [Events and gateways](events.md).

```sh
bisa new "…"                         # auto: the Workflow Agent designs how it runs, and the platform runs it
bisa new "…" --mode guided           # the agent proposes; you adopt
bisa new "…" --mode manual           # you design it on the Workflow tab
bisa new "…" --workflow bug-fix --input project=<pid> --input report="…"   # from a template, started at once
bisa new "…" --workflow <id> --no-start                                    # picked, not started
bisa workflow use <goal> <id|slug>   # choose or change the workflow of a draft
bisa run <goal> --input k=v --watch  # start the run, and follow it — or follow the live one
bisa run <goal> --new                # another run, queued behind the live one
bisa runs <goal>                     # every run, newest first, a queued one with its place
bisa stop <goal> --rationale "…"     # the live run stopped, the queue withdrawn; the goal stays open
bisa restart <goal>                  # a new run of the last run's workflow and inputs, at once
bisa status <goal>                   # status, workflow, one line per step with its mark
bisa log <goal>                      # the signed journal as an activity timeline
bisa close <goal> --rationale "the market moved"
bisa close <goal> --superseded-by <other-goal>
```

## The Goals screen

The desktop's **Goals** is one flat, live list — no sections, no lifecycle buckets. Each goal is a
**card** in three compact lines: its status glyph, title, the holder word, a *guided* chip while the
Workflow Agent designs, its tags and when it last moved; its statement, dimmed; then the run — every
step as a filled chip toned by state (the current one larger and ringed; a long workflow folds to one
line with a *+n* pill, the current step never hidden), *n of m steps*, the current step named, the
workflow's name, who is assigned, how many projects it carries, how many things wait on you, and —
when the move is yours — an **Act** button that answers, marks done or releases right there. The list orders itself by last activity, so what moved is at the top, and narrows by
filters that live in the URL: who holds it (*All · You · Agents · World · Finished · Design*), which
workflow, tags, text. Clicking a chip opens the goal's Workflow tab on that step. There is no goal
section in the sidebar; this list is the one place.

## Following a run

Opening a goal opens on **Progress**: the run read downward, one row per step of its workflow — the
kind, the name, its state, who holds it, when it started and how long it took — with the live steps
open. An open row shows what the step produced, what went wrong, its work item (*Open the work item*
shows the transcript), and the verbs the step admits right now: answer, mark done, release. Above
the tabs, **Your move** holds everything owed to you on this goal — every question and gate as the
same card the Inbox shows, including one rebuilt after a restart — so you answer here or there, and
the Inbox keeps it either way. A restart of the node itself is a note in Activity, not a lost goal:
the steps it interrupted resume on the work they had or run again, and a decision it could no longer
take — a publish, a guard's ask — is withdrawn with a line saying where to ask again. A goal whose run file this node cannot read wears *run unreadable*
and is drawn as a goal with no run; the diagnostic log names the file. The header names the goal by its words — the title with the statement under it, or the
sentence alone when it has no title, never its id — with an arrow at its left back to the Goals list,
then the holder and the run as a large strip with *n of m steps*; the run's verbs — *Start run…*, *Adopt and start…* or *New
run…* (queued behind a live run), and *Stop* while a run is live or queued — the
details toggle, and a menu (*Conversations about this goal*, *Choose workflow…*, *Promote to
library* on a goal with its own design, *Assign…*, *Projects…*, *Restart* on a goal that ran, and
the closing and retiring items). *Close goal…* asks two things, both optional: **why** — a word kept
with the goal, which is then recorded as *abandoned* — or the goal that **replaces** this one, picked
among the goals that still stand, which records it as *superseded* by that goal and keeps no other
reason beside it; the dialog says which of the two will be recorded before you press. Under the rows, **Runs** lists the goal's runs
newest first — its number, its status with its place in the queue or why it was cancelled, when it
was made or started and when it ended, *View* into the Workflow tab wearing that run, and
*Withdraw* on a queued one. A goal that listens keeps every run it ever made: the list draws a page
of them, never fewer than the runs still to end, and *Show older* brings the next. **Conversation** is the goal's thread; **Workflow** is the canvas, with
*Edit the steps* before a run and the *Designed for this goal* banner. The Details pane on the right
keeps the run's card, work, projects, files, assignees and origin — a goal a run's `spawn` step
captured has a door to that run there.

## Stopping and restarting

**Stop** ends what the goal is doing without closing it: it stops listening, every session on the
goal is ended, every queued run is withdrawn, the live run is cancelled — its work items cancelled, its questions
withdrawn, its waits disarmed — and the goal reads `draft`, its workflow still chosen, ready for
*New run…* or *Restart*. The confirm says what goes: how many steps are live, how many runs wait.
**Restart** cancels the live run and starts a new run of the same workflow with the same inputs —
at the start the last run began at, on the event that began it — at
once, ahead of anything queued (which keeps its place); it is refused, and the run left as it is,
when that start is gone from the workflow as it stands; on a goal whose run is over it only starts
that run again, with no confirm. **New run…** while a run is live queues the run — *Queued — it
starts when the live run finishes* — and a queued run can be withdrawn from the Runs list until it
starts. A stop or a restart is your own act: the Pulse says *run … stopped* or *run … restarted*
under Goals, and nothing lands in the Inbox. The same three verbs sit behind the `⋮` on a goal's
card in the Goals list, between *Open* and *Delete…* — the retirement dialog, as on the goal's page,
so a goal goes from the list too. A goal's run is stopped and restarted from its goal alone: *Stop every run* /
*Restart every run* on a workflow's card act on that workflow's runs in the workspace, and a goal's
run of it goes on ([Workflows](workflows.md#running-waiting-amending)).

```sh
bisa stop <goal> [--rationale "…"]   # → run … stopped, n queued runs withdrawn
bisa restart <goal> [--watch]        # → restarted: run …, then followed
bisa runs <goal>                     # newest first: id · status · rev · position | started · finished · cause
```

## Auto, guided or manual

A goal has a **mode**, chosen when you capture it — the dialog's *Auto · Guided · Manual* switch,
`--mode` on the CLI — and it starts on the workspace's default, **auto** (Settings › Automation ›
Goals, `goals.default_mode`). The capture asks you for no workflow. **Who carries it** is optional
and closed until you open it: pick agents and teams there — `--assignee agent:<id>` or
`--assignee team:<id>` on the CLI, `bisa assign` or the goal's *Assignees* afterwards — and in every
mode the Workflow Agent staffs each step from them alone, a team whole or one of its members; pick
nobody and it staffs from every agent and team that is installed and enabled. A goal spawned by
another inherits its parent's pick until it is given its own. The platform's own agents and a
disabled agent are never offered.

**Auto** is the goal that runs itself. The Workflow Agent reads the goal, decides for itself where
the shape is unclear, gives every input a default, starts from the closest template, validates its
design — and the platform adopts it, starts the run, and, when a run fails, takes the agent's
corrected workflow and starts again. The run is unattended, and behaves so: a step that may write
runs commands too — `cargo test` on a step that allows writes runs on its own, the guard's rules
and the redactor first as ever (Settings › Automation › Goals, *A step's ceiling in an auto goal*,
`goals.auto.ceiling`; *The step's own* keeps each step's ceiling instead, and a `read` step stays
read-only either way); a tool an agent wants above that ceiling that no guard rule decides is read
by the classifier, not put to you (*Above a step's ceiling in an auto goal*,
`goals.auto.permissions`; a call the classifier finds harmful still reaches you), and every step's
agent is told to decide with defaults rather than ask. Its agents browse **out of sight**: a page they open renders and answers their tools without
opening beside you, and the footer's Browser count shows it on request ([the
desktop](the-desktop.md#the-browser-pane)). You are asked only where a person alone can act: a `human`, `approval` or release step the
workflow hands to you, a guard rule's question or refusal, a command the classifier finds harmful
or cannot judge, a push or pull request under a gated project, and a question the agent still
chooses to ask. Two things make an auto goal stop and
ask after all, and the card says which: a design whose required input has no default, and a goal
that has failed more times than `goals.auto.repair_limit` (three) allows — so a goal that keeps
failing never loops with nobody watching. The thread gets one line per proposal — *I designed X
and started it* — and no running commentary.

**Guided** is the interactive goal: the Workflow Agent reads the goal, asks you at
most one round if the shape is genuinely unclear, starts from the closest template, validates its
draft and **proposes** it. The goal then **opens on the proposal** — its Progress tab is the card, and the same card waits in
your inbox. The card **is the proposal**: its description, every step with a one-line summary, who
does it, where it branches or loops, the inputs a run needs — with the doors: **Adopt and start**
(the inputs asked for right there), **Request changes…** (your words go to the Workflow Agent in the
goal's thread; it proposes again and the new proposal replaces the card), **Edit the steps** (opens
the goal's **Workflow** canvas, where every step is yours to change; saving makes the plan your own
design, and you start it), and **Decline**. Each canvas save is a new revision, re-validated. A question the agent asks on the way is one you type an answer to — never a
bare approve/decline — and your answer resumes the design. From the CLI:

```sh
bisa inbox
bisa approve <goal> --input project=<pid> --input feature="dark mode"   # adopt, and start
bisa approve <goal> --no --rationale "too many steps"                    # decline; the goal stays a draft
```

From the moment you capture, the goal page tells you where the agent is. The dialog says *Captured —
the Workflow Agent is designing how it runs*, and the goal's Progress and Workflow tabs show a card
with the agent's standing — *about to start*, *designing* with the elapsed time and its latest
action, *asked you a question* (answer it in *Your move*), *a workflow is proposed*, *stopped
without a proposal* or *could not start*, each with the reason — while the header's working dot,
the sessions roster and the companion light up. The agent also talks in the goal's **Conversation**
tab as itself: a greeting when it starts, its question, the proposal, and a stall with the way out —
and a `notify` step's line there is the agent's it names, or the Workflow Agent's, never yours.
Every move is in the journal too (`bisa log <goal>` prints them; `bisa status <goal>` has
a `design:` line). When it stalls or cannot start, *Retry design* — or `bisa design <goal>` —
asks it again, and *Pick a workflow* is always there instead. A node started with designing off
(`design_enabled = false`) says so on the card and offers the designer and the picker.

On a guided goal an agent proposes and only a person adopts; nothing runs until somebody does. The proposal is **the
goal's design**, not a library workflow: it appears on the goal's Workflow tab under a *Designed for
this goal* banner and nowhere in *Workflows*; declining leaves it there to edit and adopt, or to
replace; **Promote to library**, in the header's menu, copies it out when it deserves reuse
([`workflows.md`](workflows.md#the-library-and-a-goals-designs)).

**Manual** is the goal you design. The capture wakes nobody and opens the goal's Workflow tab in
the designer on a blank canvas: draw the steps, or pick a workflow from the library or a template
with *Choose workflow…*, and start it. The tab's **Agent** mode is there for help — the goal's own
conversation in the tab, where `@Workflow Agent` asks for a draft of the whole thing, a step, a fix —
and what it proposes on a manual goal lands as **your draft** on the canvas, never as a gate; `bisa
design` is refused on a manual goal by name. `--workflow` on
any mode names the workflow yourself and starts it at once unless `--no-start`.

## What a run looks like

A run walks the workflow's graph from the start it began at. Each step is one of eighteen kinds —
the events `start`, `wait`, `emit`, `end`; the gateways `decide`, `if`, `switch`, `judge`,
`parallel`; the loops `for_each`, `while`; the tasks `agent`, `human`, `approval`, `check`,
`connector`, `notify`, `spawn` — and each kind is moved by somebody different: an agent's session, you, the engine,
the Decision-Making Agent, the world, an outside platform, or nobody at all. What each does and who moves it is the table in
[03 — Workflows](../architecture/03-workflows.md#the-eighteen-kinds).

`bisa status` prints one line per step with a mark — pending, running, waiting, done, skipped,
failed, cancelled, diverted (a boundary event took the run down its own path) — and the desktop draws the same run on the designer's canvas, live. A step that
fails follows its workflow's `on_fail`: fail the run, skip ahead, or route to a remediation step;
retries come first. A loop — review sends work back — is bounded by the step's `max_visits`.

## Questions, and the three answers

A `human` step, or an agent's `ask_human` mid-step, stops and asks. A **decision** is approve or
decline; an **answer** is you telling it something, optionally from a list of options it offered
(`id`, `label`, a line of detail, at most one marked recommended).

```sh
bisa inbox                                  # what is waiting, with the offered options
bisa answer <goal|gate> "…"                 # tell it something
bisa answer <goal|gate> -o <option-id>      # pick what it offered (repeatable)
bisa answer <goal|gate> -o <id> "…"         # pick one and qualify it
bisa answer <goal|gate> --unsure            # you do not know
bisa step answer <goal> <step> "…"          # the same, naming the step
bisa step release <goal> <step>             # let a wait step through
bisa step done <goal> <step>                # mark a human step done — an interview happened, a call was made
```

**The options never limit the answer.** Free text and `--unsure` are valid on every question,
including one the daemon reconstructed from durable state with no list at all. `--unsure` is not a
decline: it resolves the question without deciding it and steers the agent to ask something
narrower — *"which database?"* becomes *"does it need to survive a restart?"*. After three unsure
answers on a goal the agent is told to proceed on its own recommendation and record the assumption
as a note. An empty answer is refused rather than recorded. On a `human` step of a run, the last
unsure answer fails the step with your words, and the run as the step's `on_fail` says — the gate
is spent only on a decision the run took.

Decide gates with `bisa approve [--no]`; keep `bisa answer` for questions. The inbox
labels which is which — `?` for a question, `⏸` for a gate, `•` for a notice you have not read — and
prints the right command under each row.

## Work items

An `agent` step creates a **work item**: the unit a harness session actually runs, with the step's
instructions rendered for it — a fresh session can act on them with no knowledge of the
conversation — the project the step names, the JSON Schema its result must satisfy, and an ordered
harness fallback chain. A work item has its own small lifecycle — `open → claimed → in_progress →
review → accepted | rejected`, with `blocked` and `cancelled` beside it — and its transitions are
typed too. A rejected result carries the validation errors back into the session; a settled item in a
git workstream is committed by the executor with an `bisa-work-item:` trailer. When the item
settles, its step is done — or failed, with the reason. The session's first prompt names the goal
it serves — its title and its statement — so a workflow's instructions never have to spell the goal
out.

Which agent *took* the item is the engine's answer: a step's `assignee` wins alone when it is set;
otherwise the goal's, the attached projects' and the parent goals' assignees are pooled and the
enabled, available agents among them are candidates. No request body has a field for the runner.

```sh
bisa assign <goal> agent:developer team:engineering human:<pubkey>
```

## Amending a running workflow

A run carries its own frozen copy of the workflow, so editing the library changes nothing in
flight. On the desktop a running workflow is read-only — no amendment on the tab; the goal's
conversation stays on its Conversation tab — so a run is changed only by the Workflow Agent's own amendment or by
`bisa amend`: steps that have not started may be edited, removed or added; everything that ran is
history. On a guided goal a step that fails outright wakes the Workflow Agent
in *repair*, and its corrected workflow waits for your adoption like the proposal did — a finished
run is not amended — an Inbox row with the whole proposal, still there after a restart; on an auto
goal the corrected workflow is adopted and started again without asking — up to `goals.auto.repair_limit` failures, after which the
next proposal waits for you. While a run is going, the goal's Details pane and its Workflow tab are
read-only — assignees, documents, files and the workflow itself wait for the run to finish.
A queued run is not the current one: an amendment is the live run's alone, and choosing another
workflow, proposing or designing waits until nothing is live and nothing is queued.

```sh
bisa amend <goal> --from fixed.json         # the whole workflow, JSON or TOML
bisa approve <goal>                          # an agent's proposed amendment
```

## Archiving and deleting

A goal that is done with — or one you want stopped for good, mid-run — can be **archived**, put
away, or **deleted**, and either is one dialog that says everything it will do before you press:
*Archive goal…* and *Delete goal…* in the goal header's menu open it with that choice made, offered
while a run is going too, and you can change your mind inside — the dialog's title and its button
follow the choice. What the dialog lays out, section by
section: **Delete is refused** when a design of the goal's own is still used by another goal — said
first, before anything stops, with archive still open; **what runs** — *Its run is cancelled: 2
steps are live*; **what stops** — *2 agent sessions aborted, 1 harness terminated and 1 shell
closed*, the engine's sessions and this desktop's own terminals rooted at the goal and at the
projects the plan touches, in the words every close on the desktop uses; its own designs, which go
with a deleted goal and stay under an archived one; the **projects made for it** — each with its
workstreams, its sessions and whether it is an adopted folder — with one choice for all of them,
*Keep · Archive · Delete*, and under Delete a box to move their managed folders to the Trash (an
adopted folder is never moved); and the projects merely **attached**, which are detached on delete
and never deleted. The one button reads the plan back — *Archive goal and 2 projects*, *Delete goal,
keep projects* — and is red only when something is deleted.

What happens when you press, in this order and no other: the refusal, if any; every session on the
goal is stopped — an engine agent's harness process is aborted on the spot, not on its next word; a
harness you opened in a terminal is terminated by closing its tab — and the run is cancelled; the
node waits, up to five seconds, for the rows to end (a harness that ignores the termination signal is reported, never
waited on for ever); the projects take their fate; then the goal is archived or its folder goes. The
dialog closes the moment the node answers, the tabs close as one move without a second question,
and the goal's screen leaves on its own — it hears the deletion on the bus, so it leaves the same
way when the goal is deleted from a terminal or another desktop.

**Archived is a mark, not a status.** An archived goal is a closed goal: archiving an open one closes
it first, stopping its live run, withdrawing its queue and its questions. It leaves the Goals list until you flip
the **Archived** switch in the filter bar, stays readable, and comes back with *Unarchive* — still
closed. From a terminal:

```sh
bisa archive goal <goal>            # close if open, put away
bisa archive goal <goal> --undo     # take it back out
bisa rm <goal> --projects keep      # delete; the projects born of it kept, detached
bisa rm <goal> --projects delete --tree
```

## Documents — the context you give a goal

A goal often starts from something you already have: a brief, a spec, a screenshot, a spreadsheet.
Give it to the goal as **documents** — in the capture dialog's *Documents* section (add files or
drop them; each uploads as it is chosen and *Capture* waits for the last), from the goal's
**Details** panel later (*Add…* on the Documents card), or from a terminal:

```
bisa new "Ship the checkout" --document ./brief.pdf --document ./mockup.png
```

They live under `goals/<id>/documents/`, named as you named them (a second `brief.pdf` becomes
`brief (2).pdf`), listed on the Details card and on the Files panel like any file the goal holds,
and recorded in the journal as `document` facts — so they sync with the goal, and a peer that
fetches the bytes gets the folder too. Whoever works on the goal is told: the Workflow Agent at its
wake, every work item in its first prompt and any session asking `get_goal` see the folder's
absolute path and each name, and read what is relevant before deciding something you may already
have settled. A document is context, not work: it is not where a step's files go, it is not editable
from the IDE, and there is no removal — give a corrected one and the older stays in the record.

## Where work runs

One rule: **a session runs in the folder of the thing it works on, inside the workspace.** An
`agent` step that has a project — the one it names, or the goal's only attached one — runs in a
workstream of it, a branch and a checkout under `projects/<slug>/workstreams/`; what it leaves is
committed when it settles, and a step that left nothing has its worktree closed and its branch
deleted, so a step that only read leaves nothing behind. A step with no project on a goal with none
runs in the goal's `scratch/`: it reads, analyses and answers there, its result is its deliverable,
and no project is made for it. A project is made only when files must be kept — the Workflow Agent's
`create_project` before it proposes, an agent's when it was asked for files, or yours from Projects —
and a goal with several needs every file-producing step to name one through a `project` input, or
the run is refused at start with the step's name. A sub-goal a `spawn` step makes inherits its
parent's projects. `scratch/` also holds the Workflow Agent's design session, a `check` command when
the goal has no project, and `TMPDIR`. There is no caller-supplied working directory anywhere on the
path. This is placement, not a sandbox: it
decides where work *starts* and where the platform itself writes, and it makes an escape visible
rather than impossible.

```sh
bisa files tree goal <goal>                 # the goal's folder, annotated
bisa files tree work_item <item>            # its workstream while the checkout exists, else the goal's scratch/
bisa files show goal <goal> --path scratch/notes.md
```

## Budgets

A goal carries a budget — tokens, cost, wall time — spent by its sessions and recorded in its
ledger. An exhausted budget fails the step that hit it; the workflow's `on_fail` says what that means
for the run. Raise the budget and amend, or close the goal. `bisa status` shows what was spent
against what was promised.

A goal made without a budget of its own takes the **workspace default**: the three settings
`budget.default.max_usd_cents`, `budget.default.max_tokens` and
`budget.default.max_wall_clock_secs` (Settings › Automation › Budgets; workspace or machine scope;
zero is no ceiling). That is the ceiling a standing goal — one that listens, and runs every day for
a year — spends against before its run stops, its listening pauses and a notice reaches you; the
goal itself stays open. A run in
the workspace takes the same default and keeps it as its own ceiling, frozen when it starts. A
library workflow that is turned On may carry a budget of its own for each run its events start,
which wins over the default, an empty one meaning no ceiling at all; the *Turn on…* dialog,
`bisa workflow on --budget-usd-cents …` and `PUT /workflows/{wfid}/listening` (`budget`) all set it.
Nothing is set by default: a fresh workspace runs unlimited.

## Sub-goals and edges

`spawn_sub_goal` (an MCP tool every session has) and a workflow's `spawn` step capture a new goal
that **refines** this one — the one edge with a writer. The edge is recorded on both goals and syncs with them.
A `spawn` step names a workflow the child can begin by hand: one that begins on events alone is
refused when the definition is saved.
A `spawn` step of a run in the workspace has no goal to refine: the goal it captures is a top-level
one, in the workspace's default mode, whose origin names the run and the step — kept after the run
itself is gone, since an origin is history.

## What is in the journal

Every fact is a signed Nostr event: notes, decisions, questions, claims, progress, results, per-turn
metrics, attachments, documents, the signal that began a run — and the run's own facts: a run queued or started, a step started, waited,
finished, failed, was diverted by a boundary event, was answered, decided, skipped or cancelled, a run amended, finished or cancelled —
stopped, restarted, withdrawn or closed.
`bisa log` renders it as an activity timeline; the desktop's Pulse holds every goal's journal
beside the messages and the engine's own facts, under *Goals*. Kinds are in [`reference/gep.md`](../reference/gep.md).
