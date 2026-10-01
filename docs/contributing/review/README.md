# Review

Every change to `main` is reviewed, by a person, against written checklists. Review is where the
project's promises — compatibility, safety, speed, licences, words — are kept, so it is strict and it
is predictable: the same questions, in the same order, for everybody, maintainers included.

## Who reviews

- **A maintainer** approves every pull request. The author never approves their own.
- **The code owner** of every path the change touches ([CODEOWNERS](../../../.github/CODEOWNERS))
  reviews it; while one person owns every path, that person's approval stands for all of them.
- **Two approvals** are required for a change to the security model, consent, keys or what leaves the
  machine; to the release, licence or CI files; or one declared *adds to the contract* in a way people
  will build on — once the project has two maintainers. Until then the owner approves, and the
  checklists below are completed in writing on the pull request.

## The stages

| # | Stage | Who | Passes when |
|---|---|---|---|
| 1 | **Triage** | a maintainer | it closes an accepted issue (or is a typo fix), the template is complete, the compatibility class is declared |
| 2 | **Gates** | CI and the author | every check is green; the author ran `just verify` |
| 3 | **Code review** | a maintainer | [Code](code.md) — every item answered |
| 4 | **Specialist reviews** | a maintainer, where the change reaches them | [Security](security.md), [Performance](performance.md), [Licences](licences.md), [Compatibility](compatibility.md), [Docs and language](docs-and-language.md) |
| 5 | **Approval** | the maintainers above | every conversation resolved; approvals given after the last push |
| 6 | **Merge** | a maintainer | squash-merged, the title written as a commit subject; the issue closes |

A specialist review applies when:

- **Security** — the change touches a guard, the redactor, the classifier, consent, keys, tokens,
  secrets, the node's doors, what a harness may run, what leaves the machine, a dependency, or any
  input that comes from outside the machine.
- **Performance** — it touches a path with a budget ([14 — Performance](../../architecture/ide/14-performance.md)),
  a poller, a cache, a lock, a stream, the index, or anything run per event, per frame or per keystroke.
- **Licences** — it adds, removes or upgrades a dependency, a font, an icon, a template or any material
  the project did not write.
- **Compatibility** — it is declared anything but *no contract change*, or the reviewer disagrees with
  that declaration.
- **Docs and language** — always.

## How review talks

- A comment is **blocking** unless it starts with *nit:* or *optional:*. Blocking comments are
  resolved by a change or by an agreed answer — never by silence.
- The author **resolves nothing alone**: the reviewer who opened a conversation resolves it.
- **Push new commits** while in review; never rewrite what a reviewer already read. The branch is
  squashed at merge.
- **A push dismisses the approvals** given before it; the reviewer reads the new commits.
- A reviewer **says why** — the rule, the page, the risk — so the answer can be checked, not obeyed.
- A disagreement that two rounds do not settle goes to the maintainers on the issue
  ([GOVERNANCE.md](../../../GOVERNANCE.md)).

## Response times

Targets, not guarantees:

| | Target |
|---|---|
| First triage of a pull request | 5 working days |
| A review round | 5 working days |
| A security report's acknowledgement | as [SECURITY.md](../../../SECURITY.md) states |
| An author's answer to a review | 14 days; a pull request idle for 30 days is closed, and can be reopened |

## What review never accepts

- A change with a red check, or a check skipped, disabled or made weaker to turn green.
- A test removed or loosened to make a change pass, unless the test itself was wrong — said and shown.
- A change declared *breaks* into 0.x ([Compatibility](../compatibility.md)).
- A secret, key, token, personal path or e-mail address anywhere in the change.
- A generated file edited by hand.
- A new dependency whose licence is outside the allow list ([Licences](licences.md)).
- An unexplained `unsafe`, `allow`, `eslint-disable` or `#[ignore]`.
