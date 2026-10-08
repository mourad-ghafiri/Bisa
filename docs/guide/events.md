# Events and gateways

A workflow is made of three families of steps. **Tasks** do work — an agent's, a person's, a check,
a call to an outside platform. **Events** are things that happen: a run begins on one, holds for
one, is interrupted by one, raises one, ends on one. **Gateways** route: they choose one path,
every path that fits, or all of them at once.

```
   start events                                   end events
  by hand · schedule · hook            ┌────────► ends its path
  message · signal · project           │          finishes the run · fails the run
  run · platform · connector · check   │
        │                              │
        ▼                              │
     a run ──► tasks ──► gateways ──► tasks ──► …
                 │            ▲
                 │ boundary   │ wait: a run holds for an event
                 │ events     │
                 ▼            │
        divert to a path · act beside the step · emit a signal
```

**An event never acts. It is written down, and a run begins from what was written.** Every
occurrence of a start event is a durable signal before anything moves; a separate worker begins
runs from the queue under the same gates, budgets, pause switch and concurrency cap as work a
person started. A flood costs the queue, not the workspace.

| Family | Kinds |
|---|---|
| Events | `start` · `wait` · `emit` · `end` |
| Gateways | `decide` · `if` · `switch` · `judge` · `parallel` |
| Loops | `for_each` · `while` |
| Tasks | `agent` · `human` · `approval` · `check` · `connector` · `notify` · `spawn` |

The designer's palette has the same four groups; [Workflows](workflows.md) covers the tasks, the
loops and how a workflow is written, run and amended. This page covers events and gateways.

## Start events

A `start` step is one way a run may begin. A workflow may carry several; a run enters the one it
began at and the others are skipped, so their paths never hold a join up.

```toml
[[workflow.steps]]
id = "ticket"
name = "A ticket arrives"
kind = "start"
on = { event = "hook" }
inputs = { ticket = "{event.payload.body}", customer = "{event.payload.customer}" }
guard = { overlap = { parallel = 4 } }
then = ["classify"]
```

| `event` | Begins a run when | Fields | The event's payload |
|---|---|---|---|
| `manual` | a person says so: *Run…*, a goal's *Start*, `bisa workflow run` | — | — |
| `schedule` | a cadence comes due | `every` seconds, or `cron` + `tz` | `{ at }` |
| `hook` | the node is called | `public` | the body |
| `message` | a message is posted | `in`, `from`, `mentions`, `contains` | `{ message, scope, author, author_kind, teams, mentions, text }` |
| `signal` | a named signal is raised | `name`, `fields` | what the signal carries |
| `project` | a project changes | `project`, `change`, `branch`, `glob` | `{ project, slug, change, branch, before, after, forced, commits, paths, pull_request }` |
| `run` | a run ends | `workflow`, `outcome` | `{ run, workflow, outcome, goal }` |
| `platform` | the engine says one of its own topics | `topic`, `fields` | `{ event, fields, goal, workflow, run }` |
| `connector` | an outside platform lists something new | `connector`, `operation`, `account`, `params`, `key`, a cadence | `{ id, item }` |
| `check` | a command's result says so | `command`, `project`, `fire_on`, a cadence | `{ exit_code, passed, output, command }` |

An event's own fields are fixed values, `{inputs.<name>}` templates or `{ input = "<name>" }`
references over **the inputs the host listens with** — read when the start is armed, so one
template serves every workspace: `cron = { input = "when" }`.

A workflow that names no `start` step begins by hand at its one root. *New workflow* and every
catalog template carry an explicit `manual` start.

### The mapping

`inputs` maps the event onto the run's inputs: each entry is a template that reads
`{event.<path>}` — `{event.payload.<dotted.path>}`, `{event.at}`, `{event.id}`, `{event.name}` —
and nothing else. **The mapping is the one place the event is read.** Every step reads typed inputs
(`{inputs.ticket}`), so a run by hand, a test run and a run an event began look the same to the
work. A mapped value is shaped to its input's kind: a `number` parses as a number, a `bool` as
`true` or `false`, every other kind takes the text. A field the event does not carry leaves an
optional input to its default and refuses a required one: an event that lacks what the run needs
starts nothing, and the listener says so.

