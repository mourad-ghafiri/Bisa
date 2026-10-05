### Bisa — the settings registry's words: one message per key, `setting-<key>`
### with the dots as dashes; its value is the label, `.help` the sentence under
### it, and `.choice-<value>` the word for each value of a Choice. Read by the
### node (`GET /settings/registry`, the CLI, the settings-docs generator) and by
### the desktop. The git config schema's words follow the same shape under
### `git-config-<key>`, with `.hint`. Held to both registries by
### `crates/bisa-i18n/tests/it/catalog.rs`.

## The settings registry (crates/bisa-core/src/settings.rs), in its order.

setting-appearance-theme = Theme
    .help = Five families, a light and a dark side each, or the machine's own — Glass, the default, on whichever side the machine is.
    .choice-system = System
    .choice-glass = Glass
    .choice-glass-dark = Glass, dark
    .choice-harbor = Harbor
    .choice-harbor-dark = Harbor, dark
    .choice-orchard = Orchard
    .choice-orchard-dark = Orchard, dark
    .choice-dune = Dune
    .choice-dune-dark = Dune, dark
    .choice-suede = Suede
    .choice-suede-dark = Suede, dark

setting-appearance-accent = Accent
    .help = The accent means your attention.
    .choice-amber = Amber
    .choice-violet = Violet
    .choice-teal = Teal
    .choice-rose = Rose

setting-appearance-density = Density
    .help = Rows sized for reading, or for holding more.
    .choice-comfortable = Comfortable
    .choice-compact = Compact

setting-appearance-type_scale = Text size
    .help = A multiplier over the base unit: 1 is a 14px body.

setting-appearance-font_ui = Interface font
    .help = The face the app reads in; `system` is the OS's own.
    .choice-system = System
    .choice-inter = Inter
    .choice-atkinson = Atkinson Hyperlegible

setting-appearance-font_mono = Code font
    .help = The face the editor, the terminal and every code span use.
    .choice-system = System
    .choice-jetbrains = JetBrains Mono
    .choice-plex = IBM Plex Mono

setting-appearance-language = Language
    .help = The language the app speaks. System follows this machine's languages to the first the platform has; English is the one shipped today. Every word — the desktop's, the node's refusals, the CLI's — follows.
    .choice-system = System
    .choice-en = English

setting-ide-default_mode = Open the Project IDE in
    .help = Which centre a workstream opens with when this machine has not chosen one for it: the documents and terminals (Project), the conversation with the agents (Agent), or every workstream's card (Board — offered while the Board is on). The switch in the IDE's header remembers per workstream.
    .choice-project = Project
    .choice-agent = Agent
    .choice-board = Board

setting-editor-theme = Editor theme
    .help = `follow_app`, or a Monaco theme id.

setting-editor-font_family = Font family
    .help = Empty means the theme's monospace stack.

setting-editor-font_size = Font size

setting-editor-line_height = Line height

setting-editor-tab_size = Tab size
    .help = Belongs to the code, not the person.

setting-editor-insert_spaces = Insert spaces

setting-editor-word_wrap = Word wrap
    .choice-off = Off
    .choice-on = On
    .choice-bounded = Bounded

setting-editor-autosave-mode = Autosave
    .choice-off = Off
    .choice-after_delay = After a delay
    .choice-on_focus_change = When focus changes

setting-editor-autosave-delay_ms = Autosave delay
    .help = Below 300 ms the timer fires between two words.

setting-editor-format_on_save = Format on save
    .help = Runs the language server's formatter when one is up.

setting-editor-minimap = Minimap

setting-editor-large_file-editable_mib = Editable up to (MiB)
    .help = Larger files open read-only.

setting-editor-large_file-refuse_mib = Refuse above (MiB)

setting-editor-delete-trash = Delete to trash
    .help = Move a deleted file to the OS trash instead of unlinking it.

setting-terminal-shell = Shell
    .help = `login`, or a path.

setting-terminal-font_family = Font family

setting-terminal-font_size = Font size

setting-terminal-scrollback_lines = Scrollback lines

setting-terminal-default_harness = Default harness
    .help = What the plain Terminal control opens.

setting-terminal-restore_scrollback = Restore scrollback

setting-harness-usage-reads = Usage limits from harness accounts
    .help = The window footer and Settings › Capabilities › Harnesses show what each harness's account has left — Claude Code's five-hour and weekly windows, Codex's, every provider OMP is signed into — read from the harness's own source with its own sign-in, on this machine, when a line asks. Nothing is stored and no credential is ever shown. Off, every line says reads are off.

setting-terminal-status_reporting = Harness status in terminals
    .help = A harness opened in a terminal reports what it is doing to the roster — through its own hooks, an extension, or its event server — so the rail shows its real state and sub-agents. Off, a harness opens as a plain terminal and nothing is added to its command.

setting-terminal-resume_start = Start a resumed harness
    .help = When a harness continues its latest session in a terminal, the tab types a short nudge once the harness has drawn its prompt, so it picks the work up at once. Off, the resumed session waits for you at its prompt.

setting-terminal-confirm_close = Confirm before closing a live shell
    .help = Closing a terminal whose shell is still running asks first — from the tab's ×, its menu, the rail, or ⌘W. Off, a live shell closes at once and its process ends. An exited tab never asks either way.

setting-terminal-confirm_terminate = Confirm before terminating a running harness
    .help = Terminating a harness — closing its tab, or the rail's Terminate — asks first while it runs. Off, it ends at once.

