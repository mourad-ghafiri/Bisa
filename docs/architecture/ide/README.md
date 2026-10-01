# The Project IDE — architecture

The workbench is a full development environment: an editor, a file tree with real file management,
a git client with a commit graph, terminals with harnesses in them, workstreams you switch between,
pull requests you review and merge, and the workspace's agents reachable inside the project they are
working in — aware of whether that project belongs to a goal.

These documents are the C4 level-3 view of that surface and the rules under it. They assume the
domain documents one level up: the [workspace / project / goal relation](../04-workspace-project-goal.md),
the [agent model](../06-agents-and-teams.md), the [layering rules](../07-layering.md) and the
[persistence model](../08-persistence.md). Where the code is, module by module, is in
[crates/](../crates/README.md) — the engine's `ide/` modules, the node's `ide` routes and the
desktop's `views/_workbench/`.

---

## Reading order

| | Document | What it answers |
|---|---|---|
| 01 | [Trust boundary](01-trust-boundary.md) | What runs in the node, what stays in the desktop shell, and the rule that decides |
| 02 | [Component model](02-component-model.md) | The components on both sides of the boundary, and the data flows between them |
| 03 | [Files and editing](03-files-and-editing.md) | Two writers on one file, the watcher, large files, tabs, session restore |
| 04 | [Git](04-git.md) | The safe tier, the consented tier, recovery refs, staging by hunk, review notes |
| 05 | [Commit graph](05-commit-graph.md) | The layout algorithm and why it does not collapse on a large repository |
| 06 | [Terminals](06-terminals.md) | Tabs in the centre, splits, scrollback restore, the three-value liveness vocabulary |
| 07 | [Workstreams](07-workstreams.md) | One checkout occupant of a project — the primary, a worktree, a copy — and why switching is free |
| 08 | [Code host](08-code-host.md) | One trait, GitHub, GitLab and Bitbucket behind it, pull requests, review, merge, conflicts |
| 09 | [Agents in the IDE](09-agents-in-the-ide.md) | Conversations about the checkout, context as chips, goal-awareness made visible; the rail draws work sessions and terminals only |
| 10 | [Language intelligence](10-language-intelligence.md) | Language servers: catalog, supervision, proxying |
| 11 | [Mermaid](11-mermaid.md) | One viewer for files, Markdown and agent output; errors on the right line |
| 12 | [Search and quick open](12-search-and-quick-open.md) | Content search, replace across files, the palette |
| 13 | [Settings](13-settings.md) | The registry, three scopes, resolution order |
| 14 | [Performance](14-performance.md) | The budgets, the fixture, and how each is measured |
| 15 | [Keymap](15-keymap.md) | Presets, the command registry, conflict resolution |
| 16 | [The Board](16-board.md) | Every workstream a card in five columns — a view over workstreams, never their lifecycle |
| 17 | [Links and paths](17-links-and-paths.md) | Every path and URL in text is a door — one scanner, one resolver, one handler; the card before the browser |
| 18 | [The embedded browser, and agents that browse](18-browser-and-servers.md) | The Browser button and its menu (*New tab · From folder…* — a picker over the checkout's tree, Root first — the servers up with Open and Stop) and the Terminal menu's run command, the browser tab as a native layer in the IDE and the Browser pane beside any screen, what its bar shows, screenshots, who may ask, artifacts on an origin of their own, annotating a served page and its three doors, the tools agents drive it with |
| 19 | [Mobile Development: Flutter on the devices beside the code](19-mobile-development.md) | The toolchain probed and the devices listed by the node, the Devices button, a device mirrored beside the code in a document of its own, the run terminal and hot reload, captures marked on the screen and sent as chips, the four mobile tools and who may ask, Settings › Capabilities › Mobile Development, the Mobile Developer and the release template |
| 20 | [Reviewing agent changes](20-reviewing-agent-changes.md) | A checkout conversation's mode (manual · auto · plan), the change ledger that attributes every edit to its turn, the conflict rule that folds an outside write into the base, settling a change by turn, file or hunk, restore to before a message, and the asks a turn answers in the conversation itself |

The workflow designer is a desktop surface but not part of the Project IDE; its contract is in
[03 — Workflows](../03-workflows.md#the-designer).

---

## The four rules everything else follows from

**1. Blast radius decides the process.** The node holds any capability whose blast radius is the
workspace — files, git, search, language servers, the code host. The desktop shell holds any capability
whose blast radius is the machine — the PTY. See [01](01-trust-boundary.md).

**2. Every write goes through the engine.** The layering rule in [07](../07-layering.md) is honoured
literally: `engine::ide::*` owns each filesystem effect, node routes are thin, and the one type an
agent must never hold — `HumanConsent` — is minted in exactly one place. See [02](02-component-model.md).

**3. Nothing in the tree can make a change unrecoverable.** The version-control crate's invariant
strengthens from *cannot revert a file* to this. Every tree-moving operation writes a recovery ref
before it runs. See [04](04-git.md).

**4. Nothing is injected that is not a chip.** An agent working in a workstream sees exactly what
the person can see and remove above the composer — including whether the project is attached to a
goal. See [09](09-agents-in-the-ide.md).

---

## What this surface deliberately does not do

- **It does not sandbox.** Placement decides where work starts and where the platform itself
  writes; a shell in a terminal can still go anywhere the account can. That rule is stated in
  [04 — Workspace, Project, Goal](../04-workspace-project-goal.md).
- **It does not move the terminal into the node.** A shell is the ambient authority of whoever is at
  the keyboard, and a token in the workspace directory does not change that argument.
- **It does not copy another IDE.** The liveness vocabulary, the review-note shape, the
  instance-lock shape and the primary workstream are each defined once, in the module that owns
  them, and documented there.
- **It does not add a second palette, a second file tree, a second chat, or a second tab strip.**
  Each exists once and is extended.
