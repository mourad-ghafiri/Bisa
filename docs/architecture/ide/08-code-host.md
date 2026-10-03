# 08 — Code host

Pull requests, review, checks and merge from inside the IDE — on GitHub, GitLab and Bitbucket,
behind one trait, each **the machine's own CLI first and the host's API second**, so a person whose
`gh` is already signed in is signed in here too.

---

## One trait

The code host is a leaf crate, `bisa-codehost`, which depends on one crate of ours — `bisa-http`,
the one place an HTTP client is built. Every code
host is one implementation of a trait — the **Strategy pattern**:

```rust
#[async_trait]
pub trait CodeHost: Send + Sync {
    fn id(&self) -> CodeHostId;
    fn capabilities(&self) -> CodeHostCapabilities;
    fn detect(&self, remote: &RemoteUrl) -> Option<RepoRef>;
    // The same host, bound to one account — the login the checkout's git
    // config names. `self` for None or the same login; a clone otherwise.
    fn for_account(self: Arc<Self>, login: Option<&str>) -> Arc<dyn CodeHost>;
    // Whose credential this is — the connection check, with the organizations
    // it can see — and whose a candidate token would be, asked before it is stored.
    async fn account(&self) -> CodeHostResult<Account>;
    // `login` is the account the person says the token is — Bitbucket's Basic
    // credential cannot be checked without it; GitHub and GitLab ignore it.
    async fn verify_token(&self, token: &str, login: Option<&str>) -> CodeHostResult<Account>;
    // Read-only: is the repository there, and may this account push to it.
    async fn repo_access(&self, repo: &RepoRef) -> CodeHostResult<RepoAccess>;
    async fn create_pr(&self, repo: &RepoRef, req: PrCreate) -> CodeHostResult<PullRequest>;
    async fn get_pr(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PullRequest>;
    async fn list_prs(&self, repo: &RepoRef, filter: PrFilter) -> CodeHostResult<Vec<PullRequest>>;
    async fn checks(&self, repo: &RepoRef, number: u64) -> CodeHostResult<Vec<CheckRun>>;
    async fn submit_review(&self, repo: &RepoRef, number: u64, review: Review) -> CodeHostResult<()>;
    // Reading reviews and resolving threads: review threads and
    // resolution are GraphQL-only on GitHub, so the client has a GraphQL path.
    async fn pr_reviews(&self, repo: &RepoRef, number: u64) -> CodeHostResult<PrReviews>;
    async fn resolve_review_thread(&self, thread_id: &str, resolved: bool) -> CodeHostResult<()>;
    // A reply on a thread, as the bound account; resolving stays its own verb —
    // the engine composes "reply, then resolve" for *Reply and resolve*.
    async fn reply_review_thread(&self, thread_id: &str, body: &str) -> CodeHostResult<()>;
    async fn merge(&self, repo: &RepoRef, number: u64, strategy: MergeStrategy) -> CodeHostResult<MergeOutcome>;
    async fn delete_branch(&self, repo: &RepoRef, branch: &str) -> CodeHostResult<()>;
}

pub struct CodeHostCapabilities {
    pub draft_prs: bool,
    pub reviewers: bool,
    pub labels: bool,
    pub merge_strategies: Vec<MergeStrategy>,   // Merge | Squash | Rebase
    pub check_runs: bool,
    pub review_comments: bool,
    pub review_threads: bool,   // the review viewer and its Resolve buttons
    pub review_thread_replies: bool, // the Reply box on a comment, and an agent's pr_thread_reply
    pub delete_branch: bool,    // the merge's "delete the branch on the code host"
    pub review_events: Vec<ReviewEvent>, // which review events the host reports (a verdict, a comment, …)
}
```

**The pattern earns its place through `capabilities()`.** The IDE renders the pull-request form from
that value and never from a GitHub-shaped assumption: a code host without draft PRs shows no draft
toggle; a code host with only `Squash` shows no strategy select. That is what makes "add a code host without
touching the IDE" true rather than aspirational, and it is a test: a `FakeCodeHost` with every
capability off must produce a form with **no disabled controls on it** — absent, not greyed.