setting-terminal-cursor_style = Cursor
    .choice-block = Block
    .choice-bar = Bar
    .choice-underline = Underline

setting-browser-enabled = Embedded browser
    .help = The browser built into the desktop app — a tab in the Project IDE, and the Browser pane beside any screen. Off, no tab opens, every launcher says so, and agents are told the embedded browser is turned off.

setting-browser-home = Home page
    .help = The URL a new tab opens on. Blank opens an empty tab with the address field ready.

setting-browser-remember_tabs = Remember tabs
    .help = The tabs you can see, and where each is, are kept across a restart; a tab an agent keeps out of sight never is. Off, the tabs are forgotten when the app closes and none is kept.

setting-browser-agents = Agents may use the browser
    .help = Which agents may drive the embedded browser through the browser tools. Everyone: any agent — the default, since the guard refuses the machine's browser and every catalog agent carries the Embedded Browser skill as its guidance. Assigned: only agents that carry that skill — the platform's own agents always may. Nobody: the tools refuse.
    .choice-everyone = Everyone
    .choice-assigned = Assigned
    .choice-nobody = Nobody

setting-browser-agents-reach = Where agents may navigate
    .help = Anywhere: any http or https page. Local only: pages served on this machine — localhost, 127.0.0.1 — such as the project's dev server or a folder served from the IDE.
    .choice-anywhere = Anywhere
    .choice-local_only = Local only

setting-browser-agents-headless = Agents' tabs out of sight
    .help = When a tab an agent opens is kept out of sight — headless: it renders and answers the tools, and nothing opens beside you; the footer counts it, and one click shows it. Unattended: in a goal in auto mode and its runs, where nobody is watching; elsewhere the tab is shown beside you. Always: every agent's tab. Never: every tab is shown. An agent may ask either way for one tab.
    .choice-unattended = Unattended
    .choice-always = Always
    .choice-never = Never

setting-browser-agents-scripts = Agents may run scripts in a page
    .help = Whether an agent may evaluate a script in a tab through browser_eval — the way a person opens the console — to read a page's state while testing it. Allow: any agent the browser tools answer. Refuse: the tool names this setting and the agent reads the page with browser_read, browser_snapshot and browser_console instead.
    .choice-allow = Allow
    .choice-refuse = Refuse

setting-browser-screenshot-width = Screenshot width
    .help = The width in pixels of a screenshot of a tab — the one an agent asks for with browser_screenshot, and the camera's copy. The page is scaled to it; the height follows the tab.

setting-draw-enabled = Draw
    .help = The canvas built into the desktop app — the Draw panel, its floating dock and the footer switch. Off, nothing opens, and agents are told the canvas is turned off.

setting-draw-agents = Agents may draw
    .help = Which agents may draw into a drawing through the drawing tools. Everyone: any agent — the default, since every catalog agent carries the Drawing skill as its guidance. Assigned: only agents that carry that skill — the platform's own agents always may. Nobody: the tools refuse.
    .choice-everyone = Everyone
    .choice-assigned = Assigned
    .choice-nobody = Nobody

setting-draw-snapshot-width = Snapshot width
    .help = The width in pixels of the picture an agent asks for with drawing_snapshot to look at what it drew. The drawing is scaled to it; the height follows the drawing.

setting-events-enabled = Events are heard on this machine
    .help = The listeners' runtime here — the ticker that looks for due schedules, checks, polls and project changes, the worker that starts runs from queued occurrences, and the hook doors. Off, nothing starts and nothing is claimed; every workflow that is On, every listening goal and their queue stay exactly as they are until this is on again. A run's own waits and boundary events go on either way.

setting-events-tick_secs = Tick
    .help = How often, in seconds, the runtime looks for a schedule, a check, a poll or a project change that has come due. A shorter tick notices sooner and asks the machine more often.

setting-events-check_timeout_secs = Check timeout
    .help = How long, in seconds, a check start's command may run. A command that hangs is the alert: past this it is terminated and counts as failing.

setting-events-files-max_depth = Files scan depth
    .help = How many folder levels a project's files change walks when the project is a plain folder. A git project reads its own status instead; a scan that stops at the bound says so.

setting-events-files-max_entries = Files scan entries
    .help = How many entries a plain folder's files scan examines before it stops. The scan reports what it saw — a partial answer on time beats a complete one that never returns.

setting-events-public_hooks = Public hooks
    .help = Whether a start event marked public may be called from outside this machine, with its secret. Off, every public hook answers as if it did not exist, and a local call under the node's token still works. The node itself binds loopback unless it is started to allow remote callers.

setting-events-pr_poll_secs = Pull request poll
    .help = How often, in seconds, a pull request's state is asked of its code host while a project start or a wait wants its changes. The code host rate-limits; a review is rarely faster than a few minutes.

setting-events-chain_depth = Chain depth
    .help = How many listeners one causal line of events may pass through — a run finished, a signal raised, a review started. Enough for the compositions people build, and short enough that a mistake is visible within one tick rather than after a night of runs.

setting-events-fires_per_minute = Start rate
    .help = How many runs one start event may begin per minute — every start but a schedule's, a person's and a hook's, which pace themselves. The backstop for causality the chain cannot see: a runaway is, by definition, fast.

setting-events-backlog_per_listener = Backlog per start
    .help = How many occurrences may wait behind the runs a start already began. Past this an occurrence is refused, and the workflow's or the goal's row says so.

