# Changelog

Every notable change to Bisa, newest first. The format is
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). What is not released yet sits under
*Unreleased*; `just set-version <version>` turns it into the dated section a release ships as its
notes (`docs/contributing/release.md`).

## [Unreleased]

### Added

- **Muse Code** is a harness. Meta's terminal coding agent (`muse`) is found by its own version,
  installed and signed in as its page says (`curl -fsSL https://dev.meta.ai/install.sh | sh`; run
  `muse` to sign in, or `muse auth set`), and driven over the Muse Session Protocol — its host
  `muse serve`, started trusted in the workstream's checkout and handed the platform's tools: the
  session is started on its model, checked against the host's own list first so a model your plan
  lacks is passed over for the plan's next; the effort rides every turn, held to the levels the
  model's row lists; every tool the host asks about is judged by the guard and answered with its
  *once* choice, never a standing grant; a steer reaches the running turn and a follow-up waits for
  its end; token usage and the account's windows — the one the host names and the week, with the
  plan's tier — reach the footer from what a session hears or from the host asked. It wears Meta's
  mark, and opens in a terminal as the bare `muse`, resumed with `resume --last` — a plain terminal,
  like Grok Build's. A custom harness descriptor named `muse-code` is refused from now on, as every
  built-in id is.
- A goal's own record changed — its statement, title, assignees, tags or budget, by a person's
  edit or an agent's `revise_statement` — is a frame on the bus from now on, `goal.updated`
  (`EnginePayload::GoalUpdated { goal }`, scoped to the goal): the goal page open on it reads it
  again, the Goals list refreshes its card, the Pulse and the activity feed file it under the goal.
  Before, a `bisa assign` left the open page stale until something else moved. A client that does
  not know the topic ignores it.
- A goal spawned by a `spawn` step names that step in its origin
  (`GoalOrigin::Spawned { parent, step }`, `step` optional and absent for a sub-goal an agent
  captured): a restart pairs each waiting `spawn` step with its own child rather than every child
  with the first waiting step. The field is additive; a record without it reads as before.

### Changed

- The workflow panel — the designer's Properties pane and the goal's Workflow tab column — is one
  component over one model (`desktop/src/views/_workflow/inspectorModel.mjs`): the workflow's own
  view (Identity · Runs · Inputs · Problems), a step's page (*This run* while a run is shown — its
  state, holder, time, error, answer, output folded, the verbs it admits —, Identity, the kind's own
  form under the kind's name, *Flow and failure* as a kit fold, its own problems under the forms,
  *Remove step* with *Undo brings it back*), a flow's page (its two steps as doors, its word, its
  branch to rename, *Remove flow*) and a word on several picked; a ghost *Workflow* door back
  instead of a close glyph; every read-only state says why — a closed goal, a run going, running in
  *n* goals, a finished run, a proposal not adopted, a design saved for the goal, a library workflow
  — with the door that answers it (*Edit the steps*, *Open in the designer*); **the goal screen has
  one right pane**: the Workflow tab's inspector is the Details pane's *Properties* tab — Details ·
  Work · Projects · Files · Properties, every tab with a glyph it folds to from the edge in — filled
  by the tab through a slot the panel publishes, a pick on the canvas showing it and opening the
  pane if it was closed, Escape on the canvas clearing the pick first and closing the pane second,
  a picked step kept in view when the canvas's room changes (the pane opening on the pick), the
  empty Properties page offering *Draw the steps* on a manual goal, each step's page keeping its
  own scroll; the kit gained `Note`, `Labelled`'s `error` and `useCommitOnUnmount`; a group of
  controls is `Labelled`, never a `<label>`; *Id* reads *Step id*, *on fail* reads *on failure*, a
  condition group's member is *Condition n*, the JSON field says once when it applies, the human
  step's two icon buttons are named; field pairs go two columns from `@sm` everywhere.
- Bug hunt's whip cracks on the press, where you click: its tip snaps on the pointer and bites 14px
  around it and along 56px of the lash lying behind the hand (`TOOLS.whip.shape`, a `lash` of tip,
  width and reach — the segment rule and its 24px minimum stroke are gone), the lash straightens
  from the tip back along its lie and rides the grip for a beat before it curls on from where it
  lay, a streak of light runs the bite to the tip, sparks and a flash sit on the pointer, the grip
  jerks back along the lash, a hit's splat and droplets are thrown the way the lash flew, a miss
  scratches from the tip back with the chips at the tip; the squeegee is the one tool that drags,
  and the strip's tooltip says *click to crack it*.

### Fixed

- The workflow designer and the goal's Workflow tab, at their edges: a blur-committed field —
  a step's id, a branch name, a JSON schema — committed on its way out the body it had been
  rendered with, so a step deleted meanwhile, or another writer's copy just taken, was written
  over by the old body; a form's patch and a rename land on the body as it stands now, and a step
  gone by then stays gone. Undo and redo wrote back a session or drawing read at render time,
  which a save settling in between made stale (the designer could stay *saving* for good); both
  move the state as it stands when the key lands. *Save changes* on the tab could be pressed twice
  and sent twice, dropped what was drawn during the save, and offered itself over a body the node
  had not judged; it sends once, keeps what was drawn meanwhile, and waits for the judgement. A
  designer a goal runs kept its save timer and forgot its unsaved edit; it saves nothing while
  frozen and still counts the edit as unsaved. The *changed elsewhere* word was said on every
  frame that carried it; once per revision now. A drawing dropped when the goal started a run was
  dropped without a word; the tab says so. Leaving a library designer in conflict by in-app
  navigation lost the person's edits; they are parked for the window's life and come back as a
  conflict to settle on the next visit. The goal page read itself again on every tick of the
  Workflow Agent's work; only a change of the design's standing moves it now
  (`goalPageModel.wantsReload`).
- An ask card and a step's verbs could send twice — a second click landing before the row went
  decided the gate again (a 409 live, a double decision after a restart); each sends once. The
  goal card's act popover acted on the run it last showed until its new read landed; opened again
  it reads afresh. Two quick picks in the workflow picker raced on the goal's record; one pick is
  in flight at a time.
- A `notify` step that read both its place and its voice from the run's inputs was judged on the
  place alone: with a scope set, an author bound to an input naming a person or a team passed the
  bound-input check and failed at the step. Both are judged now, each on its own.
- A library workflow that is there and cannot be read — a file another shape of the code wrote —
  took the whole library with it: `GET /workflows`, the Workflows screen and every picker failed on
  the one file, where the goals list skips such a goal. The library's lists skip it now, said at
  `error` as a goal's is, and keep the rest. The same workflow was answered as *workflow not found*
  by every route that names one, sending a person after a typo; it is the store's own refusal by its
  file now, as every other record's is. A goal pointed
  at such a workflow is still a row, its strip drawn without the definition and the file said at
  `error`; a goal's journal this node cannot read leaves the Workflow Agent's standing unknown on
  the page and drops the durable asks the Inbox alone could rebuild, said at `error` — the live
  gates, the page and the Inbox still answer. `bisa search` lists a goal whose current run cannot
  be read without it (`run_unreadable: true` on the row, the reason on stderr) rather than losing
  the whole list to one file, as the node does.
- A listener a problem had kept from arming — a workflow saved with a flow to nowhere, an input
  that no longer bound — went on saying *not armed* on its record after the save that mended it:
  the registry armed it again but only a run it started or a poll that worked cleared the word, so
  a hook listener mended by hand looked broken until its next call. The build that arms a listener
  again now takes back the arming trouble alone; what a dispatch or a poll said still stands until
  that works again.
- A loop of instant steps — a `for_each` over a `notify` body, an `emit`, a `check` that could not
  render — ran each step's settlement inside the effect before it, so the engine's stack grew with
  the iterations: a loop of a thousand overflowed it and took the node down. A batch of effects
  produced while one runs waits its turn on the outermost call's queue now — one loop, never a
  stack — and every batch still runs in the order it was produced.
