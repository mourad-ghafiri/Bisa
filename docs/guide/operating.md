# Operating

A workspace carries **standing obligations** — the workflows that are On and the goals that listen,
starting a run when reality drifts — plus the governance that decides who signs what and the assignment that
decides who does it. This guide is the running-platform view: the daemon, governance, deletion,
tags, templates, A2A.

## The daemon

```sh
bisa node                           # unix socket: <data>/run/node.sock, mode 0600
bisa node --listen 127.0.0.1:4477   # + one TCP address
```

The daemon runs the collaboration pump beside the engine — the relay pool on `sync.relays`, the
host's pump when there is anyone to talk through, the guest sessions of the workspaces you joined
([Collaboration](collaboration.md)). `sync.enabled` is off by default: the four default relays
are listed and can be checked, and none is contacted until it is turned on.

**A relay that stays *connecting*.** `bisa relay doctor` answers it in order: whether sync over
relays is on, how many relays are configured, whether this process can set up TLS at all (every
real relay is `wss://`), that relays are dialled directly — `network.proxy.*` is for HTTP, so a
network that lets nothing out but a proxy reaches no relay — and then a check of each configured
relay with what it answered. Settings › Workspace › Relays & sync shows the same under a relay's row: *never
connected in 4 attempts — …*, or *the connection dropped … it is retried on its own*. A relay that
never connected while everything over HTTP works is the address, the relay itself, or the network
between — try another relay with `bisa relay check wss://…`.

While a node runs, the CLI routes through it — `status`, `log`, `inbox`, `approve`, `answer`, `run`,
`step`, `amend`, `close` and the rest — so decisions resolve gates in the running engine. `--no-node`
forces embedded mode. Guided work, events, waits and the wire are live only with a daemon; without one
an auto or guided `new` keeps the CLI alive until the Workflow Agent's proposal lands — and an auto
one then follows the run it started — and `run --watch` keeps it alive until the run needs a person.

`GET /node` says what the node is — its version, pid, start, socket, listen address, data and logs folders, whether it is paused, the sessions live — the footer's node read-out reads it on a click. `POST /pause` stops scheduling (running sessions finish, nothing new starts); `GET /pause` reads the
switch; `POST /resume` restarts it.

## After a crash

Nothing to do. A node that dies — a crash, a `kill -9`, a power cut, a machine that went to sleep
for a week — leaves files, and every fact it acknowledged is on the disk: the journals, the ledger,
the signal queue and the conversation logs are fsynced line by line, snapshots are written atomically,
and the SQLite index is a cache that checks itself at open and is rebuilt from the files when it is
damaged. The next start reads the files and does, before it serves anyone:

- **ends every session** the dead node was driving, and terminates the harness process it left
  running — only when it is still that process, never a recycled pid;
- **resumes every `agent` step** on the work item and the checkout it already had, with a line in
  the prompt saying a previous session was interrupted, and no retry charged;
- **runs every interrupted `check` again**, repeats a `connector`, `notify` or `spawn` only within
  its `retries`, and fails the rest once with *interrupted by a restart*;
- **re-arms every wait** — a schedule the downtime passed fires once, a signal emitted while the
  node was down still completes the wait that listened for it;
- **withdraws the questions nobody can answer any more** — a publish, a guard escalation, a
  permission — and leaves a note saying where to ask again;
- **writes one note on each goal** it touched, and on each run in the workspace: *a restart
  interrupted 2 running steps: `build` resumed on its work item, `verify` will run again*.

Read the goal's Activity for that note — a run in the workspace keeps it in its own journal. In the desktop the sidecar restarts the node itself, on the
same port, after a delay that doubles to thirty seconds while it keeps dying, and the page reads
every list again when the stream comes back (*The node came back — everything was read again*).
The window itself loses nothing to a node that restarts: where you were and how each screen stood
are the desktop's own memory ([The desktop §Where you were](the-desktop.md#where-you-were)), written
as you go and when the window is put away, so an app that was ended rather than quit still opens
where it was — at most the last moment's scroll behind.

```sh
bisa workspace reindex     # rebuild the index by hand; refused while a node holds the workspace
```

A second engine never runs beside a first: `run/engine.lock` is held for the node's life, and a
command that needs one while the lock is held says whose it is.