setting-mobile_development-enabled = Mobile Development
    .help = AI-assisted mobile development on this machine: Flutter apps run on the iOS Simulator, an Android emulator or a phone beside the code in the Project IDE, the screen mirrored and annotated for an agent, and the mobile tools answering agents. Off by default: on, Settings checks what is installed and says how to install what is not.

setting-mobile_development-platforms = Platforms
    .help = Which platforms this machine develops for. Both: iOS and Android. iOS: the simulators and iPhones alone — Xcode, which runs on macOS only. Android: the emulators and Android phones alone. A device of a platform that is off is not listed, not booted and not captured.
    .choice-both = Both
    .choice-ios = iOS
    .choice-android = Android

setting-mobile_development-agents = Agents may use the devices
    .help = Which agents may drive the simulators, emulators and phones through the mobile tools. Everyone: any agent — the default, since the app runs on a device the person can see. Assigned: only agents that carry the Flutter Development skill — the platform's own agents always may. Nobody: the tools refuse.
    .choice-everyone = Everyone
    .choice-assigned = Assigned
    .choice-nobody = Nobody

setting-mobile_development-flutter-path = Flutter
    .help = Where Flutter is on this machine — the `flutter` program, or the folder it was unpacked in — when it is not on the path and not in one of the folders the installer uses. Empty, the machine is searched.

setting-mobile_development-android-sdk = Android SDK
    .help = The Android SDK folder on this machine, when neither ANDROID_HOME nor ANDROID_SDK_ROOT names it and it is not in Android Studio's folder. Empty, those are read.

setting-git-default_branch = Default branch
    .help = Empty means detected.

setting-git-merge_strategy = Merge strategy
    .help = How a pull request merges when the code host offers the choice; the merge dialog starts on it.
    .choice-merge = Merge
    .choice-squash = Squash
    .choice-rebase = Rebase

setting-git-pull = Pull
    .help = How Pull moves the branch after the fetch. Fast-forward only asks when it cannot; rebase replays local commits on top; merge makes a merge commit.
    .choice-ff_only = Fast-forward only
    .choice-rebase = Rebase
    .choice-merge = Merge

setting-git-delete_branch_after_merge = Delete the branch on the code host after a merge
    .help = The merge dialog's toggle starts here. The local branch is the cleanup step's.

setting-git-committer = Who commits in a new repository
    .help = inherit: the repository commits as your global git config and asks only when none resolves. pin: the global pair is written into the repository's own config at creation, so it keeps its author. ask: every new repository asks who commits, whatever resolves.
    .choice-inherit = Inherit
    .choice-pin = Pin
    .choice-ask = Ask

setting-git-fetch_interval_secs = Fetch interval
    .help = 0 turns it off. While an agent reviews or fixes for you, the lifecycle reads every 15 s regardless.

setting-workstreams-cleanup = Cleanup after a merge
    .help = What becomes of a workstream's checkout and branch once its pull request merges: keep them, ask, or remove them (a recovery ref is written first).
    .choice-keep = Keep
    .choice-ask = Ask
    .choice-remove_when_merged = Remove when merged

setting-workstreams-after_merge = After a merge
    .help = Whether to go back to the default branch and pull once a workstream's pull request merges: ask, do it, or stay where you are. Never while a session runs on the primary.
    .choice-ask = Ask
    .choice-return_and_pull = Return and pull
    .choice-stay = Stay

setting-workstreams-dirty_close = Closing a dirty workstream
    .help = What happens when a workstream is closed with its checkout and the checkout holds work nobody committed. `confirm_with_recovery`: a recovery ref is written first, then the tree goes — Safety can bring it back. `refuse`: the tree stays and the close says what it holds; closing the record alone is never refused.
    .choice-refuse = Refuse
    .choice-confirm_with_recovery = Confirm, with recovery

setting-workstreams-script-pre_create = Pre-create script
    .help = Runs in the project root before a workstream is created; a non-zero exit refuses the workstream.

setting-workstreams-script-post_create = Post-create script
    .help = Runs in the new checkout once the workstream exists; a failure is reported, the workstream stays.

setting-workstreams-script-clean = Clean script
    .help = Runs in the checkout before it is deleted; a non-zero exit keeps the checkout.

setting-workstreams-script-run = Run command
    .help = Serves or runs the checkout — `npm run dev`, `python -m http.server` — long-running, opened in a terminal from the IDE's Terminal menu; approved on this machine like the other scripts. Empty is none: the Terminal menu offers no Run item, and the Browser menu's *From folder…* serves the root or a folder of it instead.

setting-workstreams-script-timeout_secs = Script timeout
    .help = How long a workstream script may run before it is terminated and counted as failed.

setting-workstreams-script-trusted = Scripts approved on this machine
    .help = The digests of the workstream scripts this machine will run. Written by Approve under About › Settings › Workstream scripts, never by hand: a project's scripts sync, and a script nobody here approved never runs here.

setting-workstreams-board-enabled = The Board
    .help = Every workstream as a card in five columns — Backlog, Todo, Doing, Done, Archived — reached from the Project IDE's header. Off hides the Board and its doors; nothing about the workstreams changes.

setting-workstreams-board-due_soon_days = Due soon within
    .help = How many days before its due date a card starts reading as due soon.

setting-workstreams-board-wip_limit = Doing limit
    .help = How many cards the Doing column holds before its header warns. Zero is no limit. A warning, never a refusal.