- After a restart every child a goal had spawned was paired with the first `spawn` step still
  waiting on the parent's run: two spawn steps waiting side by side on two children — a `parallel`
  gateway's — ended with one step completed by the wrong child's outcome and the other waiting for
  ever. A spawned child's origin names the step that spawned it now (`step`, an additive field — a
  record from before reads with none and is paired as it always was), and the restart pairs each
  child with its own. A crash between the run's snapshot that entered a `spawn` and the effect that
  makes its child left the step waiting on nobody; the restart spawns the child now. And the pairing
  is armed before the child begins, so a child that ends before its parent's step was told of it is
  not waited for after.
- A spawned child whose start was refused — its agent step with nowhere unambiguous to run among its
  parent's projects — failed the step that spawned it and was left a draft nobody would run. It is
  closed with the reason now.
- An auto goal's proposal whose agent step had nowhere unambiguous to run — the goal has several
  projects attached and the step names none — failed the Workflow Agent's tool call after the design
  was pointed at and noted, with no gate and no run; a guided goal's adoption approved on such a
  design recorded the decision, resolved the gate, then refused the start. The placement is checked
  where the adoption's inputs are now: an auto goal opens the Adopt gate saying why, and a guided
  approval is refused with the gate kept and nothing recorded.
- A run that finished while a `check` was still at its command, or a `connector` step still calling
  — an `end` on another branch, a failure that ended the run, a divert of the step — left the shell
  command running to its timeout and let the call go out for a run that was over; only a stop took
  them with it. A cancelled step's task and call are aborted now, and a run that ends takes the rest
  with it.
- An occurrence of a listening goal that arrived while a restart waited for the goal's sessions to
  end started a run on the goal the cancel had just left idle, and the restart's own run queued
  behind it — the contract says the restart's run starts at once. The goal's occurrences are held
  from the cancel to the new run's start now, and go on the next tick, behind it.
- A person's release of a waiting `spawn` let the step go but kept waiting on the child: the child's
  end was refused on a step no longer waiting, and said in the log. The release forgets the child
  now; and the docs say what the run has always taken — a person may release any `wait` and a
  waiting `spawn`, not only a wait on a release.
- The goal's snapshot was a latest-wins write with writers outside the lock the run's writers
  take: a failed run pausing the goal's listening at the moment a person closed it could write the
  pause over the closure and reopen the goal, two edits at once were refused as stale or lost one
  another, and a workflow could be archived or deleted while a run of it was starting under the
  lock, leaving a live run under an archived workflow. Every goal writer takes the one writer now
  and the write is a compare-and-swap on the revision, the archive and the delete look for live
  runs under the same lock, and a timer's due time is written on its step's row alone — it used to
  rewrite every row of the run from a read of its own, and the run's next event cleared it.
- A capture refused for a budget or a mode this node could not read, or for a document it could not
  take, left a goal behind — the goal was written before those were read. Everything a capture
  needs is read before its first write now; and a settings layer this node cannot open refuses the
  capture rather than reading as *auto*, the one mode that runs unattended, or as *no ceiling*;
  a sub-goal takes no mode from a parent nobody could read; a run the goal names that cannot be
  read counts as a failure for the auto repair limit; and the statement of a goal whose run cannot
  be read stays as it is.
- A wait's event was taken out of the ticker before it was recorded: a run snapshot unreadable for
  a moment lost the timer, or the signal heard, until a restart. The wait stays armed until its
  event lands now.
- A signal a stopped worker was holding was put back on the queue on every sweep for ever. One
  claimed five times and never settled is failed now, with the count on it.
- Stopping or restarting every run of a workflow stopped at the first run that refused, and ending
  a run of the workspace stopped at the first goal born of it that could not be closed: every run,
  and every child, is reached now, and the first refusal is the answer after.
- A question taken back by a person's own act — a proposal edited on the Workflow tab, another
  workflow chosen, the goal started by hand, a newer proposal, a step cancelled — was withdrawn in
  memory alone, so the journal still ended in the question: the Inbox rebuilt *Adopt this
  workflow?* for a design its person had since edited or chosen past (at once, or once the run
  finished, or after a restart), and a decision sent to the goal with nothing live — `bisa approve`,
  the card after a restart — adopted the stale design and started a run nobody asked for, or
  recorded a stray decision on a step's question answered long ago and resumed the Workflow Agent.
  Every withdrawal is a `Withdrawn` fact on the journal now, and the durable decide takes the newest
  open question of the goal's own — an adoption, a held amendment, the designer's — refuses when
  there is none (`bisa answer` on a done goal says *no pending gate* where it once said *answered*),
  and tells a second decision on one decided already that it was (409).
- A durable decision — a gate rebuilt from the journal after a restart — could be taken twice: two
  hands reaching for one adoption together both journaled a decision and both began the goal, and
  the second run queued behind the first. One durable decision at a time per goal or run now; the
  other hand is told the gate was decided.
- An answer sent with no live gate — `bisa step answer` offline, a step's verbs after a restart —
  went straight onto the run, and *I am not sure* marked the step done with no decision journaled
  and no re-ask. It goes through the durable mirror now: a signed decision, the clarify rounds, the
  re-ask, and past the budget the step fails with the person's words.
- The Inbox listed a goal's live questions *or* the ones rebuilt from the run and the journal, never
  both: after a restart one new live gate — a re-ask, an agent's question — hid every other waiting
  step's question on the row and on the goal page. They are merged now, one row per question.
- The clarify budget of a human step was counted per goal: two human steps waiting side by side spent
  each other's rounds, and the first *not sure* on the second failed it. The budget is the
  question's own now.
- A stop that failed half-way — its run's snapshot unreadable when the stop reached for it — left the
  goal marked as being ended for the process's life, and every later wake of the Workflow Agent was
  refused without a word. The mark goes with the failure now.
- Closing a goal twice entered its sessions in the ending ledger before the second close was
  refused, and refused it as a bad request; it is a conflict now, met before anything is entered,
  as a stop's is.
- A step's change could be announced twice on the bus: the run-event funnel read the run before the
  store's lock and diffed against that read, so a change another writer landed between was in both
  diffs. The writer hands back the steps the event was applied to now.
- A `HEAD` git could not read — a ref file holding garbage, a lock another process held, a child
  past its budget — was taken for a repository with no commits yet, so every diff built on it read
  as *everything added*, and an unstage, which restores the index from `HEAD`, restored it from the
  empty tree and staged the file's deletion with no consent and no recovery ref; initialising a
  repository over such a folder could have committed over history it did not make. The vcs crate
  now says `NoCommits` for an unborn branch alone and hands any other failure on; the node answers
  it as git's own word.
- After a commit, the rows git could not read were answered as an empty list and nothing said so,
  and the Changes tree showed a tree with an untracked file as clean until the next read; the
  answer now carries `files_error` with git's reason beside the list and the panel reloads, and
  `GET /workstreams/{wid}` and its `/diff` say `changes_error` the same way beside an empty
  summary. Both fields are additive.
- A commit message holding a NUL byte read as *git is not available on this machine* (a 503),
  because the OS refused the spawn; it is an invalid argument now (a 400).
- Staging some lines of a hunk whose tail changed the file's last newline was refused by git: the
  picked-lines patch dropped the `\ No newline at end of file` marker after an unpicked deletion,
  so the patch claimed a newline the file lacked. The marker now travels with the line before it,
  whatever that line became.
- *Mark resolved* could save a conflicted file with Windows line endings while a marker still stood
  in a hand-edited result: the marker check never matched a line ending in `\r`.
- The changed-files cache let go of the checkouts read first rather than the ones left last past
  its cap of eight, so a checkout visited early and often was the first to forget its rows.
- A forced push with a lease skipped its *never the default branch* check when git could not name
  the default — a primary left detached with no `origin/HEAD` recorded — so the one forced push there
  is could reach the branch every workstream branches from. The project record's default is refused
  now as git's is.