`CodeHostRegistry` picks an implementation by remote URL through `detect`, or by id (`by_id`) when
there is no remote to detect from — Settings asking GitHub about a token. The remote is a
`RemoteUrl` (`parse` is total: HTTPS, `ssh://`, `git@host:`, a bare path — each with its
`RemoteProtocol`), and an SSH alias is resolved to its hostname through `ssh -G` by the engine
before `detect` is asked ([04](04-git.md#remotes)). The host is then **bound to an account**
(`for_account`) — the `codehost.account` the checkout's git config resolves — before any request,
so which login opens a pull request is a fact a person can read in the Connection card and never a
guess.

**Three kinds, one shape.** `CodeHostKind` — `github`, `gitlab`, `bitbucket` — is an API shape and a
CLI, not an address: `github.com` and a GitHub Enterprise host are one kind, `gitlab.com` and a
self-hosted GitLab one kind. A `RepoRef`'s `owner` is the whole namespace — `acme`, or GitLab's
`acme/platform` — and `RemoteUrl` reads every segment but the last as the namespace, so a
subgroup is one repository, not a URL the crate cannot name. The public hosts detect by name; a
host of the person's own is named by the checkout's `codehost.kind` (`github` · `gitlab`), and the
registry's factory (`hosts::Hosts`) makes the same implementation at that address
(`https://<host>/api/v3`, `https://<host>/api/v4`; the CLIs take `--hostname`). Bitbucket Server is
another API and is not Bitbucket Cloud's; the Connection card says so.

**CLI first, API second.** Each kind is a [`Layered`](../crates/codehost.md) code host: its CLI
(`gh`, `glab`) when the kind has one, over its API (`reqwest`, REST — and GraphQL on GitHub for
review threads, which REST cannot resolve). Every operation asks the CLI; the CLI answers with the
credential it already holds — the one the person signed in with in their terminal — and the API is
asked only when the CLI had **no say**: not installed, not signed in, not signed in as the account
the checkout names, no verb for the operation, an answer this crate cannot read. When the host
itself refused through the CLI (a merge that cannot land, a review of your own pull request, a
pull request that does not exist), that refusal is the answer and the API is not asked again — for
a creation it could repeat the act. `id`, `capabilities` and `detect` are the API layer's: the
shape of a host does not change with the program that reaches it. Bitbucket Cloud has no CLI of
its own, so it is the API alone. A CLI is run through one port (`cli::CliRunner`) — argv only,
never a shell, every prompt off, every child time-boxed, a `*auth token*` answer a `Secret` the
moment it is read — with a `FakeCli` beside it, so nothing is ever tested against a program on the
machine; the node hands the engine the real runner (`EngineConfig::cli`), a fixture none.

Every API method maps to a documented endpoint (GitHub's below; GitLab's and Bitbucket's are in
[crates/codehost.md](../crates/codehost.md)):

| Method | GitHub |
|---|---|
| `account` / `verify_token` | `GET /user` — the login, and a classic token's scopes from `X-OAuth-Scopes`; then `GET /user/orgs`, best effort, for the organizations the token can see (a 403 without `read:org` is an empty list, not a failure) |
| `repo_access` | `GET /repos/{o}/{r}` — `permissions.push`; a 404 is *not found*, not an error |
| `create_pr` | `POST /repos/{o}/{r}/pulls`, then (best effort) `…/pulls/{n}/requested_reviewers` and `…/issues/{n}/labels` |
| `get_pr` | `GET …/pulls/{n}` — `merged_at` makes it *merged*, whatever `state` says |
| `list_prs` | `GET …/pulls?state=open\|closed\|all&head=owner:branch`; *merged* is *closed* filtered on `merged_at` |
| `checks` | `GET …/pulls/{n}` for the head SHA, then `GET …/commits/{sha}/check-runs` — GitHub keys checks by commit |
| `submit_review` | `POST …/pulls/{n}/reviews` with `event` and `comments[{path, line, side: RIGHT, body}]` |
| `pr_reviews` | one GraphQL query: `reviews` and `reviewThreads` with their comments |
| `resolve_review_thread` | the `resolveReviewThread` / `unresolveReviewThread` mutations |
| `reply_review_thread` | the `addPullRequestReviewThreadReply` mutation, on the same thread node id |
| `merge` | `PUT …/pulls/{n}/merge` with `merge_method` |
| `delete_branch` | `DELETE …/git/refs/heads/{branch}` — a bodiless 204; 422 is *already gone* |

A status is read for what it is: 401 and a plain 403 are `NotAuthenticated`, 404 `NotFound`, 405 ·
409 · 422 `Refused`; a 403 or 429 carrying `retry-after` or `x-ratelimit-remaining: 0` is GitHub's
**rate limit**, a `Transport` error that says how long to wait — retried by the engine like any
transport failure, never mistaken for a bad token. GitLab reads 400 · 405 · 406 · 409 · 422 as
`Refused` — a field's reasons read out (*base has conflicts, is behind*) — and a 429 as its rate
limit, with `retry-after` when it sends one and *in a minute* when it does not; Bitbucket reads
400 · 409 · 422 · 555 as `Refused` and a 429 as *try again in a minute*. On every host an answer
that is not the shape asked for — a proxy's page where a pull request was expected — is a
`Transport` error that says *answered something unexpected*. **A list is one page**: the open pull
requests a picker offers are the 50 newest, GitHub's organizations the first 100, a merge request's
notes and a Bitbucket pull request's comments the first 100 — no `Link` header is followed over the
API, while `gh api --paginate` follows every page over the CLI. The base URL is a field
(`GitHubApi::with_base_url`, and the same seam on `GitLabApi` and `BitbucketApi`), so every API
implementation is tested against a stub server on the loopback interface (`tests/it/github.rs`,
`tests/it/gitlab.rs`, `tests/it/bitbucket.rs`) and never against a host; the CLI layers are tested over
`FakeCli` (`tests/it/gh.rs`, `tests/it/glab.rs`) and never over a program.

---

## Credentials

The CLI layer needs no credential of ours: it speaks with the one the person gave it. The API
layer's credential is a **token**, from a chain of four **per kind**, **keyed by account**; first
match wins, and none is ever logged, journaled, snapshotted or sent to the webview:

1. the kind's environment variable — `BISA_GITHUB_TOKEN`, `BISA_GITLAB_TOKEN`,
   `BISA_BITBUCKET_TOKEN` — the operator's override, whatever follows; the kind's panel says
   so when it is set;
2. the token store — one `0600` file per account, `identity/codehost/<kind>/<login>.token`, beside
   a non-secret `accounts` list, or the OS keyring (service `bisa`, entry
   `codehost:<kind>:<login>`) when `BISA_KEYSTORE=keyring` — filled from Settings › Git & code
   hosts › *the kind*, which sends a token once over the authenticated socket and never reads it
   back. A token is stored **after** the host accepts it and under the login the host answers —
   nobody types who a token belongs to (Bitbucket's API token is a Basic credential and is checked
   beside the login the person gives, the one exception). Which stored token answers is the
   **account** the checkout resolves: `codehost.account` in a profile or a local pin, else the
   kind's global default `codehost.<kind>.account` ([04](04-git.md#profiles-by-organization)); none
   named and one stored → that one; two stored and none named → a refusal that names both and the
   settings panel, never a guess;
3. **the kind's CLI, signed in on this machine** — asked for the named account's token, or its
   active one (`gh auth token --hostname <host> [--user <login>]`; `glab auth status --show-token`)
   through the `creds::CliToken` port the CLIs implement. Asked, never read: nothing of ours opens
   `~/.config/gh` or `~/.config/glab-cli`, and the guard keeps both denied to every agent
   ([11 — Security](../11-security.md)); the token lives in memory for one request;
4. **git's own credential helper** — the credential `git push` over HTTPS already uses. The engine
   asks git (`git credential fill`, `protocol=https` / `host=<host>` on stdin) through its own
   `Git` handle (`bisa_vcs::Git::credential_fill`): argv only, the hardened environment
   (`GIT_TERMINAL_PROMPT=0`, no askpass), time-boxed and terminated on expiry, so a helper that would
   prompt fails at once. Git runs whatever helper its config names — osxkeychain, `gh auth
   git-credential`, a credential manager — and answers, or does not. Nothing of ours reads a file of
   git's or a key; the codehost crate never touches SSH (the API is HTTPS, and only an HTTPS
   credential is asked for), and `bisa-ssh`, which does speak to `ssh` for the transport, reads
   **public** material only ([crates/ssh.md](../crates/ssh.md)). The codehost crate depends on
   `bisa-http` alone among our crates — never on `bisa-vcs` — so this source is a port there (`creds::GitCredentials`, per host) and an adapter
   in the engine (`codehost::GitHelperCredentials`).

Explicit before ambient: a token a person set or stored is the one they meant; the CLI's and git's
are the ones they already have.

Whether a credential *works* is the host's to say, so **connected** is a request, not a guess —
and **what the machine already has** is read before anything is asked. `GET /codehost/{kind}/health`
answers, offline, what Settings shows: the kind's CLI (`cli`: installed and where, its version, the
accounts it is signed in as and which is active — from `<cli> --version` and `<cli> auth status
--hostname`, the one program asked, time-boxed; `null` for Bitbucket, which has none), every
stored login with its `source` (`file` · `keyring`), `env_override`, `helpers` (every helper git
is configured with — what pushes and pulls over HTTPS use, from non-secret config) and
`helper_username`, `cli_login`, `store`, the `default` from the global `codehost.<kind>.account`,
who would answer with nothing named (`resolves`: `{login, source}` — the chain walked once, no
request), and the host's `token_page`. `GET /codehost/{kind}/login` is the way in: `{kind: cli}`
when the CLI is installed — the desktop shell opens a terminal running the CLI's own browser
sign-in (`gh auth login --hostname <host> --web --git-protocol https`; `glab auth login
--hostname <host>`), because the CLI wants a TTY for its one-time code and *Press Enter*, and the
credential lands where the CLI keeps it, where this platform then finds it; the argv is the shell's
closed table, the webview names a kind and a host and never a program ([01](01-trust-boundary.md));
`{kind: install, hints}` when it is not — Homebrew, apt, winget, the install page — with a token as
the other door; `{kind: token}` for Bitbucket. `POST /codehost/{kind}/accounts/{login}/check` asks
the host as that account — through the CLI when it holds the login, else the API with the stored
token — and answers a `Connection`: `connected` (the login, the scopes, `missing` — the scopes the
kind needs that a token lacks — `recommended_missing`, named when absent, never a verdict — and
`organizations`, groups or workspaces; a token that lists no scopes earns no verdict), `refused`,
`unreachable`, or `no_token` without a request. Adding an account (`PUT /codehost/{kind}/accounts`,
`{token, login?}`) verifies it first — a token the host refuses is a 401 and is never kept — and
stores it under the login the host answered; `DELETE …/accounts/{login}` forgets one;
`PUT /codehost/{kind}/default` sets or clears the kind's default. Settings › Git & code hosts ›
*GitHub* · *GitLab* · *Bitbucket* is one panel three times (`CodeHostPanel.tsx` over
`codeHostAccountsModel.mjs`): *connected* or *not signed in* as a chip, one sentence on who
requests go as and from where, the CLI row (*GitHub CLI 2.63 · signed in to github.com as
@octocat*, or *not signed in* with **Authenticate**, or *not installed* with the install commands
to copy), every stored account with **Check**, *Make default* and *Forget…*, the default account,
the environment override when it is set, what git itself does for pushes over HTTPS, and *Add a
token* — the login field beside it for Bitbucket — with the scopes the kind needs and a door to
the host's token page. The panel reads again on `git_setup_changed`, when the window regains focus
(the person comes back from the browser) and when a sign-in terminal for the kind exits. A
checkout's *Check* in the Connection card additionally asks `repo_access` for the account it
resolves — *found, can push* — read-only.

