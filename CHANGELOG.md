# Changelog

Every notable change to Bisa, newest first. The format is
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). What is not released yet sits under
*Unreleased*; `just set-version <version>` turns it into the dated section a release ships as its
notes (`docs/contributing/release.md`).

## [Unreleased]

### Added

- The website, `website/`: a tour of the app in its own order, built from the repository's own
  facts by `scripts/website/build.mjs` and held by its tests (`just website`, `just website-check`).
- Contribution guides and templates: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`,
  `SUPPORT.md`, `AGENTS.md`; issue forms, the pull request template and `CODEOWNERS` under `.github/`;
  under `docs/contributing/`, how to contribute, the review process and its checklists, triage, the
  maintainers' settings, keeping compatibility, migrations, and a guide for every area.
- `docs/reference/compatibility.md`: what every release promises about what an earlier one left behind.

### Changed

- The desktop app's look, refined across every screen without changing what any screen does. The
  accent now means one thing, as the theme's colour rules always said: something is waiting on you.
  "Your move", a question, a gate, a join request, a held ask and the Inbox's counts wear it; the row,
  tab, filter or node you are on wears a neutral selection (`--color-selected`), and dividers inside a
  surface are hairlines (`--color-hairline`), both mixed from each theme's own roles. "Agents working"
  is neutral, so it never reads as your move. One primary button per region; section and sidebar
  labels in sentence case; open empty states instead of dashed boxes; quieter tags, chips and
  explanatory prose; themed text selection and caret. The notes and draw buttons keep the content
  clear of them: a list's last row scrolls above them and a composer's Send moves aside, only while
  one stands there (`desktop/src/ui/dockClearance.ts`).
- A run's canvas is a live map: the flow the run came along moves toward the step it stands on, a
  running step breathes, a step waiting on you calls once, and a step's ring changes colour instead
  of snapping; the path already taken is drawn firm and neutral. Nothing moves under reduced motion.
- Each step family — events, gateways, loops, tasks — wears its own quiet ink on its glyph, on the
  canvas, in the palette, in thumbnails and in a run's steps (`--color-step-*`).
- Reading: Markdown, chat and documents are set at the 14px body, running text holds a measure of
  about 70 characters (`max-w-measure`), document headings sit a clear step over it, and readable text
  keeps a 12px floor. Inter's tabular numerals work again where columns line up.
- Layout: list-and-detail screens, the designer's inspector forms and Settings › Appearance split by
  the room they have rather than the window's width, so an open pane or a narrow window no longer
  squeezes them. The Inbox count arrives once when something new waits on you; clearing the last
  item says so in the empty state.
- Compatibility is a promise from 0.1.0: inside 0.x a minor or a patch release never breaks the
  workspace, the wire, the HTTP API, the CLI, MCP, the settings or the addon API; a change that cannot
  be made by addition waits for 1.0.0 and its migration. `SECURITY.md` and the release process follow it.

## [0.1.0] - 2026-10-01

The first release: the platform as one application a person can download.

### Added

- The desktop application for macOS — one universal `Bisa.app` for Apple Silicon and Intel from
  macOS 11, the node inside it — shipped as a signed, notarized disk image with its SHA-256
  (`scripts/release-macos.sh`), published on GitHub with these notes (`scripts/publish-release.sh`).
- Goals in three modes — auto, guided, manual — designed by the Workflow Agent, run as workflows of
  eighteen step kinds, with events and gateways, boundaries, approvals, budgets and runs in the
  workspace; the Decision-Making Agent as the third core agent, off by default.
- Agents and teams on the coding harnesses already installed — Claude Code, Codex CLI, OpenCode,
  GitHub Copilot CLI, Grok Build, pi, oh-my-pi and any ACP agent — each with a model plan and an
  effort; skills, MCP servers and a catalog of templates, agents, teams, channels, connectors,
  addons and pets.
- The Project IDE: projects and workstreams, files, search, git with safety refs, terminals that
  report and are guarded, agents in conversations with reviewing of their changes, language
  intelligence, the embedded browser, served folders, mobile development, pull requests on GitHub,
  GitLab and Bitbucket.
- Channels, direct messages, conversations and artifacts; notes and drawings; the Inbox and the
  Pulse; collaboration over relays with roles and a held queue; system notifications and the menu
  bar icon; settings at three scopes with a security model of asks, rules and a classifier; a
  diagnostic log; every sentence a person reads in a translation catalog.
- The `bisa` command line and the node's HTTP API, the MCP server with the platform's tools, and
  the A2A door.

### Changed

- The macOS bundle identifier is `dev.bisa.bisa`. A build under the earlier identifier kept its
  window state and the shell's own fallback log under `~/Library/Application Support/dev.bisa.desktop/` and
  `~/Library/Logs/dev.bisa.desktop/`; this release writes under `dev.bisa.bisa` and reads nothing
  from the old folders. The earlier build keeps the `bisa://` link claim until it is moved away
  (`docs/contributing/recipes.md`).
- `scripts/bundle-macos.sh` builds the app for this Mac alone (signed ad hoc) and nothing else;
  the release is `scripts/release-macos.sh`.

### Removed

- `BISA_SIGN`, `just bundle-macos-adhoc` and the shareable zip of the bundle script: a copy to
  give to someone is the release's disk image.

[Unreleased]: https://github.com/mourad-ghafiri/Bisa/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.1.0