- A push, a leased push, a remote branch delete or a pull request approved at the Publish gate went
  out even when the workstream had been closed while the question stood; the record is read again
  once the person answers, and a closed one publishes nothing, said on its home.
- A credential helper that never answered — a keychain prompt nobody saw, a browser flow — held a
  connection check for git's whole thirty-second local budget; the fill has a budget of its own,
  five seconds.
- *Replace across files* in literal mode expanded a `$` in the replacement as a regex group — `$5`
  wrote nothing — and a *word* search's preview dropped a file the search had listed when the term
  ended in punctuation, since the preview bounded the word with `\b` where the searcher uses a
  half boundary. The replacement is now the text as typed, and the preview bounds a word as the
  search does.
- A save through the editor left the Git tab's status cached as clean for up to two seconds when
  no watch was live; the save invalidates it as the watcher's flush does.
- A named pipe in the tree hung the read that opened it — the editor's, the renderer's — for good;
  what is not a regular file is refused by name before the open.
- Two saves of one base hash arriving together could both land, the second written over the first
  with no conflict said; the compare and the swap are one step now.
- A `.gitignore` written under a live watch did not change what its frames said: the rules were read
  once when the watch began. They are read again when it changes.
- A split restored from a saved layout minted the restored pane's id again on the next split, so two
  panes answered to one id; the pane counter now starts past every saved id, and a saved tree naming
  one id twice keeps the first.
- A file renamed outside the app onto a path already open — `mv a.ts b.ts` with both open — left
  two tabs with one id and wrote the moved buffer over the other's unsaved text; the moved tab
  closes, and the open one meets the disk through the watcher.
- A keystroke while a save was out dropped the *saving* word, so a second save could start on the
  same base and the watcher's frame for the save's own write raised a conflict that the save's
  answer then cleared; the word stays until the answer lands.
- The find widget opened after a picture's source was shown, and a symbol's line from the palette,
  waited on a timer that guessed the editor's mount; both wait for the editor as a search hit's line
  does.
- A folder served from a workstream outlived the workstream: closed, its listener kept serving a tree
  that was gone and the Browser menu kept offering it. A close now stops every server of its folders
  and tells the menu.
- The Git views' connection dot went dark when the connection could not be read, so a push could go
  out past a caution nobody could see; the dot now says the connection could not be read.
- A keymap override naming no command — a typo, a command another build had — was ignored in
  silence; the Keymap panel says so now.
- The header's *Hide the project rail* and *Hide the right panel* titles glued an English *Hide* to
  a translated noun; the word is the catalog's now, as *Show* was.
- The designer's panel remounted a step's identity form on every render — two siblings under one
  key — so typing lost focus and drafts, and a step's problems reflowed above the fields it was
  typed in; a rename jumped the panel to the top; a flow clicked, a box dragged or Escape on the
  canvas never reached the panel; deleting a flow cleared the step's selection; undoing a delete
  brought the step back unselected and undoing an add left a dangling pick; every keystroke was its
  own undo; a JSON body, a number, a template, an id, a branch name, an input's name or default typed
  and left without a blur was lost; `⌘⇧D` duplicated the step while the chord showed Properties; the
  panel's resize handle dragged from the stored width, not the drawn one; an edit in the panel
  skipped the derived positions and could reflow the picture; a frozen designer dropped edits
  without a word; the goal tab drew Problems twice, a run's actions and raw output above the
  step's own heading, and *This step is yours* on an agent step whose only act is a door to its
  work item; the tab's banners pointed at the wrong place and the docs described an *Agent mode*
  the tab does not have; and the goal screen stacked two right columns — the tab's own beside the
  Details pane — which never budgeted together, so at a 1024px window with the pane open the canvas
  kept about 176px, half a card.
- Bug hunt's whip did nothing on a click: it struck only on a drag of 24px or more, along the drag
  while the picture drew the snap at the press point 90ms later — behind the hand by then; a drag
  that left the glass toward the strip cracked at the edge; and a hit's splat, its droplets and a
  struck insect's knock went toward the hand instead of the way the lash flew.

## [0.5.0] - 2026-10-19

### Added

- The code host's word moves the record: a pull request merged on the code host — on the web, by
  somebody else — is *merged* on the next read (the panel's poll, the window coming to the front,
  *Refresh*), the card, the Board and the Inbox follow, and Clean up is the act; one closed there
  without merging ends the lifecycle with *Close this workstream…*. The pull request carries
  `merged_at`, `merge_commit` and `merge_state` (GitHub's `mergeStateStatus` / `mergeable_state`,
  GitLab's `detailed_merge_status`), the fake code host takes `set_checks`, and the scripted-gh
  journey has `failing`, `nochecks-real` and `merged-elsewhere` moods.
- **To a harness** — the annotation tray's third door: the request an agent would get (the lead,
  the target, the edit contract kept or undone by hand) and every annotation as the engine itself
  frames a chip, typed into a harness of the person's own — a **new terminal** per harness the
  node can run, which types the request once its harness has drawn its prompt (start words on the
  tab, never persisted, dropped and said so if the person types first), or a **live harness
  terminal** of the workstream, typed into now and shown; a tab working takes it with a word, one
  waiting on the person is held with the reason; as a bracketed paste when the program asked, one
  line otherwise, Enter a beat later; through the PTY and never as keystrokes; the tray keeps its
  rows until Clear; never a session the engine runs (`desktop/src/views/_workbench/HarnessHandoffMenu.tsx`,
  `harnessHandoffModel.mjs`, `desktop/src/terminal/typedWordsModel.mjs`, `modes.ts`,
  `desktop/src/shell/terminalsModel.mjs`, `useTerminals.ts`, `Terminal.tsx`).

- A **Bug hunt** built-in addon: a see-through scene over the whole desktop app on which
  spiders, cockroaches, beetles, ants, flies, mosquitoes and crickets walk, dart, fly and hop
  while the app keeps working underneath — the more things are running, the more that crawl. A
  strip at the right holds the tools (whip, flyswatter, slipper, newspaper, spray, squeegee), the
  score and streak, the Hunt/Work switch, the difficulty, a mute switch and the exit; while hunting
  the tool is in your hand — it follows the pointer, swings and lands with a shock ring, the
  insect's droplets flung along the swing, glass dust on a miss and the points earned rising; the
  whip's lash trails the hand and cracks on release, the squeegee drags a wet band; a hit splats,
  a miss cracks or scratches the glass, the squeegee wipes, `Esc` hands the app the pointer back
  (`library/addons/bug-hunt/`). It rides on one addition to the addon contract: a third window
  kind, `frame: "scene"` — the whole desktop under the chrome, click-through in work mode but for
  the regions the page calls solid, the pointer's in hunt mode, never dragged, resized or cut as a
  whole from the browser layer — with three bridge methods (`scene.mode`, `scene.solid`,
  `scene.release`) and one event (`scene`) (`crates/bisa-core/src/addon.rs`,
  `desktop/src/addons/AddonScene.tsx`, `addonSceneModel.mjs`, `addons/sdk/`).
