# Changelog

Every notable change to Bisa, newest first. The format is
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). What is not released yet sits under
*Unreleased*; `just set-version <version>` turns it into the dated section a release ships as its
notes (`docs/contributing/release.md`).

## [Unreleased]

### Added

- `Ctrl+Q` quits on Linux and Windows, through the same question-then-save flow as the menu bar
  icon's *Quit Bisa* — a keymap command, `quit`, rebindable under Settings › Keymap, taken from a
  composer and a focused shell alike; on a Mac the application menu's `⌘Q` is the way out.
- Bisa runs once. Launching the app while it is open — from a terminal, with `open -n`, from a
  second copy of the app, or from a `bisa://join/…` link on Windows and Linux — opens no second
  Bisa: the launch hands what it was asked to the running app, which brings its window back, and
  ends before it has a node. The app's Dock icon now shows as soon as the app is launched, the
  window following once its node answers.
- An agent asked to change a note can now rewrite it: `note_write` replaces the body at the hash
  `note_read` answered, and a note that moved since is refused with its current hash, so the agent
  reads again and writes once. `note_append` still adds under the agent's name. A rewrite never
  empties a note and never writes over a body holding a secret the platform redacted. In the notes
  panel an agent's rewrite over a clean editor is taken with **Restore my version** one click away.
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
- A project group's heading in the Project IDE's rail shows who is working under it: before its
  count, the mark of each harness open in one of its projects — Claude Code's, Codex's — and, on
  hover, which project and workstream each one is in and what it is doing. A finished or failed
  harness, or a plain shell, shows nothing; a folded group still says it.
- **Gemini CLI** is a harness. Google's agent CLI is found by its own version, installed and signed
  in as its page says (`npm install -g @google/gemini-cli` or Homebrew; *Login with Google* or
  `GEMINI_API_KEY`), and driven over the Agent Client Protocol with the platform's tools: the model
  is set on the session before the first word — `auto`, the CLI's own default, or one of the models
  its page names — a model your sign-in lacks is passed over for the plan's next, and every tool is
  asked before it runs, so the guard answers. It has no effort control, so none is sent, and no
  account-usage source, so the footer says so. It wears its own mark, and opens in a terminal as the
  bare `gemini`, resumed with `--resume latest` — a plain terminal, like Grok Build's. For every ACP
  agent, a permission is now answered with its *once* option whatever order the agent lists them, so
  one allow never becomes a standing grant.

### Changed

- On a Mac, focus moves between split terminal panes with ⌘⌥+arrows — as in VS Code — so that ⌥← and
  ⌥→ move a word. Windows and Linux keep Alt+arrows, and a chord you set yourself is kept.
- A goal's agent and team assignees now scope the Workflow Agent: its designs, repairs and
  amendments name only them (or a member of a named team), and a goal that names none inherits its
  nearest ancestor's. This includes goals that already name agents or teams; a goal naming nobody,
  or only people, keeps the whole enabled staff. `list_staff` and `validate_workflow` answer for the
  goal in its design session, and the intake ops take an optional `goal`.

### Fixed

- `⌘Q`, the application menu's *Quit*, the Dock's *Quit* and a logout now ask first while *Confirm
  before quitting* is on, and save what is unsaved — they used to end Bisa at once, the switch
  notwithstanding, because the runtime never held the quit. The window comes forward for the
  question, a logout waits for the answer, and *Cancel* leaves everything as it was.
- A second `bisa node` on a workspace a node already holds is refused at the door — before it
  opens the workspace — in its own words: which pid holds it and where the node answers. It used
  to open the workspace first and then say a daemon had been slow to answer.
- The desktop no longer waits twenty seconds on a node that ended at once — refused its workspace,
  or unable to load: the reason, with the node's last words, is in the footer within a second, and
  the app's own node starts as soon as the other one stops.
- An agent's drawing no longer vanishes or reports done with nothing on the canvas. The canvas
  registered itself before Excalidraw had loaded the scene, so every non-empty drawing read as
  changed and autosaved its old elements — which could overwrite shapes an agent had just drawn
  offscreen. The canvas is now live from its first change, an offscreen save never moves an open
  canvas's record, a conflicting save re-reads the scene instead of resending stale shapes, a
  reload the canvas could not perform yet is done once it loads, a frame heard before the editor
  mounted is judged on the mount, and closing a drawing no longer forgets a canvas reopened on it.
- A save the canvas finished after the engine stopped waiting is announced, and so is a peer's
  drawing arriving by sync, so an open canvas and the list learn what the store holds.
- An agent's note append lands under the text you are typing, live, with no banner; before, a dirty
  editor hid it behind a conflict whose merge repeated text you had already saved. A draft restored
  from a closed window now carries the hash it was typed against: over a note an agent wrote to
  since, it is offered as *Take theirs · Keep mine* instead of silently saving over the agent's
  block. A conflict flag that a landed save never cleared, which made every later save fail without
  a word, is cleared; a slow list read no longer rolls the editor back to an older text; switching
  notes no longer parks one note's draft under the other's key; a title that changed alone is taken.
