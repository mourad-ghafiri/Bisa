# Workflows

A **workflow** is how work runs: a small graph of steps — a run begins, an agent works, a check
judges, a person answers or approves, the world is waited for, a branch is chosen, paths run side by
side, a list is walked, an outside platform is called, a signal is raised, a child goal is spawned —
with typed inputs a run is started with. A run is either a **goal's** — the goal's workflow, run for
that goal — or a **run in the workspace**, started from the library with no goal at all: *Run…* on
the workflow, `bisa workflow run`, or an event the workflow listens for. Design one by hand in the
desktop's designer, take one of the catalog's templates, or let the Workflow Agent propose one for a
goal and adopt it. **A workflow says what starts it**: its `start` steps begin a run by hand, on a
schedule, when it is called, when something is said, raised or changed —
[Events and gateways](events.md) is the guide to those. The design is
[03 — Workflows](../architecture/03-workflows.md).

## The eighteen kinds of step

A step is one of eighteen kinds, in four families:

| Family | Kinds | What they are |
|---|---|---|
| **Events** | `start` · `wait` · `emit` · `end` | something that happens: a run begins, holds for something, raises a signal, ends |
| **Gateways** | `decide` · `if` · `switch` · `judge` · `parallel` | a step that routes: one path, every path that holds, or all of them at once |
| **Loops** | `for_each` · `while` | a body run once per item, or while a condition holds |
| **Tasks** | `agent` · `human` · `approval` · `check` · `connector` · `notify` · `spawn` | a step that does work |