setting-workstreams-board-show_archived = Show Archived
    .help = Whether the Board opens with the Archived column showing. The Board's own switch changes it for this window.

setting-security-redactor-enabled = Redact secrets before agents see them
    .help = Keys, tokens and values the rules recognise are replaced by placeholders in everything the platform hands an agent or a harness, and restored only where a command is about to run on this machine.

setting-security-redactor-rules = Your redaction rules
    .help = Rules of your own — a regular expression, or the name of an environment variable whose value must never reach an agent. Workspace rules and this machine's rules both apply, in that order.

setting-security-redactor-builtins_off = Built-in redaction rules switched off
    .help = The ids of the shipped redaction rules that should not apply. A built-in can be switched off; it cannot be deleted.

setting-security-redactor-env_auto = Recognise this node's own environment
    .help = Every environment variable of the node process whose name says token, secret, password, key or credential is a detector: its value is redacted wherever it appears, like a rule you wrote by name. The status carries the count and the names, never a value; a name can be switched off in the built-ins list.

setting-security-guard-enabled = Guard tool calls
    .help = Every command, path and tool an agent asks its harness to run is judged against the rules before it runs: allowed, refused with the reason, sent to you in the Inbox, or read by the classifier first.

setting-security-guard-rules = Your guard rules
    .help = Rules of your own, tried in order after the built-ins: a command pattern, a path glob or a tool name, and what to do when it matches. Workspace rules come before this machine's.

setting-security-guard-builtins_off = Built-in guard rules switched off
    .help = The ids of the shipped guard rules that should not apply.

setting-security-guard-terminal_hooks = Guard harnesses opened in a terminal
    .help = A Claude Code or GitHub Copilot CLI session opened in the IDE's terminal asks the node before each tool runs. When the node does not answer in time, the harness's own prompt stands — you are at the keyboard.

setting-security-classifier-enabled = Ask a model about unsure commands
    .help = A rule with the *classify* action puts the redacted command to an agent for one line — safe, or harmful and why. No verdict, or a harmful one, goes to you; the classifier never allows what the rules did not.

setting-security-classifier-agent = Classifier agent
    .help = The agent whose harness and models read the command. Any enabled agent; its session has no tools and no door to the platform.

setting-security-classifier-deadline_secs = Classifier deadline (seconds)
    .help = How long a verdict may take, launch included. Past it the call is treated as no verdict and the command goes to you.

setting-security-classifier-on_harmful = When the classifier says harmful
    .help = `ask` puts the call in your Inbox with the classifier's reason; `deny` refuses it outright and the agent hears why.
    .choice-ask = Ask
    .choice-deny = Deny

setting-security-classifier-provider = Who reads the command
    .help = `agent`: the classifier agent below, on its own harness and models. `harness`: one harness and one of its models, named below, with no agent in between. `decision_making_agent`: the Decision-Making Agent (Settings › Decision Settings › Decision Making), asked whether the call is harmful and in which way. Whichever reads it, no verdict or a harmful one goes to you; nothing here ever allows what the rules did not.
    .choice-agent = Agent
    .choice-harness = Harness
    .choice-decision_making_agent = Decision-Making Agent

setting-security-classifier-harness = Classifier harness
    .help = The harness that reads the command when the provider is `harness`. Its session has no tools and no door to the platform.

setting-security-classifier-model = Classifier model
    .help = The model of that harness that reads the command. Empty is the harness's own default.

setting-security-classifier-effort = Classifier effort
    .help = How hard that model works on a verdict, when the provider is `harness`. A level the model does not take becomes the nearest one below it, and a harness with no effort control is sent none.
    .choice-minimal = Minimal
    .choice-low = Low
    .choice-medium = Medium
    .choice-high = High
    .choice-xhigh = Extra high
    .choice-max = Max

setting-security-content-screen = Read web content before an agent may
    .help = A page an agent opens with the platform's browser, and a review or comment it reads from a code host, is put to the classifier first — one line: safe, or harmful and why. Harmful, or no verdict, holds the content: you see the source and the reason in the conversation (or your Inbox, in a goal), the agent reads a sentence, and Allow lets it through. Off, content is framed as data and read at once.

setting-security-content-on_harmful = When content reads as harmful
    .help = `ask` shows you the source, the reason and an excerpt where the agent works — Allow once, Allow this site for this conversation, or Deny; `deny` withholds it outright and the agent hears why.
    .choice-ask = Ask
    .choice-deny = Deny

setting-security-net-deny_hosts = Hosts a connector may never reach
    .help = A list of hosts — `host[:port]` or `*.suffix` — the platform's own outbound calls refuse whatever a connector declares. Deny wins over every allow. Workspace and machine lists both apply.

setting-security-net-allow_hosts = Hosts a connector may reach beyond its own
    .help = A list of hosts — `host[:port]` or `*.suffix` — allowed on top of the ones each connector declares. A connector's declared hosts are allowed by default; anything else is refused unless it is here.

setting-security-collaboration-classify = Read outside messages before an agent may
    .help = A message from a person on another node is put to the classifier — one line: safe, or harmful and why — before it may wake an agent. Harmful, or no verdict, holds the message: you see it with the reason, no agent does, and Release lets it through. Off, a message from outside wakes agents like your own.

setting-security-collaboration-agent_tools = Tools on behalf of an outsider
    .help = `ask`: an agent woken by a person on another node has every tool beyond reading put to you first, whatever the guard rules allow, and a classify verdict reads as ask. `as_owner`: the same rules as for your own messages.
    .choice-ask = Ask
    .choice-as_owner = As the owner