- Three **terminal games** as built-in addons, each a terminal session in the built-ins' chrome —
  the prompt types `bisa play …`, a monospace screen of character cells, a status row that speaks
  like a shell and the workspace's line (how many sessions run, how many need you, in the accent)
  so the player knows when to leave; keyboard first (arrows or `hjkl`, `p`, `r`, `m`, `?`), the
  best kept in each addon's store, the type growing with the window, sound behind a mute:
  **Tetrominal** (`library/addons/tetrominal/`) — falling blocks in a well drawn like an editor,
  the seven pieces in syntax colours, a gutter of line numbers that warns when the stack is high,
  hold and three next, a compile log for every clear, the seed on the prompt so a build replays;
  **Linefeed** (`library/addons/linefeed/`) — a worm that is the code it eats, every token its
  neck, streaks by class, a balanced bracket and a banked statement that pay; **Linerun**
  (`library/addons/linerun/`) — a λ running three lines of a file that scrolls ever faster, bugs
  and `NaN`s to hop, `TODO:`s and comment walls to dodge, tokens and near misses that pay, a bang
  that doubles, every run seeded and the best marked in the file. Each game's rules are a pure
  `logic.js` run alone by `desktop/src/scenarios/addonsLogic.test.mjs` (the same seed and keys
  replay the same game; every runner chunk is proved runnable by the game's own rules); the three
  share one terminal kit, copied word for word and held equal by `desktop/src/scenarios/addons.test.mjs`.
- A **Media player** built-in addon: drop or open an audio or video file, paste an `https://`
  link, or paste a YouTube URL — sound as a scrolling spectrogram with a level meter, video on the
  stage, YouTube through YouTube's own player beside the window, the last twenty sources one pick
  away, keyboard throughout (`library/addons/media-player/`). It rides on one addition to the addon
  contract: a `media` permission (`{media: {hosts, youtube?}}`) that widens that addon's
  `media-src` to the hosts it names and nothing else, three bridge methods (`media.embed`,
  `media.box`, `media.remove`) and one event (`media`), a token-less node page that hosts the
  official IFrame Player API (`GET /addons/{id}/player/youtube/{video}`), and a shell view that
  stands above the browser layer, loaded under the app's own base URL so YouTube sees the app
  named in the Referer. YouTube's rules are kept by construction — its own controls, nothing drawn
  over it, no playback the page starts, no key or sign-in — and the addon shows a notice with
  YouTube's Terms of Service and Google's Privacy Policy once before the first play
  (`crates/bisa-core/src/addon.rs`, `crates/bisa-node/src/addons.rs`, `desktop/src/addons/`,
  `desktop/src-tauri/src/browser.rs`, `addons/sdk/`).
- The mock harness translates a pulled report's `events` as the session events they serialise to,
  so a test's own event server can say anything the roster's fold understands
  (`crates/bisa-harness/src/mock.rs`); the scripted language server takes more moods by the file
  opened on it (`crates/bisa-lsp/tests/scripted_lsp.rs`).
- The mock harness takes two more knobs for the engine's own tests: an event stream that panics
  on a word (`panic_in_stream_on`), the shape of a reader unwinding mid-turn, and follow-ups
  answered in the open turn with no new turn marker (`answers_follow_ups`)
  (`crates/bisa-harness/src/mock.rs`).
- OpenCode 2 (`@opencode/cli`, released 2026-09-11) is supported: the adapter reads the version
  its probe answers and drives OpenCode 2 through its ACP server (`opencode acp` — its session's
  directory and MCP servers in `session/new`, the model and the effort as session config options,
  a private server of its own), while OpenCode 1 keeps the run-mode wire word for word; a terminal
  tab on OpenCode 2 opens as a plain terminal (its TUI attaches to the shared background service
  and names no port); the install hint names OpenCode 2's installer, tap and package
  (`crates/bisa-adapters/src/opencode.rs`, `crates/bisa-harness/src/install.rs`; the journey
  `opencode_two_through_the_acp_door.rs`).

### Changed

- The engine's unused `projects::placement` door — a work item's directory read for nobody, every
  caller having gone through the store's own placement — is removed (`crates/bisa-engine/src/projects.rs`).

### Fixed

- ⌘Z and ⌘⇧Z undo and redo in the Project IDE's editor on macOS: the Edit menu's predefined Undo
  and Redo claimed the keys and sent `undo:` to the webview, which answered with Monaco's hidden
  textarea's native history rather than the model's, so nothing happened. The two are now the
  shell's own items like Cut, Copy, Paste and Select All — the native verb still runs for every
  field, and the webview hands the verb to the Monaco editor whose text area holds the focus and
  replays the chord elsewhere, so the workflow designer's canvas hears it too
  (`desktop/src-tauri/src/edit_menu.rs`, `desktop/src/shell/editMenu.ts`).
- The editor's undo history survived nothing — a tab switch, Rendered → Source, the review lens, a
  format on save and a silent reload each started it empty, since every mount made its own Monaco
  model and every value pushed in went through `setValue`. The model is now the tab's, kept across
  unmounts and disposed when the tab closes, and a pushed value — a format on save, a reload, an
  agent's write, *Take theirs*, a replace from a rendering — is one undoable step with the caret and
  the scroll kept (`desktop/src/ui/monacoModels.ts`).
- Checks with problems are detected: a run that errored or failed at startup read as passed (gh's
  `state` was passed through unknown); conclusions are now gh's own `bucket`'s, GitHub's commit
  statuses join the check runs, a run in a state nobody knows is a warn rather than a pass, a GitLab
  job allowed to fail is neutral and a manual one skipped unless it blocks the pipeline, `gh pr
  checks` on a branch with no checks is *no checks* rather than *reading the code host…* forever, and
  a read that failed is said on the row with Retry.
- The merge is blocked by what the code host itself cannot merge: with the host's own word on the
  merge, a failing check nothing requires no longer blocks the in-app merge (a caution instead), a
  draft or conflicts the host names block, a GitLab merge request held by a rule no longer reads as
  *conflicts with main*, and a merge asked on a pull request already merged elsewhere answers merged
  instead of the host's *not open* refusal.
- The clean-up after a merge never skips silently: a pull that failed is reported and the delete and
  the return still run (only the return after a conflict is skipped, where *Resolve the conflicts* is
  the return); Clean up wins the act even with commits made after the merge, which are its caution;
  and with both after-merge policies off the dialog still opens on a click, nothing ticked, instead
  of an empty one.
- A node that started while wraps waited for it on a relay never heard them: the pool marked a
  wrap as seen when it arrived before anything on the node had subscribed, and every later
  catch-up passed it over. The pool now forgets a wrap nobody listened to, so the next catch-up
  hands it to the reader that came late (`crates/bisa-collab/src/relays.rs`).
- A fact published while every relay was down was recorded as sent and never sent again: the
  relay layer answers a publish with the relays that took it and the ones that did not, and the
  pool read any answer as success. A publish no relay took is now an error in words, so the host's
  outbox keeps the fact for the next flush and a relay that comes back takes it
  (`crates/bisa-collab/src/relays.rs`, `crates/bisa-net/tests/it/host_edges.rs`).

- A search-and-replace in a goal's or a run's folder never wrote: the preview read the folder
  whole while the apply resolved every path under the scope's writable root (its `scratch/`), so
  each file was reported as changed since the preview, including the ones under `scratch/`
  itself. The preview now offers only what can be written, the apply writes those where they
  are, and a file outside the writable root is failed as read-only
  (`crates/bisa-engine/src/ide/search.rs`).
- A file renamed and removed within one watcher window was reported as renamed into being: the
  OS tells both in one breath, in no order, and the later flag won. What the disk says at the
  flush now outranks the order — a path that is gone is reported gone
  (`crates/bisa-engine/src/ide/watch.rs`).
- Renaming a workstream's own branch from the IDE renamed it in git and then failed: the record
  that names the branch was written through the person's edit door, which refuses any change to
  a workstream's kind, so the route answered 400 over a branch already renamed and the rail kept
  the old name. The record now follows git's rename through the store's record writer, which
  keeps the kind's word and the state fixed and lets the branch move
  (`crates/bisa-engine/src/ide/interactive.rs`).
- A merge foreseen against a revision git cannot merge — a branch that is not there, a blob —
  read as a conflicting merge with no paths, since `merge-tree` exits 1 for both; a preview that
  wrote no tree is now git's own refusal, naming the revision (`crates/bisa-vcs/src/git.rs`).
- A live gate whose goal the index no longer listed — a record removed behind the engine's back,
  or one the index lost — was dropped from the Inbox, although the Inbox's own rule keeps such a
  gate on a row of its own so the decision it waits for is not lost: the row was built only for
  the goals the index returned. The leftover gates are now folded into the rows, keyed and
  titled by the goal's id (`crates/bisa-node/src/inbox.rs`).
- A follow-up handed to a warm chat session in the same second as its turn began lost its reply:
  the pump told a new turn from the old one by a clock in whole seconds, so the words that
  answered the second message were folded into the first turn and swallowed as already said.
  The pump now counts follow-ups, not seconds (`crates/bisa-engine/src/conversation.rs`).
- A goal closed or retired with a worker at work could say it stopped no session: the close's
  record half cancels the run first, the cancel's effects end the rows through their drivers, and
  a row ended before the close's own pass was entered in the ledger by nobody — not waited for,
  not counted, and had its harness ignored the abort, nobody's to terminate at the deadline. A
  close now enters every live session of the goal in the ledger before the record half, as a
  stop already did, and counts what it ended whichever way the two interleave
  (`crates/bisa-engine/src/ops.rs`).
- A settlement committed from the committer desk — who commits set after a refusal — while the
  run's end was closing the workstream's record made its commit and then reported a failure: the
  record, read open and closed meanwhile, refused to move, the journal said *left uncommitted* and
  nothing announced the commit that stood on the branch. A commit made is a commit: a record the
  run's end closed under it keeps its state, and the commit is journalled and announced
  (`crates/bisa-engine/src/projects.rs`).
- A memory an agent could not store because its session had no identity — or because the store
  refused — was told to it as *NOT stored — changed since you read it … merge and retry*, with
  nothing to merge: a refusal of `recall_store` that is no conflict is now the tool error it is,
  in the engine's words; a conflict still names the hash and the value that stand
  (`crates/bisa-mcp/src/client.rs`, `server.rs`).
- A remote A2A agent's session answered a question or a sign-in by whatever request id it was
  handed: an answer to a request that was not the one waiting took the waiting one with it, and
  the right answer that followed was refused as *no input request is waiting*. The request
  waiting is taken by its own id only (`crates/bisa-adapters/src/a2a.rs`).
- A harness whose installer puts its folder on the `PATH` in the interactive shell's file
  (OpenCode's `~/.opencode/bin` in `.zshrc`) read as *not installed* in the bundled app while
  it answered in every terminal: the desktop now asks an interactive login shell for its `PATH`,
  reads the answer by a marker rather than by position, asks the login shell alone when the
  interactive form says nothing usable, appends the folders installers use when they exist, and
  hands the same `PATH` to every terminal tab (`desktop/src-tauri/src/login_env.rs`,
  `desktop/src-tauri/src/terminal.rs`).
- A harness's version probe is bounded — stdin closed, five seconds, the child ended with the
  future — so a `--version` that hangs past the catalog's budget never leaves a process behind
  (`crates/bisa-adapters/src/util.rs`).
- A platform's refusal that carried a blank sentence (`"message": ""`) was explained as that
  blank, quoted, instead of the next thing the answer said (`error`, `detail`, …); the connector
  client now passes a blank over like an object or a list (`crates/bisa-connectors/src/outcome.rs`
  `an_explanation_is_the_first_candidate_that_reads_as_words`).

## [0.4.0] - 2026-10-09

### Changed

- `just set-version` moves every crate's requirement on the workspace-hack crate (`bisa-deps = {
  version = "<major.minor>", … }`, the line cargo-hakari writes) with the version's minor; a bump
  that left it behind resolved nothing (`scripts/release/releaseModel.mjs` `bumpHakariRequirement`).

### Added

- A line-coverage gate over every tree (`docs/contributing/coverage.md` § Line coverage): `just
  coverage` measures the Rust workspace with its feature-gated suites merged in, the Tauri shell and
  the desktop's models (`cargo-llvm-cov` in the tree — `just install-llvm-cov` — and Node's own
  coverage) and holds each crate and each desktop directory to `scripts/coverage/baseline.json`, a
  ratchet that only rises (`just coverage-write`); `just coverage-crate <crate>` is the loop, with
  the file list and the bare lines; `scripts/coverage/exclusions.json` names the few files that
  cannot run without this machine's OS surface, each with a reason, held by a test; CI measures in
  a `coverage` job after `rust`. The four generator binaries beside the node and the i18n ratchet
  command run under tests of their own (`crates/bisa-node/tests/it/reference.rs`).
- Every built-in connector cites the platform documentation it was checked against (`# Reference:`
  lines in its file) and carries a `revision`; the engine refreshes an installed copy at start when
  the bundle's revision is higher — its accounts, secrets and steps kept, a credential moved to the
  field a changed scheme reads — so a fix to a built-in reaches a workspace without a reinstall.
