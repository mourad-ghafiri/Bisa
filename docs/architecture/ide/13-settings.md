# 13 — Settings

Preferences live in the workspace, split by scope, with a resolution order a person can see: one
registry in `bisa-core`, three files on disk, one generated reference
([`reference/settings-keys.md`](../../reference/settings-keys.md)), and a CI check that fails when
the page is stale. What stays
outside the registry — the notes overlay's dock and maximized state, the Draw panel's dock, size and maximized state, the pet's position, which pet and how big, a
collapsed section — is per-viewer furniture in `localStorage`. The **Pet** panel (`PetPanel.tsx`)
shows every pet as a tile — the nine the platform ships first, a person's own after, the one showing
ringed — with a *Show the pet* switch, the size slider and *Reset position*; the same three are the
footer's Pet overlay (`shell/PetStat.tsx`), so the pet is settled from where the pet is. The **Addons** panel (`AddonsPanel.tsx`, under Library) is hand-written the same way over one registry key: the machine's switch, then what is installed and what each was granted, the catalog's offer, and *Import an addon* ([18](../18-addons.md)); the machine's switch is read from the addons store, not the settings, so the panel and the footer never disagree. The screen opens from the person's profile menu at the top
right of the chrome (*Identity* on the keys panel, *Settings* on the last panel), from `Mod+Comma`,
and from the Omnibox's *Go to Settings*; it is not a sidebar destination.

---

## A registry, not a struct

Every setting is declared once, with the scopes that may set it:

```rust
pub struct SettingDef<T> {
    pub key: SettingKey,            // "editor.tab_size"
    pub default: T,
    pub scopes: ScopeSet,           // which of Machine | Workspace | Project may hold a value
    pub label: &'static str,
    pub help: &'static str,
}
```

The registry is data in `bisa-core`, so the panel, the resolution and the documentation are
three readers of one list and cannot disagree. Adding a setting is one entry; the panel grows a
control, the reference page grows a row, and the write path learns the allowed scopes — with no
further code.

---

## Three scopes

| Scope | File | Syncs | Because |
|---|---|---|---|
| **Machine** | `~/.bisa/machine.json` | never | fonts, shells, keymaps and language servers are facts about this machine — and so is the list of workstream scripts a person here approved (`workstreams.script.trusted`), because the scripts' texts sync and the trust to run them must not; and so are the platform's two switches over the grants macOS gives the desktop app (`system.full_disk_access`, `system.microphone`, off by default), because whether this Mac lets the app read everywhere is a fact about this Mac; and so are the three confirmations — closing a live shell (`terminal.confirm_close`), terminating a running harness (`terminal.confirm_terminate`), quitting the app (`desktop.confirm_quit`), all on by default — because whether the person here wants to be asked is theirs; and so is whether a resumed harness is nudged into motion (`terminal.resume_start`, on by default), a habit at this machine; and so are the two menu bar switches — whether closing the window keeps the app running in the menu bar (`desktop.close_keeps_running`) and whether it shows in the Dock (`desktop.dock_icon`), both on by default — because how the app sits on this machine's screen is this machine's; and so is the language the app speaks (`appearance.language`, `system` by default), because it is this machine's person who reads; and how the platform reaches the internet — the six `network.*` keys: the proxy mode, the two URLs, the bypass, *speak HTTP/1.1 only* and the echo service asked for this machine's public address — because a proxy, and the address the world sees, are facts about this machine's network |
| **Workspace** | `~/.bisa/settings.json` | yes | how this workspace works with git, workstreams, agents and diagrams is shared with collaborators |
| **Project** | `projects/<slug>/settings.json` | yes | tab size and formatting belong to the code, not the person |