setting-security-mcp-observed = Installed MCP servers on an observed harness
    .help = Whether an MCP server installed on an agent is mounted on a harness the guard cannot judge — Codex, pi, OMP, OpenCode, a custom harness. `refuse`: the server is left off that harness and the session is told; the platform's own server is always mounted, and a judged harness (Claude Code, GitHub Copilot CLI, Grok Build, Gemini CLI, ACP) always gets every server, its tools judged like commands. `allow`: every server is mounted everywhere, and an observed harness runs its tools under its own approval prompt.
    .choice-refuse = Refuse
    .choice-allow = Allow

setting-decisions-enabled = Let the Decision-Making Agent decide
    .help = Off, every decision point runs its own rule: a work item goes to a member of its pool by lot, an unaddressed message wakes the default agent, an auto goal adopts its workflow alone. On, the Decision-Making Agent is asked first and the rule runs when it is not sure or does not answer. An agent or a workflow can switch it on for itself alone, and an `auto_route` model plan, an effort of `auto`, a `judge` step and a classifier set to `decision_making_agent` ask it whatever this says.

setting-decisions-points_off = Decision points left to their rule
    .help = The decision points the switch above does not reach, by id: `assign.pick`, `dispatch.triage`, `goal.adopt`, `browser.headless`, `agent.decide`. A point somebody selected by name — `model.route`, `model.effort`, `security.tool`, `security.message`, `security.content`, `workflow.judge` — is not switched off here; unselect it where it was selected.

setting-decisions-provider = Who decides
    .help = `harness`: a harness on this node with one of its models. `agent`: an agent, on its own harness and models. `jev`: TypeSafe AI's Jev, a model trained for calibrated decisions, over its API. `rlcd`: any other model that speaks the same wire at an endpoint you name. Every provider is held to one answer shape; a generative model's probabilities are its own estimate, a calibrated model's mean how often it is right.
    .choice-harness = Harness
    .choice-agent = Agent
    .choice-jev = Jev
    .choice-rlcd = Another calibrated model (RLCD)

setting-decisions-harness-id = Decision harness
    .help = The harness asked when the provider is `harness`. Its session has no tools and no door to the platform.

setting-decisions-harness-model = Decision model
    .help = The model of that harness. Empty is the harness's own default.

setting-decisions-harness-effort = Decision effort
    .help = How hard that model works on a judgement, when the provider is `harness`. A level the model does not take becomes the nearest one below it, and a harness with no effort control is sent none.
    .choice-minimal = Minimal
    .choice-low = Low
    .choice-medium = Medium
    .choice-high = High
    .choice-xhigh = Extra high
    .choice-max = Max

setting-decisions-agent-id = Decision agent
    .help = The agent asked when the provider is `agent`. Any enabled agent; it answers with no tools.

setting-decisions-jev-model = Jev model
    .help = The Jev model asked when the provider is `jev`. The API key is kept in this machine's keystore, never here.

setting-decisions-rlcd-endpoint = Endpoint
    .help = The base URL asked when the provider is `rlcd` — the platform posts to `/v1/systemone` under it. Its host passes the outbound host lists like any other.

setting-decisions-rlcd-model = Endpoint model
    .help = The model named in each request to that endpoint.

setting-decisions-rlcd-auth = Endpoint sign-in
    .help = `bearer` sends the key kept in this machine's keystore as a bearer token; `none` sends nothing, for a model served on this machine.
    .choice-none = None
    .choice-bearer = Bearer token

setting-decisions-deadline_secs = Decision deadline (seconds)
    .help = How long one judgement may take, retries included. Past it the decision point runs its own rule; a security point asks you.

setting-decisions-retries = Decision retries
    .help = How many times a judgement is asked again after a transient failure — a rate limit, an overloaded or unreachable provider, an answer that does not hold to the shape. A refused key or a refused request is never retried.

setting-decisions-confidence-act = Confidence to act on
    .help = How sure a judgement must be for a decision point to take it. Below it the point runs its own rule, and the judgement is recorded as unsure.

setting-decisions-confidence-security = Confidence to call a command safe
    .help = How sure the Decision-Making Agent must be that a call is not harmful for the classifier to read it as safe. Anything less is no verdict, and the call goes to you.

setting-connectors-concurrency = Calls in flight per connector
    .help = How many calls one connector may have in flight at once on this machine — a workflow fanning out over many items, a poll and an agent's read together. The next call waits for a permit; nothing is refused. A platform's own rate limit is still honoured call by call.

setting-connectors-oauth-port = OAuth callback port
    .help = The loopback port the browser is sent back to while a connector account is being connected. Bound only for the minutes a connection is pending; register `http://127.0.0.1:<port>/connectors/oauth/callback` at the platform.

setting-diagrams-theme = Diagram theme
    .choice-follow_app = Follow the app
    .choice-default = Default
    .choice-dark = Dark
    .choice-forest = Forest
    .choice-neutral = Neutral

setting-diagrams-export-scale = Export scale

setting-artifacts-html-libraries = Libraries in a page
    .help = Let an HTML artifact load scripts, styles and fonts from three public CDNs (cdnjs, jsDelivr, unpkg). Off, a page runs with only what it carries. Nothing else on the network is ever reachable from a page.

setting-agents-default = Default agent
    .help = The agent an unaddressed message reaches — in a project's threads when set there.

