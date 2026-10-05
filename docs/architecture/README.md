# Bisa architecture

Bisa is a local-first, account-free platform where humans and agents carry work from a stated
want to a running outcome. It orchestrates coding harnesses you already have — Claude Code, Codex
CLI, pi, oh-my-pi, OpenCode, GitHub Copilot CLI, Grok Build, Gemini CLI, anything speaking ACP — rather than shipping its own, and it speaks
Nostr (GEP), MCP, ACP and A2A.

A workspace is a directory. There is no account, no server to sign up for, and no deployed
infrastructure. A keypair minted on first run is the identity; collaboration is optional and rides
encrypted over relays that are dumb transport, never authorities.

---

## Status

These documents describe the architecture **as built**, in the present tense. What the platform
does and does not do today is one page: [Feature status](../feature-status.md).

---

## Reading order

| | Document | What it answers |
|---|---|---|
| 01 | [System context](01-context.md) | What this is, who and what it talks to, what runs where |
| 02 | [Domain model](02-domain-model.md) | Every aggregate, every invariant, and where each is enforced |
| 03 | [Workflows](03-workflows.md) | The graph: eighteen step kinds in four families — events, gateways, loops, tasks — start events and who listens for them, boundary events, gates, connectors, validation, the run machine — a goal's runs and the workspace's own — the status projection, three gates |
| 04 | [Workspace, Project, Goal](04-workspace-project-goal.md) | Why association is a relation and not a hierarchy — model and UX |
| 05 | [Channels](05-channels.md) | Standing and direct channels, the permanent `general`, how membership is derived, and the goal's thread |
| 06 | [Agents and teams](06-agents-and-teams.md) | Who does the work, how they are addressed, what context they get |
| 07 | [Layering](07-layering.md) | Crate boundaries, the dependency rule, and how it is enforced |
| 08 | [Persistence](08-persistence.md) | Filesystem as truth, SQLite as a cache, the schema, bootstrap |
| 09 | [GEP — the Goal Engineering Protocol](09-protocol-gep.md) | The wire format, and what is allowed on it |
| 10 | [Runtime flows](10-runtime-flows.md) | Fourteen paths a request takes through the layers, with the files on each |
| 11 | [Security](11-security.md) | The Redactor, the Tool & Commands Guard and the Classifier: what never reaches an agent, what an agent may not run, who reads the doubtful cases |
| 12 | [Artifacts](12-artifacts.md) | What an agent made for a person to look at — a page, a sheet, a deck — carried by a message, rendered live everywhere a message shows, saved and revealed from the desktop |
| 13 | [Conversations](13-conversations.md) | A saved exchange a person starts with agents — its origin (the node, the workspace, a goal, a workflow, a project, a workstream), where a turn runs, lists and search, the reply streaming with its thinking, and why a conversation's turn is never a row of the rail |
| 14 | [Collaboration](14-collaboration.md) | People on other nodes: hosted membership, the four roles and the reach rule, invitations, the protocol and the relay pool, the classifier and the judge on outside messages, the guest replica a mobile client embeds |
| 15 | [The Decision-Making Agent](15-decision-making-agent.md) | A model asked a typed question at eleven decision points, held to one contract; the providers, calibrated and generative; failing closed at the security points; what is recorded, and where |
| 16 | [The setup gate](16-setup-gate.md) | What the platform needs before it can work — git, a harness and the three core agents — checked at start, put in front of everything until it is there, with the official way to install each and the fixes the node vouches for |
| 17 | [Internationalisation](17-internationalisation.md) | A sentence is data — a `Text`, an id and its arguments — rendered in the person's language at the edges that talk to people; one Fluent catalog under `locales/`, read by the crates and the desktop alike; English shipped, a language a folder; what is never translated; the ratchet that holds the sources to it |
| 18 | [Addons](18-addons.md) | A folder of HTML that floats over the desktop as a window: the manifest, the record (kind 33407), the three walls, the bridge and its permissions, what travels |
| 19 | [Drawings](19-drawings.md) | A picture on a canvas beside Notes: the record (kind 33401, reissued), vector only and travelling; the Excalidraw canvas, templates and shape libraries; the seven drawing tools and what the desktop performs; who may draw; the repository without a pull |
| crates | [The crates](crates/README.md) | One page per crate and one for the desktop: every module, every invariant and its test, every extension point |
| ide | [The Project IDE](ide/README.md) | The editor, git, graph, terminals, workstreams, code host, agents in the IDE |

The guides and the reference — how to *use* what these documents specify — are indexed in
[`docs/README.md`](../README.md).

---

## The five rules everything else follows from

**1. The filesystem is truth; SQLite is a cache.**
Every durable fact is a file: a signed event in a journal, a snapshot, a truth record. The index
exists to *find* things and can be deleted at any moment without loss. It carries a version stamp,
and a mismatch discards it and rebuilds from the files — the index needs no migration, because it is
never the truth.

**2. Forward only; nothing breaks inside a major.**
A shape grows by addition — a new field has a default, a new value sits beside the old, a number is
never reused. What one release reads, every later release of that major reads: inside 0.x a minor or
a patch never breaks the workspace on disk, the wire, the HTTP API, the CLI, MCP, the settings or the
addon API. A change that cannot be made by addition waits for the next major, which brings its
migration. The promise is [Compatibility](../reference/compatibility.md); how a change keeps it is
[Keeping compatibility](../contributing/compatibility.md). (Before 0.1.0, the first public release,
the rule was the opposite — a shape changed by becoming the new shape — and the pages record what
that removed.)

**3. Every domain invariant is enforced in exactly one place, and that place is `bisa-core`.**
Not in a route handler, not in a database constraint, not in the UI. Constraints in the schema and
checks in the service layer are *defence against our own bugs*; they are never the definition of a
rule.

**4. A record is cheap and reversible; a filesystem effect is neither.**
The store writes records. The engine is the only thing that creates a directory, runs `git`, or
launches a process. That line is what makes the store testable and the engine auditable.

**5. A goal runs the workflow it was given, and nothing else moves it.**
There is no fixed lifecycle. A goal's workflow — designed by hand, picked from a template, or
proposed by the Workflow Agent and adopted by a person — is a graph of eighteen kinds of step, and one
run at a time carries it, per goal. A workflow also runs on its own, in the workspace, with no goal
behind it — as many runs as are started, none queued. `WorkflowRun::apply` is the only function that
changes a run of either kind; a status is a projection, never a stored field — see
[03 — Workflows](03-workflows.md).

---

## What a reader coming from the code should know first

- **`bisa-core` performs no I/O.** No tokio, no rusqlite, no axum, no reqwest — the manifest
  forbids them. Every rule in it is a pure function with a unit test.
- **`bisa-vcs` depends on nothing of ours**, and it *cannot revert a file*. It can create,
  stage, commit and push. It has never been able to discard a change, and a test greps the crate's
  own shipped source to keep it that way.
- **A shell is a capability of one machine, not a fact about the workspace.** The embedded terminal
  is a desktop-shell command and deliberately not a node route.
- **Nothing is deleted while something points at it**, and the refusal names what is holding it.
