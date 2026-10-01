# Security policy

Bisa runs on people's machines, holds their keys, drives their coding agents and pushes to their
repositories. A security fault is treated before any other work.

## Supported versions

| Version | Receives security fixes |
|---|---|
| The latest minor release of 0.x | yes — as a patch release (0.y.Z) |
| Earlier 0.x minor releases | no — upgrade to the latest; inside 0.x an upgrade is in place and keeps your workspace ([Compatibility](docs/reference/compatibility.md)) |

Releases are on [GitHub Releases](https://github.com/mourad-ghafiri/Bisa/releases). When 1.0.0 is
released, the last 0.x minor keeps receiving security fixes for six months.

## Reporting a vulnerability

**Never open a public issue, discussion or pull request for a security fault.** Report it privately
through GitHub's vulnerability reporting on the repository — **Security › Report a vulnerability** at
https://github.com/mourad-ghafiri/Bisa/security/advisories/new — with:

- what you found and what an attacker gains;
- how to reproduce it, step by step, on which release or commit, on which platform;
- whether you know of it being used.

## What happens next

| When | What |
|---|---|
| within 3 working days | an acknowledgement, and who handles it |
| within 10 working days | an assessment: accepted or not, its severity, and the plan |
| until the fix | the work happens in a private fork of the advisory; you are invited to it if you want to review the fix |
| the fix | a patch release, with the advisory published and a CVE requested through GitHub when the fault warrants one |
| after | your name in the advisory and the release notes, if you want it there |

**Coordinated disclosure:** please keep the fault private until the fix is released, or for 90 days
from your report, whichever comes first; if more time is needed we say why and agree it with you.
A report that reaches a harness, a code host or a relay the platform talks to is passed on to them as
well, with your agreement.

**Safe harbour:** research done in good faith, on your own machines and accounts, within this policy,
is welcome; we will not pursue it. Do not access data that is not yours, do not degrade services
others use, and stop and report as soon as you find something.

## What is in scope

The platform as released and the repository that builds it:

- the desktop application and its node, the command line, the MCP server, the A2A door, the addon
  walls and the collaboration wire — everything under `crates/`, `desktop/` and `library/`;
- the release and build scripts (`scripts/`), the CI workflows (`.github/`), and the website
  (`website/`).

The security model is [11 — Security](docs/architecture/11-security.md). A gap documented there is a
known cost, not a vulnerability, unless you show it reaches further than the page says. Bisa's guards
are best effort and never a sandbox; a harness's own sandbox, the model providers and the code hosts
are their vendors' to secure.

## Fixing a vulnerability (for contributors and maintainers)

- A security fix is developed in the advisory's private fork, never in a public branch, and reviewed
  with [the security checklist](docs/contributing/review/security.md).
- The public pull request, the changelog's *Security* entry and the advisory are published together,
  with the patch release.
- The fix carries a test that fails without it, written so it does not teach the attack.

## Verifying a download

A release is a signed, notarized disk image and its SHA-256, both on the release's page. Beside the
two files:

```sh
shasum -a 256 -c Bisa-<version>-macos-universal.dmg.sha256
xcrun stapler validate Bisa-<version>-macos-universal.dmg
```

The first proves the download is the file that was published; the second that Apple's notary
service accepted it and the ticket is in it. Inside the image, `codesign -dvv /Volumes/Bisa\ <version>/Bisa.app`
names the team the app was signed by.