setting-agents-effort = Effort
    .help = How hard a model works on a task when nobody said otherwise — a workflow step's own effort comes first, then the model's, then the agent's. `auto`: the Decision-Making Agent reads the task and names the level; when it is off, unsure or does not answer, the level is `high`. A level the model does not take becomes the nearest one below it, and a harness with no effort control is sent none.
    .choice-auto = Auto
    .choice-minimal = Minimal
    .choice-low = Low
    .choice-medium = Medium
    .choice-high = High
    .choice-xhigh = Extra high
    .choice-max = Max

setting-agents-conversation-mode = New IDE conversations start in
    .help = How far an agent goes on its own in a conversation about a project or a workstream; the composer changes it per conversation at any time. `manual`: edits land and wait for your Keep or Undo, and a command no guard rule decided is asked in the conversation. `auto`: edits land and commands run under the guard and the classifier; the changes stay reviewable and are kept when you send your next message. `plan`: the agent reads and replies with a plan — an edit is refused, a command is asked — and *Build this plan* hands it over. The guard's rules, the Redactor and the classifier hold in every mode.
    .choice-manual = Manual
    .choice-auto = Auto
    .choice-plan = Plan

setting-agents-review-checkpoints = Turns kept restorable
    .help = How many turns of a conversation keep what they changed, so *Restore to before this message* can go back. A turn with a change still waiting for your word is never dropped.

setting-agents-context-selection_max_lines = Selection chip max lines

setting-agents-context-terminal_lines = Terminal chip lines

setting-goals-default_mode = New goals start in
    .help = The mode a goal is captured in when the capture does not say. `auto`: the Workflow Agent designs the workflow and the platform adopts it, starts the run, repairs and restarts it by itself; a permission above a step's ceiling is read by the classifier (`goals.auto.permissions`), and only a step a person must take, a guard rule that asks or refuses, a call the classifier finds harmful, a gated publish or a question the agent still asks wait for you. `guided`: the agent proposes and you adopt, ask for changes or decline. `manual`: you design on the goal's Workflow tab, with the agent on request.
    .choice-auto = Auto
    .choice-guided = Guided
    .choice-manual = Manual

setting-goals-auto-repair_limit = Auto repairs before asking
    .help = How many failed runs an auto goal repairs and restarts on its own. Past the limit the Workflow Agent's corrected proposal waits for your decision, so a goal that keeps failing cannot loop unattended.

setting-goals-auto-permissions = Above a step's ceiling in an auto goal
    .help = What happens in an auto goal when an agent wants a tool above its step's ceiling and no guard rule decides it. `classify`: the classifier reads the redacted call — safe runs, harmful asks you or is refused as `security.classifier.on_harmful` says, no verdict asks you. `ask`: the call is put to you, as in a guided goal. The guard's rules and the redactor come first either way; a guided or manual goal always asks.
    .choice-classify = Classify
    .choice-ask = Ask

setting-budget-default-max_usd_cents = Default budget — cost
    .help = The cost ceiling, in US cents, a goal or a run in the workspace made without a budget of its own is given — one an event starts every day included, unless the workflow was turned on with a budget of its own. Zero means no ceiling. When spend reaches it the run stops and a notice reaches you; a goal is not closed. A goal or a run made with its own budget keeps it.

setting-budget-default-max_tokens = Default budget — tokens
    .help = The token ceiling a goal or a run in the workspace made without a budget of its own is given. Zero means no ceiling.

setting-budget-default-max_wall_clock_secs = Default budget — wall clock
    .help = The wall-clock ceiling, in seconds, a goal or a run in the workspace made without a budget of its own is given. Zero means no ceiling.

setting-keymap-preset = Keymap preset
    .choice-default = Default
    .choice-vscode = VS Code
    .choice-vim = Vim

setting-keymap-overrides = Key overrides
    .help = Command id to chord.

setting-lsp-enabled = Language servers

setting-lsp-servers = Server descriptors
    .help = Language, command, args.

setting-lsp-idle_ttl_secs = Idle TTL
    .help = A server stops this long after its last document closes.

setting-workflow-designer-snap = Snap to grid
    .help = Dragged steps land on the grid.

setting-workflow-designer-grid = Grid size
    .help = In pixels.

setting-workflow-designer-minimap = Minimap
    .help = A small map of the whole canvas in its corner.

setting-workflow-autosave-delay_ms = Autosave delay
    .help = How long after the last edit a workflow is saved as a new revision.

setting-workflow-runs-keep = Runs kept per workflow
    .help = How many finished runs in the workspace each workflow keeps. When a run of it ends, the oldest finished runs beyond this number are put away with their folders. Runs that are going are never counted, and a goal's runs are the goal's.

setting-cache-enabled = Caching
    .help = The master switch. Off makes every cache miss — the platform recomputes everything.

setting-cache-harness_listing-ttl_ms = Harness listing cache
    .help = How long the probed harness catalog is reused before re-probing `--version`.

setting-cache-harness_models-ttl_ms = Harness models cache
    .help = How long a harness's model list is reused before asking the adapter again.

setting-cache-git_status-ttl_ms = Git status cache
    .help = How long a workstream's git status is reused before running git again.

setting-cache-path_index-ttl_ms = Path-index cache
    .help = How long a root's file-path index is reused before walking the tree again.

setting-cache-path_index-max_roots = Path-index roots kept
    .help = How many roots' path indexes the desktop keeps before evicting the oldest.

