# How to contribute

The path every change takes, from an idea to a merged pull request. It is strict on purpose: Bisa runs
on people's machines, holds their keys, drives their coding agents and pushes to their repositories. A
change that breaks something, leaks something or slows something lands on all of them.

## 1. Start from an issue

- **Every pull request closes an accepted issue** (`status: accepted`), with two exceptions: a typo or
  a broken link in the docs, and a change a maintainer asked for in a review.
- **Search first** — open and closed issues, and Discussions.
- **Pick the form** that fits ([the forms](../../.github/ISSUE_TEMPLATE)) — bug, regression, feature,
  improvement, performance, upgrade or compatibility, translation, docs, catalog or harness,
  feedback — and the area it touches.
- **A vulnerability is never an issue.** Report it privately, as [SECURITY.md](../../SECURITY.md) says.
- **Questions and ideas** go to GitHub Discussions; an idea becomes an issue once a maintainer agrees
  it belongs.
- **Wait for acceptance** before writing code: a maintainer labels the issue `status: accepted` (or
  `status: needs-design`) and says how it should be done. Work started before that may not be merged.
- Say on the issue that you are taking it. One person per issue; an issue idle for 14 days is free
  again.

## 2. Design first, when the change is large

A change needs a written design, agreed on its issue before code, when it:

- crosses areas ([the area guides](areas/README.md)), or adds a crate, a GEP kind, a step kind, a
  route family, a settings group or a screen;
- touches the security model, consent, keys, the publish gate or what leaves the machine;
- adds to the public contract ([Compatibility](../reference/compatibility.md)) in a way people will
  build on;
- adds a dependency.

The design says what changes, which documents it reads ([the architecture](../architecture/README.md)),
which invariants it touches, how it is tested, and its compatibility class.

## 3. Prepare

- Set up the tools: [Setup](setup.md).
- Read the area's guide ([areas](areas/README.md)) and the pages it lists.
- Fork the repository and branch from `main`: `<kind>/<issue number>-<short-slug>` — for example
  `fix/123-goal-header-back` or `feature/140-copilot-effort`. Kinds: `fix`, `feature`, `improve`,
  `perf`, `docs`, `i18n`, `catalog`, `build`.

## 4. Make the change

- **One concern per pull request.** A refactor and a feature are two pull requests; so are a fix and
  the cleanup next to it.
- **Keep it small**: under about 400 changed lines, generated files aside. A larger change is split, or
  agreed on its issue first.
- **Put the change where its rule lives** — the module that owns it, never a patch at the caller
  ([07 — Layering](../architecture/07-layering.md)).
- **Follow the recipe** when one exists ([Recipes](recipes.md)): it lists every file a kind of change
  touches, in order, and the check that catches a miss.
- **Tests come with the change** ([Testing rules](testing-rules.md)):
  - a fix starts with a test that fails without it;
  - new behaviour has unit tests for its rules and, where the binary reaches it, a journey;
  - a desktop fact lives in a model with its test — never only in a component.
- **Words come with the change:** every sentence a person reads is a message of the catalog
  ([Say something to a person](recipes.md#26-say-something-to-a-person)); the vocabulary is
  [one word per concept](terminology.md).
- **Docs come with the change:** the pages that describe what you changed, and *Unreleased* in
  `CHANGELOG.md` under the right kind (*Added*, *Changed*, *Deprecated*, *Removed*, *Fixed*,
  *Security*).
- **Generated files come with the change** — regenerate, never hand-edit
  ([Regenerate and verify](recipes.md#22-regenerate-and-verify)).
- **No new dependency** without the licence review ([Add a dependency](recipes.md#28-add-a-dependency))
  and a maintainer's agreement on the issue.
- **No new `unsafe`, no new `allow` of a lint, no new `eslint-disable`** without a sentence that says
  why, and a reviewer's agreement.

## 5. Run the gates

Run the narrow tests while you work, then the gate your change touches, before you ask for review:

```sh
scripts/test module <crate> <module>   # while you work: the module you changed
scripts/test crate <crate>             # before review: every test of the crate you changed
scripts/test desktop <dir>             # a desktop change: the models and scenarios of that folder
scripts/lint-terminology               # always
just verify                            # the whole gate, once, before you mark the pull request ready
```

CI runs the gate again. A pull request is reviewed only when every check is green.

## 6. Commit

- Commits read as a history: an imperative subject of at most 72 characters (*Keep the goal's header
  title on narrow windows*), a blank line, then why — not what the diff already says.
- Never commit a secret, a key, a token, a personal path or an e-mail address; never a file the
  change does not need.
- Pull requests are squash-merged: the pull request's title becomes the commit on `main`, so it
  follows the same rule.

## 7. Open the pull request

- Open it as a **draft** early if you want eyes on the direction; mark it **ready** when the gates are
  green and the template is complete.
- Fill **every** section of the template ([the template](../../.github/pull_request_template.md)): what
  and why, the issue, the compatibility declaration, the tests, the commands you ran, the security,
  performance and licence impact, screenshots for a screen, and whether a coding agent helped.
- Keep it current with `main` by rebasing your branch; never push to `main`.

## 8. Review

What review checks and how it proceeds is in [Review](review/README.md). In short: every comment is
answered, a change is pushed as new commits until approval, an approval is dismissed by a later push,
and the pull request is merged by a maintainer once it is approved, green and every conversation is
resolved.

## Contributions made with coding agents

Welcome, on the same terms as any other:

- **A person submits and answers for every change.** You read and understand all of it, you ran the
  gates, and you answer review yourself.
- **Say so** in the pull request: which agent, and for which part.
- **Point the agent at [AGENTS.md](../../AGENTS.md)** — it carries the rules an agent must keep.
- **Never open pull requests in bulk** or on issues nobody accepted; a maintainer closes them unread.

## Licence

Bisa is MIT-licensed ([LICENSE](../../LICENSE)). By opening a pull request you agree that your
contribution is licensed under the same MIT licence, and that you have the right to submit it.
