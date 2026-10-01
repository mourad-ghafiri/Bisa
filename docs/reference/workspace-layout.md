# Workspace layout

**The filesystem is truth; `index.sqlite` is a cache** that can be deleted at any moment and is
rebuilt from the files. The one place a workspace directory name is joined is `bisa-store`'s
`paths.rs`, and a test greps every crate that knows the workspace to keep it that way. Design:
[08 — Persistence](../architecture/08-persistence.md).

```
~/.bisa/
  identity/                    keys, mode 0600, created exclusive (the file store is the default; the OS keyring only with BISA_KEYSTORE=keyring)
    codehost/<kind>/<login>.token   one token per account per kind (github · gitlab · bitbucket), mode 0600, under the login the host answered
    codehost/<kind>/accounts        the stored logins of that kind, one per line — never a secret
    git/profiles/<slug>.gitconfig   one profile by organization: author, key, account — included by your global git config for that owner's remotes
    connectors/<connector>/<AccountId>.json   a connector account: its label, non-secret parameters, the default mark, which secret fields are set — never a value (the fields are keystore entries connector:<connector>:<account>:<field>)
  workflows/state/33412-<WorkflowId>.json   the library — your workflows, and installed templates
  workflows/runs/<RunId>/      a run in the workspace — a workflow run with no goal, the same shape as a goal's run truth
    journal.jsonl              its signed history: step and run facts, questions, decisions, claims, results, notes
    state/33413-<RunId>.json   the run snapshot: its scope and budget, the frozen workflow, the inputs, every step's record
    state/33402-<WorkItemId>.json   its work items
    ledger.jsonl               its spend against its own budget
    results/<WorkItemId>.patch the patch a copy workstream left when its item settled
    scratch/                   where an agent step that names no project runs, and scratch/.tmp — every session's TMPDIR; kept under an archived workflow, gone with a deleted one
  goals/<GoalId>/
    journal.jsonl              the signed history — append-only, one Nostr event per line; step and run facts included
    state/33400-<GoalId>.json  the goal snapshot, latest revision wins
    state/33413-<RunId>.json   one snapshot per run: the frozen workflow, the inputs, every step's record
    state/33412-<WorkflowId>.json   a design drawn for this goal — the Workflow Agent's proposal, or yours; promote copies it to workflows/
    state/33402-<WorkItemId>.json   a work item, bound to its run and step
    ledger.jsonl               budget spend
    edges.json                 the goal graph (refines)
    results/<WorkItemId>.patch the patch a copy workstream left when its item settled
    documents/<name>           the files a person gave the goal as context — a brief, a spec, a screenshot — one per `document` journal fact, named as given (a taken name numbered: `brief (2).pdf`), materialised from the workspace's attachment store wherever the bytes are held
    scratch/                   the goal's scratch: an agent step that names no project (its result is its deliverable), the Workflow Agent's design session, a check command when the goal has no project, and scratch/.tmp — every session's TMPDIR. Nothing here is committed
  projects/<slug>/
    project.json               the record
    tree/                      the folder itself, for a managed root (an adopted root stays where it is)
    workstreams/<WorkstreamId>.json    the record — the primary's checkout is tree/ itself
    workstreams/<WorkstreamId>/        a worktree's or a copy's checkout — never inside the tree
    review/<NoteId>.json       review notes on diffs (IDE; local)
    settings.json              project-scope settings (IDE; syncs)
  channels/state/33405-<id>.json
  conversations/state/33415-<id>.json  a conversation's record — its origin, its title, whether it is archived; its messages are conversation/<id>.jsonl
  notes/                       the notes repository — one git repository holding every note, made by the first note; never synced, pushed only by hand
    workspace/<NoteId>.md      workspace-scope notes
    goals/<GoalId>/<NoteId>.md a goal's notes (removed with the goal)
    projects/<slug>/<NoteId>.md  a project's notes (removed with the project) — never inside the project's tree
    workflows/<WorkflowId>/<NoteId>.md  a workflow's notes
    channels/<ChannelId>/<NoteId>.md    a channel's notes
    node/<NoteId>.md           this node's notes
  drawings/                    the drawings repository — one git repository holding every drawing's .excalidraw file, made by the first drawing; pushed by hand, never pulled (19)
    state/33401-<DrawingId>.json   the record — the truth, synced like an addon's; excluded by the repository's own .git/info/exclude
    workspace/<DrawingId>.excalidraw   the export, laid out like notes/: goals/<GoalId>/, projects/<slug>/, workflows/<WorkflowId>/, channels/<ChannelId>/, node/
  agents/<id>.json             definitions; agents/state/ snapshots; agents/<id>/scratch/ where a conversation's turn runs off a checkout — about a goal, a workflow, the workspace or the node; agents/<id>/recall/
  teams/<id>.json
  skills/<id>.json
  workflows/listening/<WorkflowId>.json   a library workflow that is On: what it listens with and the budget of each run it starts — this machine's, kept apart from the definition; a goal's listening is a field of its own snapshot
  connectors/<id>.json         a connector definition — an outside platform's API, declared; connectors/state/ snapshots (kind 33414); syncs like a skill
  mcp/<id>.json                never synced
  events/                      what listening keeps on this machine — never synced
    queue.jsonl                the durable signal queue: every occurrence written down before anything acts on it, last line per id wins
    listeners/<host>-<step>.json   one listener's memory — when it next comes due, what a poll has seen, a project's last heads, why it could not be armed
    scratch/<host>-<step>/     where a check start with no project runs its command
  conversation/<scope>.jsonl         message history, append-only, one file per scope — a channel's, a goal's thread, a conversation's
  attachments/<2 hex>/<62 hex> bytes, content-addressed — an attachment's and an artifact's alike
  attachments/named/<sha256>/<name>  a blob under its maker's name, made on demand for the file manager and the default app
  pets/<id>/                   installed pet packages
  addons/<id>/addon.json       an addon's record — its manifest, origin, enabled, grants (kind 33407); addons/<id>/files/ its bundle, this machine's; addons/state/ the snapshots
  harnesses/*.json             your custom harness definitions
  sessions/                    a session's record — its kind, its item, its workstream, the conversation it is a turn of — and harness transcripts
  ide/layout/<scope>-<id>.json the workbench layout per root (IDE; never synced)
  logs/                        the diagnostic log — this machine's, disposable, never indexed, never synced ([crates/log](../architecture/crates/log.md))
    <process>/<process>.<period>.jsonl   one folder per process family — node/, cli/, mcp/, desktop/ — their own words about themselves, one JSON line each (errors only by default; `logging.*`); the oldest past `logging.keep_files` removed
    crashes/<process>.<stamp>.<pid>.json one report per abnormal end — a panic with its backtrace, a run that ended without a goodbye, the desktop's node exiting — with the last lines before it; fifty kept
    runs/<process>.<pid>.json            a run's marker while it runs, gone with its goodbye; one left by a dead process is the next start's crash report
  run/
    node.sock                  the daemon's unix socket, mode 0600
    engine.lock                one engine per workspace: an exclusive flock holding the engine's PID
    token                      the control-plane bearer token, mode 0600
    terminals/                 scrollback checkpoints (IDE)
    interactive/<session>/     a terminal harness's reporter files (a settings file, an extension), removed when it exits
  machine.json                 machine-scope settings — never synced
  settings.json                workspace-scope settings
  members.json                 the owner and the people hosted here, each at a role
  invites.json                 the invitations — a secret's hash and its state, mode 0600
  held.json                    the messages from outside no agent may hear yet, and why
  hosts/<host-pubkey>/         what this node holds of a workspace it joined: host.json, channels.json, members.json, conversations/<scope>.jsonl, seen.jsonl, read.json
  net_published.jsonl          the host's pump's ledger — (member, event) pairs delivered once
  seen.jsonl                   every event id ingest has accepted — the dedupe log, appended
  governance.json a2a.toml
  index.sqlite                 the rebuildable index, stamped SCHEMA_VERSION
```