`machine.json` lives in the workspace directory because the directory is already per machine; the
file is simply excluded from sync, as `run/` and `identity/` are. A person's git identity is not a
registry key at all: it is their **global git config**, which Settings › Git & code hosts › Identity
reads from git and writes at their request through the engine's three named functions; a
repository's own values are its local git config, edited in its Git tab. What a *new* repository
does about it is a key — `git.committer`, workspace scope, `inherit` · `pin` · `ask`
(`bisa_core::CommitterPolicy`, `git_settings.rs`) — because that is a decision a workspace
makes once, not a fact about a person; the engine reads it at every creation
([04](04-git.md#how-a-repository-gets-one)). A **profile by
organization** is a gitconfig file the platform owns and the global config includes
([04](04-git.md#profiles-by-organization)); a code host **account** is a token in the workspace's
`identity/codehost/<kind>/`, or a sign-in the machine's `gh` or `glab` already holds, and a
`codehost.account` or `codehost.<kind>.account` key in git config; an SSH key is a file under
`~/.ssh`. None of them is a registry key, and the registry never held a second copy of any.

**Window furniture stays in `localStorage`** — dock positions, pane sizes, collapsed sections, the
last selected tab — and so does **where the person was**: the place the app closed on, each
section's last place, and what every screen kept of itself (its filters, the rows opened, its
scroll, a document's view, where a thread was being read), in the four `bisa.view.*` memories
([crates/desktop §Routing](../crates/desktop.md#routing)). It is always on and has no registry key;
its one control is an action, *Forget where I was*, under Settings › Capabilities › Desktop. The window's size and
place are the shell's, in a file of its own beside the app's config. The line: a *preference* is
something you would want on your other machine or
would expect a collaborator to share; *furniture* is where things happen to sit in this window.
Appearance's six dials are preferences at `Machine` scope; their `localStorage` keys stay only as a
first-paint copy that the node's value overwrites at boot and on every `settings_changed`.

---

## Resolution

```
Project → Workspace → Machine → compiled default
```

walking **only the scopes the key allows**, first present wins. A value written to a scope the key
does not allow is **refused at write time** with the allowed scopes named — not stored and ignored,
not deprioritised. `font_size` at project scope is a `400`, because the `ScopeSet` of `editor.font_size` does
not contain `Project`. The refusal is the core's own sentence, in the words a person types and in
the reader's language (`error-core-settings-scope-not-allowed`: *allowed: machine*) — never the
developer's `Display`. Held through the binary by the journey
`crates/bisa-cli/tests/it/e2e/settings_security_and_the_log.rs` (a key at each scope, nearest wins,
a scope refused by name, an unknown key refused, unset falling back layer by layer, every write
heard on the bus as `settings_changed { scope, keys }`).

```
GET  /settings/{scope}[?project=]         → the raw values held at that scope
GET  /settings/resolved?scope=&id=        → every key, its value, and the scope it came from
PUT  /settings/{scope}[?project=] {key: value, …}   → 400 on a disallowed scope, naming the allowed ones
```

Settings changes are emitted on the `engine` stream as `SettingsChanged { scope, keys }` so an open
editor re-reads its tab size without a reload. **A setting is written through one door**
(`bisa-engine` `settings.rs`: `set`, `unset`): the store takes the value, every part of the engine
that holds the key re-reads it, and the bus says which keys moved. A write made past it — straight
into the store — would be a value nobody hears of, so whatever changes a setting comes there: a
person's edit, a project's scripts approved (`workstreams.script.trusted`), the relays an
invitation's code names on a join (`sync.relays`).

---

## Visible in the UI

Every control in a settings panel carries an **origin badge** — `default`, `machine`, `workspace` —
showing which scope the value on screen came from, and a *set at* choice over the scopes the key
allows there. The badge cannot drift from the resolution because both read the same registry and
the same resolved response. It is one component (`OriginBadge`) over one model
(`settingOriginModel.mjs`), for the registry's rows and the hand-written panels alike: a scope is
said in the word a person types (`bisa settings set machine …`) and the node names in a refusal;
Settings writes the workspace and this machine, narrowest first (`writableScopes`, `defaultTarget`);
*Reset* shows where the row's scope holds the value (`mayReset`); and a write to a scope a nearer
one stands over says so — *The workspace scope holds a value, and it wins over the machine one*
(`shadowWords`) — so a write that lands never looks like a click that did nothing
(`settingOriginModel.test.mjs`).

**Where a panel is has one spelling.** The rail — its groups, their panels, the label and the line
of each — is one model, `settingsLink.mjs` (`SETTINGS_GROUPS`); `Settings.tsx` draws from it, and
`settingsPath(tab)` says *Settings › Capabilities › Browser* from it: the group, then the panel,
because two panels may share a label under different groups (*You › Identity*, *Git & code hosts ›
Identity*). A link's label is that path, and every `Settings › …` a desktop catalog sentence spells
is held to a group and a panel of the rail (`scenarios/settingsPaths.test.mjs`).

**The rail shows where you are.** It keeps its scroll place like every region
(`data-scroll-keep="nav"`), and arriving at a panel by its route — a link, the omnibox, a sentence's
*Settings › …* — reveals the current row when the kept place hides it (`Settings.tsx` scrolls the
`aria-current` row into view, the kept place yielding first through `yieldKeptScroll`).

**The Settings screen holds no per-project setting.** It resolves for the workspace and this
machine and never for a project — there is no project picker and no *override for this project*.
A key whose `ScopeSet` admits `Project` is edited for one project **in the Project IDE**, where the
project is in front of you, through one component — `ProjectSettingsCard` (the keys and the view's
draft: a control edits the draft, *Inherit* drafts an unset and the row says *inherits the
workspace's on save*, *Undo* forgets a drafted row; the toolbar's Save writes them as one
`PUT /settings/project?project=` and one `DELETE` per inherited key) — placed under **About ›
Settings**, the one view a project is set up in (`ProjectSettingsView`,
[02](02-component-model.md)): the **Git** card (`git.default_branch`, `git.merge_strategy`, `git.pull`,
`git.delete_branch_after_merge`), the **Workstreams** card (`workstreams.cleanup`,
`after_merge`), the **Browser & devices** card (`browser.home`, `browser.agents` and its three
keys, `mobile_development.agents`), the **Agents & decisions** card (`agents.conversation.mode` — the mode a
conversation about one of the project's checkouts starts in — `agents.effort` — how hard a model
works here when no step, model or agent says ([06 § Effort](../06-agents-and-teams.md#effort)) —
and `decisions.enabled`,
`decisions.points_off`, `decisions.confidence.act`, which the engine resolves for the project a
decision stands in) and the **Editor & terminal** card (`editor.tab_size`,
`insert_spaces`, `word_wrap`, `format_on_save`, `terminal.default_harness`), beside the publishing
policy, the scripts and the checkout's connection. `agents.default` keeps
the agent pane's addressee chip and the three `workstreams.script.*` texts their own card. A test
(`projectSettingsModel.test.mjs`) reads the registry and holds those lists to *every*
project-capable key, once, so a new project key without an IDE home fails the build — and holds
every list to a card the screen mounts, since a list nobody draws is a key nobody can set. Both forms
draw one control per kind from `SettingControl`, so a setting looks the same wherever it is edited.

Groups added to the existing six in `Settings.tsx`, under a new **Project IDE** heading, each panel
**generated from the registry** by key prefix: **IDE** (hand-written for its one key —
`ide.default_mode`, the mode a workstream opens in, drawn as the header's own *Project · Agent ·
Board* switch — Board offered while `workstreams.board.enabled` is on — with the registry panel
under it for any later `ide.*` key) · Editor · Terminal (its two confirmation switches among its
keys) ·
Workstreams · **Board** (the four `workstreams.board.*` keys, shown here and omitted from
Workstreams — one group in the registry, two panels on screen) · Diagrams · Artifacts · Agents ·
**Language servers** (`lsp.*`, with each server's standing) · Keymap. Under
**Automation**, **Events** (the registry panel for `events.*` — whether this machine listens and its tick, a check's timeout and a file scan's bounds, whether public hooks are answered, how often pull requests are asked about, the chain depth, the rate and the backlog; what a workflow starts on is a step of it, drawn in the designer, and whether it listens is its own On/Off switch, never a setting), **Goals** (hand-written for `goals.default_mode` — the mode a new goal is captured
in, drawn as the New Goal dialog's own *Auto · Guided · Manual* switch with the mode's sentence
under it — with the registry panel under it for `goals.auto.repair_limit` and `goals.auto.permissions`),
**Workflows** (the registry panel for `workflow.*` — how the designer behaves on this machine) and
**Budgets** (the registry panel for `budget.*`: the three `budget.default.*` ceilings a goal or a run
of the workspace made without a budget of its own is given — the budget a workflow was turned on
with wins, and a run an event starts takes it).

**Secret fields.** Every API key, token, password or PEM block the desktop takes is typed into one
kit field, `SecretInput` (`ui/SecretInput.tsx` over `secretInputModel.mjs`; `SecretTextArea` for a
PEM block, folded to *•••••• · 3 lines* while hidden): **hidden by default**, an eye at its right
edge shows and hides what was typed, and the value **stays in the box** — masked — for as long as the
window lives, so a Save never empties it and *Save* waits for a change. What it can never show is a
secret the node already holds: the node never reads one back (11 — Security, I49), so when nothing is
typed and the node says a value is stored the box draws the mask `••••••`, dim, the eye is disabled
with its reason, a focus selects the mask and typing replaces it. Nothing is written to `localStorage`
or a session draft; the reveal is per mount. The sites: the Decision-Making Agent's API key, a connector
account's secrets (`secrets_set` says which draw the mask), a code host's token, every MCP env and
header value (the node's `••••••` is the stored state), and a proxy URL — a setting that does read
back, so the eye shows it whole, password included.

**Decision Settings** is its own group, beside Security, and holds one panel, **Decision Making**
(tab id `decision-making`): the Decision-Making Agent's
([15 — The Decision-Making Agent](../15-decision-making-agent.md)) — the agent and its readiness with a
calibration note, who answers and only the fields that provider takes, an API key in a secret field
(below — kept in its box, hidden, while the window lives; never shown back by the node),
`decisions.enabled` then every one of the eleven decision points with its own switch
(disabled where a point is selected explicitly — an `auto_route` model plan, an effort of `auto`,
the classifier's `decision_making_agent` provider, a `judge` step), the deadline and the two confidence thresholds, a *Try
it* box that decides and records nothing, and the newest judgements.

The **Git & code hosts** group is five hand-written panels for what is *not* a registry key, each
reloading on the engine's `git_setup_changed`: **Identity** (`?tab=git` — your global git config as
a form, the global file's includes read-only (each named as the profile it is, or *not the
platform's*), the profiles by organization with one dialog to add or edit and a confirmed remove,
then the registry's `git.*` keys — `GlobalGitPanel`, `GitProfilesPanel`, a `RegistryPanel`);
**SSH keys** (`?tab=git-ssh` — one card per public key: its fingerprint, whether ssh-agent holds
it, the check last run on it (pinned for the session, the platform's own memory) and its next step
— `nextStep`: put the public key on the host, check that the host knows it, bind it in a profile —
with *Copy public key*, *Add to a host* (copies the key, opens the host's SSH-keys page — `hostKeyPage`,
four known hosts), *Check on <host>* (`POST /git/ssh/test` with this key; the hosts offered are
`hostChoices`, the node's git hosts and the config's own), *Load into ssh-agent* and *Use in a
profile* (a `SettingsTabLink` to Identity); ssh-agent's line with the start command to copy when it is
down; the `Host` blocks of git hosts from `~/.ssh/config` with *Copy a Host block…* as a dialog and
*What would ssh offer* over `GET /git/ssh/resolve`; *Generate key…* as a dialog mounted only while
open. No passphrase is taken here and no file of ssh's is written — `SshPanel` and `ssh/KeyCard`,
`ssh/GenerateDialog`, `ssh/HostBlockDialog` over `sshModel.mjs`); and **GitHub**,
**GitLab**, **Bitbucket** (`?tab=github` · `gitlab` · `bitbucket` — one `CodeHostPanel` over
`codeHostAccountsModel.mjs`, three times: the connection's health — the kind's CLI installed or
not, signed in as whom, with **Authenticate** opening its browser sign-in in a terminal tab, or the
install commands to copy; every stored account with its connection line and organizations,
*Check*, *Make default*, *Forget*; who requests would go as and from where; the environment
override; git's helpers; *Add a token* with the scopes and the token page, the login beside it
for Bitbucket — [08](08-code-host.md#credentials)). The code host panel is drawn the moment it
opens — its title, the kind's chip, the token form — and the connection section says *reading the
GitHub CLI…* until the probe (`gh --version`, then `gh auth status`, each time-boxed on the node)
answers; the probe never gates the panel, a re-read says *checking again…* beside the last answer,
and a token can be typed meanwhile. The five are one group because a person
setting up a second organization touches them in order: a key, a sign-in, a profile that binds
them to the owner.

**Capabilities › System** (`?tab=system`, `SystemPanel`) opens on **Notifications**
(`notificationsSettingsModel.mjs`): the OS's permission as the notification plugin answers it —
*allowed*, *not asked yet* with *Allow*, which asks once; *not allowed* pointing at System Settings —
beside the `notifications.enabled` master and one `notifications.*` switch per category of what
happened (asks · failures · done · workflows · addons), drawn from the registry's own defs and help
through `SettingControl`, the categories dimmed while the master is off, and one sentence about the
icon a notification wears (the app's own; Terminal's under `tauri dev`). The shell's door
(`shell/notifications.ts`) reads the same six keys through `notifySettings.ts`, so a switch moves
the next notice at once. Under it the
two `system.*` switches live, each beside what macOS actually says: the switch is what the platform
**asks for** — off, nothing of ours relies on the grant; on, the panel asks for it — and the grant is
macOS's, read by the desktop shell (`src-tauri/src/permissions.rs`, [01](01-trust-boundary.md))
without ever prompting, so *granted · off here* is a state a person can read. Turning a switch on
asks: the microphone through macOS's own prompt, once, with the app's usage string; Full Disk
Access by opening the System Settings pane, since macOS has no prompt for it. Turning a switch off
revokes nothing, and the card says so. Under Full Disk Access the panel says the two things it owes:
that the grant is recommended for productivity, and that the security features protect the machine
on a best-effort basis, never as a sandbox ([11 — Security](../11-security.md)). Off the desktop app,
or off macOS, the switches are still there and the panel says the grant cannot be read from here.

**Capabilities › Network** (`?tab=network`, `NetworkPanel` over `networkModel.mjs`) is two halves
over two authorities. **This Mac** is the shell's ([01](01-trust-boundary.md)): whether the
**internet** is reachable — a TCP connect to a well-known address with its latency, whether the echo
service's name resolves, and the public IP the echo service at `network.public_ip_url` answers (a
machine-scope key, `https://api.ipify.org` by default, edited here as the registry's own row under
the Internet card; empty asks none, and the internet is then judged by the connect alone); every
**interface** that is up — its kind (Wi-Fi, Ethernet, a tunnel, by the hardware port System
Settings names), its addresses, its gateway, whether it carries the default route; whether a VPN is up
and what it is — the tunnel interface, its protocol (the connected service's kind, else the interface
family's word), the provider behind it read off the process table (Tailscale, WireGuard, a corporate
client), its addresses and peer, its MTU, whether all traffic leaves by it or only a split, the
resolvers bound to it, the public IP when all traffic leaves by it — beside the machine's own default
route and resolver, all read through the footer's store (`shell/networkStore.ts`) on
`cache.desktop.network_poll_ms`, again when the window regains focus and when its network flips
(*Check again* reads once); and the proxy System Settings names,
with **Use in Bisa** when it names a pair the platform can follow, which writes
`network.proxy.mode = manual` and the URLs and exceptions at machine scope in one `PUT` — a PAC file
or automatic discovery is a script, said as such, never followed. Nothing here changes a setting of
macOS's. **How the platform reaches the internet** is the node's: `network.proxy.mode` as a
three-way switch — *environment* (what the node's own process was given; the desktop-started node
inherits none), *none* (no proxy, and nothing the platform runs is handed one), *manual* — with the
mode's sentence under it, then in `manual` the registry's own controls for `network.proxy.https`,
`network.proxy.http` and `network.proxy.no_proxy`; an **in force** line from `GET /network` saying
what the clients do *now* — direct, or through which proxy with which bypass, the password masked,
the node's environment named as the source when it is — with the problems that keep the settings
from being in force; **Check**, one `GET` to a URL of the person's choosing through the platform's
own client (`POST /network/check`, refused for a loopback address — the check is for the way out);
then `network.http1_only` as a registry row with its origin badge. The window footer reads the same
facts on the same cadence as one word — *DOWN* (a red dot) while this Mac cannot reach the internet,
else *VPN* while a tunnel is up, else *UP* — the proxy in the sentence rather than the word, the
window's own `navigator.onLine` beating a stale read when it says offline and standing in off the
shell — with the same sentences in its overlay ([The desktop](../../guide/the-desktop.md#the-shape-of-the-window)).
The settings are one truth with
four readers: the engine's one set of HTTP clients (`bisa-http`, swapped on every write), the
environment every child the platform runs is handed — a harness session, a harness launched in a
terminal, git, `gh`/`glab`, a workstream script — so the platform and its tools agree, the status,
and this panel ([crates/http](../crates/http.md), [11 — Security](../11-security.md#outbound-hosts)).

The **Security** group is three panels over one `security.*` group — Redactor · Guard · Classifier
(`?tab=security-redactor` …; *Guard* is the menu word for the Tool & Commands Guard, whose full name
opens the panel's blurb). Each is a hand-written editor (`SecurityPanels.tsx`: the
built-in rules with switches, the person's rules as rows saved to the scope they chose — a draft is
of its scope, so the picker moving shows the other scope's rules and *Save* never writes one scope's
draft over another's list (`securityRules.draftOf`) — a *Try it*
preview on the node) above a `RegistryPanel` narrowed with `only` to that panel's switches and
numbers, so one setting has one control ([11 — Security](../11-security.md)).

**Node › Logging** (`?tab=logging`, `LoggingPanel` over `loggingModel.mjs`) is where the diagnostic log's four
`logging.*` keys live — the switch, the level, the rotation, the files kept, all machine scope, errors only by
default ([crates/log](../crates/log.md)) — under a card for the newest crash report (one sentence, *Show details* for the report whole over
`GET /logs/crashes/{name}`, the file manager's reveal on it) and a card that says where the files are (`GET /logs`:
the folder, one block per process family with each file's size and age, the crash reports, the total, the file
manager's reveal on the shell) and, once, that nothing leaves this machine. A write of one reaches the node's file layer through the engine and the
desktop's through the webview (`shell/logSettings.ts`), on the same `settings_changed`, with no restart.

The **Library** group lists each catalog kind as its own panel — Agents · Skills · Teams · Channels ·
Workflows — each a `CatalogPanel` pinned to that kind (`?tab=catalog-agent` …), so the offer
is not one crowded screen. **Skills** (`SkillsPanel`) is the one place a skill is written — *New
skill*, its fields the panel's own; the agent editor picks from the library and links here. The tab ids are `settingsLink.mjs`'s — read off the rail it holds — guarded against drift by
`settingsLink.test.mjs` and `scenarios/settingsPaths.test.mjs` (the `SettingsTab` type names exactly the rail's panels).

---

## Every panel reads the same way

A panel is drawn at once with its shape. Each read fills the place its answer goes, and that place
is in one of three phases — `loadModel.mjs`'s `phase(read)`: **pending** (nothing yet, being
read), **failed** (nothing yet, and the read refused) or **ready** (an answer is on screen, whatever
a later re-read did). A place that waits on several reads takes `phaseOf`: *failed* beats *pending*
beats *ready*, and a read with data is *ready* whatever its error.

- **Pending** is the kit's `Pending`: a `role="status"` line *reading {what}…* — the words survive
  reduced motion, where the pulse stops — over rows at the height the answer takes, one region
  with `aria-busy`. It stands where the section goes, never where the panel goes: the title, the
  static sections and the forms draw first. `pendingRows(what)` is one table of how many rows a
  section draws, so a panel never lies about its shape. **It earns its place after a beat**: the
  kit holds every pending piece back `INDICATOR_DELAY_MS` (200 ms, `ui/loadingModel.mjs`) —
  `Pending`, `SkeletonRows`, `Spinner` and the `Skeleton` block a screen sizes itself, which holds
  its box unseen for the beat (`placeholderFill`) so nothing jumps; none is exempt, and the
  immediate rectangle is private to `ui/Skeleton.tsx` — so a read that answers within it draws
  nothing but its answer and a placeholder never flashes — in a place that is already drawn; inside a surface that has just
  opened (a dialog, a popover) the indicator is immediate (`ImmediateIndicators`,
  `loadingModel.beatMs`), since a blank panel behind the veil would be the flash.
- **The registry and the resolved values are one store** (`shell/settingsStore.ts`): the registry
  is read once per session, the resolved values are kept per scope key and read again on every
  `settings_changed`, so a panel switch draws its controls at once — the first read of the session
  is the only one that can show *Pending*.
- **A re-read keeps the last answer and says so.** `useAsync` tells `refreshing` apart from
  `loading` (first load only). A section that reads again — the code hosts, Harnesses, Node,
  Cache's poll, System's grants — carries a `ReadLine`: `readWords` gives *checking again…* while
  a re-read is in flight, *could not read {what}: <reason> — showing the last answer* when it
  refused, *read 12 s ago* once settled; the line's door reads now and turns while one is going.
- **A failed first read is an `ErrorNote` with *Retry*, in the place the answer would go.** A
  panel whose only failure branch is unreachable pulses forever; `phase` decides, so it cannot.
- **Nothing draws a default before its read lands.** A switch that shows the compiled default and
  then flips to the workspace's value, a *No caches yet* said before the node answered, a registry
  row claiming *default* for a value the workspace set, a grant card saying *off* before macOS
  answered — every one is a one-row `Pending` until the truth is read.
- **Every save says what it is doing.** *Adding…*, *Saving…*, *Removing…*, a disabled switch
  while its write goes — through `attempt` and a local `busy` flag.

## Every key

The registry is the list, and [`docs/reference/settings-keys.md`](../../reference/settings-keys.md)
is generated from it by `just gen-settings-docs` — every key, its type, default and scopes, one
section per group in the order the registry first names them: `appearance.*`, `ide.*`, `editor.*`, `terminal.*`, `browser.*` (the embedded browser — **Browser** under Capabilities, [18](18-browser-and-servers.md#who-may-ask): the switch and what a restart remembers at the machine, the home page, which agents may ask, how far, when their tabs are kept out of sight and whether they may run a script in a page, at the workspace or a project, the screenshot's width), `mobile_development.*` (mobile development — **Mobile Development** under Capabilities, [19](19-mobile-development.md#the-workspaces-word): the switch, off by default, the platforms this machine develops for and the two paths at the machine, which agents may drive the devices at the workspace or a project), `addons.*` (the machine's switch over every addon window — **Addons** under Library, [18](../18-addons.md)), `harness.*`, `git.*`, `workstreams.*`, `security.*` (its `security.collaboration.*` pair — whether a message from a person on another node is read by the classifier first, and whether an agent they woke asks for every tool beyond reading — drawn beside the classifier's), `sync.*` (the relays, the interval, the relay list, the direct transport — machine scope, **Relays & sync** under Workspace, the relays as their own rows with health, [14](../14-collaboration.md)), `collab.*` (the workspace's name to guests, whether a valid code admits or asks, the default role, an invitation's life — workspace scope, **People**), `connectors.*`, `diagrams.*`, `artifacts.*`,
`agents.*` (`agents.conversation.mode` — the mode a checkout's conversation starts in — and
`agents.review.checkpoints` — how many of its turns stay restorable — beside `agents.default`,
`agents.effort` and the rest; [ide/20](20-reviewing-agent-changes.md#modes)), `goals.*`, `budget.*` (the default ceilings of a goal and of a run of the workspace — **Budgets** under Automation), `keymap.*`, `lsp.*`, `workflow.*`, `draw.*` (the canvas, [../19-drawings.md](../19-drawings.md)), `events.*` (what workflows and goals listen for — **Events** under Automation), `decisions.*` (the Decision-Making Agent), `cache.*`, `logging.*`, `rail.*`, `network.*` (how the platform reaches the internet — **Network**, after Desktop), `desktop.*` (the app's own behaviour on this machine — **Desktop**, beside System under Capabilities), `notifications.*` (which OS notifications reach this person — the master and one switch per category, **System** under Capabilities, beside the grants), `system.*`. A key is not repeated here: a hand-copied table drifts, and a test
(`crates/bisa-core/tests/it/docs.rs`) refuses a key quoted in prose that the registry does not
know.

---

## The keymap panel

The `preset` select, then a table of every command with its chord under the active preset, its
override if any, and a *record* control. Conflict detection is immediate and names the command that
already holds the chord; a conflict is refused, not stored. The reference page
`docs/reference/keymap.md` is generated from the same command registry, so it cannot drift. Details
in [15](15-keymap.md).
