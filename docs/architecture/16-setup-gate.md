# 16 — The setup gate

What the platform needs before it can work — git, one coding harness and its three core agents —
checked when the desktop opens and put in front of everything until it is there. Five checks, one truth (`bisa_engine::readiness`), read by the
desktop's gate, `GET /readiness` and `bisa doctor`. **The platform installs nothing and runs no
command for the person: it shows the official line to copy and the page it came from, and offers
the fixes it can vouch for as one click each.**

## The five checks

| Check | Ready when | Missing / unready when | The way out |
|---|---|---|---|
| `git` | `git --version` answers on `PATH` (the engine's own handle, `Git::version`); below 2.36 it is ready with a note — only profiles by organization need `hasconfig` | the program is not there (`Missing`) | git-scm.com/install — `brew install git` · `xcode-select --install`; `apt-get`, `dnf`, `pacman`, `zypper`, `apk`; `winget install --id Git.Git -e --source winget` |
| a coding harness | one compiled-in adapter probed as installed (`HarnessCatalog::list_cached`, the `GET /harnesses` path) | none did (`Missing`) | Claude Code's official page and lines first (`bisa_harness::install`); Settings › Harnesses shows every harness's |
| the Decision-Making Agent | `decisions.enabled` and `DeciderStatus.ready` — and `ready` now means the harness it names is *installed*, not merely registered (`decider::launchable` reads the warm listing) | off, or its `problem` (`Unready`) | a fix per installed harness — *Use {harness} · {model} as the Decision-Making Agent* writes `decisions.enabled`, `.provider = harness`, `.harness.id`, `.harness.model` (the model the harness recommends for judging — `HarnessAdapter::recommended_judge`, Claude Code's `claude-sonnet-5-5[1m]`, Copilot CLI's documented default `claude-sonnet-4.6` — else the first model it lists (Grok Build's, as `grok models` prints them), else its default); Jev through Settings › Decision Settings › Decision Making with its API key |
| the General Agent | enabled, its `harness` installed, a model plan with at least one model | its harness is not here, or it names no model (`Unready`) | a fix per installed harness — *Run on {harness}* (`PATCH /agents/general-agent` with the harness and the plan it recommends — `HarnessAdapter::recommended_plan`, Claude Code's `claude-opus-5-5[1m]` then `claude-sonnet-5-5[1m]`, Copilot CLI's documented default — else its first two listed models (Grok Build recommends none: an id written into the platform could retire), else the plan kept; the effort the plan names is kept); the Agents page |
| the Workflow Agent | the same | the same | the same, for `workflow-agent` |

`Readiness { ready, checks: [Check { id, state, title, detail, hint?, door, fixes }], checked_at }`
— `hint` is an `InstallHint { url, commands: [{platform, command}], verify?, sign_in? }`, `door` is
`Settings { tab }` · `Agents` · `None`, a `Fix` is `Settings { label, set }` or `AgentHarness {
label, agent, harness, models }`. Every URL and command is the official documentation's, read on
2026-09-22: git-scm.com/install, code.claude.com/docs/en/setup, the openai/codex README and
developers.openai.com/codex, opencode.ai/docs — and on 2026-09-30: the github/copilot-cli README
(GitHub Copilot CLI: its install script, Homebrew, WinGet and npm lines, `copilot login`) and
docs.x.ai/build (Grok Build: its two install scripts, `grok login` or `XAI_API_KEY`) — and on
2026-10-05: geminicli.com/docs (Gemini CLI: `brew install gemini-cli` on a Mac, `npm install -g
@google/gemini-cli` everywhere, signed in by running `gemini` or with `GEMINI_API_KEY`). `pi`, `omp`
and the generic ACP targets carry no hint — no official page is known to the platform. The missing
check's sentence names the six harnesses with a page; its one hint stays Claude Code's.

A harness is *installed* when its adapter's probe says so. For Copilot CLI, Grok Build and Gemini CLI that is
more than a name on `PATH`: the binary has to answer `--version` with a version
(`util::probe_versioned`), because an editor installs a launcher called `copilot` that answers
every word with *Install GitHub Copilot CLI?* — a harness that is not there.

## The gate

`shell/SetupGate.tsx` and `shell/useReadiness.ts` over `setupModel.mjs`. It reads `GET /readiness`
on mount, again on `settings_changed` (a fix of the Decision-Making Agent and every hand-made change
to `decisions.*` say so), after one of its own fixes lands, on *Check again*, when the node comes
back after it was away, and every twenty seconds while something is missing — or while the read
failed (`recheckMs`): a node not yet up when the desktop opened is asked again, so what is unknown
is never unknown for the life of the window. A read that failed keeps the last answer and says why
(`afterRead`).
Three modes (`gateMode`): **nothing** while ready — or while unknown, since a node not yet answered
must never block the app; a **banner** over Settings, Agents and an agent's page — *Setup incomplete
— 3 of 5 ready · Check again · Back to setup* — because that is where things are fixed by hand; the
**modal** everywhere else: an `alertdialog` on the kit's overlay and panel, with no close and Escape
held (`onEscapeKeyDown` prevented), titled *Before you start*, a progress chip, and one card per check
— its glyph, title, state chip (*ready* · *not installed* · *not set up*) and detail; for a check that
is not ready, this platform's official lines each with **Copy** (`platformOf(navigator)` picks
`mac_os` · `linux` · `windows`), the *verify* and *sign in* sentences, **Open official docs**
(`openExternal`, never a link), the **fixes** as primary buttons (`api.setSettings("workspace", set)`
or `api.patchAgent(id, {harness, models})`, then a re-check — `offeredFixes`: a fix that names no
agent, no harness, no plan or no setting is no call and no button, and nothing is made up for what
the node did not say) and the **door** (*Open Settings › Capabilities › Harnesses*, *Open Settings ›
Decision Settings › Decision Making*, *Open Agents* — a Settings door says the rail's own path for
the panel it opens, `settingsLink.settingsPath`), which navigates and so turns the modal into
the banner. Settings › Capabilities › Harnesses shows the same lines and page for a missing compiled-in harness
(`HarnessRow.install`).

## `bisa doctor`

The same five checks in the terminal: `✓` ready, `✗` missing, `!` unready, each with its detail,
this platform's first official line and the page, and the labels of the fixes the desktop offers.
Through the node when one runs, else the embedded engine; `--json` prints the `Readiness`. Exit 1
while something is missing, 0 when everything is here.

## Invariant

**I62** — The platform never installs, updates or signs in to anything on the person's behalf and
never runs an install line: the gate, Settings › Harnesses and `bisa doctor` show the official
documentation's command to copy and open the official page; a fix writes only the platform's own
settings and agent records. A harness the settings name but the machine lacks is *not ready*
everywhere — the Decision-Making Agent's readiness included. Held by `crates/bisa-harness/src/install.rs`
(every hint has an official URL and a line per platform), `crates/bisa-engine/tests/it/readiness.rs`
(a machine with no git, no harness, the Decision-Making Agent unready and a core agent on an absent harness,
all against fakes — a `MockAdapter` that says whether it is installed, a git handle pointed at a
program that does not exist), `crates/bisa-node/tests/it/readiness.rs`, `crates/bisa-cli/tests/it/cli.rs`
(`doctor_*`), `desktop/src/shell/setupModel.test.mjs`, `desktop/src/scenarios/setup.test.mjs`.