setting-cache-codehost-ttl_ms = CodeHost cache
    .help = How long a pull request and its checks are reused before asking the code host again.

setting-cache-codehost-max_entries = CodeHost entries kept
    .help = How many code host results are held before the oldest is evicted.

setting-cache-presence-ttl_ms = Presence snapshot cache
    .help = How long the sorted session roster snapshot is reused before re-sorting.

setting-cache-model_health-ttl_ms = Model-health snapshot cache
    .help = How long the model-health snapshot is reused before rebuilding it.

setting-cache-harness_usage-ttl_ms = Harness usage cache
    .help = How long a harness account's usage report is reused before its source is asked again — a provider's usage endpoint is not a thing to hit on every render.

setting-cache-desktop-ports_poll_ms = Ports poll
    .help = How often the desktop rescans listening ports while a window is visible.

setting-cache-desktop-stats_poll_ms = Stats poll
    .help = How often the desktop footer refreshes CPU, GPU, memory and every process's share of them while a window is visible.

setting-cache-desktop-disk_poll_ms = Disk poll
    .help = How often the desktop footer walks the data directory for its sizes by area while a window is visible.

setting-cache-desktop-network_poll_ms = Network poll
    .help = How often the desktop footer reads this Mac's network — the VPN, the route, the resolvers, System Settings' proxy — and what the platform's calls leave through, while a window is visible.

setting-cache-desktop-sessions_safety_ms = Sessions safety refetch
    .help = How often the desktop re-seeds the session roster in case a stream frame was lost.

setting-logging-enabled = Write a log
    .help = The master switch. Off, nothing is written and no file is opened; the last files stay where they are.

setting-logging-level = Level
    .help = The lowest level that reaches the file. Errors only by default; `info` adds the platform's own timeline — every fact the Pulse records — and `debug` every request the node answers.
    .choice-error = Error
    .choice-warn = Warn
    .choice-info = Info
    .choice-debug = Debug
    .choice-trace = Trace

setting-logging-rotation = Rotation
    .help = How often a new file starts. The file's name carries the period, in UTC.
    .choice-hourly = Hourly
    .choice-daily = Daily

setting-logging-keep_files = Files kept
    .help = How many files of each process — the node's, the desktop's — are kept; the oldest past this count is removed when a new one starts.

setting-rail-order = Rail order
    .help = Your hand-ordered project rail — projects, groups and workstreams. Set by dragging.

setting-rail-groups = Rail groups
    .help = Per-group adornment for the project rail — a photo, keyed by group name.

setting-network-proxy-mode = Proxy
    .help = `environment`: the proxy the node's own environment names (`HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`), if any — what a terminal-started node inherits; the desktop-started node inherits none. `none`: no proxy, whatever the environment says — and nothing the platform runs is handed one. `manual`: the two URLs and the bypass list below, for the platform's own calls and everything it runs.
    .choice-environment = The environment's
    .choice-none = None
    .choice-manual = Manual

setting-network-proxy-http = HTTP proxy
    .help = A URL: `http://proxy.example:3128`, with a login as `http://user:password@…` when the proxy asks for one. Empty means none for `http://` targets.

setting-network-proxy-https = HTTPS proxy
    .help = The proxy for `https://` targets, as a URL — usually the same one. Empty means none.

setting-network-proxy-no_proxy = Bypass
    .help = Hosts reached directly, comma-separated: a name (its subdomains too), a `.suffix`, an address or a CIDR — never a port. This machine's own names — `localhost`, `127.0.0.1`, `::1` — are always bypassed.

setting-network-http1_only = Speak HTTP/1.1 only
    .help = Every request the platform makes uses HTTP/1.1, never HTTP/2 — for a proxy or an inspection tool that does not speak it. Git the platform runs follows (`http.version`); a harness's own calls are its own.

setting-network-public_ip_url = Public IP service
    .help = The service asked for this machine's public address — one `https://` URL that answers the address as plain text, such as `https://api.ipify.org`. The desktop app reads it for the footer's network word and Settings › Capabilities › Network; the node never does. Empty turns the read off, and the internet is then judged by a TCP connect alone.

setting-sync-enabled = Sync over relays
    .help = Whether this node talks to relays at all. Off by default: nothing is contacted until you turn it on. Off, no invite can be claimed, no hosted member hears anything, and nothing you joined moves — the workspace works alone.

setting-sync-relays = Relays
    .help = The Nostr relays this node publishes to and reads from, as `wss://` (or `ws://` on this machine) URLs. Four public relays are the defaults — relay.nostr.com, relay.nostr.net, relay.damus.io, nos.lol — and any may be replaced; they see ciphertext addressed to pubkeys and nothing else. An entry that is not a `ws://` or `wss://` URL is refused when written, naming it. A change is picked up at once — no restart.

setting-sync-interval_secs = Catch-up every (seconds)
    .help = How often the node asks the relays for what it missed while a live subscription was down.

setting-sync-publish_relay_list = Publish the relay list
    .help = Publish a NIP-65 relay list under this node's pubkey — the hint an invite carries, and what lets a person who lost the link find this node again. Plaintext: the relay URLs and nothing else.

setting-sync-iroh-enabled = Direct connections
    .help = Open a direct QUIC endpoint for attachment bytes between this node and the people it hosts or is hosted by. Local-only by default: no outside infrastructure is contacted unless the next switch says so.