- An account's **health** in Settings › Connectors: every account row wears a chip saying what its
  last check found and when, the platform's own reason under a failing row, *Check* per row and
  *Check all* over every connector a few at a time, refreshed in every window through the bus
  (`connectors.checked`); `GET /connectors/{cid}` and `GET …/accounts` carry `health` on each row,
  `POST …/check` takes `{timeout_secs}` (1–60; 20 unsaid) and joins a check already running;
  `bisa connector accounts` shows the health as a glyph beside a running node and
  `bisa connector account check --timeout-secs`.
- A connector operation may say what a good answer looks like — `output.expect = { path, equals |
  absent, reason }` — for a platform that answers a failure with a 200: Slack's `ok`, a GraphQL
  answer's `errors`, Obsidian's `authenticated`; read before `select`, a miss is a refusal in the
  platform's words.
- A connector parameter of kind `path`: a slash-separated path on the platform whose slashes a URL
  path keeps, each segment encoded on its own — Obsidian's notes and folders.
- An OAuth2 scheme may spell a platform's dialect: `client_id_param` (TikTok's `client_key`),
  `scope_join = "comma"` and `code_challenge = "hex"`; the RFCs' words when unsaid.
- Confluence's `spaces` operation (a space key's numeric id, which v2 wants) and Obsidian's
  `list_folder` (a folder listed under its trailing slash).
- `bisa workspace check` reads every file of the workspace as the next open would, without opening
  it, without the engine lock and without the index — the owner key, `members.json`,
  `governance.json`, every settings layer, every snapshot, every journal tail, and what earlier
  opens moved under `quarantine/` — one line a finding, `--json` the list, exit 1 while anything is
  found; `bisa doctor` points at it in one line when the files have problems.
- `GET /workspace` answers `problems`: what the last open and index rebuild found wrong and worked
  around — a file quarantined or recreated, a settings layer unreadable, a record the rebuild
  skipped, an orphan run ended, a stale row repaired, two runs on one signal — each with its path,
  its sentence and where the file went. Settings › Node gains a *Workspace* section listing them,
  shown only when there is something; the footer's node overlay counts them.
- `quarantine/<stamp>/` under the workspace: where a file an open could not read is moved aside
  whole, with its relative path, never deleted.
- `bisa paths --json` names `engine_holder` — the pid and start of the process holding the
  workspace's engine, or null — without opening anything.
- `bisa node` stops when its stdin closes under `BISA_STOP_ON_STDIN_CLOSE=1`, as gracefully as on
  `SIGTERM`, and under `--json` says each phase of its boot as a line before the socket line.
- `bisa workspace reindex` prints each stage as it passes and every record it skipped.
- The desktop's node overlay and the sidebar's footer say what the node is doing while it boots —
  *Opening the workspace…*, *Rebuilding the index — 25 of 300 goals…*, *Starting the engine…* —
  and, when it will not start, why and when the next try comes, with *Restart now*, *Reveal the
  log*, *Open the data folder* and *Quit Bisa* beside the line.
- A guard rule says where it applies. `GuardRule.applies_to` — unset for everywhere, `platform` for
  the sessions the platform drives, `terminal` for a person's own harness in a terminal — is read
  against where a call comes from; `GET /security/status` carries it, Settings › Security shows it
  on the built-ins' lines and offers it on your own rules, and a rule stored before the field reads
  as everywhere. A tool rule now reads the harness's own tool name and its harness-neutral one, so
  `fetch` names Claude Code's `WebFetch` and `web_search` its `WebSearch`.