What each does, its fields, who moves it and the effect the engine performs are one table in
[03 — Workflows](../architecture/03-workflows.md#the-eighteen-kinds). Every step also has `then`
(the flows out of it — a bare step id, or `{ to, branch }` after a step that branches), `join`
(`all`, the default: every arm arrives; `any`: the first is enough; `one`: exactly one may — a
second arrival fails the step), `on_fail` (`fail` the run — the default — `skip` ahead, or `then {
step }` to a remediation step), `retries`, and `max_visits` (three, the bound on every re-entry by a
flow back). A step whose work can be stopped — an `agent`, a `human`, an `approval`, a `wait`, a
`spawn` that waits — may carry **boundary events**: a timeout, a reminder, a message or a signal
that diverts it down a path of its own or acts beside it
([Events and gateways](events.md#boundary-events)).

**Starts.** A `start` step is one way a run may begin: `on = { event = "manual" }` by hand, or on an
event — a schedule, a hook call, a message, a named signal, a project's change, a run's end, a
platform topic, a connector poll, a check. A workflow may have several; a run enters the one it
began at and the others are skipped. *New workflow* and every catalog template begin with an
explicit start by hand.

**Gates.** A `decide` step takes a rule list and takes the first rule that holds — or, with
`pick = "every"`, every rule that holds, `otherwise` only when none does; an `if` step takes one
condition and its flows are labelled `yes` and `no`; a `switch` step renders a template and takes
the flow whose case value matches it, else `otherwise`; a `parallel` step takes every flow out of it
at once, and the step its paths meet at joins them. Conditions are the closed set 03 lists, and they
compose: `all`, `any`, `one` (exactly one) and `not` take conditions of their own.

**Judging.** A `judge` step is a fourth kind of branch — the Decision-Making Agent's, not a rule's: it reads
a rendered `state` and asks the Decision-Making Agent one question, `instructions`, offering a branch per
`option` and `meaning`; the flow whose branch it chose is taken, `otherwise` when it is not sure
enough or gives no answer that holds to the contract — the step never fails for that. Naming a
`judge` step switches the Decision-Making Agent on for it, whatever the workflow's own switch says
elsewhere. See [The Decision-Making Agent](decisions.md#the-judge-step).

```toml
[[workflow.steps]]
id = "sev"
name = "How bad?"
kind = "switch"
on = "{steps.triage.output.severity}"
cases = [{ value = "sev1", branch = "page" }, { value = "sev2", branch = "ticket" }]
otherwise = "note"
then = [{ to = "page-oncall", branch = "page" }, { to = "open-ticket", branch = "ticket" }, { to = "leave-a-note", branch = "note" }]

[[workflow.steps]]
id = "worth-it"
name = "Ship it?"
kind = "if"
when = { condition = "all", of = [{ condition = "outcome", step = "tests", passed = true }, { condition = "not", of = { condition = "answered", step = "review", option = "hold" } }] }
then = [{ to = "publish", branch = "yes" }, { to = "done", branch = "no" }]
```

**Loops.** A `for_each` step renders `items` to a JSON array and runs its `each` flow once per item
— the body reads `{steps.<loop>.output.item}` (and `.index`, `.count`) and its last step flows back
into the loop step — then takes `done`; a `while` step tests `when` on every entry and takes `loop`
or `done`. Both carry `max_iterations` (a hundred by default; more items than that, or a `while` that
gets there, fails the step), and the body's steps start each iteration afresh, so their own
`max_visits` bounds a rework loop inside one iteration rather than the number of iterations.

```toml
[[workflow.steps]]
id = "each-issue"
name = "For each issue"
kind = "for_each"
items = "{steps.find.output.issues}"
max_iterations = 50
then = [{ to = "summarise", branch = "each" }, { to = "report", branch = "done" }]

[[workflow.steps]]
id = "summarise"
name = "Summarise one"
kind = "agent"
instructions = "Summarise issue {steps.each-issue.output.item.key} in three lines."
then = ["each-issue"]
```

**Connectors.** A `connector` step calls one operation of an outside platform — Slack, Jira,
Gmail, Notion — through a connector installed here; see [Connectors](#connectors) below and the
[connectors guide](connectors.md).

Strings may hold
the placeholders 03 lists — `{inputs.<name>}`, `{steps.<id>.output.<field>}`,
`{goal.statement}`, … — an unknown one is refused when you save, and one with no value when the step
runs fails the step rather than reaching an agent as text, and says why: which step, and whether it
has not run yet, failed, yielded nothing or yielded other fields. A brace that is not a placeholder
is doubled: `{{` and `}}`. **The event that began a run is read in one place**, its start's input
mapping — `inputs = { ticket = "{event.payload.body}" }` — and nowhere else: every other step reads
the typed inputs the mapping filled, so a run by hand, a test run and a run an event began look the
same to the steps. `{goal.statement}` and `{goal.title}` read the goal a run serves, so a
workflow whose steps read them **runs on a goal only**: a run in the workspace has no goal, and
starting one is refused before anything is written — `needs_goal`, one problem per step that reads
the goal. Nothing needs to: an agent step of a goal's run is told its goal's title and statement in
its first prompt, which is why no catalog template reads `{goal.…}` and every one runs in the
workspace too.

An `agent` step may pin the **model** it runs on and the **effort** the model works at —
`model = "claude-sonnet-5-5[1m]"`, `effort = "xhigh"`, or `effort = "auto"` for the Decision-Making
Agent to name it — for the one step that needs more, or less, than its agent usually gives. A pin
wins over the agent's own; a pinned model is never substituted, and a pinned effort is fitted to
what the model takes ([Agents and teams — Effort](agents-and-teams.md#effort)). Saving refuses an
effort on a step whose harness has no such control (`unsupported_effort`).

An `agent` step says what it produces in words and declares the shape as its `output_schema` — a
JSON Schema whose `required` names every field a later step reads. The session is shown the schema
with its prompt and its result is checked against it (three attempts, then the step fails with the
schema's own words), so the instructions never spell the shape as JSON; every catalog template is
written this way. Saving holds you to it: a field a later step reads must be required by the
producer's schema (a step with no schema promises nothing), a `human` step yields an answer and
not an output, and a step reads only what is **sure to have run before it on every path** — the
head of a rework loop cannot read what the loop's later steps produce (on the first pass they have
not run; give the second pass its own step, or read the result after the decision), a fan-in with
`join: any` cannot read an arm, and nothing reads a step that may have failed and been passed over
(`on_fail` skip or then). A rule — `output_equals`, `answered` — is not held to the order, since
an absent value reads false; that is how a `while` loops until its body says done. A cron, a delay's seconds or a channel read from an input is checked when the run
starts, with the value you gave, rather than when the step is reached.

Inputs are `text`, `number`, `bool`, `choice { options }`, `assignee`, `project` or `account {
connector }`, each with a label, a default and `required`. A step that needs a person, a project,
a delay's seconds, a schedule's cron or a connector's account can name an input instead of a value
— `{ "input": "project" }`, `secs = { input = "hold" }`, `account = { input = "slack" }` — which is
how a template stays reusable: it cannot know your pubkeys, ULIDs, moments or accounts. Every input
you declare has to be read by some step; one nothing reads is a problem.

On the command line an input is `--input name=value`, and the value is read by the kind the input
declares: `--input tests=true` is the command `true` where text is asked and a yes where a yes or a
no is, `--input release=2.4` the word `2.4` where text is asked and the number where a number is.
A word that is not its kind is refused by name when the run starts.

A **project** you give a goal's work — `--input project=<id>`, or picked in the run form — is
attached to the goal by the start: you said where the work is done. The run form says so under
the field; for a run in the workspace it says the steps work there and nothing is attached. An
`account` input is picked among this machine's accounts of its connector, the default first. A project named by anything
else — an event's payload, what a step hands a child — is attached by nobody, and a step that
would work there is refused until you attach it.

A `spawn` step says what its child's run is given: `inputs = { report = "{steps.cause.output.root}",
project = "{inputs.project}" }` — an input of the child's workflow to a template of this run. Every
input the child requires and no default fills has to be given; one left out, or one the child does
not declare, is a problem (`spawn_input`) before anything runs. In the designer the step's form
lists every input the child's workflow asks, marks the ones its run needs and the step has not
given, and shows what the child does not ask for so it can be removed.

A `check` command runs under `sh -c`, and every value substituted into it is **quoted as one word**:
an input holding `a; touch marker` is printed by `echo {inputs.x}`, not run. To let an input carry a
whole command line, the template says so — `command = "sh -c {inputs.command}"`, which is what
`standing-health-check` does.

## From a template

```sh
bisa workflow list --templates              # the catalog's templates, marking what is installed
bisa workflow show software-feature         # inputs, steps, problems
bisa catalog install workflow bug-fix       # into your library, with the agents its steps name
bisa new "Fix the login redirect" --workflow bug-fix --input project=<pid> --input report="…"
```

`bisa new --workflow <slug>` installs the template on first use, so the second line is optional.
Installing is transitive — a workflow brings the catalog agents its `agent` steps name, and each
agent its skills — idempotent, and never overwrites something you made. Every template, its inputs,
its steps and its tags are in [`reference/catalog.md`](../reference/catalog.md#workflows-13).
Every template begins with a start by hand, and four begin on an event too — the reference says what
each **starts on**: `weekly-review` on a schedule (Monday 09:00 unless you say otherwise),
`customer-support-triage` when its hook is called with a ticket, `standing-health-check` when its
check starts failing, `incident-response` when a run fails. A template installs **Off**: it hears
its events once you turn it on, and a run started by hand works either way
([Events and gateways](events.md#listening)).

## By hand

**Desktop.** *Workflows* in the sidebar is the library — *Yours · Templates*, every workflow and
every catalog template a card with a **thumbnail of its graph**, the picture the designer opens on
(a template's before it is installed), in sections by domain tag; *New workflow*, *Use template*.
One bar narrows either view: **search** by a word in the name, the description, the slug, a step's
name or kind or a tag; a **status** — *All · Runs · Has problems · In use* on yours, *All · Installed
· Not yet installed* on templates; the tag facets; **Archived** on yours for the ones put away. The
filters live in the address, so a link carries a search and Back restores it. *New workflow*
**creates the workflow at once** — *Untitled workflow*, a draft holding one step, *Start · by hand*,
which the first step you draw follows — and opens its designer: it is a workflow from its first second, so
the Agent pane, the conversations, the browser door and `bisa workflow …` all have it, and the
library lists it when you come back. Every card leads with **one state line** — *ready to run*, *2
problems*, *running 2 runs*, *running in one goal*, *runs on a goal*, *archived* — then what it is,
how big and where from (*3 steps · 2 inputs · yours*), its holders and its tags; and every card has
a **`⋮`** (on hover, on focus, or open): *Open*; *Run…* when it can run in the workspace — no
problems, not archived, no step reading the goal; *Turn on…* when it begins on an event and nothing
stands in the way, *Turn off* while it listens — a workflow that listens wears **On** beside its
state, *Paused* when it stopped hearing; *Stop every run* and *Restart every run* while a
run of it in the workspace goes; *Delete…* always, the designer's own dialog (what still uses it, and
Archive when a goal, a `spawn` step or another workflow's start does). A
card opens the **designer**: a palette of the eighteen kinds on the left, under four headings —
*Events*, *Gateways*, *Loops*, *Tasks* (drag one onto the canvas —
it lands where you drop it, beside a card it would cover), the graph in the middle, and on the right
one panel with a rail of three icons at the screen's edge — **Properties**, the selected step's form
(one per kind) or the workflow's own; **Agent**, the conversation about this workflow; and **Runs**,
its runs in the workspace. Press an icon to open the panel on it, again to close it; drag its edge
wider or narrower, and it is remembered; clicking a step shows its properties. The designer comes
back as you left it: leave for the Inbox and return by any door — the card, an Inbox row, the
Pulse — and the step you had picked, where the canvas looked and the conversation the Agent pane
was on are there again. The arrow at the header's left is the way back to
*Workflows*. The header carries the workflow's **On/Off** switch when it begins on an event — *Off*,
*On — listening for every Monday 09:00 · next Mon 09:00*, *Can't turn on: 2 problems* — and turning
it on asks the inputs no event supplies and a budget per run. Connect steps by dragging a handle; a
step that branches — `decide`, `if`, `switch`, `judge`,
`for_each`, `while` — has one handle per branch, and renaming a `decide`'s branch carries its rule
and its flows along; a step's boundary events are chips on its lower edge, one that diverts with a
handle of its own; renaming a step or an input
(committed when you leave the field) rewrites every placeholder and reference that reads it. An
output schema or a check's schema is typed as JSON and committed when you leave the field — it says
*Not JSON* until it parses. **A card stays where you put it**: its position is saved with the
workflow (`position = { x = …, y = … }` on the step, in a TOML file too), a drag moves it and nothing
else, and connecting two steps moves neither. A workflow nobody has placed yet — a template you just
installed, a proposal the Workflow Agent made — opens laid out top to bottom by a layered layout
(dagre), the same way every time, and the first thing you change on the canvas writes that picture
down as yours. **Tidy**, beside *Fit*, lays every card out again from the flows and puts the steps in
reading order — one edit, one undo. A loop comes back around the right side and an on-fail flow leaves
from the left. Select a flow and press `Delete` to
remove it; `Delete` on a step removes the step, `⌘D` duplicates, `⌘Z` and `⌘⇧Z` undo and redo, `Esc`
clears; snapping, the grid and the minimap are `workflow.designer.snap`, `workflow.designer.grid` and
`workflow.designer.minimap` in Settings. Every edit is validated as you type and the problems list
names the step and the rule; a save is a new revision after `workflow.autosave.delay_ms` of quiet
(or ten such delays after your first unsaved edit, if you never pause), and a save mid-edit with
problems is kept — it just cannot start until they are gone. A step dropped with a choice not made
yet — a `connector` step before you pick its connector and operation, an `if` before its
condition — is a problem row, *a choice not made yet*, never a refusal. A save never loses what
you typed while it was on its way, and undo after a save is just undo. Leaving the designer keeps
your edits: the last ones are sent on the way out, and quitting with unsaved edits asks first. If somebody else
saved the same workflow first, a banner says so and offers two exits — *Keep mine on top of theirs*
or *Take theirs* — and nothing is saved until you choose. A save the node refuses for another
reason — a public hook start taken away while its workflow is On — is no conflict: the header says
*not saved* with the node's own sentence, the draft stays, and the save goes once the cause is
gone. A workflow a goal is running right now is
**read-only** until that run finishes — the canvas, the inspector and the palette refuse edits, and
the *Running in n goals* banner names the goals; the runs keep the copy they started with. A run in
the workspace freezes nothing: it runs its own copy, and the workflow stays yours to edit while it
goes.

**The Agent pane.** The panel's second icon (`⌘⇧M`; `⌘⇧D` is Properties, `⌘⇧R` Runs, `⌘⌥B` shows
or hides the panel) is the conversations about this workflow — listed, searched, started in one
click and named later, each a thread beside the canvas where `@Workflow Agent` is one mention away;
the one you were on is remembered for the workflow, so coming back lands on it. Ask it to
analyse, fix, design or finish the workflow: it reads the inputs, the steps and the runs, validates
until clean and **saves** the workflow as a new revision, and the canvas beside it shows the change
as it lands. A new workflow is there to converse about from its first second — ask the agent to draw
the whole thing on the empty canvas; its save is refused while the definition has problems, so what
it writes is complete. If you were mid-edit when it saved, the banner offers *Keep mine on top of theirs* or *Take theirs*, as for
any other writer. A goal's Workflow tab is the canvas alone; the goal's conversation is its
Conversation tab.

**Let the Decision-Making Agent decide.** The inspector's properties — name, description, tags — carry one
more switch: *Let the Decision-Making Agent decide in runs of this workflow*. On, every decision point a run
of this workflow reaches asks the Decision-Making Agent first, whether or not the workspace's own switch
is on — the pick of a pool a step's work goes to, whether an auto goal's design reaches the goal. A
`judge` step asks it either way, switch or no switch. See [The Decision-Making Agent](decisions.md).

**CLI.** A workflow is JSON or TOML — the same shape the catalog templates are written in:

```toml
[workflow]
name = "Fix and review"
description = "Fix a bug, prove it, review it."

[[workflow.inputs]]
name = "project"
label = "Project"
kind = "project"
required = true

[[workflow.inputs]]
name = "report"
label = "What goes wrong"
kind = "text"
required = true

[[workflow.steps]]
id = "fix"
name = "Fix it"
kind = "agent"
instructions = "Fix the defect this report describes: {inputs.report}"
assignee = { agent = "developer" }
project = { input = "project" }
then = ["tests"]

[[workflow.steps]]
id = "tests"
name = "Tests pass"
kind = "check"
check = { check = "command", command = "cargo test" }
on_fail = { on_fail = "then", step = "fix" }
then = ["review"]

[[workflow.steps]]
id = "review"
name = "Ship it?"
kind = "approval"
prompt = "The tests pass. Ship the fix?"
```

```sh
bisa workflow validate --from fix.toml      # every problem, by step and kind; writes nothing
bisa workflow new --from fix.toml           # into your library — kept with its problems, if any
bisa workflow edit <id> --from fix.toml     # the next revision (--revision N to say which you edited)
bisa workflow list [--tag engineering]
bisa workflow rm <id>                       # refused while a goal or another workflow uses it, or a run of it goes
bisa workflow use <goal> <id>               # a draft goal's workflow
```

A TOML file is read the way the catalog reads a template: a `spawn` step may name another template by
slug (`workflow = "incident-response"`), which resolves to your installed copy, and a misspelled key
(`retires`, `outcom`) is refused by name rather than dropped. `new` and `edit` keep a definition with
problems and print them — with or without a daemon — and a run of it is what refuses. `edit` names
the revision it edited (the stored one by default); a workflow that moved since is a conflict that
names both revisions, and nothing is written.

What validation refuses — a missing start, a start something flows into, two starts by hand, the
event read outside a start's mapping, a boundary event on a step that cannot be stopped, a reminder
that diverts, an unreachable step, a branching step whose flows and
branches disagree, a loop whose body never returns, a placeholder that names nothing upstream, a
fixed reference that is not in this workspace, a connector step naming an operation or an account
that is not here, and the rest — is the closed table of problem kinds in
[03 — Workflows](../architecture/03-workflows.md#validation).

## Connectors

A **connector** is a declared API: a base URL, the hosts it may reach, how a call proves who is
calling (one of six schemes: nothing for an open API, an API key, a bearer token, a username and
password, OAuth2, or a token signed with a private key), and a list of operations — each a method, a path, a query, a body (JSON, a form,
multipart parts, or one parameter's raw bytes) and the parameters it takes. Fifteen ship in the catalog
(Gmail, Google Drive, Google Calendar, YouTube, Slack, X, Facebook Pages, Instagram, TikTok, Jira,
Confluence, Notion, Obsidian, Linear, Trello — [`reference/catalog.md`](../reference/catalog.md));
a custom one is the same shape in a TOML file. A connector definition syncs with the workspace like
a skill. An **account** — your login to that platform — never does: it is this machine's, its
record under `identity/connectors/` and every secret field in the keystore, added under Settings ›
Connectors or with `bisa connector account add`.

A `connector` step names the connector by slug and the operation by id, fills the operation's
parameters as templates, and runs as an account: the connector's default when none is named, a
fixed account id, or an input of kind `account`. A parameter of kind `file` is a path inside the
run's checkout — where the run's work landed — read when the step runs and sent as the body's
bytes; the model never sees them. Its output is what the operation selects from the answer —
`{steps.<id>.output.<field>}` reads it — checked against an `output_schema` when the step declares
one.

```toml
[[workflow.steps]]
id = "announce"
name = "Tell the channel"
kind = "connector"
connector = "slack"
operation = "post_message"
params = { channel = "{inputs.channel}", text = "Released {goal.title}: {steps.notes.output.summary}" }
```

An operation marked `writes` changes something on the platform; put an `approval` step before it,
as the Workflow Agent does. A call reaches only the hosts the connector declares, minus what
`security.net.deny_hosts` forbids and plus what `security.net.allow_hosts` allows; a refusal, a
failed call or an answer that misses the schema fails the step with a reason the credential never
appears in. An account read from an input is checked when the run starts.

```sh
bisa connector list                              # the connectors installed here, with their accounts
bisa connector show slack                        # the definition: hosts, auth, every operation and its parameters
bisa connector new --from mine.toml              # a custom connector, validated first
bisa connector rm <id>                           # refused while an account or a workflow step names it
bisa connector accounts slack                    # this machine's accounts, which secret fields are set and where
bisa connector account add slack --label work --secret token=@stdin
bisa connector account check slack <account>     # one request to the platform, as that account
bisa connector connect gmail <account>           # an OAuth2 connection: prints the URL, waits for the code
```

## The library and a goal's designs

Every workflow records where it was born: `workspace` (drawn by hand for the library), `catalog:<slug>`
(installed from a template), or `goal:<id>` — a **design**, made for one goal by the Workflow Agent's
proposal or by you on that goal's Workflow tab. The **library** is the first two. A design lives under
its goal: it is offered on that goal's tab, listed by `workflow list --goal <id>` (or `--all`), and
never appears in *Workflows* or in another goal's picker. A goal cannot be pointed at another goal's
design, and a design is never turned On by itself — its goal listens; both refusals say *promote it
to the library first*.

```sh
bisa workflow list                       # the library: workspace and catalog
bisa workflow list --goal <goal>         # that goal's designs
bisa workflow list --all                 # everything, origin on each row
bisa workflow use <goal> --from mine.toml   # record a design of the goal's own, and point at it
bisa workflow promote <id>               # copy a design into the library — a new id, revision 1
```

Promote copies; the original stays on its goal, because a run may hold it. Deleting a goal deletes
its own designs and nothing else; archiving a goal keeps them under it.

A workflow can be **archived** — out of the library and the pickers, turned Off, refused as
a goal's workflow or for a new run; a goal's run that copied it is untouched, and its runs in the
workspace stay as its history — or **deleted**, which is refused while a goal, a `spawn` step or a
start that hears its runs uses it, and takes its runs in the workspace, and what it listened with,
with it. The designer's menu offers both,
the card's `⋮` offers *Delete…*, and the dialog is the goal's: its runs in the workspace that are
going, which the retirement cancels first (their cause *retired*, their sessions stopped and waited
for) whatever you choose; what still uses it (then only archiving is possible — each goal a door, and
one whose run is going says *(running)*: stopping it is that goal's retirement, never the
workflow's); what becomes of its runs' history; and the projects its steps made with one choice for
all of them, their sessions stopped and waited for like a goal's. The designer open on a workflow that is deleted or archived elsewhere hears it and
leaves, or re-reads and shows the mark — and never saves it again on the way out. The **Archived** switch on the Workflows screen shows the ones
put away; *Unarchive* brings one back. `bisa archive workflow <id> [--undo]` does the same.

## With the Workflow Agent

The Workflow Agent, `workflow-agent`, is woken by an auto or guided capture, by `@Workflow
Agent` in a goal's thread — a goal's Workflow tab in Agent mode is that thread — and by `@Workflow
Agent` in a conversation about a library workflow, the designer's Agent pane — where there is no goal
and nothing to propose: it reads the workflow, validates until clean and saves it at the revision it
read, and the canvas beside the pane shows the new revision. On a goal it reads the goal, asks at most one round if the shape is unclear, starts
from the closest template, gives the work **the start it needs** — a start by hand, and for work
that recurs or waits for something the event its statement names — **staffs every agent step from
who is installed and enabled** — an agent
by id, or a team when the work needs several members' skills; it never assigns a step to a core agent and
never installs anything, and when nobody fits it names the closest and says so, or leaves you an
`assignee` input to fill — wires every step forward so every path ends in an `end`, validates until
clean and **proposes**. What happens next is the goal's mode: on an **auto** goal the platform
adopts the proposal and begins its work at once — a run, or listening when the design begins on
events nobody needs to see armed: a schedule, a signal, a run's end, a platform topic, a message; a
design that begins on a hook, a check, a connector poll or a project's change waits for you to adopt
it (the agent is told nobody adopts, so it gives every
input a default and asks nothing); on a **guided** goal the goal opens on the proposal — every step,
with *Adopt and start*, *Request changes…*, *Edit the steps* and *Decline* — and the same card waits
in your inbox, *Edit the steps* opens the canvas and *Adopt* takes the start inputs and starts the
run; on a **manual** goal, where it was asked in the conversation, the proposal is your draft on
the canvas. The agent itself never adopts, starts or amends. While it works, the goal page shows
where it stands and it talks in the goal's thread ([`goals.md`](goals.md#auto-guided-or-manual)).

A design with a rework loop — *review → verdict → rework → back to renders* — runs as you would
expect: the loop's back edge never holds the join at its target, so the first pass enters, and a
rework re-enters up to `max_visits`. A graph that leaves a step nowhere to come from (an amendment
that cut its source, say) does not finish *done* with the step cancelled: the step is failed with
`stalled: …` naming the flow it waited on, the run is failed, and the agent wakes in repair.

When a run fails at a step whose `on_fail` is `fail`, the agent wakes again in *repair* and proposes
a corrected workflow — applied and started again on an auto goal within `goals.auto.repair_limit`,
waiting for your approval on a guided one. An amendment to a live run changes only what has not
run, and is applied at once on an auto goal. See
[`agents-and-teams.md`](agents-and-teams.md#the-core-agents).

## Running, waiting, amending

```sh
bisa workflow run <id> [--input k=v] [--watch]   # a run in the workspace, no goal; follows it
bisa workflow run <id> --start <step> --data '{…}'  # a test run: as if that start's event had happened
bisa workflow on <id> [--input k=v]              # turn it On: its events start runs; a public hook's secret is printed once
bisa workflow off <id>                           # turn it Off; a run already going goes on
bisa workflow listeners [<id> | --goal <goal>]   # what is armed, when each next comes due, what waits
bisa workflow runs <id>                          # its runs in the workspace, newest first
bisa workflow stop <id> [--run <run>]            # every run of it in the workspace that goes — or that one
bisa workflow restart <id> [--run <run>]         # each started again with its inputs — or that one
bisa run <goal> --input k=v --watch              # a goal's run: start, and follow step by step
bisa run <goal> --new                            # another run of the goal, queued behind its live one
bisa stop <goal> · bisa restart <goal>           # the goal's live run stopped (its queue withdrawn), or started again
bisa status <goal-or-run>                        # one line per step with its mark
bisa inbox                                       # the human and approval steps waiting on you
bisa approve <goal-or-run> [--no] [--step <id>]  # an approval step, an adoption, an amendment
bisa step answer <goal-or-run> <step> "…" | -o <id> | --unsure
bisa step release <goal-or-run> <step>           # a wait step of kind release
bisa signal emit <name> [--data '{…}'] [--goal <goal>]   # raise a named signal by hand
bisa signal list · bisa signal release <id>      # the newest signals; let a held one through
bisa step done <goal-or-run> <step>              # a human step that happened off the record
bisa amend <goal> --from fixed.toml              # replace the steps of a goal's run that have not started
```

Where a verb takes `<goal-or-run>`, a goal's id means its current run and a run's id — a goal's or
one in the workspace — means that run; the id is looked up, never guessed.

**A run in the workspace.** *Run…* on a workflow's card, in the designer's menu or on its **Runs**
tab asks for the workflow's inputs and nothing else — no goal is picked or captured — and the run
starts at once, beside any other run of it: several go together and none ever queues. Its truth is
its own, in the workspace beside the library, and its ceiling is the workspace's default budget
(Settings › Automation › Budgets, `budget.default.*`) or, for a run an event began, the budget the
workflow was turned on with. *Run…* on a workflow with event starts offers the entry: *By hand*, or
*Test: as if … happened* with a sample payload — a test run begins at that start, its mapping read
over the sample, and is marked a test. An agent step runs in the project the step names, else in the run's own scratch folder; a
`notify` step that names nowhere posts in `general`; a `spawn` step captures a goal of its own that
refines nothing, attached to the projects the step gives it. The **Runs** tab lists the workflow's runs — the ones going first, then the newest —
each with its number, its status, when it started or ended and who started it (*by you*, *by
schedule*, *by hook*, *by message from Maya*, *by signal report.ready*, *by run #4*, *test*), and
*Stop*, *Restart* (a new run with the same inputs, at the same start and on the same event) and
*Open*: the run's own page,
`#/runs/<id>`, with *Your move* for what it owes you, its steps read downward as on a goal's
Progress tab, and its frozen workflow as a read-only canvas. The history is bounded: each workflow
keeps its newest 500 finished runs in the workspace (`workflow.runs.keep`, Settings › Automation ›
Workflows, workspace or machine scope, never below one), and the moment a run of it ends the oldest
beyond that go with their folders — a run that is still going is never counted, and a goal's runs
are the goal's. *Stop every run* and *Restart every
run* act on the workflow's runs in the workspace alone — a goal's run of it is its goal's, and goes
on.

A run that finishes — done or failed — a step that blocks, a budget spent and a design the Workflow
Agent could not finish are notices on the goal's row in the Inbox: read there, opened from there,
never owed. The workflow has a row of its own under the Inbox's *Workflows* source: a design or an
amendment the Workflow Agent wrote, a revision it proposed and the workflow's being put away are its
notices, and its runs in the workspace bring theirs — a run done or failed, a budget spent — with
the questions and approvals they wait on, answered right on the row; its door is the run that asks,
else the designer; your own saves earn none. A restart interrupts every step that was running: an
`agent` step resumes on the work item and the checkout it already had — claimed, in progress or
blocked by the interruption, the session is not the work — a `check` runs again, an `emit` raises
its signal again as the same signal, a `connector`,
`notify` or `spawn` repeats only within its `retries` and otherwise fails as its `on_fail` says, no
retry is charged for the interruption, and a note on the goal, or on the run in the workspace, names
the steps. A failed run is history — its Progress tab and its Workflow tab
name the step it failed at and why, and the doors: restart it, start a new run, or on a guided goal
wait for the Workflow Agent's repair, a corrected workflow to adopt as an Inbox row with the whole
proposal (a finished run is not amended).

**One live run per goal, the rest queued.** A run started while a goal's run is live — *New
run…* on the goal, an event the goal listens for, `bisa run --new` — is queued behind it and
starts on its own, in order, when the live run ends; the Runs list on the goal's Progress tab shows
each with its place, and a queued run can be withdrawn until it starts. *Stop* cancels the live run
and withdraws the queue; *Restart* cancels it and starts a new run of the same workflow and inputs
ahead of the queue ([Goals](goals.md#stopping-and-restarting)). A goal's run is stopped and
restarted from its goal, never from the workflow: the designer's *Running in n goals* banner names
the goals running it, each a link. The designer's header carries the **Browser** button too:
the embedded browser beside the workflow, its open tabs counted, a dot while an agent browses, the
pane shown or hidden ([the desktop](the-desktop.md#the-browser-pane)).

A `wait` holds a run for something: a `delay`, a moment (`time`), a `schedule`, a named `signal`, a
`message`, a `project`'s change, a `run`'s end, a `platform` topic, or a person's `release`
([Events and gateways](events.md#catch-events--wait)). A `wait` on a `signal` hears every signal
raised by that name — an `emit` step's, `emit_signal` from a session, `bisa signal emit`, an A2A
task the content screen passed — matched by name and `fields` and only within the run's scope: a
goal's run hears its goal's signals and the ones for no goal in particular; a run in the workspace
never hears a goal's. The engine's own news — `run.finished`, `step.changed`, `gate.decided`, … —
is what a `platform` wait names. What the wait heard is its output, so the next step reads
`{steps.<wait>.output.<field>}`.
A `wait` on a `delay`, a `time` or a `schedule` comes due on the engine's clock and survives a restart — a schedule the downtime passed fires once, a signal raised while the node was down still completes the wait that listened for it. A restart resumes an `agent` step on the work item and the checkout it already had, runs a `check` again, repeats a `connector`, `notify` or `spawn` only within its `retries`, charges no retry for the interruption, and says on the goal — or on the run in the
workspace — what it did; a
`wait`'s name and fields are templates, rendered when the step is entered, so `env = "{inputs.env}"`
waits for the value. A run finishes `done` when every path has drained; an `end` step ends its own
path, and says so when it should end more: `finish = "done"` finishes the run now and stops what is
still live, `finish = "failed"` fails it. `bisa status` says which `end` was reached when one was.

**Amending.** Amending is a goal's: a run in the workspace is stopped and started again instead. A
running workflow is read-only on the goal's Workflow tab, and the tab offers no
door into it — no amendment, no word to the Workflow Agent while the run goes. A run is amended by
`bisa amend --from <file>` (a run that failed is history: the Workflow Agent's repair is a new
proposal, adopted alone on an auto goal and gated on a guided one, that runs afresh): the steps
that have not started may change, and the node then checks
what the canvas cannot: every agent step still has one place to run, the run's inputs still bind (a
new input needs a default), and the amendment is the same workflow — and refuses by name otherwise.
When two `approval` or `human` steps wait at once, `bisa approve --step <id>` and `answer
--step <id>` say which one is meant rather than guessing.

## Events start workflows

What starts a workflow is part of the workflow: a `start` step per way in, each with the event it
begins on, how the event fills the run's inputs, and a guard over how many of its runs may go at
once. A **library workflow** hears its events once you turn it **On** — each occurrence is a run in
the workspace, under the budget it was turned on with — and a **goal** whose workflow begins on
events listens while it is open, each occurrence a run on the goal. An event never acts on its own:
it is written down first, and the run it starts passes the same gates, budgets and caps as one you
start by hand. A workflow turned on in the workspace cannot read `{goal.…}`: turning it on is
refused, with the steps that read it. Everything else a reaction needs — a condition, a
notification, a child goal, an agent's work — is a step of the workflow. See
[Events and gateways](events.md).

## What syncs

A workflow is a signed snapshot (kind `33412`) in its own namespace, so a collaborator's node holds
your library; a run (kind `33413`) rides with its home — a goal's run with its goal, a run in the
workspace in a folder of its own beside the library, with its own journal — and lands on a peer only
when every approval step it passed has that peer's copy of the signed decision. The designer is local to whichever node
you open it on; the definition is not.