## The log

Every process writes about itself under `~/.bisa/logs/`, one folder per process —
`node/node.<date>.jsonl` for the daemon, `cli/cli.<date>.jsonl` for a command,
`mcp/mcp.<date>.jsonl` for the MCP servers and hooks a session runs, `desktop/desktop.<date>.jsonl`
for the app — one JSON object per line, **errors only by default**: a 5xx the node answered, a
panic, a crashed screen, a request that got no answer. A death is a **crash report** under
`logs/crashes/`, one JSON document: a panic with its location, its thread, a backtrace and the last
256 lines the process said before it at `debug` and above, whatever the level in force; a run that
ended without saying goodbye — a signal, an abort, a stack overflow — reported by the next start of
the same process from the marker it left under `logs/runs/`; the app's node exiting on its own, with
its exit code or signal and the last lines it printed. Nothing is sent anywhere; to report a bug,
attach the newest report and the file of the day yourself — `bisa logs` names them, and
`bisa paths` says where the workspace is:

```sh
bisa logs            # the folder, each process's files, the crash reports, the newest one in a line
bisa logs --json     # the same as the node's GET /logs answers
bisa paths --json    # { "data_dir", "logs_dir" } — nothing opened, nothing made
```

Four machine-scope settings, under Settings › Node › Logging or the CLI, change what is written
without a restart:

```sh
bisa settings set machine logging.level '"debug"'     # error · warn · info · debug · trace
bisa settings set machine logging.rotation '"hourly"' # hourly · daily — the file's name carries the period, in UTC
bisa settings set machine logging.keep_files 30       # the oldest past this count is removed
bisa settings set machine logging.enabled false       # off: nothing is written, no file is opened
```