- Every verb that ends work says what it ended. A goal's stop, restart, close, archive and
  delete, a run's stop and restart, a workflow's *Stop every run* and *Restart every run*, and a
  session's Abort answer an `ended` block — the sessions told to stop, the harnesses that ignored
  it and were terminated at the deadline, the sessions that could not be ended, the spawned goals
  ended with the thing — and the desktop's toast words it after its own sentence: *Stopped. — 2
  sessions stopped and 1 harness did not answer and was terminated*. `bisa stop` says the same.
- Stop is offered on a goal whose Workflow Agent is designing it or whose thread has a turn going,
  with no run at all; the confirm says how many sessions and spawned goals end with it, and the
  close dialog says what it closes beside the run.
- The goal page's work item panel reads its item again on the frames that move it, and draws a
  cancelled item as such.
- Every session says where it comes from. A roster row (`GET /sessions`) now carries its
  `origin` — a run's step by id and name, and whether the session resumed an item a restart cut
  short; the Workflow Agent's phase, *design* or *repair*; a conversation turn's scope and the
  person on another node who woke it; a terminal; a one-shot ask's purpose — and its `cwd`, the
  folder the harness runs in. The desktop reads one model of it everywhere a session is named:
  the footer's harness rows say who, what for (*step Build of a run on Ship the cart*, *the
  Workflow Agent designing Ship the cart*, *a turn in #general — woken by Ada*, *the classifier
  reading a command for Ship the cart*), where and since when, and open where the session comes
  from — its goal, its run, its thread, its channel; the Agents screen's *Running* tab lists what
  is live first and what settled under its own heading, each with a link to its origin; the rail
  row's tooltip, the Inbox's waiting card (its goal, else its project), the pet, a notification
  and a transcript's title say the same.
- One-shot asks are sessions of the roster. The classifier reading a command, the
  Decision-Making Agent judging a point, a commit or a pull-request message being suggested each
  run as a row of kind `ask` with their purpose, harness, model and pid while they run, leave
  after the retention, and can be stopped from the row like any other session — a stopped ask
  answers *no verdict*, *no judgement* or a failed suggestion, never a guess. They are never
  written to the sessions index.
- `goals.auto.ceiling`, under Settings › Automation › Goals as *A step's ceiling in an auto goal*:
  where a step's ceiling is in a goal that runs unattended. *Runs commands* (`exec`, the default) —
  a step that may change files may also run commands and the MCP tools that act; *The step's own*
  (`step`) — every step keeps the ceiling its design gives it, and what is above it is
  `goals.auto.permissions`'s to settle, as before. A `read` step stays read-only either way, and a
  guided or manual goal keeps its step's ceiling.

### Changed

- The coverage judge leaves a Rust source's `#[cfg(test)]` items and the lines that hold nothing but
  the closing of a `?` (the early-return edge llvm-cov counts on its own line — `)?;`, and `)?)`
  inside a call — `crates/bisa-net`'s row lowered by a tenth on that widening, the one sanctioned way
  down) out of its measured lines and honours `LCOV_EXCL_LINE` / `LCOV_EXCL_START…STOP` markers, each with a reason of at least three
  words — a marker without one refuses the run, and `target/coverage/summary.json` lists every
  excused line with its reason; `scripts/coverage/baseline.json` was rewritten from that definition,
  the one sanctioned way down (`docs/contributing/coverage.md` § Line coverage).
- `i18n-ratchet`'s body is `bisa_i18n::ratchet::run` — a function over the flags, the workspace root
  and the baseline file — so its three forms run under unit tests over a folder of their own; what
  the command does and prints is unchanged.
- Eight functions nothing called are gone: `ActivitySource::workspace`, `AddonManifest::declares`,
  `ArtifactKind::looks_like_an_image`, `AskOption::detail`, `DeletableChannel::into_inner`,
  `kind::is_audience_scoped`, `Locale::id` and the log handle's `version`.
- `just verify` runs every suite once, measured, under `coverage` — in place of `test-rust` and
  `desktop-coverage`, which stay as the plain inner-loop recipes; without the tree's
  `cargo-llvm-cov` the Rust half runs plain and is said to be unjudged.
- A built-in connector installed from the catalog is no longer frozen at install: the engine brings
  it to the bundle's `revision` at start (a catalog definition was never a person's to edit, so
  nothing of theirs is overwritten). A peer on an earlier release sets a refreshed connector's
  snapshot aside, as mixed releases already do. Linear's stored `token` moves to `api_key` with its
  scheme; Obsidian's `list_vault` lists the root alone (a folder is `list_folder`); Confluence's
  `create_page` takes a `space_id` (the `spaces` operation answers it for a key) and Jira's `search`
  answers ids alone unless `fields` says what to return, as the enhanced search does.
- A goal's stop or restart stops the goals it spawned, and a close, an archive or a deletion
  closes them — recursively, each told why; a run of the workspace stopped stops the goals born of
  it. A `spawn` step's boundary divert alone still leaves the child going.
- Every harness the platform spawns runs in a process group of its own, and a stop is two-phased
  everywhere: the cancel the harness understands, then `SIGTERM` to the whole group, three seconds
  to leave, then `SIGKILL` — so the commands a tool was running, the injected MCP server and a dev
  server go with the harness; a let-go gives EOF five seconds, then the same. ACP harnesses
  (Gemini CLI, Grok Build, GitHub Copilot CLI, Goose and the others), pi and omp are ended on a
  stop, not only asked; a cancelled ACP turn ends the session for good.
- A verb that stops sessions answers only once their processes are gone — or were terminated by
  the engine at a five-second deadline, and said; a close waits too. A stopped conversation turn is
  aborted where it stands, not let finish. `Engine::stop` ends every session it drives the same
  way — a terminal's excepted, whose process is the desktop's and goes on in its tab, left alone
  by the node's stop and by the next boot — so a node that stops leaves no harness of its own behind.
- The footer counts *live harnesses* — every harness process the engine drives: the checkout's
  rows as before, and the ones no checkout holds, the Workflow Agent's design wake and the one-shot
  asks, so the footer, the node overlay and the resources overlay agree on what is running. An
  idle harness reads dim. The tray's and the pet's working count no longer counts a conversation
  turn twice — once by its row and again by its scope's hint.
- A conversation turn has the workers' wall clock (`default_wall_clock_secs`): a harness that
  never answers is aborted at the bound and the row says *wall clock exceeded*, with one note on
  the conversation. The idle time to live between turns is unchanged, and a delivered follow-up
  re-arms it.
- A step a restart cut short three times fails on the fourth — *interrupted too often* — instead
  of resuming on every boot; the step's record counts its interruptions.
- An auto goal no longer asks about ordinary commands. Every `agent` step defaults to a `write`
  ceiling, so on an auto goal every shell command — `cargo test`, `npm install` — was above it and
  went to the classifier, and any no-verdict (a slow or absent classifier, one switched off) landed
  in the Inbox as *Allow `Bash`?*. Now a `write` step on an auto goal runs commands on its own
  (`goals.auto.ceiling`). The guard's rules still come first — a `deny` refuses, an `ask` asks, a
  `classify` rule still reads the classifier — a `read` step still has the classifier read above
  it, a harmful verdict still reaches you, and a `human`, `approval` or release step still waits.
  A workspace that wants every command above a `write` ceiling read by the classifier, or asked
  (`goals.auto.permissions = ask`), sets `goals.auto.ceiling` to `step`. The Workflow Agent's
  auto-mode brief says so too.

### Fixed

- A `check` step's evidence and a `check` start event's `output` never carried what the command
  said: the grouped command runner left the child's streams on the node's own stdout and stderr,
  so `wait_with_output` collected nothing and the command's words went to the node's log instead of
  the run. The runner now pipes both streams and closes the child's stdin; a failing check's
  evidence carries the tail of its stderr as documented, and a `check` start's sample `output` is
  what the command printed (`crates/bisa-harness/src/proc.rs`
  `a_grouped_commands_streams_and_status_are_collected`, `crates/bisa-engine/tests/it/effects_edges.rs`
  `a_failing_check_keeps_what_the_command_said`).
