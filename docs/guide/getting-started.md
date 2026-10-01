# Getting started

Everything here works on a laptop with **no account, no server and nothing deployed**. The quickest
way in is the desktop application, built once at its release profile and ready to use; the node and
the CLI are the same binary for when you want a daemon or a terminal. You need:

- Rust (stable) — `cargo --version`
- at least one coding harness: [Claude Code](https://code.claude.com/docs/en/setup) (`claude`),
  [Codex CLI](https://developers.openai.com/codex) (`codex`), [OpenCode](https://opencode.ai/docs),
  [GitHub Copilot CLI](https://github.com/github/copilot-cli) (`copilot`),
  [Grok Build](https://docs.x.ai/build/overview) (`grok`), oh-my-pi (`omp`), pi, goose,
  cursor-agent — or any ACP agent — signed in
- a model for the **Decision-Making Agent** to answer with: one of an installed harness (Claude
  Code · `claude-sonnet-5-5[1m]` out of the box), or Jev with its API key — set under Settings › Decision
  Settings › Decision Making
- `git` on your `PATH` for projects (2.36 or newer for profiles by organization); for pull requests, an account on GitHub, GitLab or Bitbucket — the `gh` or `glab` CLI already signed in on your machine, or the credential your git helper already holds if `git push` over HTTPS works, or a token added from Settings › Git & code hosts › GitHub · GitLab · Bitbucket, or `BISA_GITHUB_TOKEN` (`_GITLAB_`, `_BITBUCKET_`); `ssh`, `ssh-keygen` and `ssh-add` on your `PATH` for the SSH keys panel
- for the desktop application: Node.js 20+, npm, the Tauri CLI (`cargo install tauri-cli`), and
  [`just`](https://github.com/casey/just) for the recipes below (each is one or two commands you
  can also run by hand — the `justfile` at the root is the list)

## 1. The desktop application — release, ready to use

**Download it.** Every release is on the repository's
[Releases page](https://github.com/mourad-ghafiri/Bisa/releases): `Bisa-<version>-macos-universal.dmg`
— one application for Apple Silicon and Intel Macs from macOS 11, signed and notarized, so macOS
opens it without a word — and its `.dmg.sha256`. Beside the two files:

```sh
shasum -a 256 -c Bisa-<version>-macos-universal.dmg.sha256
```

prints `OK` when the download is the file that was published. Open the image and drag **Bisa** to
**Applications**. The application is whole: its node is inside it, nothing on your `PATH`, nothing
else running; the first launch creates `~/.bisa/` and your identity. How the release is made, and
how to check its signature and its notary ticket, is on
[Release § The release](../contributing/release.md#the-release).

The application's node is also the `bisa` command, inside the bundle. To use it in a terminal, link
it into a folder on your `PATH`; while the app is open, the command line talks to the app's node:

```sh
ln -s /Applications/Bisa.app/Contents/MacOS/bisa /usr/local/bin/bisa
bisa doctor
```

**Or build it, on macOS, in one line:**

```sh
git clone https://github.com/mourad-ghafiri/Bisa && cd Bisa && scripts/start-macos.sh
```

That builds the application the first time and opens it every time after. Behind it,
`scripts/bundle-macos.sh` builds both halves at their shipping profiles — the node (`bisa`, the
`dist` profile: optimised for throughput, it runs for weeks as the app's sidecar) and the Tauri
shell (release, fat LTO) — **for Apple Silicon and Intel in one bundle**, puts the node **inside
the bundle** beside the shell, signs the result, and places it at `dist/Bisa.app`. The first build
needs both Rust targets; `rust-toolchain.toml` names them and rustup adds the missing one on the
way. `scripts/start-macos.sh --rebuild` builds it again after a change; a previous copy is moved
aside under `target/`, never deleted.

A build of your own is signed **ad hoc**: it opens on your Mac and on no other — macOS puts an
unsigned, un-notarized download in the Trash. A copy to give to someone else is the release:
`scripts/release-macos.sh` with a Developer ID, notarized and stapled —
[Release § The release](../contributing/release.md#the-release).

On Linux and Windows, `just desktop-bundle` builds the same two halves; the bundle (an AppImage
and a `.deb`, or an installer) is in the shell's `target/release/bundle/` under `desktop/src-tauri`,
and finds the node on the `PATH` an application sees — `/usr/local/bin`, not `~/.local/bin` — or
under `BISA_BIN`:

```sh
ln -sf "$PWD/target/dist/bisa" /usr/local/bin/bisa
```

**The first launch is the setup.** The node creates `~/.bisa/` and mints your keypair, stored as
an owner-only file under `~/.bisa/identity/` — the OS keyring only when you ask for it with
`BISA_KEYSTORE=keyring`. Your **npub is your identity**; there is nothing to sign up for. The
profile menu at the top right shows it under *Identity*, and *About Bisa* says which version
the desktop is and which the node is running. Nothing else is required: the workspace opens with its
three permanent objects (§4), the catalog is one click away, and every harness the node can see is
under Settings › Harnesses.

The app is laid out like a workspace: a sidebar of destinations, live sections for channels and
direct messages, one conversation surface in the middle, and **Goals** as one live list of runs —
each goal a strip of its steps. **Workflows** is the library and the designer; a goal's
**Workflow** tab draws its run on the same canvas, live. **Projects** opens the IDE, in Project Mode
(documents and terminals) or Agent Mode (the conversation with the agents in the centre). Full tours:
[`the-desktop.md`](the-desktop.md) and [`the-ide.md`](the-ide.md).

Three things are on from the first run without a setting: a key or token you paste into a message
reaches the agent as a placeholder — and so does one the node was started with in its environment,
or one an agent finds and writes back — the commands no agent should run unasked are refused before
they run, and a doubtful command is read by a model before it reaches you. Settings › Security is where
you see the rules, add your own and try them; the design is
[11 — Security](../architecture/11-security.md).

### Developing the desktop

The same shell against a debug node, with hot reload:

```sh
cargo build -p bisa-cli
cd desktop && BISA_BIN=../target/debug/bisa cargo tauri dev
```

UI only, against a node you already run: `BISA_API_BASE=http://127.0.0.1:4477 npm run dev`.

## 2. The node — the same binary, headless

The desktop's sidecar is an ordinary daemon you can run yourself, on your laptop in the background or
on a server:

```sh
bisa node                           # unix socket only: ~/.bisa/run/node.sock
bisa node --listen 127.0.0.1:4477   # + HTTP: SSE events, the desktop app, curl, webhooks
```

A node on a fresh machine creates the workspace and the identity exactly as the desktop's first
launch does. While a node is running the CLI routes through it, so decisions resolve gates in the
*running* engine and guided goals, events, waits and the collaboration pump stay live. `--no-node` forces embedded
mode. **Events are heard and waits elapse only while a node is running.** The HTTP surface is
[`reference/http-api.md`](../reference/http-api.md).

**The control plane is behind a bearer token.** The node mints 32 random bytes into `run/token`
(mode `0600`) on first start and reuses it; the CLI reads it, and the desktop hands its sidecar one.
Every route but the named exceptions answers `401` without it — `/health`, a public hook
(`/hooks/…`, opened by its own secret), the OAuth callback a sign-in ends on, the A2A card and
endpoint, an installed addon's files, and a terminal session's four doors, which take the session's
own secret:

```sh
TOKEN=$(cat ~/.bisa/run/token)
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:4477/goals
```

`--listen` refuses a non-loopback address unless you pass `--insecure-allow-remote`: a token is one
secret between that port and the whole workspace, so keep it on `127.0.0.1` and put your own tunnel
in front if something outside the machine needs in.

## 3. The CLI

`bisa` is a multicall binary: the node above, and every verb below. The release build is the
one `just desktop-bundle` made (`target/dist/bisa`); for day-to-day development a debug build
tracks every rebuild:

```sh
cargo build --workspace                         # debug build; binary at target/debug/bisa
ln -sf "$PWD/target/debug/bisa" ~/.local/bin/bisa
```

or an installed copy: `cargo install --path crates/bisa-cli`. Everything below assumes
`bisa` resolves.

```sh
bisa init
```

`init` does by hand what the first launch of the desktop or the node does on its own: it creates
`~/.bisa/` and mints your keypair, and prints the npub. `--data-dir <path>` on any command keeps
a separate workspace. From 0.1.0, a later 0.x release opens your workspace as it is
([Compatibility](../reference/compatibility.md)); take a copy before you upgrade all the same. A
workspace a development build wrote before 0.1.0 has no upgrade path: move it aside and start fresh;
copy `identity/` across (or keep the keyring), so the new workspace opens as the same person:

```sh
mv ~/.bisa ~/.bisa.old
bisa init
```

Check which harnesses Bisa can see:

```sh
bisa harness list            # ✓/✗ per harness, with versions
bisa harness probe claude-code
```

## 4. What a fresh workspace holds

Exactly three permanent objects: the **General Agent** (`general-agent`), the **Workflow Agent**
(`workflow-agent`) and the **`general` channel**. No teams, no other channels, no skills, no
MCP servers, no projects, no workflows of your own. The third core agent, the **Decision-Making
Agent**, is part of the platform rather than of the workspace: it holds no record, so it is in no
list here — Settings › Decision Settings › Decision Making and `bisa decisions status` are where it shows.

```sh
bisa agent list
#   general-agent    General Agent    claude-code   claude-opus-5-5[1m] → claude-sonnet-5-5[1m]
#   workflow-agent   Workflow Agent   claude-code   claude-opus-5-5[1m] → claude-sonnet-5-5[1m]
bisa channels list
#   general
```

The General Agent knows the whole workspace and its job is to **install and assign** the staff a
piece of work needs rather than doing the work itself. The Workflow Agent's job is to **design how a
goal runs** (§5). You cannot remove or disable either; their harnesses and model plans are yours to
change.

The quickest thing to do with a fresh workspace is message the General Agent — in the desktop, open
**Messages** and write to it; from the terminal:

```sh
PK=$(bisa --json agent list | jq -r '.agents[] | select(.id=="general-agent") | .pubkey')
bisa dm send $PK --text "what is in this workspace, and what should I install?"
bisa msgs <scope-ulid>       # the conversation, including the reply
```

The reply is signed by the agent, not by you. It runs with its harness's full tools in a scratch
folder of its own, so "read ./notes.md and summarise it" works — and it can start tracked work for
you if the request deserves it.

### The catalog

The catalog's seventh kind is an **addon** — a small window that floats over the desktop, installed
from Settings › Library › Addons or `bisa addon install <slug>` ([Addons](addons.md)); a fresh
workspace runs none.

Everything else is a **catalog** compiled into the binary — agents named after jobs you already
recognise, the skills they carry, teams, channels and workflow templates. None of it exists in your
workspace until you say so; the whole list is [`reference/catalog.md`](../reference/catalog.md), and
in the desktop it is Settings › Library.

```sh
bisa catalog list                          # everything, marking what is installed
bisa catalog list --kind workflow
bisa catalog list --tag engineering
bisa catalog show agent developer          # prompt, model plan, skills, tags
bisa catalog show workflow software-feature   # inputs and steps
bisa catalog install agent developer
bisa catalog install team engineering      # brings its five agents and their skills too
bisa catalog install workflow bug-fix      # brings the agents its steps name
```

Ids are bare slugs — `developer`, `qa-engineer`, `engineering`, `bug-fix`. An install is transitive,
reports what it created, is idempotent, and refuses a collision by name rather than overwriting.
Install the smallest staff that does the job: every agent is a keypair minted and a candidate every
unassigned step routes across.

Three things to know before you post anywhere:

- **In a channel, mention the agent.** A standing channel belongs to the whole workspace, so an
  agent answers there when named: `bisa msg <channel> "@researcher what are the options?"`.
  A roster is a *directory*, not a subscription — rostered agents do not auto-reply, but the
  channel's own handle addresses all of them at once (`@engineering`).
- **A message that names nobody still lands somewhere.** Post a question without naming anyone and
  the General Agent picks it up, answers, or hands it to the agent that should. This is *triage*.
- **By default an installed agent answers only you.** `--respond members` when you create one lets
  it answer any workspace member.

An agent needs a running engine to answer: the desktop's sidecar, a daemon (`bisa node`, §2)
or the CLI process that is holding the conversation open.

## 5. A goal, from a sentence to done

A goal is a stated want and a **workflow** — a small graph of steps that says how the want becomes
real: agents work, checks judge, you answer and approve, the world is waited for. One **run** at a
time carries it, and the goal's status — `draft`, `running`, `waiting`, `done`, `failed`, `closed` —
is wherever the run has got to. Full guides: [`goals.md`](goals.md) and
[`workflows.md`](workflows.md).

You do not design the steps by hand unless you want to. The Workflow Agent does:

```sh
bisa new "Create hello.txt containing a friendly greeting in the demo project"
```

That is the whole command. The agent reads the goal, asks what it needs to know, starts from the
closest template, validates its draft, and **proposes** it:

```
Workflow Agent is looking at it…
  → get_workflow · validate_workflow · propose_workflow
  ⏸ waiting on you: Adopt this workflow? (3 steps: write · check · ship)
```

In the designer's own Agent pane — a conversation about a library workflow, no goal in sight — the
same agent reads, validates and **saves** instead (`save_workflow`), and the canvas beside it shows
the new revision.

It never adopts anything itself — it holds no scheduler slot and decides no gate. The proposal
waits in your inbox; adopting it takes the inputs the run starts with and starts the run:

```sh
bisa inbox
bisa status $GID                              # the proposed steps, none started
bisa approve $GID --input project=<pid>       # adopt, and the run starts
```

Everything waiting on you is in one place — beside what happened to what you asked for — and the
things owed come in three shapes, each with one command:

```sh
# A question — a human step, or an agent asking mid-step
bisa answer $GID "put it in the demo project, under 80 characters"
bisa answer $GID -o demo                      # pick an offered option
bisa answer $GID -o demo "but call it hi.txt" # pick one and say more
bisa answer $GID --unsure                     # you genuinely do not know

# A gate — an approval step, a proposed workflow, a proposed amendment
bisa approve $GID --rationale "looks right"
bisa approve $GID --no --rationale "the scope is wrong"
```

**Three ways to answer, and all three are always open.** The options are what the agent thinks the
answers are; they never limit what you can say. `--unsure` is not a decline: it resolves the
question without deciding it and steers the agent to ask something narrower. Three of those per goal,
after which the agent proceeds on its own recommendation and journals the assumption.

Then watch it go:

```sh
bisa status $GID        # status, workflow, one line per step with its mark
bisa log $GID           # the signed journal as an activity timeline
bisa pulse              # everything happening across the workspace
bisa search hello       # full-text search over goals
bisa files tree goal $GID           # what the work left on disk, annotated
bisa files show goal $GID --path work/notes.md
```

### From a template

```sh
bisa workflow list --templates
bisa new "Fix the login redirect loop" --workflow bug-fix \
  --input project=$PID --input report="After logout, /login redirects to itself"
bisa new "Weekly review" --workflow weekly-review --input channel=general
```

`--workflow` picks a template (installed on first use) or a workflow of your own, and starts the run
at once with the inputs given — no proposal, no adopt gate. The templates are listed in
[`reference/catalog.md`](../reference/catalog.md#workflows-13); `bisa workflow show <slug>` prints one.

A workflow also runs with no goal at all — a **run in the workspace**, started from the library:

```sh
bisa catalog install workflow weekly-review        # prints the installed workflow's id
bisa workflow run <id> --input channel=general --watch
bisa workflow runs <id>                            # its runs in the workspace, newest first
```

Nothing appears in Goals: the run is the workflow's, listed on its **Runs** tab in the desktop,
with a page of its own ([Workflows](workflows.md#running-waiting-amending)).

### Driving it yourself

`--mode manual` hands the design to you (and `--mode guided` has the Workflow Agent propose for
you to adopt; the default, `auto`, runs on its own); with a workflow named, the shape is yours:

```sh
bisa new "…" --workflow <id> --no-start             # picked, not started
bisa workflow new --from ./fix.toml                # your own, JSON or TOML
bisa workflow validate --from ./fix.toml           # every problem, by step
bisa workflow use $GID <id>
bisa run $GID --input project=$PID --watch
bisa step answer $GID ask-reporter "cannot reproduce on main"
bisa step release $GID hold                        # a wait step you release by hand
bisa amend $GID --from ./fixed.toml                # replace the steps that have not started
```

A workflow's `agent` step takes `harness` as an ordered fallback chain and `output_schema` to make
the result machine-validated; `project` must name a project attached to the goal. There is no
`--agent` anywhere: which agent *ran* a step is recorded by the engine, not chosen by you.

## 6. Projects, workstreams and the `publish` gate

A **project** is a real folder in the workspace, with or without git. It belongs to the workspace
and is **attached** to zero or more goals; attaching and detaching move nothing. Each piece of work
happens in a **workstream** — a branch and a checkout of its own. Full guide:
[`projects.md`](projects.md).

```sh
# A bare repository standing in for a remote, so nothing leaves your machine.
git init --bare ~/tmp/origin.git

PID=$(bisa --json project new storefront | jq -r .project.id)     # no goal needed
bisa project attach $PID $GID                                     # one record
bisa project show $PID
```

A new project is a git repository with no commits yet, and a repository with no commits has nothing
to branch from — so give it one before you run work in it if you want branch isolation from the
first step:

```sh
bisa project stage $PID README.md
bisa project commit $PID -m "first"            # local, and passes no gate
cd ~/.bisa/projects/storefront/tree && git remote add origin ~/tmp/origin.git
```

Open a workstream, commit, and push:

```sh
WID=$(bisa --json workstream open $PID --goal $GID --label landing | jq -r .workstream.id)
bisa workstream diff $WID
bisa workstream commit $WID -m "Landing page skeleton"
bisa workstream push $WID
```

A commit is local and reversible. A push leaves the machine, so it passes the **`publish` gate**:
on a terminal the CLI asks you inline (`[y/N]`, or `--yes` up front); with no terminal it says the
gate is open and pushes nothing; over HTTP the call answers `202` and the daemon finishes the push
once you decide it in the inbox. `bisa project new … --publish auto|manual` sets the policy
per project. A pull request, when the project has a GitHub, GitLab or Bitbucket remote and an
account answers for it (your machine's `gh` or `glab` signed in, git's credential helper, or a
token under Settings › Git & code hosts — `bisa git health --host github` says which; the
checkout's About › Settings says which account it will use):

```sh
bisa workstream pr $WID --title "Landing page" --body "First cut."
bisa workstream close $WID --tree
```

## 7. Collaborate — people on other nodes

```sh
bisa relay add wss://relay.example.com                     # Alice: a relay; ciphertext only
bisa workspace invite --role guest --channel design        # prints a link and a code, once
bisa workspace join <code> --label bob                     # Bob, on his own node
bisa workspace people                                      # Alice: Bob, a guest on #design
bisa governance set approval members                       # let the members decide gates
```

Bob joins as a human only: he reaches the channels his role allows, posts and reads there, and
brings no agents; Alice's node hosts the workspace and relays each message to who reaches it. Full
guide: [`collaboration.md`](collaboration.md).

## 8. Operate — events, governance

```sh
bisa catalog install workflow standing-health-check     # installs Off
bisa workflow list --tag ops                            # its id
bisa workflow on $WF --input command="curl -fsS http://localhost:8080/health" \
  --input interval=300 --input channel=operations       # On: the check runs every five minutes
bisa workflow listeners $WF                             # what is armed, and when it next comes due
bisa governance show
```

What starts a workflow is part of it: `standing-health-check` begins by hand, and — once it is
turned On — every time its check **starts failing**. An event is written down before anything runs,
and the run it starts passes the same gates and budgets as yours. Full guides:
[Events and gateways](events.md) and [`operating.md`](operating.md).

## First run: the setup gate

The desktop checks five things when it opens and puts a **Before you start** panel in front of
everything until they are there: `git` on your `PATH`, one installed coding harness, and the three
core agents — the Decision-Making Agent switched on and able to answer, the General Agent and the
Workflow Agent each on an installed harness with a model. Each missing one shows your platform's official install lines with a
**Copy** button and **Open official docs** (nothing is ever run for you), the Settings tab or the
Agents page where it is fixed by hand, and — once a harness is installed — one-click fixes: *Use
Claude Code · claude-sonnet-5-5[1m] as the Decision-Making Agent*, *Run on Claude Code* for an agent —
which gives it the plan the harness recommends, Opus 5.5 then Sonnet 5.5. Settings and
Agents stay usable under a banner while you fix things; **Check again** re-reads — the panel also
reads again on its own every twenty seconds while something is missing — and it closes on its own
when everything is ready. `bisa doctor` prints the same five checks in the terminal and
exits 1 while something is missing ([16 — The setup gate](../architecture/16-setup-gate.md)).

## Troubleshooting

**macOS moved Bisa to the Trash, or says it cannot be opened?** The copy was built for one Mac —
`scripts/start-macos.sh` signs ad hoc — and another Mac refuses it. Take the release instead: the
disk image on the [Releases page](https://github.com/mourad-ghafiri/Bisa/releases), signed with a
Developer ID and notarized, opens anywhere (§1); the person who releases makes it with
`scripts/release-macos.sh` ([Release § The release](../contributing/release.md#the-release)).

The desktop says it could not start the node? It takes `BISA_BIN` when set, else the node
inside its own bundle, else `bisa` on the system `PATH`, else a debug build beside the
repository — `scripts/start-macos.sh --rebuild` remakes a whole bundle; elsewhere put the release
node at `/usr/local/bin/bisa` (§1) or launch with `BISA_BIN`. Nothing happening after `bisa
new`? The Workflow Agent needs a working harness — `bisa harness list`, or Settings › Harnesses.
Guided work stalls with no daemon: start `bisa node`, or keep the desktop open. A workflow that is
On and hears nothing, a `wait` step never elapsing? Same answer — and `bisa workflow listeners`
says what is armed and why a listener is not. A step that names a project the goal is not
attached to fails and says so — `bisa project attach`. A workstream that will not open on a
fresh repository needs one real commit. `bisa inbox` is always the answer to "what does it want
from me?". The log is `~/.bisa/logs/` — errors only by default, a crash report under `crashes/` for every
death; `bisa logs` names them, `bisa settings set machine logging.level '"debug"'` writes
everything ([Operating](operating.md#the-log)). `index.sqlite` rebuilds from the
filesystem — delete it any time. A second machine not seeing decisions is almost always governance
(`bisa governance show`).