`info` adds the platform's own timeline — one line per fact the Pulse records, so the log reads
what happened around an error — and the *log started* and *log stopped* lines of every run;
`debug` adds one line per request the node answers. A line carries ids, kinds, statuses and an
error's sentence, never a message, a file's text or a secret
([11 — Security](../architecture/11-security.md#the-diagnostic-log)). A file past
`logging.keep_files`, a crash report past fifty and a stale run marker are the things the platform
removes on its own. `RUST_LOG=debug bisa node` still prints to the terminal as well — the
environment governs stderr, the settings govern the file. The daemon stops on ctrl-c or `SIGTERM`
and says which in its goodbye. It waits on nobody to do so: the event streams a desktop or a script
holds open are ended — each listener reads the end of its stream — the engine is stopped, and the
socket goes with the node. What was running is the next start's to pick up. An open desktop reads
the end of its stream as the node going away, never as an error: the top of the window says
*waiting for the node…* at once, the footer's node read-out *unreachable*, it asks again at a pace
that slows to every fifteen seconds, and when the node is back every list is read again, once, with
a word saying why the screen moved. A read that outstays thirty seconds is one list that could not
be read — the node is there — and the rest of the screen stands.

**One engine per workspace.** The engine takes an exclusive lock on `run/engine.lock` before it
binds anything and records its PID there; a second engine — a second `bisa node`, or a CLI
command that would embed one while a daemon is busy and slow to answer — is refused with the
holder's PID rather than started. A dead holder releases the lock with its process.

**The control plane is behind a bearer token** in `run/token` (mode `0600`), minted on first start
and reused, presented as `Authorization: Bearer <token>` or `?token=` for an `EventSource` or an
`<img>`. `/health`, `POST /hooks/{host}/{step}` (a public hook: its listener's own secret), the
A2A endpoints, the connectors' OAuth callback, an installed addon's files and a terminal
session's four doors (`report`, `guard`, `exit`, `close`, under the session's own secret) are the
named exceptions. `BISA_API_TOKEN` in the environment overrides the file for both the node and the
CLI. `--listen` refuses a
non-loopback address without `--insecure-allow-remote`.

## Governance — who may sign which gate

Three gates: `approval` (an approval step, adopting a proposed workflow, accepting an amendment),
`escalation` (a question — a human step, or an agent's `ask_human`) and `publish` (anything that
leaves the machine). Truth is `governance.json`, enforced when a decision is recorded **and** when
one arrives from a person on another node. An older `governance.json` — one that names gates that no longer exist — is
refused at open by name, never defaulted.

```sh
bisa governance show
bisa governance set approval members
bisa governance set escalation team:<team>
bisa governance set publish <pubkey>
```

Policies: `owner` (default) · `admins` (you and every admin) · `members` (you, the admins and
every member — never a guest) · a list of pubkeys and/or `team:<id>` references (expanded to the
team's humans at check time; a listed person still needs a role that decides gates). A run snapshot arriving from a peer that claims an
approval step was passed lands only once a qualifying decision has arrived with it. A listed entry
is a reference: deleting the team it names, or the agent whose pubkey it holds, is refused until the
entry is removed.

Precedence, exactly: an explicitly configured policy is authoritative; otherwise, on the default
`owner` policy, the humans of the goal's full assignee union may decide that goal's gates in
addition to the owner; otherwise owner only — except `publish`, which never defers
([03 — Workflows](../architecture/03-workflows.md#three-gates)).

## Deleting things

**Nothing is deleted while something points at it, and the refusal names what.**

| Object | Held by |
|---|---|
| skill | an agent's `skills` |
| MCP server | an agent's `mcps` |
| agent | a team · a channel roster · a goal's or project's assignees · an unsettled work item · a workflow's `agent` or `notify` step, or a start, a wait or a boundary event that names who said something · its pubkey in a `listed` policy |
| team | assignees · unsettled work items · a workflow's steps · a `team:<id>` policy entry |
| workflow | a goal that runs it · another workflow's `spawn` step · another workflow's start or wait that hears its runs · a run of it in the workspace still going (its finished runs, and what it listened with, go with it) |
| addon | nothing points at an addon: removing one takes its record, its bundle and its snapshot, and a built-in can be installed again ([Addons](addons.md)) |
| channel | never deletable: `general`; any other channel deletes with its history — a workflow's `notify` step, a boundary event's post or a `message` start or wait naming it holds it first |
| project | attachments are removed, never blocking; `--tree` is refused on an adopted folder |
| goal | detaches its projects and deletes none; its runs go with it |

```sh
bisa agent usage <id>          # "3 things point at agent developer: team (1) … Remove it from the team first."
bisa team usage <id>
bisa skill usage <id>
bisa mcp usage <id>
bisa workflow rm <id>          # refused with the goals and workflows that use it, or while a run of it goes
```

Over HTTP the same question is `GET /usage/{kind}/{id}`, and the desktop uses it to label the button
rather than offer one that can only be refused. The General Agent, the Workflow Agent and the `general` channel
are refused on identity, whatever points at them.

## Tags

Agents, teams, channels, skills, MCP servers, projects, goals, workflows and connectors all carry
tags; work items inherit their goal's filing. One normalisation rule (trim, lowercase,
spaces/underscores/slashes/dots to dashes, runs collapsed — idempotent by test), one index table, one
`?tag=` filter that answers the same way everywhere. A write path rejects a tag it cannot slugify;
deserialisation normalises and drops. At most 16 tags per object, 32 bytes each. The catalog draws
only from the vocabulary `crates/bisa-core/src/tags.rs` holds, rendered in
[the catalog reference](../reference/catalog.md#the-tag-vocabulary) — twenty-three words, `mobile` and `web` among them.

On the wire tags are NIP-12 `t` tags beside `d` and `revision`, so a peer filters without decoding.

## Settings

Three scopes — machine, workspace, project — and one registry; a key is held only at a scope its
definition allows, and resolves `project → workspace → machine → default`
([ide/13 — Settings](../architecture/ide/13-settings.md)). Every key, its kind, default and scopes
are in [`reference/settings-keys.md`](../reference/settings-keys.md), generated from the registry.

```sh
bisa settings show [--project <id>] [--group editor]
bisa settings get editor.tab_size --project <id>
bisa settings set workspace editor.tab_size 2
bisa settings set project editor.tab_size 8 --project <id>
bisa settings set machine appearance.theme '"dune-dark"'
bisa settings set workspace workflow.autosave.delay_ms 1500
bisa settings set machine network.proxy.mode '"manual"'
bisa settings set machine network.proxy.https '"http://proxy.example:3128"'
bisa settings unset workspace editor.tab_size
bisa settings registry
```

A change is announced on the bus as `settings.changed`, so an open desktop panel — or the designer —
re-reads without a reload.

## Security

Three features stand between your machine and a model ([11 — Security](../architecture/11-security.md)),
all on by default and all configured under `security.*`:

- **The Redactor.** A key, a token or a value the rules recognise never reaches an agent, a harness
  or a remote: it travels as a placeholder (`«secret:github_token:7f3a2c»`) the agent can still use,
  and is restored only where a command is about to run on this machine. What an agent writes back
  — a message, a note, a result, a commit message, a pull request — goes through the same rules on
  the way in, so a secret it read with its own tools is stored as a placeholder too. Your own rules
  are `security.redactor.rules` — a pattern, or `env_value` naming an environment variable whose
  value must never leave; a shipped rule is switched off by id in `security.redactor.builtins_off`.
  While `security.redactor.env_auto` is on (machine scope, the default), every variable of the
  node's own environment whose name says token, secret, password, key or credential is a rule of
  its own (`env:GITHUB_TOKEN`), switchable like a shipped one; the node's bearer token itself is
  removed from every harness's environment.
- **The Tool & Commands Guard.** Every command, path and tool a guarded harness is about to run,
  and every `check` and probe command, is judged by ordered rules — the shipped refusals, then the
  workspace's `security.guard.rules`, then this machine's. The first match decides: `allow`, `deny`
  (the agent hears the rule's name), `ask` (a card in the Inbox — answered once per goal: the same
  call again is decided by your earlier answer), `classify`. No match falls to the step's tier
  ceiling: within it the call runs; above it a guided or manual goal asks you — once per goal, the
  answer remembered — and an auto goal has the classifier read it first (`goals.auto.permissions`),
  so an unattended run is not one you answer *Allow Bash?* for. A workstream script you approved is still read line by line, and a line a rule
  refuses stops the script. `security.guard.terminal_hooks` (machine scope) guards a Claude Code
  or GitHub Copilot CLI session opened in the IDE's terminal through its own hook. Only a harness
  that asks before a tool runs can be stopped — Claude Code, GitHub Copilot CLI, Grok Build and any
  ACP agent; Codex, pi, OMP, OpenCode and the rest are observed,
  and `bisa security status` says which is which.
- **The Classifier.** `security.classifier.provider` picks who reads: `agent` (`security.classifier.agent`,
  the default — the General Agent, so on its plan: Opus 5.5 out of the box), `harness`
  (`security.classifier.harness` with `security.classifier.model` and `security.classifier.effort` —
  `claude-sonnet-5-5[1m]` at `high` out of the box, the quicker reader of the two), or
  `decision_making_agent` — [the Decision-Making Agent](decisions.md), asked which of a named set of harms, if any,
  the call carries. A generative reader answers `SAFE` or `HARMFUL: <why>` within
  `security.classifier.deadline_secs`; the Decision-Making Agent's reader is sure to `decisions.confidence.security`
  or it counts as no verdict. Harmful goes to you, or is refused when `security.classifier.on_harmful`
  is `deny`; no verdict goes to you. Whoever reads, it never allows what the rules did not.

```sh
bisa settings set workspace security.guard.rules \
  '[{"id":"ask_push","label":"Ask before pushing","action":"ask","matcher":{"kind":"command","regex":"^git push\\b"}}]'
bisa settings set machine security.guard.builtins_off '["publishing"]'
bisa settings set workspace security.classifier.on_harmful '"deny"'
```

The rule lists **merge** across the workspace and machine scopes rather than resolving first-holder-wins,
so a team's rules and yours both apply. `GET /security/status` and `bisa security status` name
every rule in force, the rules the node could not read, how many environment variables are
detectors, each harness's reach, the classifier's readiness and the last decisions — redacted, never
a value; `bisa security try` previews a redaction or a rule. Switching the redactor or the
guard off is never silent: the node logs it, the status says it, the panel draws a banner. Every
decision is a `guard` fact in the goal's journal and a `guard.decided` event in the Pulse and the
activity feed (`bisa pulse`).

## The Decision-Making Agent

[The Decision-Making Agent](decisions.md) is off by default and costs nothing until it is switched on
somewhere. On, each judgement is one bounded call — a harness or agent session with no tools, or one
HTTP request to a calibrated model — within `decisions.deadline_secs` (20 s by default, retries
included); a call that runs a harness or an agent counts against that provider's own usage the way
any other session does, and a remote provider (Jev, or an RLCD endpoint) is billed however that
service bills. When it is down — no key stored, the endpoint unreachable, the deadline passed — every
decision point simply runs its own rule, the same as if the Decision-Making Agent had never been switched
on; nothing stalls waiting for it. A remote provider's key lives only in this machine's own keystore
(`bisa decisions key set jev --from @stdin`), never in a setting and never synced, and is never read
back — `bisa decisions status` says only whether one is stored.

## The browser is the desktop's

The embedded browser lives in the desktop app: a `browser_*` tool parks its request for the desktop
to act on, and a node whose desktop has not been heard from within 45 seconds refuses the tool at
once with *nobody home* rather than waiting. A node running on a server with no desktop open cannot
browse; a goal that reads or drives pages needs the desktop open on some machine of the workspace,
and a standing goal on a headless node reaches the outside through connectors and the shell instead.
Where a page wants a login, the agent stops and the person signs in on the desktop's tab
(ide/18 § Agents browse here); the agent continues on the signed-in page, and the credential never
passes through it.

## Caching

The node caches what is slow to compute and safe to serve a little stale — a harness's model list, a
code host's pull request, a checkout's `git status`, the toolchain a mobile probe found — through one
toolkit (`crates/bisa-cache`): every cache has a name, counts its hits and misses, and takes its
time-to-live from a `cache.*` setting at the moment it is asked ([settings keys](../reference/settings-keys.md#cache)).
`cache.enabled = false` zeroes every TTL, so nothing is served from memory; a single key's TTL at `0`
turns that one cache off. `GET /cache/stats` lists every cache with its counters and `POST /cache/clear`
empties them all; in the desktop the same two live under Settings › Performance › Cache, with
*Clear caches*. Nothing that must be exact is cached — a write, a consent decision, a security
verdict, anything bound for the wire — and a mutation invalidates the cache it makes stale (a commit
drops the checkout's status, a `cache.*` write rebuilds the caches the key names).

## Notes

A note is a markdown scratchpad attached to one of six things: the workspace, a project, a goal, a
workflow, a channel, or this node. It is **local** — never synced, no GEP kind — and **nothing
depends on it**: the statement, the workflow and the journal are the record. Two writers, you and
any agent you talk to in the conversation beside a note: saves carry the hash of what was read and a
stale one is a `409` with the current text; an agent's only write is `note_append`, and only when asked. A listing is every note, every note of one
kind, or one scope's — the desktop's tabs — and deleting the record takes its notes with it.

```sh
# over HTTP today — no CLI verb:
GET /notes                       # every note
GET /notes?scope=project         # every project's notes (a kind alone)
GET /notes?scope=goal&id=<id>    # one scope's
```

## A2A (feature `a2a`)

- **Expose**: with `<data>/a2a.toml` (`public_base_url`, optional `skills`, optional `workflow`
  and `auto_start`) the node serves an agent card at `/.well-known/agent-card.json` and a JSON-RPC
  endpoint. Each task becomes a goal — and a run of the configured workflow when `auto_start` is on
  — and raises the named signal `a2a.task`, so a workflow that begins on it, or waits for it, reacts
  to a remote agent's request as it reacts to a hook call; what the task says came from outside, so
  the signal is redacted and held for the content screen first. The task's state is the goal's status. The card and `POST /a2a` take no token — a
  remote agent holds no control-plane token — so on the unix socket the socket's `0600` is the
  door and on loopback TCP any local process can submit a task: put the tunnel's or reverse
  proxy's own authentication in front of them when exposing them beyond this machine.
  The file is read whole or not at all: a key it does not know — `workflow` misspelt — is
  refused by name in the log, and the node starts with A2A left off rather than exposed as
  something its person did not write.
- **Consume**: a remote A2A agent is a worker when named as a harness candidate:
  `--harness a2a:https://agent.example.com`.

## Model health

`GET /models/health` is the engine's per-process ledger of `(harness, model)` cooldowns and
failures; the desktop's model-plan editor renders it as live badges. A restart forgets every
cooldown on purpose. See [Agents and teams — model plans](agents-and-teams.md#model-plans).