- An `approval` step whose prompt could not be rendered — a field a released wait's payload did
  not carry — held its run forever: the engine failed the step with the reason, but the run
  machine refused a `StepFailed` on an approval, so no gate opened, no error was written and the
  run stayed *waiting* with nothing to wait for. The run machine now takes an approval's failure
  as it takes a human step's; the step fails with the reason and `on_fail` decides
  (`crates/bisa-core/src/run.rs` `an_approval_whose_prompt_cannot_be_asked_fails`,
  `crates/bisa-engine/tests/it/effects_edges.rs`
  `a_prompt_that_cannot_render_fails_the_step_instead_of_asking`).
- `error-core-run-no-start` in the English catalog said what the refusal no longer does; the
  sentence now matches the error — *the run has no way in: its start is gone, or the workflow has no
  start by hand* — and a test holds the English of every core refusal to its catalog message
  (`crates/bisa-core/src/error_text.rs`).
- Six built-in connectors pointed at APIs that are gone, moved or mis-called, checked against the
  platforms' own documentation: Jira searched through `/rest/api/3/search`, which Atlassian removed
  (now `/search/jql`); Confluence read and created pages through v1 endpoints that are gone (now
  v2); Facebook Pages and Instagram called Graph API v19.0, expired in May 2026 (now v26.0); X
  called `api.twitter.com` (now `api.x.com` and `x.com`); TikTok sent `client_id`, space-joined
  scopes and a base64url challenge where TikTok reads `client_key`, a comma and hex, and asked for
  counts its scopes did not cover; Linear sent a personal API key as `Bearer`, which Linear
  documents for OAuth tokens alone.
- Every OAuth2 connection — Google, X, TikTok — was refused at the consent page unless the person
  allow-listed the consent and token hosts by hand: those hosts are the scheme's own now, declared
  by its URLs, the deny list still read first.
- A Slack or Obsidian check passed with a bad token: Slack answers every failure as a 200 with
  `ok: false`, Obsidian's status page answers without a key — both are read through `expect` now.
- An optional parameter left empty — a calendar event's description, an Instagram caption, a Notion
  search's query, a Linear issue's description — failed the whole call as unresolved; it is left out
  of the JSON body now, as an empty query pair always was.
- Obsidian's three vault operations could not work: a note's path lost its slashes, a folder was
  asked for without the trailing slash the plugin needs, and the root listing was unresolved.
- A host's circuit stayed refused for the engine's lifetime when the probe call let through after
  the pause was dropped before it answered — a stopped run, a deadline above the call; the probe is
  abandoned now and the next call probes. The same shape in the MCP probe: a probe whose request was
  dropped mid-flight pinned the server to *dropped* until something edited it.
- A dated `Retry-After` on a 429 was read against zero and became a wait of decades; a 3xx — a
  wrong URL in a definition — was taken for a platform failure and stopped an un-keyed write.
- An account check waited for a concurrency permit and the operation's whole deadline with no budget
  of its own; it answers *unreachable* within twenty seconds now.
- The connectors guide's custom-connector examples wrote a body without its `kind`, which no
  definition accepts.
- A stop entered a session in its ledger only after the run's cancel, which ends the rows through
  their drivers; a row ended first was never waited for, and a harness that ignored its abort was
  left running until the next start found it. Every live session of the goal or run is entered
  before the cancel now, so the deadline terminates what lingers whichever way the two interleave,
  and the stop's answer counts every session it told.
- A stopped harness was sent `SIGTERM` before it had read the end of its input, so an agent that
  leaves cleanly on EOF — and records its own end — was cut off mid-word. An abort now closes the
  harness's stdin first and gives it half the grace to leave, then `SIGTERM`, then `SIGKILL`; one
  that ignores both is ended within the same grace as before.
- A crash in the desktop's own tree — a hook of the shell's, a provider, the toast rail — stranded
  the window: the card had no doors that worked without the node and had taken the close and quit
  listeners down with the tree, so the red button did nothing, ⌘Q and the Dock's Quit hung, and only
  a Force Quit ended Bisa. The ways out now stand above the boundary for the life of the page, the
  card offers *Try again*, *Reload*, *Restart the node*, *Reveal the log*, *Open the data folder*
  and *Quit Bisa* — none needing the node — moving to another screen clears it, and a close or quit
  the page does not acknowledge within five seconds stands anyway, the window's place kept and the
  node stopped.
- A Force Quit of the desktop left its node alive holding the workspace's engine lock, so every
  later launch was refused at the door — *an engine already holds this workspace* — until the
  machine was restarted or the folder deleted. The node now lives and dies with the desktop that
  spawned it (its stdin is a pipe the shell holds), a stray node the desktop itself left behind is
  stopped before a new one starts, and a node a person runs themselves is named in the footer and
  never signalled.
- The desktop killed a node that had not answered `/health` within twenty seconds and tried again
  for ever, so a rebuild of the index after an upgrade — or any boot longer than that — never
  finished, and every killed boot interrupted the running steps again until they failed as
  *interrupted too often*. The shell now waits as long as the node reports progress, stops a node
  gracefully before it kills one, and says a failed restart to the window instead of retrying in
  silence.
- One file a crash tore stopped the workspace from opening: a torn `members.json`,
  `governance.json`, settings layer or `general` channel snapshot, a constraint error while
  reconciling the index, or one unreadable record met by the index rebuild — and the refusal
  told the person to move the workspace aside and start fresh. The owner key alone refuses an
  open now; every other torn file is quarantined or skipped and named, governance is read as the
  owner's alone until written again, a settings layer costs its values until a setting is saved
  there, and `index.sqlite` is never trusted stamped and empty after a rebuild cut short.
- A journal tail a crash tore swallowed the next fact written after it; the next write now starts
  a new line.
- A crash between a run's snapshot and its goal's left an orphan run that no sweep visited, whose
  signal was dispatched again, and whose duplicate `dispatched` row failed every later index
  rebuild — a workspace that never booted again. The goal's snapshot is written before either is
  indexed, an orphan is ended when the engine next starts, two runs on one signal are both indexed, and a
  finished run whose index row still said *running* is brought back in step.
- A Claude Code or GitHub Copilot CLI session opened in the IDE's terminal was refused the
  machine's browser, a headless one and a project's test runner with a hint about `browser_*` tools
  it does not have — nothing of the platform's is injected into a terminal harness. The four guard
  rules that steer an agent to the platform's own tools now apply to the platform's sessions alone,
  and a terminal harness keeps the machine's browser and its own prompt; what protects the machine
  still reads its calls as anyone's.
- The two built-in asks on a harness's own page fetch and web search never fired for Claude Code:
  the rules read the raw tool names, `WebFetch` and `WebSearch`, and were written in the neutral
  ones. They ask now, as the docs promised, in every session the platform drives.
- A row read *aborted* beside a harness still at work, and every stop, close, restart and
  deletion answered at once whatever the harness did: the stop ended the row before the harness
  and the wait watched the rows alone. The engine now keeps every stopped session in a ledger
  until its driver has torn the session down, waits on that, and terminates what is left.
- Only the harness's own pid was ever signalled, so the commands its tools ran, the injected MCP
  server and any server it started outlived the stop; ACP, pi and omp sessions were never signalled
  at all; a `check` command ran to its timeout after its run was stopped; a cancelled work item did
  not stop the mark before dropping it.
- A session still starting when the stop landed ran on: a worker's row stood only after its
  launch, a design wake checked its reason only before launching, a turn's session was inserted
  after its row, an ask's stop began after its launch. Every driver's row now stands before the
  launch, and the launch and the first prompt are raced against the stop; an item cancelled while
  its harness started aborts the session rather than leaving the row guard to end the row.
