# 06 — Terminals

Many shells, one tab each, any installed harness with resume, xterm 6 with the WebGL renderer,
mounted outside the routed screen so a build survives navigation.
They are **tabs in the workbench's centre**,
and the layer that draws them is positioned over the centre from the shell (`shell/layerSlots.ts`)
rather than docked below. Splits, scrollback that survives a restart, buffer search, and a liveness
vocabulary with room for *I do not know* are below.

---

## What does not change

- **The PTY is a desktop-shell command and never a node route.** See [01](01-trust-boundary.md).
- **The webview names a scope, an id and a harness id** — never a path, never a program. The shell
  asks the node `GET /placement` and `GET /harnesses`, now with the bearer token.
- **A harness runs inside the login shell** (`$SHELL -l -c 'exec …'`), so it sees the same `PATH`
  and environment the person's own terminal does.
- **Every terminal is reachable, always**: the centre strip lists the shells rooted in the current
  workstream, and the rail lists every shell under the workstream it stands in. A dead shell keeps
  its tab and its buffer. Nothing respawns on its own.
- **No two sessions share a mount key; no PTY outlives the list.** The two rules in
  `terminalsModel.mjs` and `TerminalPanel.tsx` stay exactly as they are.
- **A setting reaches a mounted shell.** `terminal.font_family`, `terminal.font_size`,
  `terminal.cursor_style` and `terminal.scrollback_lines` are applied to every open emulator and it
  re-fits; nothing waits for a respawn.

## The GPU, links and the keyboard

- **The WebGL renderer is leased by visibility** (`terminal/webglPoolModel.mjs`): eight slots for
  the page, taken by the panes on screen — the focused one first — and handed back by a pane that
  has been hidden for two seconds, which draws with the DOM renderer until it is seen again. A lost
  context hands the slot back too. A budget by arrival degraded the pane you were not touching.
