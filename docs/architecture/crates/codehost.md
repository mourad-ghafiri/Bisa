# bisa-codehost

Pull requests, checks, reviews and merges behind one `CodeHost` trait — the Strategy pattern — for
three kinds of code host: **GitHub, GitLab and Bitbucket**. Each is one shape, `Layered`: the
machine's own CLI (`gh`, `glab`) is asked first and the host's REST or GraphQL API is asked when the
CLI has no say — not installed, not signed in, not signed in as the bound account, no verb for the
operation. Bitbucket Cloud has no CLI and is the API alone. A code host is **bound to one account at
a time** (`for_account`): the CLI layer runs as that account with a token asked of the CLI for one
command and held in memory; the API layer's credential is a token read through one chain per kind —
the environment, the stored token of the bound login (or the only one stored), the CLI's own token,
git's credential helper for the host — never logged, journaled, snapshotted, sent to the webview or
returned by a route; whether it works, and which organizations, groups or workspaces it can see, is
the host's to say (`account`). No test and no production path reads another program's files: a CLI
is *asked*, through the crate's `CliRunner` port, the way git's helper is asked through
`GitCredentials`.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | `CodeHostKind { GitHub, GitLab, Bitbucket }` (serde `lowercase`; `id()`, `as_str()`, `label()`, `public_host()`, `env_var()` — `BISA_<KIND>_TOKEN`, `cli()` — the `CliProgram` or none, `token_page(host)`, `of_host(host)`, `FromStr`, `Display`); `trait CodeHost { id, capabilities, detect(&RemoteUrl), for_account(self: Arc<Self>, login) -> Arc<dyn CodeHost>, account, verify_token(token, login), repo_access, create_pr, get_pr, list_prs, checks, submit_review, pr_reviews, resolve_review_thread, reply_review_thread, merge, delete_branch }` — a reply on a thread as the bound account, behind `review_thread_replies`, resolving left to its own verb so the engine composes *reply, then resolve*; — `for_account` is object-safe and answers `self` for `None` or the same login, a clone bound to the login otherwise; `verify_token` takes the login Bitbucket's Basic credential needs; `RemoteUrl { raw, protocol, user, host, port, path, owner, name }` — **`raw` and `user` never carry what a URL's userinfo hides**: the value is an API answer and rides in refusals, so an `https` URL's userinfo reads `***` (the user may itself be a token) and an `ssh` URL keeps its user and loses a password; nothing runs git with it — with `RemoteProtocol { Https, Ssh, Scp, Local, Other }` — `parse` is **total** over a non-empty string, and **`owner` is the namespace**: every path segment but the last joined by `/`, so a GitLab subgroup (`acme/platform/web` → `acme/platform` · `web`) and a Bitbucket workspace fit one field — `repo_ref(host)`, `is_on(host)`, `with_host`, `summary`; `CodeHostId`, `CodeHostCapabilities` (incl. `review_threads`, `delete_branch`, **`review_events: Vec<ReviewEvent>`** — the verdicts the host takes, GitLab having no *request changes*), `RepoRef` (`slug()` = `owner/name`), `PrState`, `PullRequest`, `PrCreate`, `PrFilter`, `CheckRun`, `ReviewEvent`, `Side`, `ReviewComment`, `Review` (`words()`, `needs_words()`, `is_verdict()`), `ReviewSummary`, `ReviewThread`, `ReviewThreadComment`, `PrReviews`, `MergeStrategy`, `MergeOutcome`, `Account { login, scopes, organizations, name, email, id }`, `RepoAccess { found, push }`, `Connection` (`Connection::of(outcome, required, recommended)`, `login()`), `CodeHostError`; `trait CodeHostFactory { at_host(kind, host) }` (a self-hosted instance made on demand); `CodeHostRegistry` (`new(code_hosts)`, `with_factory`, `detect(&RemoteUrl)` by public host, `detect_as(&RemoteUrl, kind)` for a host `codehost.kind` named, `by_id`, `by_kind`, `all`, `ids()`) |
| `src/cli/mod.rs` | **the CLI port and what is read from a CLI** (`Cli { timeouts, http }` — `with_http` hands the engine's clients in for the environment the programs get: the same proxy the API path uses, set then removed after `HARDENED_ENV`, so `gh` and `glab` leave this machine the way the API does): `CliProgram { Gh, Glab }` (`binary`, `label`, `kind`, `install_hints() -> InstallHints { brew, apt, winget, url }`, `login_args(host)` — the browser sign-in argv the desktop shell runs, never this crate), `trait CliRunner { installed(program) -> Option<PathBuf>, run(program, args, env, stdin, timeout) -> Output }` with `Cli` the real one (`which` for presence; a hardened environment — `GH_PROMPT_DISABLED`, `GH_NO_UPDATE_NOTIFIER`, `GH_PAGER`, `GLAB_CHECK_UPDATE`, `NO_COLOR`, `PAGER`, `GIT_TERMINAL_PROMPT=0`, `TERM=dumb`, `LC_ALL=C`; `spawn_blocking`, a deadline, the child terminated at it), `Timeouts`, `Output` (its `Debug` redacts stdout, since a token may be there), `CliError`; `CliProbe { installed, path, version, accounts: Vec<CliAccount { host, login, active, protocol }>, detail }` from `parse_version`, `parse_gh_auth_status`, `parse_glab_auth_status` — pure parsers over the CLIs' text; `CliStep<T> { Answer(T), Host(CodeHostError), Fallback(Fallback) }` with `Fallback { NotInstalled, NotSignedIn, NoSuchAccount, NoVerb, Unparsable }`, `CliOutcome`, `classify_failure` (exit code and words → the host's refusal or a reason to ask the API); `trait CodeHostCli` — the trait's operations, each answering a `CliStep`, plus `probe()` and `token(login)`; each pipe drained on its own thread and a read that fails is `Failed`, never a truncated answer read as whole; a body written to stdin is judged after the CLI's own exit |
| `src/cli/fake.rs` | `FakeCli` — a `CliRunner` scripted by program and argv words (`with_program`, `answer`, `always`), recording every `Call { program, args, env, stdin }` (`calls()`, `lines()`, `unconsumed()`), so a test asserts the exact verb run and that no program is run at all |
| `src/cli/gh.rs` | `GhCli` over the port: `GH_HOST` for a non-public host, `GH_TOKEN` from `gh auth token --hostname H --user L` for the bound account — one command's environment, never stored; porcelain `pr create` · `view --json` · `list` · `checks` (exit 1 and 8 with JSON are answers) · `review` · `merge` — the porcelain's `author` is its own object (`GhJsonAuthor`, a login beside a GraphQL node id) and `gh api user` a REST user (`GhJsonUser`, the numeric id the no-reply address needs), one struct per wire; `gh api` for the GraphQL reviews-and-threads read, the resolve mutation, the branch delete and an inline review (`--input -`); `auth status` parsed into the probe; a pull request URL yields its number |
| `src/cli/glab.rs` | `GlabCli`: `GITLAB_HOST`; `mr create` · `view -F json` · `approve` · `note` · `merge`; `glab api` for approvals, notes, discussions, pipelines and jobs; the token from `auth status --show-token`, read from stderr where `glab` prints it; one account per host, so a bound login that is not it is `NoSuchAccount`; what `glab` has no verb for is `NoVerb` |
| `layered.rs` | `Layered { cli: Option<Arc<dyn CodeHostCli>>, api: Arc<dyn CodeHost> }` implementing `CodeHost` — the `first_cli!` rule: the CLI's `Answer` is the answer, its `Host` refusal is returned as is and the API is not asked again, a `Fallback` asks the API once; `id`, `capabilities`, `detect` are the API's; `verify_token` is API-only (a token is checked by the thing that will use it); `for_account` binds both layers; `api_only(api)`; `probe()` |
| `github.rs` | `GitHubApi { …, http, account }` — `new(tokens)` against `api.github.com`, `at_host(host, tokens)` for GitHub Enterprise (`https://<host>/api/v3`), `with_base_url` for a test's stub, `with_http` for the engine's clients (the process's shared set until then; every request through `outbound()` with a 30 s deadline); `whoami` (`GET /user`, scopes from `X-OAuth-Scopes`, `GET /user/orgs` best effort), `repo_access`, `detect` (`github.com` in every spelling), `status_error`, `rate_limited`, `REQUIRED_SCOPES` (`repo`, `workflow`), `RECOMMENDED_SCOPES` (`read:org`); `list_prs` filters *merged* from *closed* on `merged_at`; `REVIEWS_QUERY`, `resolve_thread_mutation`, `reviews_of`, `review_body` shared with `GhCli` so the CLI and the API read one GraphQL |
| `gitlab.rs` | `GitLabApi` over REST v4 — `new(tokens)` against `gitlab.com`, `at_host` for a self-hosted instance (`https://<host>/api/v4`); a project is its URL-encoded path; a merge request is a `PullRequest` (its `iid` the number, *Draft:* prefixed when asked, reviewers looked up by username); the latest pipeline's jobs are the checks, none when there is no pipeline; a review is an approval, a note and inline discussions — never a request for changes, and `capabilities().review_events` says so; reviews are three reads (approvals, notes, discussions); a thread id is `owner/name#iid#discussion` and resolves by it; merge squashes when asked and removes the source branch when asked; `list_prs` by GitLab's `state`; a branch is deleted by its encoded name |
| `bitbucket.rs` | `BitbucketApi` over 2.0 — Basic auth, `login:token` (`basic_parts`), which is why `verify_token` takes the login; a pull request by `id`, the commit statuses on its head as checks, the participants who approved or requested changes and the top-level comments as reviews, inline comments and their replies as threads (`owner/name#pr#comment`, resolved when Bitbucket says so), `merge_commit` or `squash`, `refs/branches` delete; `BbPage<T>` for its paging; public host only — Bitbucket Server is a different API and `Hosts` refuses to make one |
| `hosts.rs` | `Hosts` — the three kinds built the same way (`new(dir, runner)` / `file_only` / `with_git` / `with_cli` / `with_http`, the engine's clients handed to every API it makes): `host(kind, host)` (the public host, or a self-hosted instance through the factory), `public()`, `registry()`, `store_for(kind, host)`, `cli_for(kind)`; the token directory per kind, `identity/codehost/<kind>/` |
| `creds.rs` | the credential chain, **keyed by login, per kind**: `Secret` (redacting `Debug` and `Display`, `expose()`), `Credential { username, password: Secret }`, `TokenSource { Env, File, Keyring, Cli, Git }`; `TokenStore::new(kind, host, dir)` / `file_only` over `<dir>/<login>.token` (0600) and the non-secret `<dir>/accounts` list (the keyring entry is `codehost:<kind>:<login>`, only when `BISA_KEYSTORE=keyring`), `with_git(Arc<dyn GitCredentials>)`, `with_cli(Arc<dyn CliToken>)`, `store`, `forget`, `logins`, `env_override`, `cli_login`; the ports — `GitCredentials { fill(host), helpers() }` (implemented in the engine over its `git` handle) and `CliToken { token(login), active_login() }` (implemented by `GhCli` and `GlabCli`); `token()` / `token_source()` — the environment, then the named login (a named account with no token stays a hard refusal unless the CLI holds that login), else the only stored one, else the CLI, else git's helper, else a refusal naming the ways in and `settings_place(kind)` (*Settings → Git & code hosts → GitLab*); `accounts_status → AccountsStatus { env_override, store, accounts, helpers, helper_username, cli_login }`; `normalize_login` — one grammar for the three hosts, `[A-Za-z0-9][A-Za-z0-9._-]*`, up to 255, no `/` |
| `fake.rs` | `FakeCodeHost` — in memory, every capability off by default, `of_kind(kind, host)` so it detects as a kind; able to claim a local bare repository whose name matches its host, so an engine test can push to a real `origin` and still reach a code host; its `FakeState` behind an `Arc`, so a clone `for_account` rebinds shares the state — `asked_as()` says which login each request was made as; `seed_pr(repo, author, head, base)` seeds an open pull request somebody else opened and `set_pr_state(number, state)` moves it, so a test can watch a workstream adopt one and the engine refuse one that closed; `submit_review` holds the rules a real host holds; `verify_token(token, _login)` refuses only an empty token |

---

## Entry points

`Hosts::new(codehost_tokens_root(), Cli)` in production, built by the engine (`Inner.hosts`) from
`EngineConfig.cli` (`None` in every fixture, so no test engine runs the machine's `gh`); `Hosts`
over `FakeCli` and a `tempfile` directory in tests, or `CodeHostRegistry::new(vec![fake])` through
`EngineConfig.code_hosts`. `RemoteUrl::parse(raw)` → `registry.detect(&url)` (or `detect_as(&url,
kind)` for a remote `codehost.kind` names) → `(Arc<dyn CodeHost>, RepoRef)` → `host.for_account(login)`
before any request, so the account the checkout's git config names is the one that asks; `by_kind`
for the settings surface, which has no remote to detect from.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| The chain is the environment, then the named or only stored account, then the CLI, then git's helper — explicit before ambient; two stored and none named is a refusal that names both logins and the settings place; the status names every stored login, the CLI's login, the helper and its username, and never a token; a `Secret` never prints; one login grammar serves the three hosts | `creds.rs::the_chain_is_the_environment_then_the_named_or_only_account_then_the_cli_then_gits_helper`, `the_status_reports_the_accounts_the_cli_and_gits_helper_and_never_a_token`, `a_secret_never_prints`, `logins_take_one_grammar_for_the_three_hosts`; `tests/it/github.rs::gits_credential_helper_is_the_third_source_and_a_stored_token_outranks_it`, `with_no_source_at_all_the_refusal_names_the_ways_in_and_sends_nothing` (a fake `GitCredentials` — no helper, keychain or remote is touched) |
| The file store keeps one token per login at 0600 with a non-secret list of logins, and never asks the keyring unless chosen | `creds.rs::the_file_store_keeps_one_token_per_login_with_owner_only_permissions_and_a_list`, `the_keyring_is_opt_in_only` |
| **CLI first, API second**: without the CLI installed the API answers and no program is run; with it signed in the CLI answers and the API is not asked; not signed in falls through; the host's own refusal through the CLI is the answer and the API is not asked again; binding an account binds both layers and the CLI's token travels in one command's environment; a token is checked by the API alone | `tests/it/layered.rs` — the seven tests named for those sentences and for a reply on a thread, over `FakeCli` and a `FakeCodeHost` |
| `gh` is run with the verbs it has — `pr create` then `pr view` to read back, `pr checks` with its non-zero exits, `gh api` with the REST body on stdin for an inline review — an enterprise host travels as `GH_HOST`, a login `gh` does not hold is a fallback, and the probe reads the status text into accounts while the token line is ignored | `tests/it/gh.rs` (six tests), `src/cli/mod.rs::gh_auth_status_is_read_into_accounts_and_the_token_line_is_ignored`, `versions_are_read_from_either_clis_banner`, `a_failed_exit_is_the_hosts_refusal_or_a_reason_to_ask_the_api`, `the_programs_are_a_closed_set_with_their_words_and_sign_in` |
| `glab` is run with the verbs it has, reads its status and token from stderr, steps aside for what it has no verb for | `tests/it/glab.rs` (three tests), `src/cli/mod.rs::glab_auth_status_is_read_into_one_active_account` |
| Every GitHub method hits the documented endpoint with the documented body; every request carries the bearer token, `Accept` and `X-GitHub-Api-Version`; a rate limit is a wait, a refusal carries GitHub's own reason; a review carries `side`, `start_line` and `start_side`; merged pull requests are closed ones with a `merged_at` | `tests/it/github.rs` against the loopback stub (`tests/it/support/mod.rs`); `github.rs` unit tests |
| Every GitLab request carries the bearer token and a project is its encoded path; a merge request, a job, approvals, notes and discussions read into the crate's words; a review never requests changes; a thread resolves by its composite id; a merge squashes when asked and a branch is deleted by its encoded name | `tests/it/gitlab.rs` (nine tests) against the stub; `gitlab.rs` unit tests |
| Every Bitbucket request carries the login and token as a Basic credential; a token is checked beside its login; pull requests, statuses and comments read into the crate's words; a review approves, requests changes or comments, and threads are inline comments that resolve | `tests/it/bitbucket.rs` (six tests) against the stub; `bitbucket.rs` unit tests |
| A remote URL is taken apart in every spelling and `parse` is total; the owner is the namespace, so a subgroup path is one owner | `lib.rs::a_remote_url_is_taken_apart_in_every_spelling_and_is_total` |
| The registry holds the three public hosts, makes a self-hosted GitHub or GitLab on demand and refuses a Bitbucket Server; a fake detects as its kind | `hosts.rs::the_registry_holds_the_three_public_hosts_and_makes_a_self_hosted_one`, `tests/it/fake.rs::the_registry_picks_a_code_host_by_remote` |
| A fake rebound to another account shares its state and says who asked; the desktop's form renders from `capabilities()`, `review_events` included | `tests/it/fake.rs` |
| A `Connection` reads the account and names the required scopes a token lacks and the recommended ones separately; a token that lists no scopes earns no verdict | `lib.rs::a_connection_reads_the_account_and_names_the_scopes_it_lacks` |
| A token is never logged or returned | review, `Output`'s and `Secret`'s redacting `Debug`, and the node's credential tests asserting on the body |

---

## Errors

`CodeHostError`: `NotAuthenticated`, `NotFound`, `Refused`, `Unsupported(what)`, `Transport`. The
engine wraps it as `EngineError::CodeHost` and retries only `Transport`; the node renders them as
**401**, **404**, **409** (`Refused` and `Unsupported`) and **502** with the sentence. A CLI's
failure is classified into one of those or into a `Fallback` — never a fifth kind.

---

## Extension points

| To add… | Touch, in order |
|---|---|
| a code host | a `CodeHostKind` variant (its public host, environment variable, token page, CLI or none) → `src/<name>.rs` implementing `CodeHost` over that host's API with a base-URL seam → a `CodeHostCli` in `src/cli/<name>.rs` when the host has a CLI (its `CliProgram`, its parsers) → one line in `hosts.rs` → the desktop's `codeHostWords.mjs` and a settings tab |
| a CLI verb | the `CodeHostCli` method answering `Answer` where the verb exists and `Fallback::NoVerb` where it does not; a `tests/<cli>.rs` case over `FakeCli` asserting the argv |

---

## Tests

`tests/it/fake.rs` (the trait's contract through the in-memory host); `tests/it/github.rs`,
`tests/it/gitlab.rs`, `tests/it/bitbucket.rs` (each API against a stub HTTP server on `127.0.0.1`,
`tests/it/support/mod.rs` — canned answers, recorded requests, no network); `tests/it/gh.rs`,
`tests/it/glab.rs` (each CLI over `FakeCli` — the argv recorded, canned stdout and stderr, no program
run); `tests/it/layered.rs` (the rule); unit tests beside `creds.rs` (a `tempfile` token directory),
`src/cli/mod.rs` (the parsers on fixture text), `github.rs`, `gitlab.rs`, `bitbucket.rs`, `hosts.rs`
and `lib.rs`. No test reads a machine's token, keychain, helper, key, `gh`, `glab` or global git
config.

---

## What this crate refuses to do

- depend on any crate of ours beyond the leaf `bisa-http`;
- read another program's files or keys — a CLI is *asked* (`CliRunner`) and git's helper is *asked* (`GitCredentials`), both through ports; a CLI's token is held for one command and never stored; the CLI's own sign-in is never run here;
- guess between two stored accounts — none named and two stored is a refusal that names both;
- carry five parallel fields for five code hosts — one trait, one `PullRequest`, one `RemoteUrl`;
- log, journal or return a credential.
