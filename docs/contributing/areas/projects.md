# Projects

For a person, a project is a real folder on disk, with or without git, that work runs in. It belongs to
the workspace; a goal is attached to it — never the other way round. A **workstream** is one checkout
of it — the primary (the project's own root), a git worktree on a branch of its own, or a copy — that
survives to a commit, a push and a pull request. The `publish` gate stands before anything leaves the
machine, and the Board shows every workstream as a card. In code: the core's project, workstream and
Board types, the store's records, and the engine's `projects` module, which makes folders, runs git and
settles worktrees.

## Where it lives

- `crates/bisa-core/src/project.rs`, `crates/bisa-core/src/attachment.rs`, `crates/bisa-core/src/origin.rs` — `Project`, `Slug`, attachment, where a project was born.
- `crates/bisa-core/src/workstream.rs` — the lifecycle (`Workstream::apply`) and branch names.
- `crates/bisa-core/src/board.rs` — the Board's column and placement, apart from the lifecycle.
- `crates/bisa-core/src/placement.rs` — which project and checkout an agent step runs in.
- `crates/bisa-store/src/projects.rs`, `crates/bisa-store/src/workstreams.rs` — the records; one primary per project, born with it.
- `crates/bisa-engine/src/projects.rs` — create, clone, import, adopt, attach, publish, settle; `crates/bisa-engine/src/scripts.rs` — workstream scripts.
- `crates/bisa-node/src/projects.rs` — the project and workstream routes; `crates/bisa-cli/src/projects.rs` — `bisa project` and `bisa workstream`.
- `desktop/src/views/_work/`, `desktop/src/views/_board/` — the workstream panels and the Board.

## Read first

- [Projects guide](../../guide/projects.md) — four ways in, attaching, workstreams, the primary, [the `publish` gate](../../guide/projects.md#the-publish-gate).
- [04 — Workspace, Project, Goal](../../architecture/04-workspace-project-goal.md) — association as a relation, not a hierarchy.
- [ide/07 — Workstreams](../../architecture/ide/07-workstreams.md) — the lifecycle, where a workstream starts, after a merge, scripts.
- [ide/16 — The Board](../../architecture/ide/16-board.md) — a view over workstreams, never their lifecycle.
- [10 — Runtime flows § A pull request through the Publish gate](../../architecture/10-runtime-flows.md#a-pull-request-through-the-publish-gate).

## Rules a change must keep

- A project's existence never depends on a goal's; origin is history and attachment is the relation (I13); detaching moves no bytes and rewrites no origin (I14).
- A goal never enters a path, a URL, a slug or an id; no attachment is implicit; nothing cascades from a goal to a project's bytes unless the person chose it, project by project ([04 § What this design refuses to do](../../architecture/04-workspace-project-goal.md#what-this-design-refuses-to-do)).
- A slug is an allowlist (I10); a branch name is sanitised, never refused (I11).
- Never write into a folder the platform did not create: adopting writes nothing, and the platform's scratch never lands in an adopted root (I41).
- A workstream's state changes only through a `WorkstreamTransition`; a Board column is a separate field a person sets, and no drag moves a branch (I42).
- An agent step runs in its project's workstream or in its home's scratch folder; no project is born of a step (I46).
- A push and a pull request pass the `publish` gate, the one gate that never defers to an assignment ([03 § Three gates](../../architecture/03-workflows.md#three-gates)).
- The platform writes git config only for the keys in its schema, at the layer a person named (I45); who commits is decided by I47.
- A workstream never syncs: it names a directory on one machine ([09 § The one question](../../architecture/09-protocol-gep.md#the-one-question)).

## Testing a change

- `scripts/test lib core workstream::` — every cell of the lifecycle; `scripts/test lib core board::`, `scripts/test lib core placement::`.
- `scripts/test module engine projects`, `scripts/test module engine placement`; `scripts/test module engine projects publish` narrows to the publish tests.
- `scripts/test module node node`, `scripts/test module node ide`, `scripts/test module node project_delete`.
- `scripts/test desktop views/_work`, `scripts/test desktop views/_board`.
- Journeys: `crates/bisa-cli/tests/it/e2e/projects_and_workstreams.rs`, `crates/bisa-cli/tests/it/e2e/a_branch_goes_out.rs`.
- A git fixture is a temporary repository with a local bare `origin`, so a push leaves nothing but the machine it ran on ([What a test may never do](../testing-rules.md#what-a-test-may-never-do)).
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add an HTTP route](../recipes.md#1-add-an-http-route), [Add a CLI verb](../recipes.md#12-add-a-cli-verb)
- [Add a setting](../recipes.md#2-add-a-setting) — `workstreams.*` and `git.*` keys.
- [Add a workspace path](../recipes.md#10-add-a-workspace-path) — a new folder under a project.
- [Add an engine event](../recipes.md#4-add-an-engine-event) — a workstream fact the rail and the Board follow.

## Compatibility

- Project and workstream records are [the workspace on disk](../../reference/compatibility.md#the-workspace-on-disk); the project, workstream and publish routes with their refusal codes are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events) — a client switches on `code`, never on the words; `bisa project` and `bisa workstream` are [the command line](../../reference/compatibility.md#the-command-line); `workstreams.*` and `git.*` are [settings keys](../../reference/compatibility.md#settings-keys).
- Inside 0.x a minor or patch release never breaks the public contract: add a field with a default, a route, a verb, a code.
- Renaming a lifecycle state, a publish policy or a code cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Does anything write into an adopted folder, move a project's bytes on a goal's account, or put a goal in a path ([security review](../review/security.md))?
- Does a state change go through the one writer and stay apart from the Board's column ([code review](../review/code.md))?
- Does every outward action pass the `publish` gate and answer with the documented status and code?
- Do git tests use temporary repositories with a local bare origin and none of this machine's git config?
- Are lists bounded for a thousand cards and a large repository ([performance review](../review/performance.md))?
