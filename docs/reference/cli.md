# CLI

`bisa [--data-dir <path>] [--json] [--no-node] <command>`. With a running node the CLI routes
through it, presenting the token from `run/token` (or `BISA_API_TOKEN`); `--no-node` embeds an
engine in the process, and is refused while another engine holds the workspace. An embedded engine
hears no start event — what an event wrote down waits for the node, which alone starts runs from
events. `--json` prints the wire shape. `--input name=value` is read by the kind the workflow's
input declares — a word where text, a choice, an assignee, a project or an account is asked, a
number or `true`/`false` where one is — and a project given that way is attached to the goal.

## Workspace and identity

| Verb | Does |
|---|---|
| `init` | create the workspace and mint the owner keypair |
| `harness list` · `harness probe <id>` | which harnesses this machine has |
| `doctor` | what the platform needs before it can work — git, a harness, and the Decision-Making, General and Workflow Agents — each ✓, ✗ or !, with this platform's official install line and page, and the fixes the desktop's setup gate offers; exit 1 while something is missing ([16 — The setup gate](../architecture/16-setup-gate.md)) |
| `node [--listen <addr>] [--insecure-allow-remote]` | run the daemon, with the collaboration pump beside it; a non-loopback address is refused without the flag; refused at the door while an engine holds the workspace — it names the holder's pid and where the node answers, and opens nothing |
| `relay add\|remove\|list` · `relay check [url]` · `relay doctor` · `relay peer-add <pubkey> <node-id> <addr>` | the relays this node talks through (`sync.relays` — four public relays by default, off until `sync.enabled` is on; `list` shows each relay's state when a node runs, `off` rows while the switch is off), `list` printing under a row why it is not connected, when it says; `check` trying the relay named or every configured one once; `doctor` answering why a relay does not connect, in order — the switch, how many relays, whether this process can set up TLS at all, that relays are dialled directly and never through `network.proxy.*` — then a check of every configured relay, one JSON report under `--json`; and a direct peer by hand (`sync.iroh.peers`) |
| `workspace invite [--role admin\|member\|guest] [--channel <id>]… [--label]` · `workspace invites` · `workspace revoke <id>` | an invitation as a link and a code, printed once; every invitation and where it stands; withdraw one |
| `workspace join <link\|code> [--label]` · `workspace hosts` · `workspace leave <host>` | join a workspace on another node as a human, the workspaces you are in, leave one |
| `workspace people` · `workspace role <pubkey> <admin\|member\|guest>` · `workspace remove <pubkey>` | the owner and the people hosted here; change a role; remove a person |
| `workspace reindex` | rebuild the SQLite index from the files; refused while a node holds the workspace (it names the pid) — the files are the truth, the index a cache, and a node rebuilds a damaged one at open on its own; prints each stage as it passes and every record it skipped, and `--json` carries the `problems` |
| `workspace check` | read every file of the workspace as the next open would, without opening it, without the engine lock and without the index: the owner key, `members.json`, `governance.json`, every settings layer, every snapshot, every journal tail, and what earlier opens moved under `quarantine/`; one line a finding, `--json` the whole list, exit 1 while anything is found — what to run after a crash, and what `doctor` points at ([Operating](../guide/operating.md#after-a-crash)) |
| `governance show` · `governance set <approval\|escalation\|publish> <owner\|admins\|members\|list>` | who may sign which gate — you, the admins, the members, or a list; set through the node when one runs, since its engine is the one that reads it |
| `paths` | where this workspace is — the data directory and the log folder — without opening anything; what the desktop shell asks before it starts the node, so its own log lands under the workspace's `logs/` from the first line |
| `logs` | the diagnostic log on this machine: the folder, each process's files newest first, the crash reports and the newest of them in a line — the same answer as the node's `GET /logs`, without a node; what to attach to a bug report ([Operating](../guide/operating.md#the-log)) |
| `settings show [--project] [--group]` · `settings get <key> [--project]` · `settings set <machine\|workspace\|project> <key> <value> [--project]` · `settings unset <scope> <key> [--project]` · `settings registry` | three scopes, one registry |

## Goals and workflows

| Verb | Does |
|---|---|
| `new <statement> [--title] [--mode auto\|guided\|manual] [--workflow <id\|slug>] [--input k=v]… [--no-start] [--assignee …] [--document <path>]… [--tag]` | capture a goal in a mode (`goals.default_mode` when absent): `auto` — the Workflow Agent designs and the platform adopts, starts and repairs alone; `guided` — it proposes, you adopt; `manual` — you design. With `--workflow` the goal's work begins at once unless `--no-start` — a run, or, when the workflow begins on events, listening, each public hook's secret printed once. `--document` gives the goal a file as context — stored, then kept under the goal's `documents/` before anything runs |
| `inbox` | what concerns you — every ask with its offered options and its command, every unread notice under its row |
| `answer <goal\|run\|gate> [TEXT] [-o <option>]… [--unsure] [--step <id>]` | answer the question a goal — or a run, a goal's or one in the workspace, by its id — is waiting on; `--step` names the step when several wait |
| `approve <goal\|run> [--no] [--rationale] [--input k=v]… [--step <id>]` | decide the pending gate, through the goal or the run's home; an approved adoption begins the goal's work with the inputs — a run, or listening when the design begins on events, each public hook's secret printed once — an approved amendment applies it; `--step` names the step when several wait — its gate is decided and no other, and with none named two waiting are refused rather than guessed |
| `run <goal> [--input k=v]… [--start <step> [--data <json>]] [--watch] [--new]` | begin the goal's work. A goal whose workflow begins on events **listens**: no run is made, what it listens for is printed with each public hook's secret, once — and on a goal whose listening is paused it listens again, with what it listened with before unless `--input` gives it anew. Any other goal starts a run of its workflow (or follows the live one) until it settles. `--start` names where a run begins: the start by hand is a run now, whatever else the workflow begins on; an event start is a **test run**, begun there as if the event `--data` samples had happened. `--new` makes another run even while one is live, queued behind it |
| `stop <goal> [--rationale <text>]` | stop the goal: it stops listening, its sessions ended, its queued runs withdrawn, its live run cancelled (cause *stopped*); the goal stays open, a draft again |
| `restart <goal> [--watch]` | a new run of the goal's last run's workflow and inputs, at the start it began at and on the event that began it, started at once ahead of the queue; a live run is cancelled first (cause *restarted*); refused when that start is gone from the workflow as it stands |
| `runs <goal>` | every run of the goal, newest first: id, status, revision, a queued one's place in line, when it started and ended, why it was cancelled |
| `design <goal>` | ask the Workflow Agent to design the goal's workflow again — after a stall, a failure or a restart; needs a running node |
| `step answer <goal\|run> <step> [TEXT] [-o <option>]… [--unsure]` · `step release <goal\|run> <step>` · `step done <goal\|run> <step>` | one step of a goal's current run, or of the run the id names: answer a `human` step, release a `wait` step, mark a `human` step done by hand |
| `amend <goal> --from <file\|->` | replace the not-yet-started steps of the running workflow |
| `close <goal> [--rationale <text>] [--superseded-by <goal>]` | cancel the live run and the queued ones, and close the goal for good |
| `archive goal\|workflow\|project <id> [--undo]` | put away — an open goal closed first, a project's sessions stopped — or take back out |
| `rm <goal> [--projects keep\|archive\|delete] [--tree]` | delete the goal: refused first while a design of its own is used elsewhere; every session on it aborted — its harness process ended at once — its run cancelled, the rows waited for (up to five seconds; what is still ending is said), then its journal, runs and work items gone; the projects born of it kept (the default), archived or deleted — `--tree` to the Trash, never an adopted folder; an attached project only detached |
| `status <goal\|run>` · `log <goal\|run> [--limit]` · `search [query] [--tag]` · `pulse [--limit] [--concept <all\|workspace\|goals\|workflows\|projects\|channels\|agents\|node>]` | read: status, workflow, one line per step — a goal's, or one run's by its id — and, for a goal that listens, what it listens for, since when, and why it is paused; with `--json` both answer where they stand under one key, `status` (a goal's beside `goal`, `run`, `work_items`, `spent`; a run's beside `run`, `holder`, `started_by`, `work_items`, `spent`); `log` the journal of the goal, or of the run's home |
| `workflow list [--templates] [--goal <id>] [--all] [--tag]` · `workflow show <id\|slug>` | the library — this workspace's workflows, or the catalog's templates; `--goal` one goal's designs, `--all` everything with its origin — and one definition with its problems and its origin (`workspace`, `catalog:<slug>`, `goal:<id>`) |
| `workflow new --from <file\|->` · `workflow edit <id> --from <file\|-> [--revision N]` · `workflow validate --from <file\|->` · `workflow rm <id>` | record a definition (JSON, or TOML in the template shape) as a draft with its problems, save the next revision of the one you edited (a moved copy is a conflict), check one without recording it, forget one nothing uses (archive it while something does) — its finished runs in the workspace with it, refused while one goes |
| `workflow use <goal> <id\|slug>` · `workflow use <goal> --from <file\|->` | point a goal at the workflow its next run will use (a slug is installed on the way; another goal's design is refused), or record a design of the goal's own from a file and point at it |
| `workflow promote <id>` | copy a goal's design into the library — a new id, revision 1; the original stays on its goal |
| `workflow run <id> [--input k=v]… [--start <step> [--data <json>]] [--watch]` | run the workflow in the workspace — no goal captured — started at once beside any other run of it, then followed until it settles; refused with `needs_goal` when a step reads `{goal.…}`. By hand it begins at the workflow's start by hand; `--start` naming an event start makes a **test run**, begun there as if the event `--data` samples had happened, its mapping read over the sample. It never turns the workflow on |
| `workflow on <id> [--input k=v]… [--budget-usd-cents\|--budget-tokens\|--budget-secs <n>]…` · `workflow off <id>` | turn a library workflow **On**: its start events are heard from now on, each occurrence a run in the workspace; `--input` gives what its events read and do not supply, a budget flag the ceiling of each run it starts (zero lifts one; none leaves the workspace default, `budget.default.*`). Prints what it listens for, and each public hook's secret **once**. Refused, in the engine's words, while it has problems, is archived, is a goal's design, begins on no event, or reads its goal. **Off**: its events are no longer heard; a run already going goes on |
| `workflow listeners [<id> \| --goal <goal>]` | the listeners — every start event armed for a workflow that is On or a goal that listens: what each begins on, when it next comes due, what waits behind it, how a hook is called, and why one is not armed; one workflow's, one goal's, or all |
| `workflow hook-secret workflow\|goal <id> <step> [--rotate]` | a hook start's standing — its local path, its public path and whether its secret is minted, never the secret — or, with `--rotate`, a new secret, shown once: the old one stops verifying at once |
| `workflow runs <id>` | the workflow's runs in the workspace, newest first: its number among them, status, who started it, when it started and ended, why it was cancelled |
| `workflow stop <id> [--run <run>] [--rationale <text>]` · `workflow restart <id> [--run <run>] [--watch]` | stop, or restart with the same inputs — at the same start, on the same event — every run of the workflow in the workspace that goes — or the one `--run` names (a restart then follows the new run); a goal's run of it is its goal's, and is left alone |
| `assign <goal> <assignee>… [--replace]` · `unassign <goal> [<assignee>…]` | who carries it |
| `files tree <scope> <id> [--path] [--depth]` · `files show <scope> <id> --path <file>` · `files search <scope> <id> <query> [--regex] [--case smart\|sensitive\|insensitive] [--word] [--include <glob>]… [--exclude <glob>]… [--limit]` · `files replace <scope> <id> <query> <replacement> [--apply]` (the search's flags too) · `files new <scope> <id> --path <p> [--dir]` · `files mv <scope> <id> <from> <to>` · `files cp <scope> <id> <from> <to>` · `files rm <scope> <id> --path <p> [--recursive]` | what is on disk, read, searched and — with `--apply`, never by default — rewritten; scope is `goal`, `workstream`, `work_item` or `run` (a run in the workspace, its own folder) — a project's own tree is its primary workstream, under the project's id. Every one goes through the node when one runs. A replacement that finds a file changed since its preview leaves it alone and says so; a path that leaves the root is refused unread |

## Projects and workstreams

| Verb | Does |
|---|---|
| `project new <slug>` · `project clone <url> [--slug] [--depth <n>]` · `project import <path> [--slug]` · `project adopt <path> [--slug]` — all with `[--name] [--goal <id>] [--publish gated\|manual\|auto] [--assignee] [--tag] [--committer "Name <email>"] [--git-config key=value]…` | four ways in; the repository inherits your global git config unless `--committer` or `--git-config` writes its local layer — the one way an adopted folder is written into |
| `project attach <project> <goal>` · `project detach <project> <goal>` | one record; nothing moves. These, the four ways in, `assign`, `rm`, `git-init`, the git verbs (`stage`, `unstage`, `commit`, `message`, `identity`, `git-config`) and `workstream open\|commit\|push\|close` and the pull request verbs go through the node when one runs, an engine of their own otherwise |
| `project list [--goal <id>] [--tag]` · `project show <project>` | read; each prints where the project was born — `workspace`, `goal <id>`, or `step <step> of <workflow> in goal <id>` |
| `project git-init <project> [--git-config key=value]…` | turn a plain-folder project into a git repository — an adopted folder too, the one write adopt allows, because this is the ask: `git init`, the who-commits policy as at creation, an empty root commit when someone can commit; existing copy workstreams stay copies; refused when the project already is one |
| `project identity <project> [--name <name> --email <email>]` | who authors commits in the project's repository and where that comes from (`local` · `global` · nobody); with both flags, set it — two keys of the repository's **local** git config, shared by every worktree; setting it answers the platform's *who commits?* and commits a settlement the missing identity had refused |
| `project git-config [<project> \| --global] [--set key=value]… [--unset key]…` | the git config the platform knows — `user.name`, `user.email`, `user.useConfigOnly`, `user.signingkey`, `commit.gpgsign`, `pull.rebase`, `core.autocrlf`, `init.defaultBranch` (global only), `codehost.account`, `codehost.kind`, `codehost.github.account` · `codehost.gitlab.account` · `codehost.bitbucket.account` (global only) — at a project's local layer, or with `--global` your global git config, which Bisa writes through its three named functions and nowhere else |
| `git profile list` · `git profile set <slug> --label … --owner … [--host github.com] [--alias <ssh-host>]… --name … --email … [--ssh-key <path>] [--account <login>]` · `git profile rm <slug>` | profiles by organization ([ide/04](../architecture/ide/04-git.md)): one git config file the platform owns per owner, included by your global config for that owner's remotes — the author, the SSH key `core.sshCommand` names, the code host account; `list` also names the includes of your global file that are not the platform's |
| `git ssh keys` · `git ssh keygen <name> [--comment …]` · `git ssh load <name>` · `git ssh resolve <host> [--key <path>]` · `git ssh test <host> [--user git] [--key <path>]` | your public keys with whether ssh-agent holds them and the `Host` blocks for git hosts; an ed25519 key pair (no passphrase from here — `ssh-keygen -p` adds one); a key loaded into ssh-agent; what ssh would do for a host, offline; one handshake a git host greets, nothing written on either side |
| `git account [--host github\|gitlab\|bitbucket] list` · `git account add [--login <login>]` (the token on **stdin**; `--login` for Bitbucket) · `git account rm <login>` · `git account check <login>` · `git account default <login> \| --clear` | the code host accounts of one kind (`github` by default): each login with where its token lives, the CLI's login, git's helpers; a token verified with the host and stored under the login it answers; whose a token is and which organizations, groups or workspaces it sees; the default account — the global `codehost.<kind>.account` |
| `git health [--host …]` | what Settings › Git & code hosts shows for that kind, offline: the CLI (installed, version, signed in as whom), the stored accounts, git's helper, the default, and who a request would go as and from where |
| `git login [--host …]` | how to sign in, printed: the CLI's browser sign-in command to run in this terminal, the install commands when the CLI is missing, or the host's token page — the CLI's sign-in is never run for you |
| `git connection <wid> [--check]` | what the checkout will use to reach its remote — the remote and its protocol, the code host, the profile, the author and where it comes from, the SSH key or git's helper, the account, the cautions; `--check` runs the three read-only probes (the code host, the SSH handshake, `git ls-remote`) |
| `project assign <project> <assignee>… [--replace]` | who carries it — agents, teams and people this workspace has: a name nobody answers to is refused |
| `project rm <project> [--tree]` | forget; `--tree` also deletes a managed folder |
| `project files\|stage\|unstage\|diff\|commit\|message <project> …` | the project's own tree — its primary workstream — per file |
| `workstream list [--project]` · `workstream open <project> [--goal] [--label] [--agent] [--from <spec>] [--base <branch>]` · `workstream show\|diff <wid>` | `list` shows each project's primary first; `close` refuses the primary; `--from` is where the checkout starts — a new branch name, `new:<name>@<ref>`, `branch:<name>`, `remote:<remote>/<name>`, `tag:<name>`, `new-tag:<name>@<ref>` or `pr:<number>` (default: a derived new branch); `--base` the branch the work goes back to |
| `workstream commit <wid> -m <msg>` · `workstream push <wid> [--yes]` · `workstream pr <wid> --title [--body] [--draft] [--yes]` · `workstream close <wid> [--tree]` | push and pr pass the `publish` gate: a project that publishes by itself is done at once, one that publishes by hand refuses by name, one that asks first opens a gate on the goal the work is for — decided here by `--yes` or at the terminal's `[y/N]`, and the verb then waits for the act's end: the record as it stands once it landed, or *approved, and it did not go out* with the reason (the workstream's Inbox row carries the same notice). With no terminal and no `--yes` the gate stays open and nothing is pushed. Close stops every session standing in the workstream first and says how many (`stopped_sessions`) |
| `agent-context [--json]` | every tool per session scope (work item · goal · conversation — a note's conversation among them) and the chat framing, from the MCP server's own router |
| `workstream pr-view <wid>` · `workstream pr-checks <wid>` · `workstream pr-review <wid> --event approve\|request-changes\|comment [--body]` · `workstream pr-merge <wid> [--strategy merge\|squash\|rebase] [--keep-remote-branch] [--yes]` | the code host behind `origin` (GitHub, GitLab or Bitbucket — through its CLI when signed in, else its API); merge passes the `publish` gate and deletes the branch on the code host unless `--keep-remote-branch` |

## Agents, teams, skills, MCP servers, catalog

| Verb | Does |
|---|---|
| `catalog list [--kind agent\|skill\|team\|channel\|connector\|workflow\|addon] [--tag] [--match any\|all]` · `catalog show <kind> <slug>` · `catalog install <kind> <slug>` | the catalog |
| `addon list` · `addon show <id>` · `addon install <slug>` · `addon import <path> [--grant <permission>]… [--enable]` · `addon enable\|disable <id>` · `addon remove <id>` · `addon grant\|revoke <id> <permission>…` | the addons installed here ([Addons](../guide/addons.md)): a built-in from the catalog, a folder of your own with the grants you name and nothing more, the switch, the grants, the removal — the node first, so an open desktop draws the window |
| `connector list` · `connector show <id>` · `connector new --from <file\|-> [--id <slug>]` · `connector rm <id>` | the connectors here — the catalog's and your own — with how many accounts this machine has; one definition whole (hosts, auth, operations and their parameters, the accounts); your own definition from a TOML in the catalog's shape or a JSON, validated before it lands; a removal refused while an account or a workflow step names it |
| `connector accounts <id>` · `connector account add <id> --label <L> [--param k=v]… [--secret field=@stdin\|field=@path\|field=VALUE]… [--default]` · `connector account check <id> <account> [--timeout-secs <s>]` · `connector account default <id> <account>` · `connector account rm <id> <account>` · `connector connect <id> <account>` | this machine's accounts for a connector — the label, the default mark, which secret fields are set, **never a value**; an account added with its secrets read once (`@stdin` or `@path` — a PEM key file — keeps them out of the shell's history); one live request as the account (the connector's `check` operation); the default a step with no account named runs as; the account and its secrets forgotten; an OAuth account connected — the consent URL printed to open, the node's own page the browser comes back to, a pasted code when the platform cannot redirect |
| `agent list [--tag]` · `agent show <id>` · `agent add --name --prompt\|--prompt-file [--harness] [--model]… [--strategy] [--effort] [--description] [--decision-making] [--skill]… [--mcp]… [--tag] [--respond]` · `agent edit <id> … [--decision-making true\|false]` · `agent usage <id>` · `agent rm <id>` | definitions; `--strategy` is `fallback \| weighted \| round-robin \| least-busy \| auto-route`, `--model` is `id[=weight][@effort][: suited_for]` (the sentence after `: ` is `auto-route`'s own; the word after `@` is that model's effort, read as one only when it is one, since an id may hold an `@`; quote an id that ends in `[1m]`), `--effort` is `inherit \| auto \| minimal \| low \| medium \| high \| xhigh \| max` — the agent's, for every model of its plan that names none; `inherit` says nothing again and on `edit` `--effort` alone leaves the models as they are ([06 § Effort](../architecture/06-agents-and-teams.md#effort)), `--decision-making` lets the Decision-Making Agent stand in at the points this agent reaches ([15 — The Decision-Making Agent](../architecture/15-decision-making-agent.md)) |
| `agent skill add\|rm\|list <agent> [<skill>]` · `agent mcp add\|rm\|list <agent> [<mcp>]` | references. What writes the roster or its library — `agent add\|edit\|rm`, these two, `team create\|add-member\|remove-member\|rm`, `skill add\|edit\|rm`, every `mcp` verb that writes — goes through the node when one runs and into the record otherwise, the answer the same either way. An edit is one write: `agent edit --enabled false` beside a word that is refused stands nobody down, and what stands an agent down or up is said once in every channel it is in |
| `agent models <harness>` | what a harness advertises |
| `team create <name> [--purpose] [--agent]… [--human]… [--tag]` · `team list` · `team show <id>` · `team add-member\|remove-member <id> --agent\|--human` · `team usage <id>` · `team rm <id>` | teams |
| `skill list\|show\|add\|edit\|usage\|rm` | the skill library |
| `mcp list\|show\|add\|edit\|enable\|disable\|usage\|rm\|probe` | the MCP registry: three transports (`--command`, `--url`, `--url --sse`), headers and a working directory, secrets never printed back, and `probe` — who answered, the negotiated revision and era, the tools, or the stage it stopped at |
| `sessions list` · `sessions abort <id>` | the live roster, a line a session: its id, the state it is in, its kind (`worker`, `conversation`, `guided`, `terminal`) and what it is at work on — its item, else its conversation, else its goal. `abort` stops one for good, whatever drives it: a worker's harness is aborted and its step's item settles failed, a conversation's turn is let go of (the next message starts a session of its own), the Workflow Agent's design wake ends and the goal says so; the row reads *aborted* at once. Both need a running node: sessions live in it |
| `recall list <agent>` · `recall get <agent> <slug>` | an agent's memory |
| `tags [--entity <kind>]` | facet counts |

## Channels and messages

| Verb | Does |
|---|---|
| `channels list [--tag]` · `channels create <name> [--topic] [--agent]… [--team]… [--human]… [--tag]…` · `channels edit <id> [--topic] [--agent]… [--team]… [--human]… [--tag]…` | the standing channels; a roster's three halves — agents, teams, people (pubkeys) — each replaced only when named on `edit`, kept otherwise. A write goes through the node when one runs, into the record otherwise |
| `msg <scope> <text> [--reply-to <id>] [--mention …] [--attach <path>…] [--artifact <path>[:title]…]` · `msgs <scope> [--limit] [--before]` | post and read; scope is a channel, a goal or a conversation id. While a node runs the words go through it, so its engine hears them — the agents they address answer, and a start, a wait or a boundary event that listens for a message hears it; with no node they are kept, and nobody is there to answer. `--attach` hands a file over as a file; `--artifact` shares one to be looked at — rendered live where it is read, under its title ([guide](../guide/artifacts.md)) |
| `conversation list [--origin <kind> --id <id> \| --project <id>] [--agent] [--q <words>] [--archived] [--limit]` · `conversation new <origin> [id] [--title]` · `conversation show <id>` (one JSON answer: `{conversation, messages}`; reading marks the conversation read) · `conversation post <id> <text> [--mention …]` · `conversation rename <id> [title]` · `conversation archive\|unarchive <id>` · `conversation delete <id>` | conversations — a saved exchange with agents about something: the node, the workspace, a goal, a workflow, a project, a workstream, a drawing or a note (`<origin>` is one of those words; a workstream's project is read from its record). None starts an engine of its own. While a node runs, what changes a conversation — `new`, `post`, `rename`, `archive`, `unarchive`, `delete` — goes through it: every open window is told, putting one away or deleting it stops the turn running in it, and a deletion takes its review with it; with no node the record is the whole of it. The answer is the wire's either way ([13 — Conversations](../architecture/13-conversations.md)) |
| `conversation mode <id> [manual\|auto\|plan]` (shown when no mode is given) · `conversation changes <id>` (turn by turn, a file `overlapped` said) · `conversation keep <id> [--turn T \| --path P]` · `conversation undo <id> [--turn T \| --path P] [--force]` · `conversation restore <id> <turn>` | how far a checkout conversation's agent goes on its own, and reviewing what it changed. `mode` goes through the node when one runs — what was allowed *for this conversation* under the old mode ends with it, and a turn under way reads the new one at its next call — and writes the record when none does; `changes`, `keep`, `undo` and `restore` are the running node's alone — they write the checkout — and refuse without one. No MCP tool mirrors them, so an agent never settles its own change ([ide/20 — Reviewing agent changes](../architecture/ide/20-reviewing-agent-changes.md)) |
| `read <scope>` · `unread <scope>` | the local watermark — moved through the node when one runs, as `msgs` moves it, so what a window shows as unread follows |
| `dm list` · `dm send <pubkey>… --text <text>` | direct channels; `send` opens or reuses the conversation and posts through the running node when there is one, as `msg` does — an agent written to answers |

## Signals

| Verb | Does |
|---|---|
| `signal emit <name> [--data <json>] [--goal <goal>]` · `signal list [--limit <n>]` · `signal release <id>` | raise a **named signal** by hand — dotted lowercase words, `report.ready` — in the workspace or on a goal: heard by the `signal` starts, the waits and the boundary events that name it, and nothing listening is a quiet answer; list the newest signals, queued or settled, each with its state and never its payload; let a **held** signal through — an outside payload the content screen would not pass, read by a person. Nothing here starts a run: an event is written down, and the node's worker starts runs from the queue ([Events and gateways](../guide/events.md)) |

## The MCP server

`bisa mcp --socket <engine socket> (--work-item <id> | --goal <id> [--agent <id>] | --conversation <scope> --agent <id>)` is the stdio server the engine
injects into every harness session. The engine spawns it; you never run it by hand.

## The reporter

`bisa session report --harness <id> [payload]` is the other personality nobody types: a
harness a person opened in a desktop terminal runs it from its own hooks (Claude Code, Codex,
GitHub Copilot CLI — through a plugin mounted for the one launch) or a generated extension (OMP,
pi), with the payload on stdin or as the one argument. It reads `BISA_SESSION`, `BISA_SESSION_SECRET` and `BISA_NODE_URL`
from the environment the terminal set, translates the payload with the adapter's own mapping, posts
the events to `POST /sessions/{id}/report` under the session's secret, and exits 0 whatever
happens — a hook must never slow the harness. See
[ide/06 — Terminals](../architecture/ide/06-terminals.md#reporting--a-harness-in-a-terminal-is-a-roster-session).

## The guard

`bisa session guard --harness <id> [payload]` is the third personality, and the one hook that
is allowed to wait: Claude Code and GitHub Copilot CLI run it on `PreToolUse` beside the reporter
while `security.guard.terminal_hooks` is on. It posts the payload to `POST /sessions/{id}/guard`
under the session's secret and prints the verdict in the shape the harness named reads — Claude
Code's `hookSpecificOutput` (`permissionDecision` `deny` with the reason, `ask`, or `allow` with
`updatedInput`, a redacted placeholder restored), Copilot CLI's flat `permissionDecision`,
`permissionDecisionReason` and `modifiedArgs`. When the guard has no opinion and nothing was
restored, when the node does not answer within the hook's timeout or is not there, it prints
nothing and exits 0: the harness's own prompt stands. See
[11 — Security](../architecture/11-security.md).

## Security

`bisa security status` prints the three features as the node runs them, one fact per line: the
redactor (on, or *OFF — nothing is redacted on this node*; how many rules and how many switched
off; how many of the node's environment variables are detectors), the guard (on or off, the rules,
the terminal hooks), the classifier (the agent, its deadline, what a harmful verdict does, whether
it is ready and why not), every rule the node could not read, each harness's reach — *judged before
it runs*, *judged before it runs · restores placeholders*, or *observed only — runs under its own
sandbox* — the last ten decisions with who decided (a rule by id, the classifier, *you*, or *you:
remembered from your earlier answer on this goal or run*), and how many secrets the vault holds. Never a
value. `bisa security try --text '<text>'` shows what the redactor would do to a text, on a
scratch vault; `bisa security try --tool Bash --command '<command>'` or `--tool Read --path
<path>` what the guard's rules alone say — the verdict, the rule, the reason, the paths it read.
Both need a running node. `--json` prints the route's answer.

## The Decision-Making Agent

[15 — The Decision-Making Agent](../architecture/15-decision-making-agent.md).

| Verb | Does |
|---|---|
| `decisions status` | who answers, whether it can be asked, whether a key is stored, every decision point with whether the switch reaches it |
| `decisions list [--limit]` | the newest judgements, newest first (50 by default, 200 at most); with `--json`, `{judgements}` |
| `decisions try --state <text\|json> (--noul <instructions> \| --choice <instructions> --option id=meaning …)` | one question to the provider as it is set up; nothing decided, nothing recorded |
| `decisions key set <jev\|rlcd> --from @stdin\|@path` · `decisions key clear <jev\|rlcd>` | a remote provider's API key, kept in this machine's keystore and never read back |

A key is always read from `@stdin` or a file, never typed on the command line where a shell keeps it
in history.

## Not verbs

`identity export|import` (NIP-49) and `note …` have no verb; the node routes exist for notes
(`/notes`) and the desktop uses them. See [feature status](../feature-status.md).