A `manual` start maps nothing and has no guard.

### The guard

```toml
guard = { debounce_secs = 300, overlap = "queue" }
```

| Guard | Default | Meaning |
|---|---|---|
| `debounce_secs` | `0` | drop an occurrence within this many seconds of the last run the listener started |
| `overlap` | `queue` | what an occurrence does while runs the listener started are still going |

| `overlap` | Meaning |
|---|---|
| `queue` | one run at a time; later occurrences wait in the durable backlog and begin their run as the one before ends |
| `skip` | an occurrence is dropped while a run goes — and it says so |
| `{ parallel = 4 }` | up to four at once; the rest wait |

The guard is counted over the listener's **live runs**, never over its dispatches, and decided one
signal at a time. The backlog is bounded: past `events.backlog_per_listener` waiting occurrences a
new one is refused and the listener says so once. The debounce is met once, when an occurrence is
first taken up: one that waited its turn is never dropped by it later.

On a goal, a listener runs one run at a time and keeps at most one queued behind the goal's live
run, whatever its guard says: a goal's runs take turns.

### Schedule

`every` is whole seconds; `cron` is an expression. Five-field expressions are crontab's — minute,
hour, day of month, month, day of week, `0` and `7` both Sunday, so `0 9 * * 1` is Monday at nine;
six- and seven-field expressions carry seconds and years. `tz` defaults to **UTC** and takes an
IANA name (`Europe/Paris`, case-sensitive), `local`, or a fixed offset. An expression with no
future occurrence is a problem of the definition.

Turning a host on starts its cadence afresh: the first tick arms the schedule and begins nothing.
However many occurrences a stopped node missed, a schedule begins one run when the node is back
and moves on.

### Hook

A hook start begins a run when the node is called. It is **local by default**: the call is made on
this machine under the control-plane token. With `public = true` — and with public hooks allowed
on this machine (`events.public_hooks`, off by default) — it also answers a caller outside, who
proves itself with the start's secret ([Hooks](#hooks)). The body is the payload: an object as it
came, text as `{ text }`, anything else as `{ value }`.

### Message

| Field | Meaning |
|---|---|
| `in` | the conversation it lands in — a channel, a direct channel, a conversation, a goal's thread; absent, anywhere |
| `from` | `you` (the default) · `agents` · one agent, one person or one team (`{ agent = "…" }`, `{ human = "…" }`, `{ team = "…" }`) · `{ input = "…" }` |
| `mentions` | it mentions this agent, team or person |
| `contains` | its text contains this, case-insensitively |

A message is heard where the conversation responder hears it — after the hold a message from
another node waits in — and once. `from = "you"` is this node's person; a person hosted from
another node is heard only when the start names them. **An announcement is never heard**: a
`notify` step's post is the platform speaking, so a workflow cannot wake itself by talking.

### Signal

A named signal is raised by an `emit` step, a boundary event's `emit`, a session's `emit_signal`
tool, a person (`bisa signal emit`, `POST /signals`) or an incoming A2A task. `name` is dotted
lowercase words (`report.ready`); `fields` are exact matches on the payload, by dotted path. A
goal's listener hears its goal's signals and the workspace's; a library workflow hears every one.

### Project

| `change` | Begins a run when | Seen by |
|---|---|---|
| `commit` | a branch head moved | reading the branch heads each tick |
| `push` | a remote-tracking branch moved: this machine pushed or fetched | reading the remote-tracking branches each tick |
| `pull_request` | a pull request one of the platform's workstreams opened or adopted changed state | asking the code host every `events.pr_poll_secs`, sooner when a workstream moved |
| `merge` | one of those pull requests was merged | the same |
| `files` | files in the tree changed | a git project's status between ticks; a plain folder's bounded scan |

**A project is looked at, never listened to.** The IDE's watcher runs only for the roots a person
has open and the code host sends nothing, so the ticker reads what each listener needs. The first
look learns what is there and begins nothing. `branch` narrows a change to one branch; `glob` —
`*` and `?`, against the file name, or against the whole path when the pattern holds a `/` —
narrows `files`. A scan of a plain folder is bounded by `events.files.max_depth` and
`events.files.max_entries` and never follows a link.