- A harness handed no MCP server — pi, Oh My Pi, a custom harness — is told in a note's or a
  drawing's conversation that it has no tools for the document, instead of being asked to use tools
  it has not got and describing a change it never made. Every agent is told to say what it did only
  after the tool answered, and to say the refusal if it refused.
- Notes and drawings are written under one writer's lock each, so two writes at once never lose a
  block, and a drawing save that changes nothing writes nothing.
- A harness's raised hand in the Project IDE drops the moment you answer its dialog in the terminal
  — Enter, Escape, a number — rather than when the approved tool finishes or the next prompt comes.
  No hook of Claude Code's, Codex's or Copilot CLI's says how a dialog was answered, only that it
  showed; the tab that showed it now tells the node, once per wait, through the session's own
  `answered` door. A declined call closes at once (Claude Code's `PermissionDenied`).
- A dialog a sub-agent raises is the sub-agent's hand, nested under its harness, which keeps reading
  *running sub-agent*; the Inbox row says whose it is (*↳ explore · permission: Bash*) and its own
  tool running clears it. Before, the parent read *waiting* for the sub-agent's whole run.
- Waits are told apart: two dialogs are two waits, and a tool starting or finishing ends only the
  wait on that tool — never another's — so with parallel tools the hand no longer vanishes while a
  dialog is still on screen. Every tool call carries its id, so two calls of one name close by their
  own ids, a refused call never reads *running* whichever hook landed first, and a repeated progress
  update is one tool, not a stack of them.
- Codex's permission wait is filed after its `PreToolUse`, as Codex fires them; a Claude Code
  compaction mid-turn no longer makes the row *idle*; a sub-agent whose announcement was lost is a
  sub-agent, never mistaken for the session itself; a late word from a finished sub-agent makes no
  ghost row.
- Every session frame carries a `revision`, so two reports landing together in the other order never
  leave a row reading an older word, and a roster read lands beside the frames by the newer of each.
- One settled roster: a tab that exited settles its mark, pill, footer row, tray and pet count alike
  — before, a mark and its pill could disagree for a moment. The pet's and an addon's *waiting*
  count is the Inbox's alone; a harness at its prompt was counted twice.
- A session's mark dwells 300 ms on a working word, so a harness flipping between *thinking* and
  *running* on quick tool calls no longer flickers between the sparkle and the loader; the subject a
  pulse line or the pet follows no longer swaps between two working sessions on every token.
- The floating Notes and Drawings buttons wear a count of their own: every note and every drawing
  there is, shown from launch without opening a panel, kept live while the panel is closed — a note
  an agent writes into, one made from a conversation, a drawing deleted, a project gone with its
  notes — and still there when the button is hidden from the footer and shown again. The number
  was the open panel's list, one tab's and alive only while the panel was open, so it was missing
  after a launch and froze while the panel was closed.
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
- A relative path printed in a terminal or a harness session now opens with ⌘-click, and reveals:
  it is read from where the shell stands — `src/lib.rs:42` after a `cd` into a crate is that
  crate's file, `../README.md` climbs, `../../lib/a.ts` as far as it says — then from the
  checkout's root, then by its tail across the checkouts. A path into an ignored or hidden folder
  (`target/out.log`, `.github/workflows/ci.yml`), or a file made since the Files index was read, is
  confirmed with the node and opens too; a word that merely looks like a path still says *Not
  found*. A path under a goal's or a project's root can now be revealed in the file manager.
- Find works in a rendered file in the Project IDE: ⌘F opens the bar right after the file is
  opened, after switching to *Rendered*, after scrolling, and from the file's own bar — a rendered
  file now has the keyboard as soon as it is shown (never taken from a field you are typing in, a
  shell, or the Files tree), and gets it back when the bar closes. Switching between *Rendered* and
  *Split* with the bar open no longer loses the highlights, nor does a replace that keeps the
  text's length, nor another open document.
- A table of contents in a rendered Markdown file works: `[Build and upload](#10-build-and-upload)`
  scrolls to that heading. Headings now carry the anchors GitHub gives them, and a same-document
  link is followed in the rendering — the window never navigates.
- The footer's harness usage shows up when the app opens, and comes back on its own. Four things
  conspired against it: the node's `PATH` came from the login shell under a two-second budget that
  a cold start missed, leaving every harness "not installed" until a restart — the shell now gets
  four seconds, its `PATH` is remembered between launches and used when it is late, and a slow
  shell is named in the log; a version probe that timed out was held as "not installed" for half a
  minute while the installed-harness list was read once per window — the node no longer keeps such
  a listing, and the desktop reads the list again when the node comes back, on a short backoff while
  it names nothing launchable, and whenever *Refresh* is pressed; a usage read that failed waited
  minutes for the poll — it is asked again at 15 s, 30 s and 60 s, and every shown line is read
  again when the node comes back; and a stale Claude Code credential file shadowed the fresh
  sign-in in the Keychain, whose lookup could hang — the file's own expiry is honoured and the
  Keychain is asked under a five-second budget.
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
