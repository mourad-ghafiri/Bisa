# The Project IDE

For a person, the Project IDE is the Projects destination: an editor and a file tree, search, a git
client with a commit graph, terminals with harnesses in them, language servers, an embedded browser,
Mobile Development for Flutter apps, pull requests on GitHub, GitLab and Bitbucket, and the workspace's
agents in conversations about the checkout. In code it is the engine's `ide` modules and their
neighbours, the node's IDE routes, five leaf crates — `bisa-vcs`, `bisa-lsp`, `bisa-mobile-development`,
`bisa-codehost`, `bisa-ssh` — and the desktop's workbench; a terminal is the Tauri shell's.

## Where it lives

- Files and editing: `crates/bisa-engine/src/ide/files.rs`, `crates/bisa-engine/src/ide/watch.rs`, `crates/bisa-node/src/ide.rs`.
- Search and quick open: `crates/bisa-engine/src/ide/search.rs`, `crates/bisa-engine/src/ide/index.rs`.
- Git: `crates/bisa-vcs/` (the safe tier in `crates/bisa-vcs/src/git.rs`, the consented tier in `crates/bisa-vcs/src/interactive.rs`), `crates/bisa-engine/src/ide/git.rs`, `crates/bisa-engine/src/ide/interactive.rs`, `crates/bisa-node/src/ide/interactive.rs`, `crates/bisa-node/src/ide/consent.rs`.
- Commit graph: `crates/bisa-engine/src/ide/graph.rs`.
- Terminals: `desktop/src-tauri/src/terminal.rs`, `desktop/src/terminal/`, and `crates/bisa-engine/src/interactive.rs` for a harness reporting from a tab.
- Language servers: `crates/bisa-lsp/`, `crates/bisa-engine/src/lsp.rs`, `crates/bisa-node/src/ide/lsp.rs`.
- Browser and served folders: `desktop/src-tauri/src/browser.rs`, `desktop/src/browser/`, `crates/bisa-engine/src/browser.rs`, `crates/bisa-node/src/browser.rs`, `crates/bisa-node/src/ide/serve.rs`.
- Mobile Development: `crates/bisa-mobile-development/`, `crates/bisa-engine/src/mobile_development.rs`, `crates/bisa-node/src/mobile_development.rs`.
- Pull requests and code hosts: `crates/bisa-codehost/`, `crates/bisa-ssh/`, `crates/bisa-engine/src/codehost.rs`, `crates/bisa-engine/src/gitprofiles.rs`, `crates/bisa-node/src/codehost.rs`, `crates/bisa-node/src/ssh.rs`.
- Agents in the IDE and reviewing their changes: `crates/bisa-engine/src/conversation.rs`, `crates/bisa-engine/src/changes/`, `crates/bisa-node/src/changes.rs`.
- The desktop: `desktop/src/views/_workbench/`, `desktop/src/views/_work/`.

## Read first

- [The Project IDE — architecture](../../architecture/ide/README.md) — the four rules everything else follows from.
- [ide/01 — Trust boundary](../../architecture/ide/01-trust-boundary.md), [ide/02 — Component model](../../architecture/ide/02-component-model.md) — what the node holds, what the shell holds, the data flows.
- The page for the piece you touch: [03 Files and editing](../../architecture/ide/03-files-and-editing.md) · [04 Git](../../architecture/ide/04-git.md) · [05 Commit graph](../../architecture/ide/05-commit-graph.md) · [06 Terminals](../../architecture/ide/06-terminals.md) · [08 Code host](../../architecture/ide/08-code-host.md) · [09 Agents in the IDE](../../architecture/ide/09-agents-in-the-ide.md) · [10 Language intelligence](../../architecture/ide/10-language-intelligence.md) · [12 Search and quick open](../../architecture/ide/12-search-and-quick-open.md) · [18 Browser and servers](../../architecture/ide/18-browser-and-servers.md) · [19 Mobile Development](../../architecture/ide/19-mobile-development.md) · [20 Reviewing agent changes](../../architecture/ide/20-reviewing-agent-changes.md).
- [The Project IDE guide](../../guide/the-ide.md) — every surface, as a person meets it.
- The crate pages: [vcs](../../architecture/crates/vcs.md), [lsp](../../architecture/crates/lsp.md), [mobile-development](../../architecture/crates/mobile-development.md), [codehost](../../architecture/crates/codehost.md), [ssh](../../architecture/crates/ssh.md).

## Rules a change must keep

