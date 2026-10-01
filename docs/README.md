# Bisa documentation

Three kinds of document, and the line between them is kept on purpose. A **guide** teaches one
thing end to end and is written for a person using the platform. A **reference** enumerates and is
written to be looked up. **Architecture** explains how the system is shaped and why each rule holds.

Everything here is present tense and describes the code at `HEAD`. What the platform does today,
what is partial, and what it does not do is one page: [Feature status](feature-status.md).

## Start here

| | |
|---|---|
| [Feature status](feature-status.md) | what is implemented, what is partial, what is not — crate by crate and screen by screen |
| [Getting started](guide/getting-started.md) | build, first run, a goal from a sentence to done |
| [Architecture](architecture/README.md) | the domain, its rules, the crates and the Project IDE |
| [Setup](contributing/setup.md) | toolchain and targets for anyone changing the code |

## Guides

| | |
|---|---|
| [Getting started](guide/getting-started.md) | build, first run, a goal from a sentence to done, templates, the daemon, the desktop app |
| [Projects](guide/projects.md) | folders, attachment, workstreams, the primary workstream, the `publish` gate |
| [Goals](guide/goals.md) | the status projection, auto · guided · manual, what a run looks like, questions and answers, work items, amending, budgets |
| [Workflows](guide/workflows.md) | the eighteen step kinds in four families, the catalog's templates, the designer, the TOML shape, the Workflow Agent, runs in the workspace and on a goal, running and waiting |
| [Agents and teams](guide/agents-and-teams.md) | the three core agents, the catalog, definitions, model plans and effort, skills, MCP servers, assignment |
| [Channels](guide/channels.md) | standing channels, `general`, rosters, direct channels, triage |
| [Artifacts](guide/artifacts.md) | what an agent made for you to look at — pages, charts, sheets, decks, documents — rendered live in every conversation, saved, revealed, opened |
| [Collaboration](guide/collaboration.md) | people on other nodes: invitations, the four roles, relays with their health, held messages, hosted sections, direct links |
| [Events and gateways](guide/events.md) | what starts a workflow — start events, turning a workflow On, a goal that listens — what a wait holds for, boundary events, `emit` and `end`, the gateways, hooks, durability and the loop guard |
| [The Decision-Making Agent](guide/decisions.md) | switching it on, choosing who answers, trying it, reading judgements, auto-routing a model plan, choosing the effort automatically, the `judge` step, the security classifier |
| [Operating](guide/operating.md) | the daemon, governance, deleting things, tags, templates, where the browser lives, A2A |
| [Real-world scenarios](guide/real-world-scenarios.md) | a hundred jobs people bring, each as the generic parts that do it, and what each still needs from a person |
| [The desktop](guide/the-desktop.md) | the collaboration app: every screen, the conversation surface, the workflow designer, notes, the pet, the addons |
| [The Project IDE](guide/the-ide.md) | the Projects destination: editor, files, workstreams, git, review notes, the commit graph, pull requests, agents in a project, terminals |
| [Addons](guide/addons.md) | the widgets that float over the app — installing a built-in, importing a folder, what each may do; writing one: the folder, the manifest, the library |

## Reference

| | |
|---|---|
| [CLI](reference/cli.md) | every verb |
| [HTTP API](reference/http-api.md) | every route — **generated** from the node's route tables by `just gen-api-docs` |
| [Workspace layout](reference/workspace-layout.md) | what is on disk, and which of it is truth |
| [GEP](reference/gep.md) | the Goal Engineering Protocol: kinds, envelopes, what is and is not on the wire |
| [Catalog](reference/catalog.md) | every agent, skill, team, channel and workflow template — **generated** from `library/catalog/` by `just gen-catalog-docs` |
| [MCP tools](reference/mcp-tools.md) | what an agent's session can call, by scope |
| [Settings keys](reference/settings-keys.md) | every setting, its kind, default and scopes — **generated** from the registry by `just gen-settings-docs` |
| [Keymap](reference/keymap.md) | every command, its scope and chord per preset — **generated** from the desktop's keymap model by `just gen-keymap-docs` |
| [The addon API](reference/addon-api.md) | every method `window.bisa` offers an addon, the permission each needs, the events, the refusals |
| [Compatibility](reference/compatibility.md) | what every release promises about what an earlier one left behind: the 0.x rule, surface by surface, deprecation, the first major |

## Architecture

[`architecture/`](architecture/README.md) — the domain and its shape, the runtime flows, the
[crate pages](architecture/crates/README.md) that map every module of the code, and the documents
on the [Project IDE](architecture/ide/README.md).

## Contributing

Start with [CONTRIBUTING.md](../CONTRIBUTING.md) — the map — and, for a coding agent,
[AGENTS.md](../AGENTS.md).

| | |
|---|---|
| [How to contribute](contributing/how-to-contribute.md) | the path: an accepted issue, a design when it is large, a branch, the change, the gates, the pull request |
| [Area guides](contributing/areas/README.md) | every part of Bisa — where it lives, what to read, the rules, the tests, the recipes |
| [Review](contributing/review/README.md) | the stages, who approves, and the checklists: code, security, performance, licences, compatibility, docs and language |
| [Keeping compatibility](contributing/compatibility.md) | how a change keeps the promise, surface by surface; deprecating; declaring |
| [Migrations](contributing/migrations.md) | how a major release and its migration are prepared |
| [Triage](contributing/triage.md) | labels, priorities, response targets, good first issues |
| [For maintainers](contributing/maintainers.md) | the repository's settings: Discussions, reporting, the ruleset on `main` |
| [Setup](contributing/setup.md) | toolchain, `just` targets, the desktop |
| [Recipes](contributing/recipes.md) | to add a route, a setting, an event, a tool, a command, a model, an adapter, a kind, a path, a step kind — the files in order and the gate that catches a miss |
| [Feature status](feature-status.md) | what the platform does not do today, and where each change lands |
| [Testing rules](contributing/testing-rules.md) | what a test may never do, and the tests that enforce it |
| [Feature coverage](contributing/coverage.md) | every implemented feature, the suites that prove it, and the command that runs them |
| [Performance](contributing/performance.md) | the node's build profile, the pause-when-hidden rule for pollers, stable store snapshots, bounded caches, and how to measure |
| [Terminology](contributing/terminology.md) | the vocabulary, the lint, and how to excuse a third-party identifier |
| [Layering](architecture/07-layering.md) | crate edges, the write rule, and how both are enforced |
| [Release](contributing/release.md) | versions and what each may change, `just verify`, the generated files, the release checklist, supported versions |

## How this set is kept true

Four pages are **generated** and never edited by hand — the HTTP API, the settings keys, the keymap
and the catalog — and CI fails when any is stale. The rest is held to the code by guard tests that
run with `cargo test --workspace` and `npm test`; the full list is in
[Testing rules](contributing/testing-rules.md#guard-tests).