A public hook's secret, agent keys and a connector account's secret fields are **not** in any of those
records: they are keystore entries (`hook:<host>:<step>`, the agent's key,
`connector:<connector>:<account>:<field>`) — `0600` files under `identity/` by default, on every
machine; the OS keyring holds them only with `BISA_KEYSTORE=keyring`.

## What is truth, what is cache, what is local

| | Files | Syncs |
|---|---|---|
| **Truth** | journals, snapshots, records under `goals/` (runs and work items included), `workflows/` (the runs in the workspace included), `projects/*.json`, `channels/`, `agents/`, `teams/`, `skills/`, `connectors/`, `addons/` (the records), `conversation/` | yes, through GEP |
| **Truth, this machine's** | workstreams, `mcp/`, an addon's bundle (`addons/<id>/files/`), notes, review notes, settings scopes marked local, `run/`, `ide/`, `identity/` (keys, code host tokens, connector accounts, git profiles, hook secrets), `workflows/listening/` (which library workflows are On), `events/` (the signal queue, each listener's memory) | never |
| **Cache** | `index.sqlite` and its `-wal`/`-shm` siblings | never; rebuilt on a version mismatch |
| **Disposable, this machine's** | `logs/` — the diagnostic log ([crates/log](../architecture/crates/log.md)): a file past `logging.keep_files` is removed by the process that writes its family, a crash report past fifty by the next one written, a stale run marker by the start that reports it; nothing reads a file back but a person, and a crash report is read back by `GET /logs/crashes/{name}` and `bisa logs` | never |

Bootstrap on open, in order: the owner member, the General Agent and the Workflow Agent, the `general` channel — each
ensured on presence of its file, so a workspace with an unreadable definition elsewhere still opens
with something that answers. A goal snapshot, a run or a `governance.json` written by a shape of the
code from before 0.1.0 is refused by name at open, and a run, a work item or a journal fact filed in
the older goal-only shape is unreadable; there is no conversion. From 0.1.0 a record only grows
([Compatibility](compatibility.md)).

## Nothing runs in `~/.bisa` itself

A session runs in the folder of the thing it works on: a workstream for an `agent` step with a
project — the one it names, or the goal's only one — and `goals/<id>/scratch/` for one with none,
for the Workflow Agent's design session and for a `check` command when the goal has no project;
`workflows/runs/<id>/scratch/` for a step of a run in the workspace that names no project;
`agents/<id>/scratch/` for a chat. A project is made only by an agent's `create_project`. There is no flag anywhere for a working directory.
`bisa files tree <scope> <id>` reads all of it back.

## Upgrades

`SCHEMA_VERSION` is a cache-invalidation marker, not a ladder: a release that changes the index
rebuilds it from the files. Inside 0.x the files only grow, so a later release opens the workspace as
it is; going back to an earlier release is not promised — restore the copy you took before upgrading.
The first major release brings its migration ([Compatibility](compatibility.md)).