A pull request somebody opens on the code host is not one of the platform's: a public `hook` start
called by the code host is how that is heard.

### Run

`workflow` narrows to one workflow's runs; `outcome` is `done`, `failed` or `cancelled`. A restart
is not an end. A goal's run ends for its goal and for the workspace's listeners; a run of the
workspace ends for everybody.

### Platform

The advanced door: one of the engine's own bus topics, with exact matches on the event's own
fields — `run.finished` has `run`, `workflow` and `outcome`; `step.changed` has `run`, `workflow`,
`step`, `state` and `kind`; `goal.closed` has `reason`. The topics are one per engine payload, and
`crates/bisa-engine/tests/it/docs.rs` holds this page to the list:

`session`, `work_item.scheduled`, `work_item.execution_ended`, `session.state`, `session.gone`,
`gate.opened`, `question.asked`, `guided.status`, `agent.thinking`, `agent.streamed`,
`agent.replied`, `gate.decided`, `result.accepted`, `run.started`, `run.queued`,
`run.finished`, `run.cancelled`, `step.changed`, `goal.closed`, `goal.created`,
`workflow.proposed`, `workflow.changed`, `workflow.deleted`, `workflow.archived`,
`attachment.changed`, `goal.archived`, `goal.deleted`, `project.archived`, `project.deleted`,
`document.added`, `project.created`, `committer.needed`, `committer.set`, `project.changed`,
`workstream.edited`, `note.changed`, `drawing.changed`, `drawing.request`, `workstream.opened`,
`workstream.changed`, `workstream.committed`, `workstream.script_ran`,
`workstream.publish_failed`, `workstream.server_changed`, `browser.request`, `mobile_development.changed`, `mcp.probed`,
`connectors.checked`,
`people.changed`, `invite.changed`, `message.held`, `message.released`, `content.screened`,
`hosted.changed`, `relays.changed`, `guard.decided`, `decision.judged`, `security.redacted`,
`changes.moved`, `changes.settled`, `conversation.ask_opened`, `conversation.ask_settled`,
`model.switched`, `signal.received`, `listener.fired`, `listener.failed`, `listening.changed`,
`boundary.fired`, `file.changed`, `lsp.notified`, `settings.changed`, `git.setup_changed`,
`connectors.changed`, `addons.changed`, `conversation.created`, `conversation.changed`,
`engine.paused`, `engine.resumed`.

A topic the engine does not emit is a problem of the definition (`unknown_topic`). The listening
runtime's own news — `signal.received`, `listener.fired`, `listener.failed`, `listening.changed` —
is never heard back, so a start cannot feed on its own fires.

### Connector

A connector start polls an outside platform the way a `connector` step calls one
([Connectors](connectors.md)): on its cadence it runs one **read** operation of an installed
connector, as the account named or the connector's default, with `params` as templates over the
listening inputs, and takes the operation's selected answer as a list (a lone object is one item).
`key` is the dotted path, inside one item, of the field that tells items apart — `id` for most
platforms. **The first poll only learns what is there**; every later one is one occurrence per key
it has not seen, its payload `{ id, item }`. What a poll has seen is the listener's own memory (the
newest 1000 keys), kept across restarts. An operation that writes is a problem of the definition: a
start polls, it never writes. A failed poll — a host the policy refuses, an account that is
missing, a platform that errs — is said once and remembers nothing, so the next poll sees the same
items.

### Check

A check start runs a shell command on its cadence — in its project's tree, or in a folder of its
own — and `fire_on` says which results begin a run:

| `fire_on` | Begins a run on |
|---|---|
| `starts_failing` (the default) | the first failing result after a passing one: the alert, once per outage |
| `failing` | every failing result |
| `passing` | every passing result |
| `always` | every result |

