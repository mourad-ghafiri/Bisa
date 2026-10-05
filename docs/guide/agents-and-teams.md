# Agents and teams

The platform has **three core agents** — the General Agent, the Workflow Agent and the
Decision-Making Agent — and installs the rest from a **catalog** when the work needs it. A fresh
workspace opens with the first two, each with a key and a prompt of its own; the third judges, and
holds neither. This guide covers the core agents, what an agent is, the catalog, model plans,
skills, MCP servers, teams and assignment. The full catalog is [`reference/catalog.md`](../reference/catalog.md); the architecture
is [06 — Agents and teams](../architecture/06-agents-and-teams.md).

## The core agents

| | General Agent | Workflow Agent | Decision-Making Agent |
|---|---|---|---|
| id | `general-agent` | `workflow-agent` | `decision-making-agent` — reserved |
| what it does | triage and staffing | designs and repairs workflows | judges, where a decision point asks it |
| how you reach it | a message, `@General Agent` | a message, `@Workflow Agent` | you do not: a decision point, a `judge` step or an agent's `decide` tool asks it |
| harness | `claude-code` — **editable** | `claude-code` — **editable** | who answers is `decisions.provider` — out of the box `claude-code` with `claude-sonnet-5-5[1m]` |
| models | `claude-opus-5-5[1m]`, then `claude-sonnet-5-5[1m]` — **editable** | the same — **editable** | — |
| effort | the `agents.effort` setting, `high` — **editable**, with the plan | the same — **editable** | `decisions.harness.effort`, `high` |
| decision making | off — **editable** | off — **editable** | off until you switch it on |
| respond | `members` — any workspace member may direct it | `members` | — |
| everything else | fixed | fixed | fixed |
| lifecycle | cannot be removed, cannot be disabled | the same | cannot be removed |
| where you set it up | its edit form, `bisa agent edit` | the same | Settings › Decision Settings › Decision Making, `bisa decisions` |

The General Agent and the Workflow Agent **hold a record**: their definitions are
`library/core/general-agent.toml` and `library/core/workflow-agent.toml`, ensured at
`Workspace::open` and nowhere else, so a workspace that is open has both, by construction. Each has
exactly three things you may change: its harness, its model plan — the models, the strategy and
the effort — and its decision-making switch
(`bisa agent edit general-agent --model "claude-sonnet-5-5[1m]" --effort medium
--decision-making true`); any other change is refused by name rather than silently restored.

**The Decision-Making Agent** holds no record — no key, no prompt, no conversation — and takes no
work: nothing messages it and nothing is assigned to it, and no agent you create may take its id
or, since an id is made from a name, its name. It judges instead of doing: which model of a plan
leads, who of a pool takes a work item, whether a tool call is harmful, and more, always beside a
rule that runs when it is off, unsure or does not answer. See
[The Decision-Making Agent](decisions.md).

Everything below about rooms, wakes and tools is about the two that can be addressed.

**The General Agent** is the workspace's **triage** and its staffing: a question that names nobody,
in a standing channel or a goal's thread, wakes it, and it answers or hands the question to the agent
that should. On a goal it sharpens the statement while nothing runs, installs and assigns the staff
the work needs from the catalog, attaches projects, captures work that recurs or waits for
something as a standing goal — *every Monday…*, *whenever someone posts in #support…* — and hands
the *shape* of the work to `@Workflow Agent` rather than designing steps itself.

