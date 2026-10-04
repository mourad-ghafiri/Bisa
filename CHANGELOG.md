# Changelog

Every notable change to Bisa, newest first. The format is
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). What is not released yet sits under
*Unreleased*; `just set-version <version>` turns it into the dated section a release ships as its
notes (`docs/contributing/release.md`).

## [Unreleased]

### Added

- A file copied in Finder and pasted into a terminal with ⌘V — or dragged onto it, one or several —
  is typed as its path, quoted as Terminal.app quotes it, in a shell and in a harness alike: Claude
  Code takes an image's path as the image. A screenshot on the clipboard is saved to a file in the
  machine's temporary folder, and that path is typed. Plain text pastes as before.
- A Mac terminal's text editing, in a shell and in a harness's prompt: ⌘← and ⌘→ go to the start and
  the end of the line, ⌥← and ⌥→ move a word, ⌘⌫ deletes to the start of the line and ⌥⌦ the next
  word. Each is a terminal command in Settings › Keymap, rebindable and unbindable.
- The New Goal dialog asks, optionally, **who carries it**: pick agents and teams, and the Workflow
  Agent designs and repairs the goal's workflow with them alone — a team whole, or one of its
  members. Pick nobody and it chooses from every enabled agent and team, as before. The picks are
  the goal's assignees, shown and changed on its Details.

### Changed

- On a Mac, focus moves between split terminal panes with ⌘⌥+arrows — as in VS Code — so that ⌥← and
  ⌥→ move a word. Windows and Linux keep Alt+arrows, and a chord you set yourself is kept.
- A goal's agent and team assignees now scope the Workflow Agent: its designs, repairs and
  amendments name only them (or a member of a named team), and a goal that names none inherits its
  nearest ancestor's. This includes goals that already name agents or teams; a goal naming nobody,
  or only people, keeps the whole enabled staff. `list_staff` and `validate_workflow` answer for the
  goal in its design session, and the intake ops take an optional `goal`.

### Fixed

- Staging, committing, amending, unstaging, discarding and stashing no longer fail when a file in
  the Changes list was deleted after the list was read — an agent's temporary file, typically. The
  file that is gone is left out and the rest go through; a discard with nothing left says *nothing
  to discard*. A failed git command's error now names its first arguments and counts the rest, so
  git's own reason stays readable.
- The screen no longer goes blurry while a shell or a harness prints: the Project IDE's centre —
  an open file, the empty landing — and any other screen under where the terminals were last
  drawn. The hidden terminal layer's scrollbar showed through it, and the glass frost came back
  with it; a hidden layer now paints nothing, frost included, and no small grey bar shows at the
  centre's edge.
- A terminal in a background tab or pane no longer shows its scrollbar over the one in front.
- Editor and terminal scrollbars fade out again instead of vanishing at once.
- The code editor's suggestions keep their row layout: long labels are cut inside the row.
- The drawing canvas's chart dialog lays its choices out at their own width.

## [0.2.0] - 2026-10-03

### Added

- The website, `website/`: a tour of the app in its own order, built from the repository's own
  facts by `scripts/website/build.mjs` and held by its tests (`just website`, `just website-check`).
- Contribution guides and templates: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`,
  `SUPPORT.md`, `AGENTS.md`; issue forms, the pull request template and `CODEOWNERS` under `.github/`;
  under `docs/contributing/`, how to contribute, the review process and its checklists, triage, the
  maintainers' settings, keeping compatibility, migrations, and a guide for every area.
- `docs/reference/compatibility.md`: what every release promises about what an earlier one left behind.
- **Suggest** in the pull request dialog: the General Agent drafts the title and the body from the
  branch's commits and its diff against the base, as the commit box drafts a message. The draft
  lands in the fields to be read and edited; a field you typed in while it was asked keeps your
  words, *Undo* puts back what was there, and closing the dialog drops the ask. Read-only: nothing
  is pushed or opened until you open the pull request. On the wire, `POST
  /workstreams/{wid}/pr/suggest`, always `200` with `{suggested, title, body, agent, error}`.
- A note or a drawing is deleted straight from its list: a trash on each row, shown on hover or
  focus, asks first in the same words as the open note's or drawing's *Delete*.
- **Catalog** in the top bar of Agents and Teams, beside *New agent* and *New team*: the catalog's
  agents or teams, ready to install (Settings › Library).


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
- Every index screen — Inbox, Agents, Teams, Workflows, Goals, Pulse, Channels, Messages — opens with
  the same band: its tabs, then search and filters, then the count and the screen's one *New…*
  button, always last. Tag filters show the six most used and *N more*; the workflow library adds
  columns from the room it has (three beside a 1024px window, four at 1440, six at 1920).
- Side panes give way before the work does: the Details pane always leaves the screen beside it at
  least 420px, the workflow designer's and a goal's Workflow tab's step palette folds to its icons
  on a narrow window, and on Agents and Teams the detail stays in view while the list scrolls.
- The footer's read-outs are icons with their counts — terminals, harnesses, ports, browser tabs —
  and the network shows *VPN* only while a tunnel is up.
- The Browser button and annotating a page for an agent are the Project IDE's alone; beside any
  other screen the embedded browser opens with `⌘⇧L` or *Browser* in the search. The note box an
  annotated page shows matches the app.
- Import a project offers *Link it in place* first, and chooses it: the folder stays where it is and
  nothing is written into it. *Copy it in* is the second choice.
- The Teams page no longer offers *New goal for this team*, nor the *Carrying* section under a team;
  a goal is made from Goals or the New goal dialog.
- Quieter and easier to read: a step not yet reached says *pending* in dim words instead of a chip;
  avatar initials read at 4.5:1 in every colour; Pulse names what changed (*a workspace note
  changed*) and no longer repeats an id under the heading that already names it; an empty state's
  button is always drawn as a button. The Notes and Draw panels stand side by side when both are open.

### Fixed

- Channels and Direct messages said *No channels yet* or *No direct messages* before the workspace
  had been read, and when reading it failed; they now show the list's outline while it loads and the
  reason with *Retry* when it fails. The sidebar no longer offers to create one in the meantime.
- On a 1024px window a goal's steps lost their names beside the Details pane, channel names were cut
  to their first letters, and the workflow canvas was squeezed to a sliver.
- Dialogs and panes were see-through on the Glass themes, and truncated text wrapped instead of
  ending in an ellipsis across the app.
- A failure is said in words — what failed and why, or that the diagnostic log has the detail —
  never as a raw error.

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

[Unreleased]: https://github.com/mourad-ghafiri/Bisa/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/mourad-ghafiri/Bisa/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.1.0
