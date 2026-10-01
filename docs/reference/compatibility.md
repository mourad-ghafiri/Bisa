# Compatibility

What a release of Bisa promises about everything an earlier release left behind: your workspace, the
programs and scripts that talk to it, the workflows you wrote, the addons you installed. Read this
before you upgrade, before you build on an interface, and — as a contributor — before you change one.

## The promise

Bisa follows [Semantic Versioning 2.0.0](https://semver.org/), held to a stricter rule in the 0.x line
than the specification asks for. Semantic Versioning lets a 0.y.z release change anything; Bisa does
not, from 0.1.0, its first public release:

| Release | What it may do |
|---|---|
| **Patch** — 0.y.**Z** | fix behaviour; nothing on disk, on the wire or in any interface below changes shape |
| **Minor** — 0.**Y**.0 | add: new routes, verbs, flags, keys, tools, kinds, fields with a default; everything that worked keeps working |
| **Major** — **1**.0.0 | the first release allowed to change something incompatibly — and only with a migration tool and a migration guide ([The first major release](#the-first-major-release)) |

A change that cannot be made by addition waits for the next major. It is never released in a minor or
a patch.

**Upgrading inside 0.x:** install the new release over the old one; your workspace (`~/.bisa`) is read
as it is. Back it up first all the same — a copy or a Time Machine snapshot. If you might want to go
back to the earlier release, the backup is the only way: see [Not promised in 0.x](#not-promised-in-0x).

## Surface by surface

### The workspace on disk

- **Promised:** a later 0.x release opens a workspace an earlier 0.x wrote — goals, runs, workflows,
  projects, agents, teams, channels, notes, drawings, settings, keys — and keeps it as it is.
- **May grow:** new files and folders; new fields that have a default when they are absent.
- **Not promised:** `index.sqlite` — a cache rebuilt from the files whenever its schema changes (a
  rebuild clears read marks, and the changelog says when one happens); the diagnostic log; this
  machine's window and layout memories.
- The layout is described in [Workspace layout](workspace-layout.md).

### The HTTP API and its events

- **Promised:** every route and its method; the fields a request accepts and the fields a response
  carries, with their types; every `ErrorCode`; the streams of `GET /events` and the frames' shapes.
- **May grow:** new routes; new optional request fields; new response fields; new error codes; new
  engine events and topics.
- **A client must:** ignore response fields, error codes, events and topics it does not know; switch
  on an error's `code`, never on its words or its status alone; send only the fields the node it talks
  to documents (a field a node does not know is refused by name).
- The reference is generated from the code: [HTTP API](http-api.md), and the wire types in
  `desktop/api-schema.json`.

### The command line

- **Promised:** every verb and flag of `bisa`; the exit statuses; under `--json`, the output's shape
  and the rule that a failure prints nothing on standard output.
- **May grow:** new verbs and flags; new fields in a `--json` answer.
- **Not promised:** the human-readable output — its wording and layout.
- The reference: [CLI](cli.md).

### MCP tools

- **Promised:** every tool name and the inputs it accepts, in each scope.
- **May grow:** new tools; new optional inputs.
- **Not promised:** the tools' descriptions.
- The reference: [MCP tools](mcp-tools.md).

### Settings keys

- **Promised:** every key, its kind, the scopes it may be set at, and its choices.
- **May grow:** new keys; new choices.
- **May change in a minor:** a default — always named in the changelog.
- The reference is generated from the registry: [Settings keys](settings-keys.md).

### The keymap

- **Promised:** every command id — `keymap.overrides` is keyed by them.
- **May grow:** new commands.
- **May change in a minor:** a default chord, named in the changelog.
- The reference: [Keymap](keymap.md).

### The collaboration wire

- **Promised:** every GEP kind number. From 0.1.0 a number is never reused: a retired number stays a
  hole for good.
- **May grow:** new kinds.
- **Not promised in 0.x:** nodes on different minor releases in one shared workspace — see
  [Not promised in 0.x](#not-promised-in-0x).
- The reference: [GEP](gep.md).

### The addon API

- **Promised:** every method, permission and event of the bridge an addon talks to, and every field
  of an addon's manifest.
- **May grow:** new methods, permissions and events.
- The reference: [Addon API](addon-api.md).

### Workflow, connector and catalog files

- **Promised:** a workflow or connector definition a release accepts is accepted, and means the same,
  by every later release of the line.
- **May grow:** new optional keys; new step kinds, start events and conditions.
- **Not promised:** that an earlier release reads a definition using something added later — a key it
  does not know is refused by name, never dropped.
- **The catalog's content** — which agents, skills, teams, templates, connectors, addons and pets ship —
  may change in any release. What you installed is yours: an upgrade never changes an installed copy.

## Not promised in 0.x

| What | Why | What to do |
|---|---|---|
| **Going back** to an earlier release on the same workspace | every record is read strictly: a field a release does not know is refused by name, never dropped — so an earlier release may refuse a workspace a later one wrote | keep the backup you took before upgrading; restore it to go back |
| **Mixed releases in one shared workspace** | a record written by a newer minor may carry what an older peer cannot read yet; the older peer sets it aside | upgrade every node of a shared workspace together |
| **Internals** — the Rust crates' APIs, the desktop's code, `index.sqlite`, the log's format, the words of messages, machine-local memories | they are the implementation, not the contract | build on the HTTP API, the CLI, MCP and the addon API |

## Deprecation

A part of the contract that is to be retired:

1. is marked deprecated in the minor release that decides it — on its reference page, and under
   *Deprecated* in the [changelog](../../CHANGELOG.md);
2. keeps working for the rest of the 0.x line;
3. is removed only in a major release, by the migration that release brings.

## The first major release

1.0.0 is the first release allowed to change the contract incompatibly. It is made only when a change
cannot be made by addition, and it brings:

- a **migration tool** that copies the workspace aside before it changes anything, offers a dry run,
  can be run again safely, verifies what it wrote, and leaves the copy in place as the way back;
- a **migration guide** that names every incompatible change and what to do about each;
- **release candidates**, so the migration can be tried on real workspaces before the release.

How a major is prepared is in [Migrations](../contributing/migrations.md).