- Blast radius decides the process: the node holds what reaches the workspace — files, git, search, language servers, the code host; the shell holds what reaches the machine — the PTY, never a node route.
- Every write goes through the engine: the engine's `ide` modules own each filesystem effect, and node routes stay thin.
- A file write lands only under a writable root, never on a truth file (I36); it states the hash it read, and a mismatch is refused with the current text (I37); a path stays inside its base (I9).
- Nothing in the tree can make a change unrecoverable: the safe tier cannot revert a file (`crates/bisa-vcs/src/git.rs` never spells `checkout`, `restore`, `reset`, `clean` or `stash` as a command); a tree-moving operation needs a `HumanConsent`, minted in one place from the request's `Authorization` header, and writes a recovery ref first (I38, I39); no agent-facing crate names the consented tier; the plain force flag is never spelled.
- A path from a selection reaches git `:(top,literal)`-wrapped (I12); the global git layer is written by three named engine functions and nowhere else (I45).
- Nothing is injected that is not a chip: an agent sees exactly what the person sees above the composer (I43); the Workflow Agent is never offered in a conversation about a checkout.
- No private key is opened and no credential is logged, journaled or returned; tests reach no real code host, CLI, keychain or SSH directory.
- The platform installs no language server and never runs `flutter run` itself; a served folder hands out only its own files.

## Testing a change

- Files and search: `scripts/test module engine ide_files`, `scripts/test module engine ide_search`, `scripts/test module node ide`.
- Git: `scripts/test crate vcs`, `scripts/test module engine projects`, `scripts/test module engine ide_review`, `scripts/test module engine ide_graph`.
- Terminals: `scripts/test module engine interactive`, `scripts/test module node sessions`, `scripts/test tauri`, `scripts/test desktop terminal`.
- Language servers: `scripts/test crate lsp` (a scripted server, never a real one), `scripts/test module engine lsp`.
- Browser and Mobile Development: `scripts/test module engine browser`, `scripts/test module engine mobile_development`, `scripts/test lib mobile-development`.
- Code hosts and SSH: `scripts/test crate codehost`, `scripts/test lib ssh`, `scripts/test module engine gitsetup` — loopback stubs and fakes only.
- Agents and their changes: `scripts/test module engine conversation`, `scripts/test module engine changes`, `scripts/test module node changes`.
- The desktop: `scripts/test desktop views/_workbench`, `scripts/test desktop views/_work`, and the scenarios under `desktop/src/scenarios/`.
- Journeys, each with `scripts/test module cli e2e <name>`: `files_and_search`, `git_in_a_checkout`, `a_branch_goes_out`, `a_harness_in_a_terminal`, `a_folder_served_and_a_browser_asked`, `a_conversation_about_a_checkout`.
- One module at a time; the whole workspace (`just verify`) only at the end of a pass ([Running](../testing-rules.md#running)).

## Common changes

- [Add an HTTP route](../recipes.md#1-add-an-http-route) — an IDE effect is an engine `ide` module; the route parses and delegates.
- A consented git operation: [crates/engine § Extension points](../../architecture/crates/engine.md#extension-points).
- [Add a Tauri command](../recipes.md#13-add-a-tauri-command) — a capability of the machine.
- [Add a keymap command](../recipes.md#6-add-a-keymap-command), [Add a desktop model](../recipes.md#7-add-a-desktop-model), [Add a screen or a settings panel](../recipes.md#14-add-a-screen-or-a-settings-panel).
- [Add a setting](../recipes.md#2-add-a-setting) — `editor.*`, `terminal.*`, `lsp.*`, `browser.*`, `mobile_development.*`.

## Compatibility

- The IDE's routes and bus frames are [the HTTP API and its events](../../reference/compatibility.md#the-http-api-and-its-events); its command ids are [the keymap](../../reference/compatibility.md#the-keymap) — `keymap.overrides` is keyed by them; its keys are [settings keys](../../reference/compatibility.md#settings-keys); the browser's and Mobile Development's tools are [MCP tools](../../reference/compatibility.md#mcp-tools); the git and workstream verbs are [the command line](../../reference/compatibility.md#the-command-line). Layout, scrollback and other memories of this machine are not promised.
- Inside 0.x a minor or patch release never breaks the public contract: add a route, a field, a command, a key.
- Renaming a command id, a key or a route cannot be made by addition; it waits for 1.0.0, which brings a migration tool ([the first major release](../../reference/compatibility.md#the-first-major-release)).
- Declare the compatibility of your change in the pull request.

## Review focus

- Which process owns the capability, by blast radius, and does every write go through the engine ([code review](../review/code.md))?
- Can anything discard a change without consent and a recovery ref, reach outside its root, or carry a credential ([security review](../review/security.md))?
- Large repositories and files: caps held at exactly-at and one-past, the graph and search within their budgets ([performance review](../review/performance.md)).
- A new editor, renderer or viewer is a licence the platform ships ([licences review](../review/licences.md)).