A missing credential is `CodeHostError::NotAuthenticated`, rendered as an instruction naming the
ways in — the panel included — not a stack. Every test of this chain runs against a fake helper in
a temp `GIT_CONFIG_GLOBAL` (with `GIT_CONFIG_NOSYSTEM`), a `tempfile` token directory and the
loopback stub — never a machine's helpers, keychain, tokens or remote.

---

## Outward actions pass the `Publish` gate

Opening a pull request already passes `Publish`. Merging is an outward action that cannot be taken
back, which is the stated test for the gate,
so it passes too. Under the project's policy:

| `project.publish` | In the IDE |
|---|---|
| `auto` | proceeds |
| `gated` | a `Publish` gate opens in the Inbox; the route answers `202` and the banner offers *Decide it in the inbox*. Nothing exists on the code host until it is approved; the approval is journaled like any other |
| `manual` | **refused** (`409`, code `publish_manual`): nothing publishes from Bisa, agent or person; a person runs git. The banner names the policy and its *Change the policy* button opens About › Settings, where the policy is set |

**What was approved and did not go out is said.** A `202` is the last word the caller hears: the
gate opened, nothing had left the machine, and the act goes on in the node once a person says yes.
When it then fails — the remote refuses the push, the code host the pull request or the merge, the
network is gone — nobody is waiting on the call, so the act says its own end
(`engine::projects::Asked::heard`, behind every act that passes the gate: a push, a push with
lease, a remote branch's deletion, a pull request opened, a merge): `workstream_publish_failed
{ workstream, project, what, reason }` on the bus and in the feed, a line of the home's journal,
and a **notice on the workstream's row of the Inbox** (`publish_failed`, trouble) — `what` the act
as the gate asked it, `reason` the refusal in words, redacted first since a remote's refusal can
quote the address it was reached at. The record does not move: the lifecycle still offers the act.
The desktop hears the frame for the checkout it stands on (`publishOutcome.publishFailure`, the
banner's `failed` kind — `views/_work/publishOutcome.test.mjs`) and draws the notice **under the
step that still offers the act**: *Approved, and it did not go out* — `what` and `reason` as they
came, the node's sentence never matched as English — with the verb beside it; the Git tab's sync
bar shows the same banner for a push, so the two never disagree, and the rows are read again since
nothing moved. A project that publishes by itself says nothing more than its route's own answer —
its caller is there to hear it.

`Publish` never defers to an assignment — unchanged. Over HTTP the routes keep the existing
contract: `200` finished, `202` waiting on the gate with nothing having left the machine, `409`
refused — with a `code` naming which refusal (`publish_manual`, `publish_no_goal` for a gated
project whose workstream has no goal to ask on, `publish_declined`, `nothing_to_publish` for a branch
with no commits beyond its base, `workstream_state` for what the table refuses,
`pull_request_state` for a workstream asked from a pull request that is not open), because a status
alone once read every one of them as the manual policy.

**Opening pushes first.** A pull request is opened on pushed work. A branch that is not on the
remote yet is pushed by the same call, under **one** gate whose question says so ("push X and open a
pull request for it"); the record is reconciled with the checkout first, so a commit made in a
terminal counts. The desktop's one button reads *Push and open pull request* until the branch is
pushed, then *Open pull request* — one name for one act, in the stepper and in the form.

---

## Surfaces

Everything below is the **lifecycle in the Workstreams occupant** (`PullRequestLifecycle.tsx`,
under the checkout's header, `⌘⇧U`), drawn on a workstream with a branch beside the primary —
never on the primary, a copy or a plain folder. Managing the checkout — its name, its sessions,
closing it, its path copied from the menu — is the same panel's, above the lifecycle
([07](07-workstreams.md#the-card)); the lifecycle draws the one act under its own step — the step
in hand — and nothing else, so the panel never shows a merge button before its time, and never
above the review.

| Surface | What it shows |
|---|---|
| **The lifecycle** | the panel's spine (`prLifecycleModel.lifecycle`): *Commit · Push · Open pull request · Checks · Review · Merge · Clean up*, each **done**, **current** (the one step in hand, the accent dot), **waiting** (in flight and nobody's act — checks running, *reading the code host…*, *the code host is still checking* — a ring that pulses), **blocked** (with the reason: *a draft*, *2 checks failing*, *conflicts with main*, *commits not pushed*, *closed on the code host*) or to come, and the **one legal act under its own step**: the model names the step each act belongs to (`cta.step` — *Push and open pull request* under Push, *Merge* under Merge, *Clean up branch* under Clean up) and makes it the step in hand (`current`), so the merge control is drawn under *Merge* and nowhere else — never under *Checks* because the checks happened to be running, never under *Open pull request* because the host closed it; a blocked act still sits under its step, off, with the reason on the row once. Checks still running are a **caution** the merge names, first (*checks still running*), not a block — every code host merges through them. **Each step hosts its own surface under its row** (`LifecycleStepper`'s `render`) — the pull request card under *Open pull request*, the review stage under *Review*, the merge under *Merge* — so the panel reads in the order the work happens — the review above the merge, though the review is optional and never gates it. A branch with no pull request yet shows the stepper alone with its first legal act — the panel is where a pull request starts |
| **New pull request** | `PrForm`: title (prefilled from the branch tip's commit subject, `prTitleFrom`), body (Markdown, growing with what it holds; ⌘Enter opens), draft toggle, reviewers, labels — each control present only when `capabilities()` says so, absent otherwise. **Suggest** (the agent glyph, as on the commit composer) asks the General Agent for the title and the body from the branch's commits and its diff against the base: `POST /workstreams/{wid}/pr/suggest` → `projects::suggest_pull_request` (`git log base..HEAD`, at most 50 subjects, and `git diff base...HEAD` cut to the commit suggester's budget; the same read-only session, no MCP servers, ninety seconds), always 200 with `{suggested, title, body, error}`. The draft lands in the fields to be read and edited (`prFormModel.prSuggestionOutcome`): a field typed in while it was asked keeps what was typed (`applyPrSuggestion`), *Undo* puts back what the fields held, closing the dialog drops the ask, and a branch with nothing beyond its base is refused before anyone is asked |
| **Pull request** card | under *Open pull request*: number and title linking to the code host, `head → base`, the author, draft and conflicts chips, and — when the code host reports check runs — the checks' one-line summary as a chip (*3 passed · 1 failed*): the card is the record, the runs themselves are the Checks step's. The card stays after the merge: the record keeps the pull request it merged through |
| **Checks** step | under *Checks*, once a pull request exists (`ChecksStep.tsx`; nothing on a code host without `check_runs` — the row already says so): every run with its conclusion, its name as the door to the log and the host's summary, and on a run that **failed** (`failedCheck`: completed as a failure, a timeout, an action required or a cancellation — the one definition, which `checksSummary` reads too) **Fix with ▾** (`AgentMenu`): the agents the checkout can reach, the last one a check was handed to first (`draftKeys(scope).checkAgent`, starting on the comments' fixer). Picking one hands the failure over — `checkFixPrompt`: the check's name, conclusion, summary and log, *reproduce it in this checkout first, fix the cause, commit on this branch, do not push or merge, reply here* — as a `ReviewRun { kind: check, check }` and the run is followed under the list by the same `ReviewRunLine` a fix has: *fixer is fixing check ci / lint…*, what it is doing, *Stop*, then *fixer fixed check ci / lint — 2 new commits on the branch, to keep or discard in Git › Changes.* with the door. The row reads *fixing…* meanwhile |
| **Review** step | under *Review*, **from the moment the branch exists** (`ReviewStep.tsx` over `reviewStepModel.mjs`, `prReviewModel.mjs` and `agentReviewModel.mjs`) — one surface, two targets: **before a pull request** it reviews the **branch against its base** (the agent request alone, `AgentReviewRequest` with a *branch* target: the agent is asked, in the checkout's conversation, to run `git diff <base>...HEAD` in the checkout, judge it and reply there with its verdict in one line then findings citing files and lines, changing nothing — `agentReviewModel.branchReviewPrompt`; no code host is touched; the run ends with its session, kind `branch`; a branch with no commit beyond its base says *Nothing to review yet* with the door to Git › Changes); **after one**, the pull request. **Optional and never the panel's act** — its row is a todo that says *optional — an agent's, yours, or none* until anyone reviews, then done with who and what they said; the merge is offered either way: one sentence on where the review stands (`statusLine`), then the parts, in the order they usually happen. **Agent** — `AgentReviewRequest`: an agent from a plain `Select` (`AgentSelect.tsx` over `agentChoices` — the agents the thread can reach, no search, no teams; kept in the checkout's session, so a tab switch keeps it), an **optional line of words** that leads the request, and *Review with <agent>*: the request goes into the checkout's conversation as a mention and **the step follows the run right there** — nothing navigates. The run is `useReviewRun`'s (one owner: the draft in the checkout's session, `draftKeys(scope).run`; its session found on the roster by `reviewRun` — live or ended, the latest of the agent's in this workstream started at or after the ask; the verbs *ask · stop · dismiss*; the agent's reply, its latest message after the ask in the checkout's conversation, read on every conversation frame and named through the workspace's pubkey map, `agentReply`) and `ReviewRunLine` draws it under the control in three moments: **starting** — the mark and *general-agent is reviewing…* before the session shows (`GRACE_SECS`), or *getting ready…* while it is up and idle before its first turn — nothing runs, so no *Stop*; **live** — the session runs or waits on the person (`RunState.live`, by `sessionState.isStoppable`): the mark in the session's state, what was asked, how long (`LiveDuration`), and under it what the agent is doing this moment (`activityWords` over `headlineOf`: *running Bash · cargo test*, *thinking…*, *· 2 sub-agents*), *Stop* (`api.abortSession`, recorded as *stopped* — offered then and only then) and a quiet icon door to the Agent panel — a session **waiting on the person** says so in the caution's colour with *Answer in Agents*, the one time the panel is named in words; **ended** — the outcome sentence (`outcomeWords`: *general-agent reviewed the branch:*, *fixer fixed 3 comments — 2 new commits on the branch, to keep or discard in Git › Changes.* — the count from the branch's commits beyond its base at the ask, `run.ahead`, against now — *general-agent was stopped before reviewing.*, *… failed — <reason>.*, *…'s session ended before anything landed.*), the reply folded under it (`FoldedText`), *Open Git › Changes* after a fix, and × Dismiss. A run ends with its session's terminal state, or — the usual end of a branch review or a fix — when the session sits **idle** with the agent's reply in the thread (`reviewRun`'s `replied`, from `agentReply`), or has sat idle the whole `GRACE_SECS` with none; a finished agent never holds the step busy. The first time the step sees a run end (`how`: `done · aborted · failed · stopped · landed · gone`) it **records the end into the run** (`ended`), so the outcome outlives the roster dropping the session, and reads the code host again. A pull request's review that **lands** clears its run instead: the agent reviews through the `pr_review_submit` MCP tool as a comment, its verdict in the words, and the **engine signs** the body's first line with the agent's id (`codehost::AGENT_REVIEW_MARK`, `Reviewer::Agent`) — one credential posts every review, so the code host's author cannot tell the two apart and the body does; `reviewerOf` reads it back, and the row then shows the agent, when, its words folded to a first sentence and *Review again* — the review row is the outcome. **You** — one optional box and the buttons the code host takes, in a fixed order, the first one primary (`reviewButtons`): **Approve** · **Submit review** (a comment) · **Request changes**; a comment or a change request with the box empty is off with its reason (`wordsReason`), an approval never needs words; the connected account opened every pull request the platform opened, and a code host refuses its author's approval or change request, so on an own pull request *Submit review* is the one button and the row says why (`allowedEvents`, `ownPrNote`); GitLab has no *Request changes*; given, the row shows your verdict, when, and *Review again*. Under them **Comments** — the reviewers' inline comments on the code host (its *threads* on the wire, `ReviewThread`; the desktop's one word is *comment*), grouped by file, open first (`commentsByFile`), the count and *3 open · 2 resolved* on the header (`commentsSummary`), each folded to *line 12 · open · @alice This rounds twice.… · +2* with its replies unfolding under it — each reply named by who left it, an agent by its id (`replierOf` reads `AGENT_REPLY_MARK`, the engine's signature on every reply an agent leaves, as `reviewerOf` reads the review's), its words without the signature line (`replyWords`). Every open comment's row offers, on hover, its **three hands**: **Fix with ▾** (`AgentMenu`) — the agents the checkout can reach, the last one a comment was handed to first (`draftKeys(scope).fixAgent`, starting on the reviewer), so **each comment goes to an agent of the person's choice**; picking one posts `fixPrompt` for that comment — its place and its thread id, *commit on this branch, do not push or merge, reply on the thread with `pr_thread_reply` and resolve what you addressed* — as a `ReviewRun { kind: fix, count: 1, comment, ahead }` and remembers the agent; **Reply** — a box for your own words, sent as *Reply* or *Reply and resolve* (`POST …/pr/threads/{id}/reply` `{body, resolve}` → `engine::codehost::reply_review_thread`, the person's words untouched, the thread resolved in the same act when asked; the box exists only where the code host has `review_thread_replies`); and **Resolve** / **Reopen** (`POST …/pr/threads/{id}/resolve`). The header's **Fix all 3 open ▾** (`fixAllLabel`, the same `AgentMenu`) hands every open comment to one agent. The agent handed a comment answers on the thread itself — `pr_thread_reply`, signed by the engine, `resolve: true` for what it addressed; `pr_thread_resolve` alone reopens or resolves — so the code host reads what happened without anyone relaying it. The row reads *fixing…* while its comment is in hand, and the run is followed at the **top of the section**, held open, by the same `ReviewRunLine` — *fixer is fixing 1 comment…* · *running Edit · src/cart.rs* · *Stop*, then the outcome with the commits it added and its reply — and its commits land on the branch — accept by merging, reject by discarding in Git › Changes. **One agent at a time in the checkout**: while any run is live — a review, a comment, a check — every other *Fix with* on the step, and the Checks step's, gives way to who is busy (`busyWords`: *alpha is fixing line 12*, *alpha is fixing check ci / lint*, *alpha is reviewing*); replying and resolving stay the person's whatever runs. Then **Reviews by others**, one line per other account that reviewed on the code host. **The step reports, it never gates** (`reviewFacts.given`, `verdict`): with no review the row reads *optional…*; with any — an agent's alone, yours alone, another account's — it is done and says *reviewed by general-agent and you*, *approved by @alice*, or *changes requested by @carol*; a standing request for changes is the one thing the merge carries onward, as a caution |
| **Merge** | the lifecycle's act (`MergeControl`) under *Merge*, offered **whenever the pull request is open** — a review is optional and never holds it. **Merge** with the strategy as its caption, a menu only when the code host offers more than one, starting on the project's `git.merge_strategy` (default `merge`); the confirmation says in words how the branch lands and whether the branch on the code host goes with it (`git.delete_branch_after_merge`, default on, when `capabilities().delete_branch`); the Publish gate. Checks still running — *checks still running* — comments still open — *3 comments open* — and a standing request for changes — *changes requested by @carol* — are the **cautions**, in that order: the row wears them, the button stays live, the confirmation lists them under *Worth a second look* and reads *Merge anyway*. Only what the code host itself cannot merge blocks; a mergeability the host is still computing is *waited on* — the row wears the ring and the note, the button is off with the reason — not a block |
| **After the merge** | the lifecycle's last step, **Clean up** (*Clean up branch*): one dialog — pull the default branch, delete this workstream's checkout and branch, return — driven by `workstreams.cleanup`, `workstreams.after_merge` and `git.pull`, never while a session runs on the primary ([07](07-workstreams.md#after-a-merge)) |
| **Workstream card** | the linked PR's number as a chip linking to the code host, from the record — no code host call ([07](07-workstreams.md#the-card)) |
| **New workstream › Pull request** | the dialog's *Start from* offers the **open pull requests** on the code host behind `origin` (`GET /projects/{pid}/prs` → `engine::codehost::list_open_prs` → `CodeHost::list_prs` with `PrFilter { state: open }`, read fresh, *Refresh* on the field): pick one and the workstream opens on its head, tracking it, with its base as the base and the record **born `pr_open`** (`PrAdopted`, [07](07-workstreams.md#where-a-workstream-starts)) — the lifecycle picks up where the pull request stands: the card, the checks, the review, the merge. A pull request not open is `409 pull_request_state` with `detail.number` and `detail.state`; a head that is not on `origin` (a fork's) fails at the fetch and says so; a project with no code host shows the caution with the door to About › Settings |
| **Settings › Git & code hosts › GitHub · GitLab · Bitbucket** | one panel per kind (`CodeHostPanel.tsx` over `codeHostAccountsModel.mjs`): the health — the CLI, the stored accounts, git's helper, who requests go as — **Authenticate**, *Check*, *Add a token*, *Forget*, the default account — §Credentials |
| **About › Settings, the Connection card** | which account this checkout's pull requests will be opened as and where that comes from — a pin, a profile, the kind's default, the environment, the only one stored, **the CLI signed in on this machine**, **git's credential helper** — with the `Select` to pin one (the stored logins and the one the CLI or git answered with), and the `account_outside_owner` caution when the account's organizations do not include the remote's owner ([04](04-git.md#the-repository-under-about--settings)). The `no_account` caution fires only when none of those answers: a machine whose `gh` or git already knows the host is not one |
| **New project, the code host line** | under the URL of a clone or the folder of an import (`CodeHostLine` in `NewProjectDialog.tsx` over `projectCodeHostModel.mjs`, `POST /codehost/inspect`): *GitHub · acme/web · SSH via github-acme · as @ada* — the host, the repository, the protocol, the alias `ssh -G` resolved, and the account it would speak as — with a `Select` of every account that could (the profile's, the kind's default, the CLI's, a stored one, git's helper's; the suggested one first) whose choice becomes the repository's `codehost.account` through the creation body's `git_config`; when nobody is signed in, the caution and a door to the kind's Settings panel; a host nobody named a kind for says so and names `codehost.kind`. Offline: the node reads git, the CLI's status and its own store, and nothing reaches the remote. The project record keeps what was found (`Vcs::Git.code_host`: the kind, the instance, `owner/repo`) |

The code host emits no events, so the lifecycle asks it again — the pull request, its checks and
its reviews together — on `git.fetch_interval_secs` while a pull request exists and the panel is
on screen (`usePullRequest(base, { following })`, on the shared visibility-gated clock; `0` turns
the polling off), on the panel's *Refresh* and on the read line's door; and **while an agent run is
live it follows it**: every `FOLLOW_MS` (15 s, above the engine's code host cache — `cache.codehost.ttl_ms`, ten seconds unless set —
whatever the fetch interval says — the person asked for it by asking the agent) the code host's side
*and* the checkout's status are read, so a fix's commits move the Commit and Push rows within
seconds; when the agent **replies** in the thread, the same read; when the run **ends**, the record,
the status, the pull request, its checks and its reviews are read at once, so every row settles
together (`useReviewRun`'s `onMoved`, which is why the lifecycle — not the Review step — owns the
run). Under the spine a line says when the code host was last read — *code host read 12 s ago*,
*following general-agent · read 5 s ago* (`readWords`, on `usePullRequest.readAt`, the latest of the
three reads) — with the refresh door. Never otherwise. The checkout itself is read once (`useWorkstream`) and
handed to the lifecycle; the code host is asked only on top of it, and only for a branch with a
pull request.

---

## Conflicts

A merge, rebase, cherry-pick or revert that stops on conflicting content returns a typed
`VcsError::Conflict` carrying the conflicted paths and the operation left half-done. From there the
Git tab drives it to its end in the person's words — the **Resolve card** above every Git view
naming the sides by branch and commit from git's own directory, listing the conflicted files with
their kinds as a checklist, and holding *Continue*, *Skip* and *Abort*; the **conflict document**
resolving each file block by block (*Keep mine · Keep theirs · Keep both · Edit…*, a Review of the
whole result, *Mark resolved* saving it and staging the path); a path one side deleted or a binary
settled by one of two explicit outcomes through git (`resolve {take}`, consented); the dialogs
looking ahead before a merge or a rebase runs. All of it is [04 §Conflicts, continued](04-git.md#conflicts-continued);
*Abort* restores the tree the recovery ref pinned before the operation began. A pull request the
code host reports as not mergeable (`mergeable: false`) is the same conflict met on the base branch:
the way through is a merge of the base into the workstream, or a rebase onto it, from the Branches
view — and the same card and document.

---

## CLI parity

`bisa workstream push|pr|pr-view|pr-checks|pr-review|pr-merge <workstream>` over the same engine
functions — **through the node when one runs**, an engine of their own otherwise — with the same
inline `[y/N]` for the gate, or `--yes`: asked of a running node, the gate is decided through its
home's own door (`POST /goals/{id}/decide {gate}`) and the verb **waits for the act's end** on the
node's events — the workstream moved to where the act lands, or *approved, and it did not go out*
with the reason; with no terminal and no `--yes` it says the gate is open and that nothing was
pushed; `bisa git account list|add|check|forget|default
--host github|gitlab|bitbucket`, `git health --host` (the same `CodeHostHealth` Settings reads,
printed) and `git login --host` (the plan printed: the command to run in this terminal, the
install hints, or the token page) over the same doors. The CLI is a first-class surface and a code
host capability that only the desktop could reach would be the second feature to fall out of that
shape.

---

## The three, and a fourth

GitLab's vocabulary maps onto the crate's without loss: a *merge request* is a `PullRequest` (its
`iid` the number), a pipeline's jobs are the check runs, approvals and top-level notes are the
reviews, resolvable discussions with a diff position are the threads (a thread's id is
`owner/name#iid#discussion`, since a discussion is resolved by project and merge request; a reply
is a note on the discussion, `POST …/discussions/{id}/notes`, and inherits its resolvability), a
merge squashes when asked and there is no rebase merge; GitLab has approvals and comments but no
*request changes* review, and `capabilities().review_events` says so — the desktop offers only
those. Bitbucket Cloud: a pull request's `id`, the commit statuses on its head as checks, the
participants who approved or requested changes as reviews, the inline comments as threads (one
comment and its replies, resolved when Bitbucket says so; a reply is a comment whose `parent` is
the thread's root comment — the id the thread carries), `merge_commit` or `squash`; it says
nothing about mergeability until the merge is asked for, so the record reads *mergeable* and a
refusal is relayed. The desktop says the host's own words through `codeHostWords.mjs` — *merge
request* on GitLab, *pull request* elsewhere — and never *GitHub* for a host that is not.

A fourth kind is a bounded job: one `CodeHostKind` variant with its public host, environment
variable and token page; one API file implementing `CodeHost` with a base-URL seam for the loopback
stub; a `CodeHostCli` over the port when the host has a CLI, or `Layered::api_only`; one line in
`hosts::Hosts`; a settings tab and a word in `codeHostWords.mjs`. A profile by organization already
names its `host`, and the SSH crate already greets `codeberg.org`, so the git side needs nothing
new. Five parallel `linked*PR` fields for five code hosts would be a compatibility artefact. One
trait, one type.