Passing is exit `0`. A command that outlives `events.check_timeout_secs` is terminated and did not
pass. The last 2000 characters of output are the payload's `output`. The command is judged by the
Tool & Commands Guard ([11 — Security](../architecture/11-security.md)) when its host is turned
on — a refused command refuses the turn-on — and again at every fire, where a refusal is a result
that did not pass, its `output` saying why.

## Listening

A start event is heard once its **host** listens. A host is a library workflow — its events begin
runs of the workspace — or a goal, whose events begin runs on the goal.

### A library workflow: On and Off

A library workflow hears nothing until a person turns it **On** — in the designer's header, on its
library card, with `bisa workflow on`, or `PUT /workflows/{wfid}/listening`. Turning it on asks for
what its events do not supply — every required input that no start maps and no default fills, and
every input an event's own fields read and no default fills — and takes an optional budget, the
ceiling of each run it begins (absent, the workspace default, `budget.default.*`). An input nobody
gave is read at its default: `weekly-review` turned on with nothing listens on its Monday morning.
The desktop's *Turn on* dialog asks each of them as required — an input that is optional for a run
by hand is still needed to listen — and never one a default fills.

Turning on is refused, before anything is written, while the workflow has problems, is archived,
is a goal's own design, has no start on an event, reads its goal (`{goal.statement}` — a run of
the workspace has none), or carries a check start whose command a guard rule refuses.

What a workflow listens with is kept beside its definition, never in it: turning it on or off is
not a revision. Off removes it, and what its events had queued settles as *not listening*.
Archiving a workflow turns it off, and taking it back out does not turn it on again. Catalog
templates install Off.

### A goal listens while it is open

Starting a goal whose workflow has event starts **arms** them: nothing runs until an event happens,
and between runs the goal reads *waiting*, on the world. Each occurrence begins a run on the goal.
*Run now* begins a run by hand at the manual start, whatever else the workflow begins on.

A run that **fails pauses** the goal's listening and withdraws the runs its events had queued, so a
repair has a gap to land in. The goal reads *failed*, and yours, until a repair is adopted or you
say *Listen again* — it then listens with the inputs it had, unless you give it others. A goal whose
budget is spent pauses too. Stop, close, archive and delete end its listening.

A design with event starts that an **auto** goal proposes is adopted with nobody asked only when
every event is one nobody needs to see armed — a schedule, a signal, a run's end, a platform topic,
a message. A hook opens a door to the outside, a check runs a command, a connector start spends an
account and a project start reads a repository: each of those opens the Adopt gate, saying why.

One-shot work that waits for something — *when the report is ready, publish it* — is a manual start
and a `wait` step, not a start event.

## Catch events — `wait`

A `wait` step holds its run until something happens.

```toml
[[workflow.steps]]
id = "approved"
kind = "wait"
until = { until = "signal", name = "review.approved", fields = { pr = "{inputs.pr}" } }
then = ["ship"]
```

| `until` | Holds for | Fields | The step's output |
|---|---|---|---|
| `delay` | seconds from when the step was entered | `secs` | — |
| `time` | a moment: Unix seconds or an RFC 3339 time | `at` (a template) | — |
| `schedule` | the next occurrence of a cron expression | `cron`, `tz` | — |
| `signal` | a named signal | `name`, `fields` | the signal's payload |
| `message` | a message | `in`, `from`, `mentions`, `contains` | the message |
| `project` | a change in a project | `project`, `change`, `branch`, `glob` | the change |
| `run` | a run's end | `workflow`, `outcome` | the run's end |
| `platform` | one of the engine's topics | `topic`, `fields` | the event |
| `release` | a person — or a call, which may carry a payload | — | the payload, when one came |

The filters are the start events' own, their templates rendered against the run. A goal's run
hears its goal's events and the workspace's; a run of the workspace never hears a goal's. A delay,
a moment and a schedule count from when the step was entered: a restart buys none a fresh clock,
and one that fell due while the node was stopped is due at once. After a restart a `signal` wait
reads back the signals raised since it was entered — every named signal is kept once, heard or not
— and the first that matches completes it.

A run's waits never depend on a person's switch: `events.enabled` off stops starts from being
heard, never a run from moving.

## Boundary events