**The Workflow Agent** designs. An auto or guided capture wakes it (a manual goal asks it in the
conversation): it reads the goal, asks you at most one round if the shape is genuinely unclear —
and nothing at all on an auto goal, where nobody adopts — starts from the closest catalog template, names
installed agents — only the agents and teams the goal names to carry it when it names any (*Who
carries it* in the New Goal dialog, the goal's *Assignees*), a team whole or one of its members —
reaches outside platforms only through the connectors installed here — its wake
prompt carries a `CONNECTORS` list beside `STAFF`, and `list_connectors` answers the same to any
agent: each connector, its operations with their parameters, which read and which write, which
accounts are connected (any agent may also *read* through one live with `call_connector`, screened
as content from outside; a write is a step) — puts an approval before any operation that writes
(the validator refuses a write with no gate upstream unless the step says `unattended: true`, your
word to give), branches with `if` and `switch`, walks a list with `for_each`,
validates until clean and **proposes** a workflow. You adopt it — with the inputs
the run starts with — or edit it first. When a run fails at a step whose `on_fail` is `fail`, it wakes
again in *repair* and proposes an amendment that changes only what has not run. It never adopts,
starts or amends on its own. Guide: [`workflows.md`](workflows.md#with-the-workflow-agent).

**Both are participants in every team and every channel and members of none.** Membership is added
at read time and stripped on every write — a list you can edit is a list you can empty. Both are
excluded from the work-routing pool, because they guide and design rather than implement; an `agent`
step whose own `assignee` names one still reaches it. Neither blocks: a guided wake takes no
scheduler slot.

A human wakes either of them; either may wake one other agent, the other of the two included;
nobody wakes itself — so a chain stops at two hops.

Their tool sets differ. Both get `workspace_overview`, `list_staff` and `list_catalog` on top of the
common set, which holds `list_connectors` for every agent.
The General Agent alone gets `install_catalog_entry`, `assign` and `capture_goal`; the Workflow
Agent alone gets `list_workflow_templates`, `get_workflow`, `validate_workflow` and
`save_workflow` — the last writes the library workflow a conversation is about, and only from that
conversation, at the revision it read — and, in a goal's session or in the goal's thread when asked
for changes, `propose_workflow` and `amend_workflow`. A design wake hands it the goal, the staff, the connectors and the templates in
its prompt, so it fetches nothing it was given and starts designing on its first turn; a failed run
is repaired by a new proposal. There is no removal tool and no self-edit tool.
Every install, assignment, capture and proposal is journaled as a note on the goal. Neither agent
turns a workflow on, adopts or starts anything: a person arms what listens, save the starts an auto
goal's adoption may arm on its own ([Events and gateways](events.md#listening)). See
[`reference/mcp-tools.md`](../reference/mcp-tools.md).

## What an agent is

| Part | Meaning |
|---|---|
| system prompt | the **role**: how it works, what it produces, what it refuses to do |
| harness | which coding agent runs it — `claude-code`, `codex`, `opencode`, `copilot`, `grok`, `gemini`, `acp:goose`, … (`bisa harness list` says which are installed) |
| model plan | an *ordered list* of models plus a strategy |
| skills | ids into the shared skill library, resolved at launch |
| MCP servers | ids into the local MCP registry, resolved at launch |
| tags | the categories it files under |
| respond | `owner_only` (the default) or `members` |
| origin | `local`, `catalog { slug }`, or `core` — recorded at creation, never accepted from a caller |
| pubkey | its own keypair, minted at creation and attested by you |
| enabled | a disabled agent takes no work and leaves the addressing directory |
| photo | a picture this machine holds, scaled to a small square in the editor, drawn wherever the agent is named — *Remove* clears it |

Its work is signed as itself, so a collaborator's node can verify which agent did what.

**An edit is one write.** Every word of it is read before anything changes, so an edit that is
refused — a skill nobody wrote, a name left empty — changed nothing and stood nobody down, from
the command line and from the desktop alike. With a node running the command line's writes are
the node's; with none they go into the record.

What an agent is *told* passes the Redactor, what it *hands back* passes it again, and what it asks
its harness to *run* passes the Tool & Commands Guard ([11 — Security](../architecture/11-security.md)):
a key in a message, a goal's statement or a note reaches it as a placeholder it can still use, and a
key it found on its own is stored as one; a command a rule refuses is refused with the reason, and
the agent goes on; one a rule sends to you is a card in the Inbox, answered once per goal; a doubtful
one is read first by the classifier — a session of the agent named in `security.classifier.agent`, with
no tools and no door to the platform. None of this changes how you address, staff or assign an
agent; it changes what reaches the model and what the model may do.

```sh
bisa agent list [--tag <tag>]
bisa agent show <id>
bisa agent add --name Auditor --prompt-file ./auditor.md --harness claude-code \
  --model <primary> --model <backup> --tag review --respond owner-only
bisa agent edit <id> --strategy least-busy
bisa agent edit <id> --enabled false  # stood down: said once in every channel it is in
bisa agent usage <id>                 # what still points at it
bisa agent rm <id>                    # refused while anything does
bisa agent models claude-code         # what a harness advertises; empty means unknown, not unsupported
bisa sessions list                    # the live runtime roster: id, state, kind, what it is at work on
bisa sessions abort <session>         # stop one for good: its harness is told, not only its row
```

### Where an agent's files land

An agent has no one directory; the *thing being worked on* does. Answering in a conversation about a goal, a workflow, the workspace or the node, it runs in
`agents/<id>/scratch/`, its own scratch; in one about a workstream or a project, in that checkout
([the desktop](the-desktop.md#conversations)). Running an `agent` step, it runs in a project's workstream —
the project the step names, or the goal's only one — or in `goals/<id>/scratch/` when the goal has
none, where its answer is the result it yields; in a run in the workspace, which has no goal, it
runs in the project the step names, else in the run's own `scratch/` under `workflows/runs/<id>/`. Designing a goal's workflow, it runs in that same
`goals/<id>/scratch/`. Every prompt names the place and says what it is for, and `create_project` is
in the common tool set so that "put it somewhere real" is an action rather than advice — and the
only way a project is made.

### Skills

**The system prompt is the role; a skill is a procedure.** The catalog's **Artifacts** skill, for
one, is the procedure for making something a person looks at — a self-contained page, the right
format per thing, one title per thing — and posting it as an artifact ([Artifacts](artifacts.md)).
A skill is one library entry
(`skills/<id>.json`), referenced by id from any number of agents, so twenty agents share one
code-review checklist instead of twenty drifting copies. It is **written in one place** — Settings ›
Capabilities › Skills (*New skill*), or `bisa skill add` — and **picked** everywhere else: an agent's editor searches
the library and attaches by id, with a door to Settings › Capabilities › Skills beside the picker; it has no skill
form of its own.

Every skill an agent carries is delivered into every session that agent runs: as
`.claude/skills/<id>/SKILL.md` in the session's directory for `claude-code`, and as a prompt appendix
for every other harness. **In an adopted root, skills are always an appendix** — the platform never
writes its scratch into a folder it did not create.
A body is capped at 32 KiB;
a catalog agent carries at most six — four of its own and the two platform skills every one carries, Embedded Browser and Drawing — enforced by a test; a reference that no longer resolves costs
that skill and nothing else.

```sh
bisa skill list [--tag <tag>]
bisa skill show <id>
bisa skill add --id <slug> --name "…" --description "when to use it" --file ./skill.md
bisa agent skill add <agent> <skill>
bisa agent skill rm <agent> <skill>
bisa skill usage <id>
```

A skill id is immutable; deleting one is refused while any agent carries it.

### MCP servers

An MCP server is a registered object (`mcp/<id>.json`) that agents carry by id. It has deliberately
**no GEP kind**: a peer cannot run `npx some-server` on your behalf, so agent snapshots carry no
command lines or environment references off the machine. Nothing ships in the registry — a bundled
config that shells out to a third-party package is a supply-chain decision the owner makes.
`bisa` is a reserved server name.

A server speaks one of three transports: a **command** this machine runs (stdio — arguments, an
environment, an optional working directory), a **URL** over Streamable HTTP, or a URL over the older
HTTP+SSE transport (`--sse`); a remote one takes **headers** — a bearer token, an API key. Secrets
are written once and never read back: Settings and `bisa mcp show` print `••••••`, and saving an
entry without retyping a value keeps it. **Test connection** in Settings › Capabilities › MCP servers dials a server as
typed before you save it — its answer is drawn over the draft it dialled, and goes when the draft is
edited; a refusal of the form is said beside the field it is about; **Check** on a row (or **Check all**, or `bisa mcp probe <id>`) dials a
registered one and keeps the answer as its health — who answered, which protocol revision it
negotiated (either era), its capabilities and tools, or the stage it stopped at. A disabled server is
skipped at launch and never dialed; a probe never calls a tool.

A server an agent carries is mounted on its sessions beside the platform's own, and its tools are
**judged like commands** ([11 — Security](../architecture/11-security.md)): a person's guard rule on
`mcp__<server>__*` allows one server; with no rule, a tool that reads runs under the step's ceiling
and one that acts is asked about — or read by the classifier in an auto goal. A harness the guard
cannot judge (Codex, pi, OMP, OpenCode, a custom one) is not handed an installed server, and the
session is told, unless `security.mcp.observed` is set to `allow`.

```sh
bisa mcp list [--tag <tag>]                                  # ✓ ✗ · — the health, when the node runs
bisa mcp add --id <slug> --name <name> --command <bin> --arg … [--env K=V] [--cwd /abs] [--tag <tag>]
bisa mcp add --id <slug> --name <name> --url <https://…/mcp> [--header 'Authorization=Bearer …']
bisa mcp add --id <slug> --name <name> --url <https://…/sse> --sse    # the 2024-11-05 transport
bisa mcp probe <id> | bisa mcp probe --command … | --url … [--timeout-secs 10]
bisa mcp enable|disable <id>          # sessions skip a disabled one; agents keep the reference
bisa agent mcp add <agent> <mcp>
bisa agent mcp rm <agent> <mcp>
bisa mcp usage <id>
```

### Model plans

One model is one quota wall away from stopping. An agent carries a plan:

```sh
bisa agent edit <id> --model "claude-opus-5-5[1m]" --model "claude-sonnet-5-5[1m]"   # ordered fallback
bisa agent edit <id> --strategy weighted --model "claude-opus-5-5[1m]=3" --model "claude-sonnet-5-5[1m]=1"
bisa agent edit <id> --strategy round-robin                          # strategy alone
```

Every agent the platform ships — the two core agents that hold a record and the catalog's
thirty-two — carries the same plan: **`claude-opus-5-5[1m]`, then `claude-sonnet-5-5[1m]`**, in
`fallback` order. Both are Claude Code's ids for Opus 5.5 and Sonnet 5.5 with the one-million-token
context window ([models overview](https://platform.claude.com/docs/en/about-claude/models/overview),
[Claude Code — model configuration](https://code.claude.com/docs/en/model-config), read
2026-09-29). Quote an id in a shell: `[1m]` unquoted is a pattern to zsh. An agent on another
harness names that harness's own ids.

**An agent on GitHub Copilot CLI or Grok Build** names that CLI's ids — `bisa agent models
copilot` lists the ones Copilot's reference documents (`claude-sonnet-4.6` its default,
`gpt-5.4`, `claude-opus-5.5`, …), `bisa agent models grok` the ones your sign-in's own
`grok models` prints, its default first:

```sh
bisa agent edit <id> --harness copilot --model claude-sonnet-4.6 --model gpt-5.4
bisa agent edit <id> --harness grok --model <an id `bisa agent models grok` lists>
```

**An agent on Gemini CLI** names the ids its own page gives — `bisa agent models gemini` lists
`auto` (the CLI's default, which picks the model for the task), `gemini-3-pro-preview`,
`gemini-3-flash-preview`, `gemini-2.5-pro` and `gemini-2.5-flash`; one your sign-in lacks is
passed over for the plan's next, and the harness has no effort, so none is asked of it:

```sh
bisa agent edit <id> --harness gemini --model gemini-2.5-pro --model gemini-2.5-flash
```

All three speak ACP, so the model is set on the session itself before the first word, and the effort
after it. A model your account does not offer there is not run on something else in silence: that
session ends before it is prompted and the plan's next model is tried — which is what a second
`--model` is for. Install and sign-in lines for both are under **Settings › Capabilities › Harnesses** (and
`bisa doctor`); neither is there until its own `--version` answers with a version.

| Strategy | Order it produces |
|---|---|
| `fallback` *(default)* | plan order: the first entry that launches and survives |
| `weighted` | the head chosen proportionally to `weight`; the rest by descending weight |
| `round-robin` | strict rotation |
| `least-busy` | fewest in-flight sessions first; plan order breaks ties |
| `auto-route` | the Decision-Making Agent picks the model that suits the task, from a `: sentence` on each entry (`--model "opus: design work"`); plan order otherwise — see [The Decision-Making Agent](decisions.md#routing-a-model-plan-automatically) |

A harness's prose about a dead model is read once, at the adapter boundary, and becomes a typed
`ModelUnavailable`. The engine skips models in cooldown, relaunches a running step on the next
model, bounds the attempts (`max_model_attempts`, default 6) and journals every switch — a retry,
not a failure. The health ledger is keyed on `(harness, model)`, in memory and per process; `GET
/models/health` exposes it and the desktop renders it as live badges.

### Effort

How hard a model works on a task is its **effort**, in six levels, lowest first: `minimal`, `low`,
`medium`, `high`, `xhigh`, `max`. More effort is more thought and more tokens; less is quicker
and cheaper. It is `high` unless somebody says otherwise, and four places may say so — the first
that does, decides:

| Who | Where you set it |
|---|---|
| the step | a workflow's agent step, **Effort** beside its model — `effort: xhigh` in the definition ([workflows](workflows.md)) |
| the model | its row in the agent's model plan — `--model "claude-opus-5-5[1m]@max"` |
| the agent | **Effort** above its model plan — `bisa agent edit <id> --effort max` |
| the setting | `agents.effort`, for a project or the workspace — `bisa settings set workspace agents.effort medium` |

```sh
bisa agent edit <id> --effort max          # this agent, whichever model of its plan runs
bisa agent edit <id> --effort auto         # the Decision-Making Agent names the level, task by task
bisa agent edit <id> --effort inherit      # say nothing again: the setting decides
bisa agent edit <id> --model "claude-opus-5-5[1m]@max" --model "claude-sonnet-5-5[1m]@high"
```

The agents the platform ships state no effort of their own, so `agents.effort` is one dial for
all of them.

**A model runs only at a level it takes.** Harnesses and models differ, so the level asked for is
fitted to the model about to run: the level itself when the model takes it, else the nearest
below, else the lowest above. The editor offers only what each model takes; a level kept from
before stays visible and is fitted at launch.

| Harness | Levels |
|---|---|
| Claude Code | `low` `medium` `high` `xhigh` `max` on Opus 5.5 and Sonnet 5.5 (and Fable, Opus 4.7 and later, Sonnet 5); no `xhigh` on Opus 4.6 and Sonnet 4.6; none on Haiku |
| Codex | `low` `medium` `high` |
| OpenCode | by the model's provider: `high` `max` on Anthropic's, `minimal` to `xhigh` on OpenAI's, `low` `high` on Google's |
| pi, Oh My Pi | all six; the harness fits the level to the model itself |
| GitHub Copilot CLI | `low` `medium` `high` `xhigh`, and `max` on a Claude model; held to what the session offers for the model it is on |
| Grok Build | `minimal` to `xhigh`; a model that takes no effort is sent none |
| Gemini CLI | none — the harness has no effort control, and is sent nothing |
| an ACP agent | what the agent offers when the session opens |
| Goose, Cursor Agent, an A2A agent, a custom harness | none — the harness's own default runs, and nothing is sent |

**`auto`** hands the choice to the [Decision-Making Agent](decisions.md#choosing-the-effort-automatically):
it reads the task and names a level among those the model takes. When it is off, unsure or does
not answer, the next level down the list above runs — the agent's when the model said `auto`, the
setting's when the agent did, `high` when nobody named one.

The level is chosen when a session starts and stays for that session: a conversation keeps the
effort its first message was given; a session that idled long enough to be parked is not taken up
again — the next turn starts a fresh one, which decides its level again. A session you open
yourself in a terminal is yours: the platform passes it no model and no effort.

## The catalog

```sh
bisa catalog list [--kind agent|skill|team|channel|connector|workflow|addon] [--tag <tag>] [--match any|all]
bisa catalog show <kind> <slug>
bisa catalog install <kind> <slug>
```

Two of the catalog's agents exist to make what is already there read as a person wrote it. The
**Code Humanizer** rewrites code and its comments in the project's own style — behaviour, names,
outputs and tests left exactly as they were, comments that say why rather than what, the diff kept
small — and stops rather than change what the code does. The **Document Humanizer** rewrites a page
in place in the project's documentation conventions — every fact, path, flag and anchor kept, the
tells of machine prose cut, the register of the neighbouring pages matched. Both carry the
**Plain Voice** skill, which is the one place the tells, the moves and the boundaries are written
down. The Code Humanizer sits in *Engineering*; the Document Humanizer in *Launch* and *Design*.

The **Mobile Developer** builds and ships Flutter apps for iOS and Android on the devices this
machine can reach — a simulator, an emulator, a phone — and reads the screen back before it calls
a change done ([ide/19](../architecture/ide/19-mobile-development.md)). It carries four skills:
**Flutter Development** (the loop — the toolchain, the devices, `flutter run` in a terminal, hot
reload, a screenshot — and both platforms kept in step), **App Store Publishing** and **Google
Play Publishing** (each a numbered procedure from the account to the release and an update, the
submission always the person's press), and the workstream workflow. It sits in *Engineering*, and
the **Mobile release** template builds, checks, reviews and puts an approval before anything goes
to a store.

An install is **transitive** (a team brings its agents, an agent brings its skills, a channel brings
its roster, a workflow brings the agents its steps name), **reported** (the result names everything
created, per kind), **idempotent**, and **never destructive** — an id already held by something you
made is refused by name. An installed definition is yours to edit; `origin` is provenance, not a
lock. The `general` channel, the General Agent and the Workflow Agent are not in the catalog: they are
permanent objects ensured at every open; the Decision-Making Agent is not in it either — it is
compiled in, with nothing to ensure.

## Teams

A team is a named set of members, each a human (pubkey) or an agent (id). **A team cannot contain
another team**, so one level is the whole model. A team can be **disabled**: a disabled team takes
no work, a message or a workflow's `notify` that names it reaches nobody, and the change is
announced as a membership event in every channel whose roster names it. It keeps its members and
comes back as it was.
A team takes a **photo** in its dialog, as an agent does in its editor — the same picker, the same
small square — and its card and every picker that names it draw it.

```sh
bisa team create "delivery" --purpose "ship it" --agent developer --agent qa-engineer --human <pubkey>
bisa team add-member <team> --agent <id>
bisa team remove-member <team> --human <pubkey>
bisa team list [--tag <tag>]
bisa team show <team>                 # members, and the goals it is carrying
bisa team usage <team>
bisa team rm <team>                   # refused while anything names it
```

The catalog's teams are in [`reference/catalog.md`](../reference/catalog.md#teams-9); the widest is the
most expensive to assign — every member is a candidate every unassigned `agent` step routes across —
so prefer a narrower team when the work belongs to one phase.

## Assignment

`Assignee` is one word for "who can be given work", with exactly three wire forms:

| Wire form | Meaning |
|---|---|
| `agent:<id>` | an agent definition. **Takes work** |
| `human:<64 hex>` | a person. **Decides gates and answers questions** |
| `team:<id>` | expands to its members at resolution time |

A bare id is refused — it would be ambiguous between the three.

```sh
bisa assign <goal> agent:developer human:<pubkey> team:<team>
bisa assign <goal> agent:researcher --replace
bisa unassign <goal> agent:researcher
bisa project assign <project> agent:developer
```

Resolution is one union in one place: the step's `assignee` when it has one — it wins alone — else
`project.assignees ∪ goal.assignees ∪ the parent goals'`, nearest first, teams expanded in place. Two
filters read it: `workers()` keeps enabled agents whose harness probes available here (humans, the General Agent and the Workflow Agent
are dropped — a human-only assignment yields an empty pool and the step runs as an unassigned one
does: on the harnesses it names, else on the default), and `approvers()` keeps the humans, who may decide that goal's gates
while a gate is on the default `owner` policy. `publish` never defers to an assignment. Which agent
takes a step is deterministic per work item, so a re-run is not a reshuffle — unless the Decision-Making Agent
is on for the workspace, the run's workflow, or one of the pool, in which case it picks by each
candidate's own description first, falling back to the same deterministic pick when it is unsure or
does not answer ([The Decision-Making Agent](decisions.md)).

## Recall — what agents remember

Each agent has a private memory keyed by slug, encrypted to the agent↔owner conversation key and
addressed by a blinded tag, so a relay learns neither the content nor the subject while you can
always read it. Writes are base-hash guarded; records link with `[[slug]]`; orphans are surfaced,
never deleted.

```sh
bisa recall list <agent>
bisa recall get <agent> <slug>
```

## Deleting things

**Nothing is deleted while something points at it, and the refusal names what.** `bisa <kind>
usage <id>` answers before you try; an id nothing knows about is an error, not an empty answer. What
holds what is the table in [`operating.md`](operating.md#deleting-things). The General Agent, the Workflow Agent and
the `general` channel are refused on identity, whatever points at them.
