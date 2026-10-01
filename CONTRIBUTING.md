# Contributing to Bisa

Thank you for wanting to make Bisa better. Bisa runs on people's machines, holds their keys, drives
their coding agents and pushes to their repositories, so the way in is strict: every change is
discussed before it is written, tested before it is reviewed, and reviewed against written checklists
before it is merged. This page is the map; each step links to the page that holds its detail.

## What every change protects

- **Stability.** Inside 0.x, a minor or a patch release never breaks what an earlier release left
  behind — a workspace, a script, a workflow, an integration. A change that cannot be made by addition
  waits for 1.0.0, which brings a migration ([Compatibility](docs/reference/compatibility.md)).
- **Safety.** Nothing a change does can make someone's work unrecoverable, leak a secret, or reach
  beyond what the person allowed ([11 — Security](docs/architecture/11-security.md)).
- **Speed.** The budgets hold; a slower path is argued with numbers
  ([Performance](docs/contributing/performance.md)).
- **The architecture's rules.** The files are the truth; one place per invariant; only the engine has
  effects ([Architecture](docs/architecture/README.md)).
- **The words.** One word per concept, every sentence a person reads in the message catalog
  ([Terminology](docs/contributing/terminology.md)).

## Ways to contribute

| You want to | Start here |
|---|---|
| ask a question, share an idea | GitHub Discussions |
| report a bug, a regression, a slowness, an upgrade problem | an issue — [choose the form](.github/ISSUE_TEMPLATE) |
| report a vulnerability | privately — [SECURITY.md](SECURITY.md), never an issue |
| propose a feature or an improvement | an issue; a large one needs a design first |
| add an agent, a skill, a template, a connector, an addon, a pet or a harness | the *catalog or harness* form, then [the catalog guide](docs/contributing/areas/catalog.md) |
| translate Bisa | the *translation* form, then [the translation guide](docs/contributing/areas/translation.md) |
| improve the docs or the website | the *docs* form, then [the docs guide](docs/contributing/areas/docs.md) |
| write code | an accepted issue — then the path below |

## The path

1. **An accepted issue.** Every pull request closes an issue a maintainer accepted, except a typo fix.
   Search first; say on the issue that you are taking it. ([How to contribute](docs/contributing/how-to-contribute.md))
2. **A design, when the change is large** — crosses areas, touches security, adds to the public
   contract or a dependency. Agreed on the issue before code.
3. **Set up** — [Setup](docs/contributing/setup.md) — and read **your area's guide**
   ([area guides](docs/contributing/areas/README.md)).
4. **A branch** from `main` on your fork: `<kind>/<issue>-<slug>`.
5. **The change, with its tests, its docs, its changelog entry, its regenerated files** — following
   the [recipe](docs/contributing/recipes.md) when one exists, and [the testing rules](docs/contributing/testing-rules.md).
6. **The gates, green** — the narrow tests while you work, then `just verify` before review.
7. **A pull request** with every section of [the template](.github/pull_request_template.md) filled —
   including the **compatibility declaration**: no contract change, adds, deprecates, or breaks.
8. **Review** — stage by stage, against [the checklists](docs/contributing/review/README.md): code,
   security, performance, licences, compatibility, docs and language.
9. **Squash merge** by a maintainer, once approved, green and every conversation resolved.

## What a pull request must carry

- [ ] It closes an accepted issue and does one thing.
- [ ] A fix starts with a test that fails without it; new behaviour has its tests.
- [ ] `just verify` passed on your machine; every CI check is green.
- [ ] The compatibility class is declared, and *breaks* is never merged into 0.x
  ([Keeping compatibility](docs/contributing/compatibility.md)).
- [ ] The docs that describe the change are updated; *Unreleased* in [CHANGELOG.md](CHANGELOG.md) has its
  entry.
- [ ] Generated files are regenerated, never hand-edited.
- [ ] No new dependency without the licence review; no secret, key, token, personal path or e-mail
  anywhere.
- [ ] Screenshots, light and dark, for a screen it changes.
- [ ] It says whether a coding agent helped, and with what.

## Contributions made with coding agents

Welcome, on the same terms as any other. A person submits and answers for every change: you read it,
you understand it, you ran the gates, you answer review. Say in the pull request which agent helped and
with what, and point the agent at [AGENTS.md](AGENTS.md). Pull requests opened in bulk, or on issues
nobody accepted, are closed unread.

## Conduct

Everyone in Bisa's spaces follows the [Code of Conduct](CODE_OF_CONDUCT.md). How the project is run —
roles, decisions, becoming a maintainer — is in [GOVERNANCE.md](GOVERNANCE.md); where to get help, in
[SUPPORT.md](SUPPORT.md).

## Licence

Bisa is released under the [MIT licence](LICENSE). By opening a pull request you agree that your
contribution is licensed under the same terms, and that you have the right to submit it.

## Everything for contributors

| Page | For |
|---|---|
| [How to contribute](docs/contributing/how-to-contribute.md) | the path in detail: issues, design, branches, commits, pull requests |
| [Area guides](docs/contributing/areas/README.md) | where each part of Bisa lives and how to change it |
| [Setup](docs/contributing/setup.md) | the tools and the commands |
| [Recipes](docs/contributing/recipes.md) | every kind of change, file by file |
| [Testing rules](docs/contributing/testing-rules.md) | what a test may never do; the guards; how to run |
| [Review](docs/contributing/review/README.md) | the stages and the checklists |
| [Keeping compatibility](docs/contributing/compatibility.md) · [Migrations](docs/contributing/migrations.md) | how the promise is kept; how a major is prepared |
| [Triage](docs/contributing/triage.md) | labels, priorities, response times |
| [Release](docs/contributing/release.md) | versions, the changelog, how a release is made |
| [Terminology](docs/contributing/terminology.md) · [Performance](docs/contributing/performance.md) · [Coverage](docs/contributing/coverage.md) | the words, the budgets, what each feature's tests are |
| [Architecture](docs/architecture/README.md) | how Bisa is built |