A boundary event listens **on a live step**. It either **diverts** the step — the step stops, its
work is cancelled, and the run takes the flows labelled with the boundary's name — or **acts
beside** it: a post, or a signal, while the step goes on.

```toml
[[workflow.steps]]
id = "review"
kind = "approval"
prompt = "Ship it?"
boundaries = [
  { name = "late",  on = { event = "after", secs = 172800 }, act = "divert" },
  { name = "nudge", on = { event = "every", secs = 86400, max = 5 },
    act = "notify", template = "Still waiting on your decision: ship it?" },
]
then = ["ship", { to = "escalate", branch = "late" }]
```

| `on` | Fires | Fields |
|---|---|---|
| `after` | once, this long after the step was entered — a timeout | `secs` |
| `every` | at this cadence, at most `max` times (3 when not written) — a reminder | `secs`, `max` |
| `message` | when a message arrives | `in`, `from`, `mentions`, `contains` |
| `signal` | when a named signal is raised | `name`, `fields` |

| `act` | Does | Fields |
|---|---|---|
| `divert` | stops the step; only the flows labelled with the boundary's name are taken | — |
| `notify` | posts beside the step, as a `notify` step does | `scope`, `template`, `mentions`, `author` |
| `emit` | raises a named signal beside the step, as an `emit` step does | `signal`, `payload` |

- A boundary sits only on a step whose work can be stopped: `agent`, `human`, `approval`, `wait`
  and a `spawn` that waits. A `check`, a connector call or a `judge` cannot be stopped mid-flight.
- A reminder never diverts: a divert on a cadence would stop the step at its first tick.
- A diverted step reads **diverted**, naming the boundary. Its session is ended and its work item
  cancelled, its question or approval withdrawn, its wait disarmed; a `spawn` stops waiting and the
  child goes on.
- An act has no flows. A side process is an `emit` and another workflow that starts on the signal.
- What one tick — or one event — wakes fires in order: the step's own catch or completion first,
  then the diverts, then the acts, each in the order it was declared.
- A boundary fires for the visit it was armed for: one of an earlier visit of a looping step moves
  nothing. A reminder goes on from how often it already fired, across restarts.
- `on_fail` stays the error boundary: where a step's failure goes.

**A race** — the first event wins — is a `wait` with divert boundaries: the signal it holds for,
another that diverts it, a timeout that diverts it. Whichever comes first takes its path.

## Throw and end

**`emit`** raises a named signal, its payload rendered against the run:

```toml
[[workflow.steps]]
id = "tell"
kind = "emit"
signal = "report.ready"
payload = { url = "{steps.publish.output.url}" }
```

Its output is `{ signal, id }`. An emit never fails because a listener refused it, and a step that
runs again after a restart raises the same signal, not a second one.

**`end`** is an end event:

| `finish` | Ends |
|---|---|
| `path` (the default) | this path: the run is done once every path has drained |
| `done` | the run, now: what is still live is cancelled |
| `failed` | the run, now, as failed: the end step is the step that failed |

## Gateways

| Gateway | Chooses |
|---|---|
| `decide` | the first rule that holds — or, with `pick = "every"`, every rule that holds |
| `if` | `yes` or `no` |
| `switch` | the case whose value the subject equals |
| `judge` | the option the Decision-Making Agent picks ([The Decision-Making Agent](decisions.md)) |
| `parallel` | every flow out of it, at once |

```toml
[[workflow.steps]]
id = "route"
kind = "decide"
pick = "every"
rules = [
  { when = { condition = "input_equals", input = "size", value = "big" }, branch = "heavy" },
  { when = { condition = "input_equals", input = "urgent", value = true }, branch = "rush" },
]
otherwise = "plain"
then = [
  { to = "lift", branch = "heavy" },
  { to = "hurry", branch = "rush" },
  { to = "stroll", branch = "plain" },
]
```

With `pick = "every"` a parcel that is big and urgent is lifted and hurried at once; `otherwise` is
taken only when no rule holds. A `parallel` gateway is done the moment it is entered. Where
branches meet, the step they flow into joins them: a join waits only for the flows that can still
arrive, so a path that was not taken never holds it up.

