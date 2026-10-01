# Governance

How Bisa is run: who decides, how, and how a contributor comes to share the work.

## Roles

| Role | Who | Does |
|---|---|---|
| **Owner** | the project's creator, Mourad Ghafiri | sets direction; appoints maintainers; holds the repository's settings and the release keys; makes every release; decides when the maintainers cannot agree |
| **Maintainer** | appointed by the owner | triages; reviews and approves; merges; owns one or more areas in `.github/CODEOWNERS` |
| **Area owner** | a maintainer named for an area | is the required reviewer for the paths of that area |
| **Triager** | appointed by the maintainers | labels, reproduces, asks for what is missing, closes duplicates; does not merge |
| **Contributor** | anyone who opens an issue, a discussion or a pull request | follows [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md) |

## How decisions are made

- **Everyday changes** — an accepted issue and an approved pull request ([Review](docs/contributing/review/README.md)).
- **Larger changes** — a design agreed on the issue before code
  ([How to contribute](docs/contributing/how-to-contribute.md#2-design-first-when-the-change-is-large)).
- **Direction** — proposals start in Discussions (*Ideas*). A maintainer moves an agreed proposal to
  an accepted issue; the owner decides the ones that change what Bisa is.
- **Breaking the contract** — only for a major release, with a migration
  ([Migrations](docs/contributing/migrations.md)).
- **Disagreement** — argued on the issue with reasons; when two rounds do not settle it, the
  maintainers decide by consensus, and the owner when they cannot.
- Decisions are written where they were taken — the issue, the pull request, the discussion — so
  they can be found and questioned later.

## Becoming a maintainer

A contributor who, over several months, has had substantial pull requests merged in an area, reviews
others' work carefully, and keeps the Code of Conduct, may be invited by the owner. Maintainers who
step back, or are inactive for six months, return to contributor; their work stays credited.

## Releases

Only the owner makes a release, following [Release](docs/contributing/release.md). Nothing publishes on
its own.

## Changes to this document

By pull request, approved by the owner.