- **A URL or a path a shell prints is a door** ([17](17-links-and-paths.md)): one link provider
  over `findLinks` finds URLs and paths (`src/main.rs:42` in a compiler's error) on the *logical*
  line a row belongs to — a link the row edge cut in two, folded by xterm or broken by a harness's
  own frame, is one door underlined across its rows (`terminalLinksModel.mjs`) —
  and ⌘-click (Ctrl-click elsewhere) hands either to the link handler with the terminal's own root —
  the card before the browser, scoped to `http` and `https` (`opener:allow-open-url`); the menu before
  a file. A plain click keeps selecting.
- **A focused shell keeps its keyboard**, except for a declared chord a PTY cannot mean
  (`keymapModel.interceptsInTerminal`): on macOS every ⌘ chord — the PTY never sees ⌘ — and
  elsewhere `Ctrl+Shift+…` and function keys; never copy, paste or find. Those bubble past xterm
  (`reserveKey`) to the app's handler, so ⌘\ splits and ⌘⇧E opens Files from inside a shell.
- **Settings reach open shells**: `terminal.*` is re-read on `settings_changed`, not once.
- **The centre's rect is not polled**: the layer re-measures on resize, on a finished CSS
  transition, and when the sidebar, the rail or the right panel say they moved
  (`notifyLayoutChanged`).

---

## Splits and panes

The layer holds a pane tree over `ui/SplitPane.tsx`: a binary tree whose leaves hold a tab set,
owned by `paneTreeModel.mjs` and tested once. Documents do not split ([03](03-files-and-editing.md));
the tree is the terminals'. Split right, split down, close
pane, move a tab to a pane. Focus follows the pointer and the keyboard (`⌥←→↑↓` between panes — the `pane_left` … `pane_down` commands of the `terminal` scope, rebindable like any chord and the app's from inside a shell — under
the default keymap).

A terminal tab **reorders and has a menu**: dragged along a split pane's strip it lands
where the bar shows (`reorderTerminal` moves the leaf's order); on the centre strip it is one
run with the documents, in the root's strip order ([03](03-files-and-editing.md) — newest last,
a shell between two files if that is where it was dropped), while the session list keeps the
order shells were started in, since a mount key is a mount key. Right-click a tab — on the centre strip
or a split pane's — for *Focus · New shell here · Restart* (exited only) *· Send the last lines to
the agent · Show in the Agent panel* (a harness shell) *· Close* (red while the shell lives, since
closing ends it) *· Close the other tabs here · Close N exited* (`tabMenuModel.terminalTabMenu`;
`closeOtherTerminals`, `closeExitedTerminals` — a live shell is never swept). The rail's terminal
rows offer the same closes.

---

## Scrollback restore

Scrollback survives a restart. The mechanism:

1. `@xterm/addon-serialize` serialises the buffer to a replayable escape-sequence stream.
2. The client checkpoints it — on a 2-second debounce after output stops, on tab close, and on clean
   shutdown — to the desktop shell over IPC, which writes `run/terminals/<key>.scrollback` in the
   workspace. Local state, never synced. Capped at 4 MiB (`SCROLLBACK_CAP`) and at the `terminal.scrollback_lines` setting.
3. On restart, a tab with a checkpoint is recreated in the same pane, the checkpoint is **replayed
   into the emulator first**, a divider line marks where the old session ended, and the PTY is
   respawned fresh below it. Restored tabs mount only once the settings have answered, so
   `terminal.restore_scrollback` is read with its real value, never a default.

The PTY itself does not survive — that is the detached-daemon design, deliberately not taken. What
survives is what a person actually goes back to look at: the output.

---

## Liveness — three values, not two

`TerminalSessionState.exit: { code } | null` is a two-value model: exited, or not. It has no way to
say *the PTY host stopped answering and I do not know*. Today that is invisible because the PTY is
in-process with the webview's own backend. The moment a terminal can outlive a window — and the
moment a checkpoint from a previous run appears on screen — the third value is mandatory.

```ts
export type Liveness =
  | { status: "live" }
  | { status: "exited"; code: number | null }
  | { status: "unverifiable"; reason: string };
```

**Loss of contact is never evidence of process death.** A restored tab whose PTY has not yet been
respawned is `unverifiable`, not `exited`; an IPC channel that stops answering is `unverifiable`; only
a positive exit from the owning host is `exited`. The tab renders the three differently — live,
exited (with the code, in the danger tone when non-zero), and *unverifiable* with the reason on
hover.

**Liveness is all a shell knows about itself.** It says nothing about what runs in it, and nothing
is ever inferred from the bytes it prints: a live shell reads *open* in the neutral tone, an exited
one *exited (N)* in the danger tone when the code is non-zero, and no shell ever wears the working
dot. The word is one function's — `shell/terminalsModel.livenessWord` (*open · exited · exited (N) ·
unverifiable*), read by the tab strip, the rail's shell row and the footer's rows; what runs is
`runningWord` and `shellWord` (the harness's label, *run*, *shell*), and a finished tab's note
`ranWords` — no surface spells a liveness of its own (`shell/terminalsModel.test.mjs`,
`shell/footerSessionsModel.test.mjs`, `scenarios/terminals.test.mjs`). What the harness in a shell is *doing* is the roster's, below. What *runs* in a shell is the
process table's — the next section.

---

## What runs in a shell

A person opens a plain shell and types `claude`. The shell process knows: every live PTY's
login-shell pid is in `TerminalRegistry`, and the machine's process table is what the port scanner
already reads to attribute sockets to those pids ([01](01-trust-boundary.md): the machine's, so the
shell's, never the node's). A **watcher** thread (`watch_processes`, `src-tauri/src/terminal.rs`)
runs while any shell is live and stops itself when none is: every two seconds it reads the table
once (`sysinfo`, with each process's executable and argv) and, for each live PTY, walks the
descendants of its shell breadth-first for the first process whose program is one the catalog
names — `harness_of`: the executable's name, `argv[0]`'s, or the script's under an interpreter
(`node`, `bun`, `deno`, `python`…, so an npm-installed `claude` running as `node …/claude` is
found); the programs come from `GET /harnesses` (`launch.program` → id), read when the watcher wakes
and again every minute. The shell itself counts, since a launched harness is `exec`'d over it. A
change — `None → Some(id)`, `Some → None`, another id — is one `TerminalEvent::Process { harness }`
on the tab's **own ordered channel**, between its bytes, so a tab never hears of a harness after the
exit that ended it; the same answer twice says nothing.

On the desktop the tab carries two facts: `harness`, the launch's own, set once at open and never
after (the mount keys the PTY on it — mutating it would respawn the shell and kill the harness just
found), and `running`, what the table shows now (`noteRunning`, generation-checked; cleared by an
exit and a restart; never persisted). **`harnessOf(session)` — the launch's harness, else the
running one — is what every surface reads**: the tab's glyph, label and title, the rail's terminal
row and its *Terminate*, the tab menus' *Terminate* against *Close (ends the shell)*, the close
guard's harness switch, the footer, the quit question. So the moment `claude` starts in a shell
the tab and its row are Claude Code everywhere, and a shell again when it exits; the rail reads
the fact (`row.harness`), never the label. A harness that spawns another shows as the one nearest
the shell.

What a hand-typed harness is not: a **roster session**. Reporting is a command-line flag the engine
composes at launch (`--settings`, `-c notify=…`, `-e`, `--port`) and a plain shell mints no session
and no secret, so nothing reports and there is no state and no sub-agents — the tab is a harness
*terminal* row, exactly what a launched harness is with `terminal.status_reporting` off. The
watcher is a machine reader in the shell like the port scanner: tens of milliseconds every two
seconds while a shell is live, nothing when none is.

---

## Reporting — a harness in a terminal is a roster session

A harness a person opens here is registered with the node before it runs (`POST
/sessions/terminal` from `desktop/src-tauri/src/terminal.rs`, with the scope, id and harness the webview
named). **A session stands where a terminal can open**: the desk holds the scope and the id to the
store's own placement rule (`file_root`, what `GET /placement` answers from) before anything is
minted or written — a scope nobody knows and an id that is no id are a 400, an id of nothing the
workspace has a 404, and neither leaves a row or a file. A harness opened in the person's home
folder (the `machine` scope) stands in nothing of the workspace's: the shell opens it as a terminal
and asks the node for no row. The engine's interactive desk (`crates/bisa-engine/src/interactive.rs`) mints a
session id and a per-session **secret** — 32 random bytes from the generator the workspace's
identities and the node's token are made with — registers the row in the same presence fold every
engine-driven session uses, asks the harness's adapter for its **reporting plan**
(`HarnessAdapter::interactive_reporting`), writes the plan's files under
`run/interactive/<session>/` — never into a project — and answers the environment and arguments
the shell applies to the command. The harness then reports through its own means, translated by
the same adapter (`HarnessAdapter::translate_report`) into the engine's `SessionEvent` vocabulary:

| Harness | How it reports | What a row reads |
|---|---|---|
| Claude Code | `--settings <run/interactive/…/claude-settings.json>` — a one-session `hooks` block (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `PermissionRequest`, `Stop`, `StopFailure`, `Notification`, `SubagentStart`, `SubagentStop`, `PostModelSwitch`) running `bisa session report --harness claude-code` | the nine states, *running <tool>*, *waiting on you — permission / question*, sub-agents with their own state (nested by `agent_id`, named by `agent_type` and described by `subagent_input.prompt`; the spawn tool `Agent` — `Task` before 2.1.63 — is never a tool of the session's; a sub-agent's own `Stop` ending its turn and never the session's; `SubagentStop` carries no verdict, so a stopped sub-agent has left); a turn a person interrupted — on which `Stop` never fires — ends on the `idle_prompt` notification or on the next prompt, and one an API error ended on `StopFailure` |
| Codex | one `-c hooks.<Event>=[{hooks=[{type="command",command=<reporter>,timeout=5}]}]` per event on the session config layer (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`, `Interrupt`, `SubagentStart`, `SubagentStop`) and the CLI's own `--dangerously-bypass-hook-trust`, since a hook on the session layer is never persisted as trusted — the flag's name is the vendor's, the hook it lets through this platform's reporter | the nine states, *running <tool>*, *waiting on you — permission* (over when the tool then runs or the turn ends), sub-agents by `agent_id`; no verdict on a tool or a sub-agent, and no word while the model writes — none invented |
| OMP · pi | `-e <run/interactive/…/bisa-reporter.ts>` — a generated extension that shells out to the reporter per event, the tool's `isError` forwarded | turns, tools with their verdict, approvals (OMP) and extension prompts (pi), the model when a turn starts on a named one; no sub-agents (pi ships none by design; OMP's `task` fan-out is not exposed to an extension) |
| GitHub Copilot CLI | `--plugin-dir=<run/interactive/…>` — the session's own folder is a plugin for that launch: `plugin.json` names `hooks.json`, whose events (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `Notification`, `Stop` — PascalCase, the names that deliver the Claude-shaped payload) each run `bisa session report --harness copilot`; the guard is a second `PreToolUse` entry when the machine guards terminals | turns, *running <tool>* with its verdict (`PostToolUseFailure`), *waiting on you* in Copilot's own words — its `permission_prompt` and `elicitation_dialog` notifications carry a sentence and no tool — over when something then runs, ends or is said; no sub-agents (its `subagentStart` hook carries no id to nest under), no model, and no `PermissionRequest`, which fires before Copilot's own rules for calls nobody is asked about |
| Grok Build | nothing — its TUI takes no hook and no plugin for one launch (`--plugin-dir` is `grok agent`'s alone) and the platform writes nothing under `~/.grok` or into a project; the reporting plan is the empty one | liveness only, as a plain terminal: no session, no roster row, no guard |
| OpenCode | `--port <n>`; the engine subscribes to the TUI's own `GET /event` stream — nothing is injected; every pulled event passes the Redactor like a reported one; every frame is raised by its `sessionID`, so the engine sorts the root from the child sessions | started (`server.connected`), turns, tools, permissions, questions (its `question` tool), cost (`step-finish` parts), the model (an assistant `message.updated`), sub-agents as child sessions (`parentID`): announced by id and title, an idle one has left, an erroring one failed |
| a plain shell, a custom or preset binary (Goose, Cursor) | nothing — a preset is detected, not driven | liveness only — and, for a shell, the harness the process table shows running under it (§What runs in a shell), as a harness terminal row |

The reporter is this binary in its reporter personality: it reads `BISA_SESSION`,
`BISA_SESSION_SECRET` and `BISA_NODE_URL` from the environment the shell set, takes the
hook's payload from stdin (or its one argument), translates it, and posts `{events}` to
`POST /sessions/{id}/report` with the secret as its bearer — never the control-plane token — inside
two seconds, exiting 0 whatever happens so a hook never slows the harness. The node's answer is
read as bytes and whole or not at all (`session::response_body`): a length it names is the length
it has, a chunk may end inside a character, and an answer cut short is *not read* — said on stderr
— never half a verdict taken for one.

Claude Code runs an event's hooks **in parallel**
([hooks reference](https://code.claude.com/docs/en/hooks), read 2026-09-30: "All matching hooks
run in parallel"), and every hook is a process of its own: the node may hear the report of a tool
before or after the guard's question about it, and a report after the exit it preceded. The fold
takes them in any order (`presence::tests::a_report_that_arrives_late_twice_or_out_of_order_never_makes_a_row_lie`).

A node that restarts forgets every interactive row — the row lives in the engine's presence, not
in a file. The tab keeps its process and its scrollback; the reporter's next post finds no session
and is dropped (best effort by design, `Reporter::send`), so the roster does not show the harness
again until a new tab opens it. The files the forgotten sessions were handed
(`run/interactive/<session>/`) are put away when the next engine starts, under its lock
(`interactive::put_away_what_was_left`): a session's files go with its tab's close, and a process
that ended with tabs open would otherwise leave them for ever. The footer's roster reads the node's list again when the stream
comes back, so what it shows is what the node knows.

**The tab is the row.** A terminal session's row (`kind: terminal` on the roster — the desk keeps its
module name, `interactive.rs`) exists exactly as long as its terminal tab — and, while the harness
waits on the person at its own prompt, the Inbox has a `session` row for it whose door is that tab
([09 §The marks](09-agents-in-the-ide.md)),
and the tab's owner — the desktop host — is the only thing that ends it. The host's PTY pump is the
one thing that reports how the process ended, through two more doors keyed by the session's secret:

| What happened | The host does | The row |
|---|---|---|
| the process ended by itself — `exit`, a crash, a signal | posts `POST /sessions/{id}/exit {code, signal}` | *done* on `0`; *failed — exited with status n*; *failed — ended by <signal>*; *failed — exit status unknown* when neither is known. The row is **held**: no retention clock — it stays beside its tab, worded like it (*exited (n)*) |
| the person closed the tab — ⌘W, ×, the strip's or the rail's **Terminate**, the window closing | marks the entry *closing*, sends SIGHUP, then SIGTERM after 250 ms and SIGKILL after 750 ms while the pid lives; once the process is gone, posts `POST /sessions/{id}/close` | gone at once (`presence.forget` → `session_gone`) — a live row is never painted *failed* on its way out |
| the tab closed after the process had already ended | posts `close` | gone |
| `POST /sessions/{id}/abort` — *Stop* in the Agents pane (offered while the session is stoppable, `isStoppable`), the CLI, a retirement stopping every session on a goal | the desktop closes the tab whatever its liveness, so `close` follows — the `aborted` rows of one tick gathered and their tabs closed as one move (`watchAborts`), so a retirement's burst is one store update and one render | *aborted* until the tab closes, then gone; a desktop that is not running leaves it to the retention clock, which takes the desk's own state with the row — the secret, the files, the reader of the harness's events — so a `close` that comes later finds nobody (404). A later `exit` changes nothing — a person's decision outranks the code the process chose on its way out |
| the spawn failed after registration; a tab opened while the app was quitting | posts `close` before answering the error | never seen |
| the login shell spawned | posts `report` with the engine's own `process_started {pid}`, off the spawn path | learns its pid — the port scanner's root, and what the node's sweep checks: a row whose reported process is gone, with nobody here to post the exit (the desktop died with it), ends as *failed — the process is gone* within `SWEEP_SECS` (30 s), its sub-agents with it |

**An end is an end.** Whatever door ends a row — the PTY's exit, an abort, a retirement, the
stream's own end — takes its sub-agents, its open tools and its pid with it (`presence::end_row`),
and nothing a hook says afterwards opens a terminal's row again — the hook that says the harness
*started* included, which is one more call that can land late. The process behind a terminal's row
is the tab's, and a tab that starts a process again registers a session of its own; what a session
cost is a fact and still counts;
the desktop does not wait to be told: a session whose claiming tab has exited reads as the tab says
(`terminalsModel.settledByTab` — *done* on a clean status, else *failed*, as of the exit, no
sub-agents), so the rail never shows a harness counting up under a tab that reads *exited*.

**The footer counts the same rows.** The window footer's *open terminals* and *running harnesses*
are `shell/footerSessionsModel.mjs` over the same `claimedSessions` / `isDrawn` rule the rail
reads: a reported harness is one row, its tab remembered; an unclaimed interactive row is nowhere;
a tab is open while it has not exited — a restored tab counts before its terminal mounts, since
loss of contact is never evidence of death. Every row says where it stands, compactly — the
project and the workstream, *Bisa › feat/a* — from one index over the workspace
(`placeIndex` / `placeWords`), the workstream's word the cards' own rule (`cardTitle`). The footer's resource overlays read the same claim: a terminal's share of CPU, memory or disk activity is its shell's whole process tree, and a harness a tab claims is the harness's share, not the tab's (`shell/resourceModel.mjs`). Every row is a door through `shell/sessionDoors.ts`,
the one way anything — the rail, the footer, a menu — opens a terminal tab (focus it, then the
route to its place with `?doc=terminal:<key>`) or a harness session (its tab, else the session
followed and the Agents pane on its workstream).

`terminal_close` returns as soon as the entry is marked *closing*; the escalation and the report run
off the UI thread, and quitting the app waits at most 1.5 s for every tab's process to be gone. A
`write` or a `resize` after the process ended is refused with *has exited; restart it* — an exited
tab is a record, not a shell.

What the two restarts mean. A **node** restart forgets every interactive row (presence is in
memory), and a running harness **cannot** register again — its secret is baked into the live
process, so its hooks 404 from then on. The desktop reseeds `GET /sessions` on the bus's
closed→open edge, the tab is drawn as a plain terminal row from then on, and its eventual `close`
404s harmlessly. A **desktop** restart carries no session ids (never persisted), and the sidecar
node dies with the app, so there is nothing to reconcile. Against an external node
(`BISA_API_BASE`) a crashed desktop leaves orphan interactive rows: they are not drawn (the
rule below), and *Stop* in the Agents pane aborts them by hand — there is no boot sweep, which could
abort another desktop's live harness.

**The drawing rule** — `claimedSessions` and `isDrawn` in `workstreamSessionsModel.mjs`, read by
the rail, the Workstreams panel, the Agents pane's rail and the activity fold: an interactive
roster row is drawn and counted **only through the tab that claims it** (`sessionId`) — as the
agent row, with its state word, its sub-agents nested under it, and a click that lands in the tab.
An unclaimed interactive row — mid-open, or an orphan — is neither drawn nor counted; a tab whose
session the roster does not know is a terminal row; a claimed tab is never also a terminal row.
A harness row's one verb is **Terminate** — on hover, in its menu, on its tab in the strip — and
it is the tab's close: the same close guard a live shell gets, then the escalation above. Whether
the guard asks first is a switch (machine scope, on by default): `terminal.confirm_close` for a
live shell, `terminal.confirm_terminate` for a running harness — the fact is
`closeGuardModel.confirmsClose`, the words `closeQuestion` — each question naming the switch that governs it and its panel as a door (`CONFIRM_DOORS`: Settings › Project IDE › Terminal for a shell or a harness, Settings › Desktop for the quit question), drawn by the dialog as a link that drops the question as it is followed — and an exited tab never asks either
way; every close surface calls `terminalCloseGuard.requestClose*`, and the one dialog
(`CloseConfirmDialog`) draws whatever question is pending, the quit question included. A plain
shell that exits badly still says *failed* on its own row; a harness's state comes from presence
alone.

`terminal.status_reporting` (machine scope, on by default) turns the whole mechanism off: every
harness then opens as a plain terminal with nothing added to its command.

---

### What holds it

| The promise | Held by |
|---|---|
| A row from the tab's opening to its close, as the host and the hooks speak: registered under the token, the recipe a file of the session's own (`0600`, no secret in it), every hook the reporter, the guard beside it on `PreToolUse`; the states hook by hook — started with its model, a turn, a tool, a wait that is an Inbox row until the tool ran, a sub-agent nested and gone, the turn's end; the guard's *ask* and *deny* printed as the harness reads them and nothing printed where it has no opinion; the secret refused by the control plane and the token by a session's door; an exit held beside the tab, hooks that land after it moving nothing; the close, the files gone with it, a hook after the close told so and ending well | the journey `crates/bisa-cli/tests/it/e2e/a_harness_in_a_terminal.rs` — the hooks are the command lines the node wrote, run through `sh` with the payload on their input |
| A node that starts again has forgotten every terminal: no row, no file, every door a 404, the hooks ending well, a tab opened afterwards a session like the first | the same journey, its second test |
| A process that went with nobody to say so ends its row as *failed — the process is gone*, its sub-agents with it, held until a close | the same journey, its third test (the sweeper as the node runs it); `interactive::a_terminal_row_whose_process_is_gone_is_swept_as_failed` |
| A session is opened only where a terminal can open; nothing is written for one that is refused | `crates/bisa-engine/tests/it/interactive.rs` · `crates/bisa-node/tests/it/sessions.rs` (`a_session_is_opened_only_where_a_terminal_can_open`) |
| An aborted row nobody closes leaves on the retention clock, the desk's state with it; what the last process left is put away at a start; a hook after the exit never opens the row again | `interactive.rs` (`an_aborted_row_nobody_closes_leaves_on_the_retention_clock`, `a_start_puts_away_the_session_files_the_last_process_left`, `a_hook_that_lands_after_the_exit_never_opens_the_row_again`) · `presence::tests::a_terminals_row_that_ended_is_opened_again_by_nothing_a_hook_says` |
| The report door takes no harm from what a hook sends: too large a 413, a shape nobody knows a 400 in the error body's shape, a late report taken and moving nothing | `sessions.rs` (`a_sessions_report_door_takes_no_harm_from_what_a_hook_sends`) |
| The four doors take the session's secret and nothing else | `crates/bisa-node/tests/it/auth.rs` · `sessions.rs` |
| The hook's request is HTTP as a server reads it, and its answer is read whole wherever it was cut | `crates/bisa-cli/src/session.rs` unit tests |
| The PTY, its escalation, the process watcher, the scrollback checkpoints, a harness in the home folder asking for no row | the shell's `desktop/src-tauri/src/terminal.rs` unit tests (`scripts/test tauri`) |
| On the desktop: a tab's liveness and what runs in it are one word each, read by the strip, the rail's shell row and the footer; a harness opened in the home folder is a terminal and never a roster row; a node that started again leaves every claimed tab a shell row; an ended row is opened again by nothing; the footer counts what the rail counts | `desktop/src/shell/terminalsModel.test.mjs` · `shell/footerSessionsModel.test.mjs` · `scenarios/terminals.test.mjs` |
| A harness's usage asked for again while a read is in flight is read once more when that read lands, never dropped | `desktop/src/shell/harnessUsageModel.test.mjs` |

**Left open.** A call the guard refused was announced by the reporter as a tool that started, and
the row reads *running* until the harness's next word or the turn's end: whether Claude Code fires a
hook after a call its `PreToolUse` hook blocked is not said on its hooks reference (read
2026-09-30), so nothing is folded on a guess.

---

## Buffer search

`@xterm/addon-search`: `⌘F` inside a terminal opens the kit's find bar (`ui/find/FindBar.tsx`, the
same bar a rendered document shows) over the buffer with match count, next/previous, regex and
match case. It is scoped to the focused pane. `⌘R` means nothing to a scrollback and stays the
shell's (`interceptsInTerminal` leaves `r` with the PTY beside `c`, `v` and `f`).

---

## The harness picker, from the palette

A fourth kind of tab runs the project's **run command** ([18](18-browser-and-servers.md)): the
Terminal caret's *Run `<command>`* item — the second item, on a checkout whose project sets one, and
absent otherwise — opens it with `run: true` on the target (`shell/useRunCommand.ts`), as does `⌘⇧R`
while no server is up; the shell asks the node what the command is for the checkout and refuses one
this machine has not approved, and the tab is *run · <command>* — a shell running the text through
the login shell, never resumed, remembered as a run across a restart.

The caret menu exists — *Shell*, the run command when the project sets one, every harness with an
interactive form, *resume* vs *fresh session*, disabled entries for harnesses the node did not find. There is a palette entry per harness
(`Terminal: Claude Code here`, `Terminal: Codex here (fresh)`), so the picker is reachable without a
pointer, and a default harness per project (`terminal.default_harness`, project scope) that the
plain **Terminal** control opens. **Every harness resumes with its own form**, the tool's own
words and never a guess (`InteractiveLaunch::resume_args`, the adapters and the preset table):
Claude Code `--continue` ("the most recent conversation in the current directory"), Codex
`resume --last` ("skip the picker and resume the most recent chat from the current working
directory"), OpenCode `--continue`, pi `--continue` ("continue most recent session"), OMP
`--continue` ("continue previous session" — its `--resume` with no id is a picker, which is what
a resumed OMP used to sit in), GitHub Copilot CLI `--continue` ("resume the most recent session in
the current working directory" — falling back, in Copilot's own words, to the globally most recent
one), Grok Build `--continue` ("continue the most recent session for the current working
directory"), Goose `session --resume`, Cursor `--continue` ("the previous session"); a custom
descriptor's is its own; an `acp:*` or `a2a:*` target has no interactive form — Copilot CLI and
Grok Build speak ACP under ids of their own, and open by their bare commands.

## A resumed session starts

A resume is one argv word, and every harness comes up with its conversation loaded and **waits at
its prompt** — the engine's own headless resume sends a first message for the same reason. So the
tab says something: once the harness has drawn its prompt, it types a short nudge and Enter —
*Continue where you left off.* — through the same path a keystroke takes
(`terminal/resumeStartModel.mjs`, tested; `Terminal.tsx` binds it). Typing, not a flag: an opening
message on the command line is a different flag on every tool and none on some, while the
harness's own input works for all of them, a custom descriptor included. "Has drawn its prompt"
is read from the PTY — output seen, then quiet for `START_QUIET_MS`; never before a beat after the
first byte, never after `START_WITHIN_MS` from opening (a harness still signing in is not nudged
into a login form), never twice, and never once the person has typed, whose words go first. A
sign-in shell is a form, not a session, and is never nudged. Whether to do it at all is
**`terminal.resume_start`** (machine scope, on by default) in Settings › Project IDE › Terminal;
off, the resumed session waits for you at its prompt.

---

## Session ids

Session keys are minted from a counter and never reused. An id that embeds a locator with
delimiters invites a second parser somewhere, and a second parser is a bug waiting; `launchKey`
uses a NUL separator for exactly that reason. The rule is stated once here: **any id that carries components uses a
separator that cannot appear in a component, and has exactly one parser.** `launchKey` already
follows it; the pane tree's ids follow it too.

---

## Environment

The variables added are `TERM`, `COLORTERM`, `BISA_TERMINAL=1` and, for a harness,
`BISA_HARNESS=<id>`; a harness that reports also gets `BISA_SESSION`,
`BISA_SESSION_SECRET` and `BISA_NODE_URL`, which its hooks inherit. The node's bearer
token is **not** exported into a terminal — a shell has no business on the control plane — and is
**removed** from the PTY's environment when the app itself was started with one (`SCRUBBED_ENV` in
`terminal.rs`, the same names the node strips from every harness child); the session secret opens
exactly one roster row's four doors — `report`, `guard`, `exit` and `close` — and nothing else, and
a harness that wants the platform speaks MCP.

macOS is the target and its webview composites on the GPU by itself. A Linux build would want
`WEBKIT_FORCE_COMPOSITING_MODE=1` set in `main.rs` before the webview starts; it is not set, since
no Linux build ships ([feature status](../../feature-status.md)).