## Durable, deduplicated, and never in a loop

**Durable before dispatch.** An occurrence is appended to the queue and indexed before anything
acts. One signal makes one run: the run names the signal that made it, once, so a dispatch
replayed after a crash finds the run it began and settles — never a second run.

**Deduplicated.** Every source gives its occurrences a key, and a key heard twice is one signal:
a schedule's due time, a hook's delivery id (`Idempotency-Key` or `X-GitHub-Delivery`), a
message's id, a run's id, a branch and its commit, a pull request and its state, a poll's item
key, an emit's step and visit.

**The loop guard.** A signal carries the listeners that led to it, and a run keeps that chain —
widened by every chained event it hears. A listener already in the chain refuses the signal, so a
workflow that raises the signal it starts on runs once. A chain stops at `events.chain_depth` hops.
Every start but a schedule's, a hook's and a person's begins at most `events.fires_per_minute`
runs a minute; a `signal` start's overflow waits in the backlog, the others' is dropped and said.

**What comes from outside is read first.** A public hook's body, an item a connector start's poll
listed and a signal raised by an incoming A2A task are redacted before they are stored and held for
the content screen
([11 — Security](../architecture/11-security.md#what-arrives-from-outside-as-an-event)) before they
can begin anything. One the classifier is sure is safe goes on; a harmful one, or no verdict, stays
**held**, its reason on it and on its host's row in the Inbox, until a person lets it through
(`bisa signal release`, `POST /signals/{id}/release`) or leaves it. A call from this machine, under
the control-plane token, is your own and is never held. `security.content.screen` off holds
nothing.

**A listener that cannot work says so once.** A start whose inputs stopped binding, a poll that
failed, a full backlog: the listener's row in the Inbox — the workflow's, or the goal's — says what
is wrong, once, and again only when it changes.

| Signal state | Means |
|---|---|
| `queued` | written down, waiting for the worker |
| `running` | the worker took it |
| `waiting` | kept behind a run of its listener that is still going, or over its rate |
| `held` | waiting on a person: an outside payload the screen would not pass |
| `done` | it began its run |
| `skipped` | dropped, and why: the guard, the chain, a host that stopped listening |
| `failed` | its run could not start, and why |

## Hooks

| Call | Route | Proves itself with |
|---|---|---|
| local, a library workflow's | `POST /workflows/{wfid}/hooks/{step}` | the control-plane token |
| local, a goal's | `POST /goals/{id}/hooks/{step}` | the control-plane token |
| public | `POST /hooks/{host}/{step}` | the start's secret |

`{host}` is `workspace:<workflow id>` or `goal:<goal id>`. A call that is taken answers `202` with
the signal's id — written down, not yet run: the worker starts the run. The body is the event's
payload: a JSON object as it came, any other JSON value under `value`, text under `text`. An
`Idempotency-Key` header — or a code host's `X-GitHub-Delivery` — makes a redelivery the same
signal: one occurrence, one run.

| A local call answers | When |
|---|---|
| `202` `{signal}` | taken |
| `409` | the host is not listening, or this machine does not listen (`events.enabled`) |
| `404` | the step is no hook start of that host |
| `413` | the body is over 64 KiB |
| `429` | the start's backlog is full |

A **public** hook answers only while the start says `public = true` **and** the machine allows
public hooks (`events.public_hooks`, off until you turn it on). The node opens no network interface
by itself — reachability is your tunnel decision. The secret is 32 random bytes kept in
the keystore, **shown once** — when its host is turned on, when a goal that listens is started or
its design adopted, and when it is rotated — and never disclosed by any read route. It is kept
across Off and On. Authenticate with `X-Bisa-Token: <64 hex>` or a GitHub-compatible
`X-Hub-Signature-256: sha256=<hmac>` over the raw body, the key being the secret as it was shown;
both are compared in constant time.

| A public call answers | When |
|---|---|
| `404` | `events.public_hooks` or `events.enabled` is off on this machine — whoever asks |
| `413` | the body is over 64 KiB |
| `429` | past the rate limit (with `Retry-After`), or the backlog is full |
| `401` | an unknown listener, a missing or a wrong secret — one answer for all three, so the route says nothing about which listeners exist |
| `403` | the secret is right, and the host is Off or paused |
| `404` | the secret is right, and the start no longer takes public calls |
| `202` `{signal}` | taken |

Saving a workflow without a public hook start its listening host still answers on is refused:
turn it off first.

## Settings

The runtime's settings are **Settings › Automation › Events**.

| Setting | Default | Scope | Meaning |
|---|---|---|---|
| `events.enabled` | on | machine | events are heard on this machine; off, nothing is ticked, claimed or called, and everything waits as it is |
| `events.tick_secs` | 15 | machine | how often schedules, polls, checks and projects are looked at |
| `events.check_timeout_secs` | 60 | machine | how long a check start's command may run |
| `events.files.max_depth` | 8 | machine | how deep a plain folder is scanned |
| `events.files.max_entries` | 5000 | machine | how many entries a scan examines |
| `events.public_hooks` | off | machine | hook starts marked public answer callers outside |
| `events.pr_poll_secs` | 300 | machine | how often pull request states are asked of the code host |
| `events.chain_depth` | 3 | workspace | how many listeners one causal line of events may pass through |
| `events.fires_per_minute` | 30 | workspace | how many runs a start may begin in a minute |
| `events.backlog_per_listener` | 5 | workspace | how many occurrences may wait per start |

Events are heard only while the node is running — `bisa node`, or the desktop's sidecar. A command
that runs without one writes down what it is asked and leaves: it starts no run from an event.

## Using them

```sh
bisa catalog install workflow weekly-review            # a template installs Off
bisa workflow list                                     # its id — the verbs below take the id

# A review of the workspace every Monday at nine.
bisa workflow on $REVIEW --input when="0 9 * * 1" --input channel=general

# A support desk: tickets arrive on a hook, four at a time, each under a ceiling of its own.
bisa workflow on $TRIAGE --input channel=support --budget-usd-cents 200
curl -H "Authorization: Bearer $TOKEN" -d '{"ticket":"printer on fire","customer":"ada@example.com"}' \
  http://127.0.0.1:$PORT/workflows/$TRIAGE/hooks/ticket

# Try an event start without waiting for its event: a test run.
bisa workflow run $TRIAGE --start ticket \
  --data '{"ticket":"printer on fire","customer":"ada@example.com"}' --input channel=support

bisa workflow listeners                 # every listener: its event, when it is next due, its backlog
bisa workflow listeners $TRIAGE         # one workflow's — or --goal <goal> for a goal's
bisa workflow hook-secret workflow $TRIAGE ticket            # its paths, whether its secret is minted
bisa workflow hook-secret workflow $TRIAGE ticket --rotate   # a new secret, shown once
bisa workflow off $REVIEW

bisa run $GOAL                          # a goal whose workflow begins on events: it listens
bisa run $GOAL --start start            # a run now, at the start by hand
bisa stop $GOAL                         # it stops listening, and its runs stop

bisa signal emit deploy.finished --data '{"env":"prod"}' --goal $GOAL
bisa signal list --limit 20
bisa signal release $SIGNAL             # let a held signal through
```

**In the desktop**, a workflow's designer header carries its **On/Off** switch and says what it
listens for; its library card shows *On*. *Run…* offers *By hand* or *Test: as if … happened*, with
a sample payload. A goal's page says *Listening*, for what, and *Paused* with the reason, and
carries *Listen again*, *Run now…* and *Stop listening*. A run's line says who began it — *by you*,
*by schedule*, *by hook*, *by message*, *by signal*, *by project*, *by run*, *test*.

**The General Agent** captures work that recurs as a standing goal (`capture_goal`): a statement
that says when — *every Monday at nine…*, *whenever someone posts in #support…* — for the Workflow
Agent to design with the start event it names.

`GET /events` carries `signal_received` (after the write, never before), `listener_fired` (with the
run it began, or why it began none), `listener_failed`, `listening_changed` and `boundary_fired`.
