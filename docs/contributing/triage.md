# Triage

How an issue or a pull request is read, labelled and moved, so nothing waits unseen and the most
important work goes first.

## Labels

| Family | Labels | Meaning |
|---|---|---|
| **Kind** | `kind: bug` · `kind: regression` · `kind: feature` · `kind: improvement` · `kind: performance` · `kind: compatibility` · `kind: translation` · `kind: docs` · `kind: catalog` · `kind: feedback` | set by the issue form |
| **Area** | `area: core` · `area: workspace` · `area: goals` · `area: workflows` · `area: projects` · `area: ide` · `area: node` · `area: cli` · `area: mcp` · `area: harnesses` · `area: collaboration` · `area: agents` · `area: channels` · `area: inbox` · `area: settings` · `area: desktop` · `area: connectors` · `area: addons` · `area: notes-drawings` · `area: decision-making` · `area: security` · `area: translation` · `area: catalog` · `area: website` · `area: docs` · `area: build-release` · `area: mobile-app` | one per area guide ([areas](areas/README.md)) |
| **Status** | `status: needs-triage` · `status: needs-info` · `status: accepted` · `status: needs-design` · `status: blocked` · `status: duplicate` · `status: wont-do` | where it stands |
| **Priority** | `priority: critical` · `priority: high` · `priority: normal` · `priority: low` | see below |
| **Compatibility** | `compat: additive` · `compat: deprecation` · `compat: breaking` | the class of the change ([Keeping compatibility](compatibility.md)) |
| **Help** | `good first issue` · `help wanted` | a way in for a new contributor |
| **Process** | `no-changelog` · `security` | a pull request with no changelog entry by agreement; work on a fixed vulnerability once it is public |

## Priorities

| Priority | For | Target |
|---|---|---|
| `priority: critical` | data loss, a security hole, the app or the node will not start, a release that breaks the contract | worked on at once; a patch release |
| `priority: high` | a regression; a core flow broken with no workaround | the next patch or minor |
| `priority: normal` | a bug with a workaround; an accepted improvement | when someone takes it |
| `priority: low` | polish; a rare edge | when someone takes it |

A **regression** — something that worked in an earlier release — is at least `priority: high`.

## The flow

1. **New** — the form adds `status: needs-triage` and the kind and area labels.
2. **Read** — within 5 working days, a maintainer:
   - closes it as a duplicate, or as not something Bisa does, saying why;
   - asks for what is missing (`status: needs-info`; closed after 30 days without an answer);
   - moves a report that is really a vulnerability out of public view, as [SECURITY.md](../../SECURITY.md)
     says, and hides the public text;
   - or sets a priority and `status: accepted` (with how it should be done), or `status: needs-design`
     (a design is agreed on the issue first — [How to contribute](how-to-contribute.md#2-design-first-when-the-change-is-large)).
3. **Taken** — a contributor says so on the issue; a maintainer assigns them. An assignment idle for
   14 days is released.
4. **Done** — the pull request that closes it is merged; the release that carries it is named in the
   changelog.

## Good first issues

An issue is a `good first issue` only when it is accepted, small, in one area, and says which files to
read, what to change and how to test it. A maintainer writes that before adding the label.

## Pull requests

Triage of a pull request is the first stage of [Review](review/README.md): an accepted issue, a
complete template, a declared compatibility class. One that has none of these is closed with a pointer
to [How to contribute](how-to-contribute.md).