- A stopped goal could be woken again by the stop itself — the aborted wake's task re-scheduled
  the Workflow Agent and drained the thread's queued message; a closed goal's thread could still
  wake a turn; a restart stopped nothing when the last run was not live and never aborted connector
  calls; a retirement stopped sessions before cancelling the run; a run's settle removed a copy
  workstream's tree under a live harness; `delete_goal` hid survivors from the roster.
- A terminal tab's harness survived a lost frame: the roster re-read now announces the rows it
  moved. The goal page showed a stopped item *in progress* until reopened. The retirement toast
  promised counts the node never confirmed.
- Stopping a session that was waiting on you now stops it. *Terminate* on the roster, `bisa
  sessions abort`, a goal's stop, a retirement or a cancelled step left a worker blocked at a
  permission or a question with its row reading *aborted*, its harness still running and its
  question still in the Inbox. The question is now withdrawn, the harness answered a refusal and
  aborted, and the withdrawal is never remembered as your *no*.
- Closing a goal stopped nothing: its design wake and the turns in its thread ran on. They are
  stopped first now.
- A worker's harness process was unknown to its row and its record whenever the adapter announced
  it before the engine listened — which Claude Code and Codex always do — so the resources
  overlay could not attribute it and a restart could not terminate it; the broadcaster now
  replays the announcement, and a chat turn's and a design wake's process are recorded too.
- A session whose launch was refused, whose prompt failed or whose driver panicked could leave a
  row stuck in *starting* and a live record on disk; every driver now ends its row and its record
  on the way out. A parked session kept a stale pid and stayed on the roster; it now leaves after
  the retention like an ended one. A goal's `active_items` never shrank on a normal settle.
- The *agent writing* dot stayed on after one lost `agent_replied` frame until the window was
  reloaded; it is now cleared by the roster too — a row that ended, parked or went idle, and the
  roster read whole.
- Settings › Automation › Goals drew *Above a step's ceiling in an auto goal* twice — as the
  panel's switch and again as a registry row under it; the row is gone.

## [0.3.0] - 2026-10-06

### Added

- **Update**, in the You menu before *About Bisa*: a dialog that asks GitHub for the latest
  release — only when opened, never on its own — and sets it against the version this desktop
  was built as: *This is the latest release*, *Bisa 0.3.0 is out — this desktop is 0.2.0* with
  the release page on GitHub (the disk image and its checksum) as the one primary, *What changed*
  opening the changelog at that release's tag, and the release's own notes reading inline under
  *What's new*; a build from the source says it is ahead; no release yet, GitHub unreachable or
  rate-limiting, and the node not answering each say so in a sentence, with *Check again*. The
  node reads it (`GET /updates`, facts and never a comparison, through its one outbound client
  and the `network.*` policy) and holds the answer for `cache.updates.ttl_ms` (an hour); an
  engine nobody configured asks nobody, and `BISA_RELEASES_API` points a rehearsal at a stub.
- `POST /ide/files/{scope}/{id}/delete` removes several entries as one act: every entry is checked
  before anything goes — a refused one refuses the whole batch untouched — then the list goes to
  the OS Trash as one move, or is unlinked in order; the answer lists every path that went and,
  when it halted, the first that did not. The desktop's deletes send it, one entry or many;
  `DELETE …?path=` stays the one-entry form the CLI uses.
- A Markdown file in the Project IDE renders its raw HTML as GitHub renders it: a `<details>`, a
  row of badges in `<p align="center">`, a `<br>`, a `<kbd>` draw, through DOMPurify's prose
  profile — nothing that runs, styles, frames, submits, plays, pops over or hides is kept, and no
  `data-*` attribute a README could use to pose as the app's own marks. Messages and notes keep
  their HTML as text, so `Vec<T>` in an agent's prose keeps its brackets.
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

- A whole-app audit of the desktop's screens, dialogs and overlays, with its findings fixed. Names
  and keyboard: the Notes and Drawings panels' row menus and the repository strip's *More* were
  buttons inside buttons — invalid HTML React reported on every screen while a panel was open, two
  controls where a screen reader expects one — so a menu or a popover handed a button now makes it
  the trigger itself; every popover's panel carries its trigger's name instead of reading as a
  nameless dialog; every right-click menu carries a name; a keyboard-focused flow on the workflow
  canvas is drawn as a selected one, where it showed nothing; the canvas's dotted ground and
  arrowhead definitions are hidden from assistive technology; a Settings tile's preview is hidden
  too, so a theme tile is read by its name and not by the words of the window in miniature inside
  it; the Keymap panel's group headings follow the panel's title without a level skipped; the mode
  switch's track wears the kit's ring; an artifact card's live preview is a button with a name; one
  tooltip provider at the root lets a second tip skip its delay; the motion library follows the
  OS's reduced-motion setting at the root. Fit: the Project IDE's columns count their resize
  handles, so the occupant rail no longer ends six pixels past the window, and Agent Mode's centre
  yields its width like Project Mode's, where it had pushed the row past the window whenever a menu
  or dialog opened over it; the New agent dialog's Skills and Servers boxes no longer run past the
  dialog's edge at 1024 wide, where a picker row's one-line name had held each column at its full
  width; the right panel's Git header wraps at its narrowest, where *Diff against base* ran past
  the panel's edge; a channel header's topic and roster end in an ellipsis instead of running past
  the edge; a thread's sticky day divider sits at the true top of its scrollport, where the
  thread's head padding had held it eight pixels short with a strip of the scrolled words showing
  above it, and on Glass it no longer lets those words ghost through its rule. Words and states: a
  destructive confirm the kit's `ConfirmDialog` cannot draw (retiring a goal, deleting a branch,
  deleting a library ref) fills solid danger like every other; *Retire* no longer offers a bare "…"
  while it reads, and says "projects made for it" only when there are some; the Settings rail
  reveals the current panel's row when its kept scroll place hid it; the Inbox's key legend shows
  only while there are rows; Settings › Addons' empty *Installed* list is an open empty state with
  its Import door; two held primaries say why; with the node away, an error note says *node
  unreachable* in the app's own words where it had shown the engine's "Failed to fetch", and the
  Projects rail and the IDE's landing say the projects could not be read, with Retry, instead of
  *No projects yet*. Theme: the Board card in flight wears the family's floating shadow; a terminal
  find's matches wear the app's amber wash from the theme's roles. Every section header in a panel wraps its actions at the panel's
  narrowest, where the remotes' *Fetch* and *Add remote…* were cut at the edge.
- The Notes and Drawings panels, the companion and the addon layer load behind the first paint as
  chunks of their own.
- Deleting a folder's untracked files from Git › Changes, every untracked file from the toolbar,
  a selection of several rows in the explorer, or the files an agent's turn made when the turn is
  undone moved them to the Trash one by one, and macOS played its trash sound once for each.
  They go as one move now — one sound, one *Put Back* in the Finder — and a batch the node cannot
  take whole is refused before anything goes.
- Find in a rendered Markdown file shows its matches again: every match is marked in amber, the one
  you are on darker and underlined and brought into view inside the rendering, and closing the bar
  leaves it selected. The highlights were drawn over text nodes the page had just replaced — React
  sets a rendering's HTML again on every re-render unless the HTML object is the same one — so the
  bar counted matches nobody could see. The walk is also redone when the rendering changes under
  it, so a diagram or a Word document that arrives late is searchable, a diagram's own stylesheet
  no longer counts as matches, and ⌘F reopens the bar after Escape closed it.
- A rendered Markdown file no longer shows its HTML comments — a pull request template's
  `<!-- … -->` — or its YAML front matter as text.
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

[Unreleased]: https://github.com/mourad-ghafiri/Bisa/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/mourad-ghafiri/Bisa/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/mourad-ghafiri/Bisa/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/mourad-ghafiri/Bisa/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/mourad-ghafiri/Bisa/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mourad-ghafiri/Bisa/releases/tag/v0.1.0