setting-sync-iroh-n0_relays = Use n0's public relays for direct connections
    .help = Let the direct endpoint use n0's public relay servers to reach a node behind a NAT. Off, only addresses you or a peer announced are tried. On contacts infrastructure operated by n0.

setting-sync-iroh-peers = Direct peers by hand
    .help = Peers reached without any relay, for a LAN: a list of `{"{"}member_pubkey, node_id, addrs{"}"}`. Empty means peers are learned from their announcements.

setting-collab-name = Workspace name
    .help = What people you invite see this workspace called — in their sidebar, on the join screen. Empty shows your pubkey's first letters.

setting-collab-join = When a valid invite is claimed
    .help = `admit`: the person is a member at once, at the role and on the channels the invite named. `ask`: the claim lands in your Inbox and the person waits until you admit or refuse them.
    .choice-admit = Admit
    .choice-ask = Ask

setting-collab-default_role = Invite as
    .help = The role a new invite starts on. Guest: only the channels you put them on, and direct messages. Member: every standing channel, direct messages, the agents.
    .choice-guest = Guest
    .choice-member = Member

setting-collab-invite_ttl_hours = An invite lasts (hours)
    .help = How long a code may be claimed after it is made. A code claimed once is spent whatever this says.

setting-desktop-confirm_quit = Confirm before quitting
    .help = Quitting — ⌘Q, the menu bar icon's Quit Bisa, or closing the window while it does not keep Bisa running — asks first, naming what is running and what is unsaved; unsaved documents are saved after you confirm. Off, the app quits at once — still saving what is unsaved.

setting-desktop-close_keeps_running = Closing the window keeps Bisa running
    .help = The window hides and Bisa stays in the menu bar, its icon showing what the platform is doing and how many things need you; agents, terminals and the node keep going. Bring the window back from the icon or the Dock; quit from the icon's menu or with ⌘Q. Off, closing the window quits.

setting-desktop-dock_icon = Show in the Dock
    .help = macOS: Bisa's icon in the Dock, wearing the number of things that need you. Off, the menu bar icon is the app's only door — the window comes back from there — and Bisa leaves ⌘Tab and the application menu. On other platforms the switch has no effect yet.

setting-notifications-enabled = System notifications
    .help = Whether Bisa sends notifications through this machine's notification centre at all. Off, nothing reaches it — not an ask, not a failure, not an addon's notice, not the app's own word that a document stayed open. The menu bar icon, the Dock badge and the Inbox still say what needs you.

setting-notifications-asks = Asks — waiting on you
    .help = Something waits on your word: an agent's session held at a permission, an approval gate or a sign-in; a step's gate opened; a question an agent asked; a project with nobody set to commit after an agent's work.

setting-notifications-failures = Failures
    .help = Something failed with nobody looking: an agent's session, a listener that could not start its run, a workstream script that exited badly. Each is said once.

setting-notifications-done = Finished work
    .help = An agent's session finished. Off by default — finished work is not an interruption; turn it on to hear every completion.

setting-notifications-workflows = Workflows
    .help = The Workflow Agent proposed a workflow for you to adopt, or could not finish designing one and needs you.

setting-notifications-addons = Addons
    .help = Notices addons send — a pomodoro's turn, a widget's reminder — each addon only with the notify permission you granted it, at most one every ten seconds.

setting-addons-enabled = Addons
    .help = Every addon window on this machine — the clock, the load read-outs, a game, one you imported. Off, no addon runs or draws; what each addon is and what it was granted is kept. A fact about this screen, never synced.

setting-system-full_disk_access = Full Disk Access
    .help = Let the desktop app read everywhere macOS otherwise fences off — Library, Documents on a managed Mac, other users' folders. Off, nothing of ours relies on it. A fact about this Mac, never synced.

setting-system-microphone = Microphone
    .help = For voice mode, which is coming. Nothing listens while this is off, and nothing asks macOS until you turn it on.

## The git config schema (crates/bisa-vcs/src/config_schema.rs).

git-config-user-name = Name
    .hint = Who authors commits.

git-config-user-email = Email
    .hint = The author's email.

git-config-user-useConfigOnly = Use config only
    .hint = Git refuses to guess an identity from the machine when the name or email is unset.

git-config-user-signingkey = Signing key
    .hint = The GPG or SSH key that signs commits.

git-config-commit-gpgsign = Sign commits
    .hint = Sign every commit with the signing key.

git-config-pull-rebase = Rebase on pull
    .hint = Rebase instead of merging when pulling.

git-config-core-autocrlf = Line endings
    .hint = How git converts line endings on checkout and commit.
    .choice-true = True
    .choice-false = False
    .choice-input = Input

git-config-init-defaultBranch = Default branch
    .hint = The branch a new repository starts on.

git-config-codehost-account = Code host account
    .hint = The login whose credential opens and reviews pull requests here — a pin on this repository, or a profile's.

git-config-codehost-kind = Code host kind
    .hint = Which code host a remote on a host of your own is — GitHub Enterprise or a self-hosted GitLab. A public host needs no kind.
    .choice-github = GitHub
    .choice-gitlab = GitLab

git-config-codehost-github-account = Default GitHub account
    .hint = The GitHub login a repository with no pin and no profile speaks as.

git-config-codehost-gitlab-account = Default GitLab account
    .hint = The GitLab login a repository with no pin and no profile speaks as.

git-config-codehost-bitbucket-account = Default Bitbucket account
    .hint = The Bitbucket login a repository with no pin and no profile speaks as.
