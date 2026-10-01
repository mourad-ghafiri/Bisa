# For maintainers: the repository's settings

The process in these pages leans on settings of the GitHub repository that a file cannot carry. The
owner applies them once, by hand, in the repository's settings, and checks them after any change to
the team.

## Community

| Setting | Where | Set to | Source |
|---|---|---|---|
| Discussions | Settings › General › Features | on, with the categories *Q&A*, *Ideas*, *Show and tell*, *Announcements* | [GitHub Docs — Discussions](https://docs.github.com/en/discussions/quickstart) |
| Reported content | Settings › Moderation options › Reported content | accepted from **all users**, so a first-time visitor can report conduct | [GitHub Docs — reported content](https://docs.github.com/en/communities/moderating-comments-and-conversations/managing-how-contributors-report-abuse-in-your-organizations-repository) |
| Blank issues | `.github/ISSUE_TEMPLATE/config.yml` | already off | [GitHub Docs — issue templates](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/configuring-issue-templates-for-your-repository) |
| Labels | Issues › Labels | the list in [Triage](triage.md#labels) | — |

## Security

| Setting | Where | Set to | Source |
|---|---|---|---|
| Private vulnerability reporting | Settings › Advanced Security | enabled | [GitHub Docs — private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository) |
| Secret scanning and push protection | Settings › Advanced Security | enabled | [GitHub Docs — secret scanning](https://docs.github.com/en/code-security/secret-scanning/introduction/about-secret-scanning) |
| Dependabot alerts | Settings › Advanced Security | enabled | [GitHub Docs — Dependabot alerts](https://docs.github.com/en/code-security/dependabot/dependabot-alerts/about-dependabot-alerts) |

## The ruleset on `main`

Settings › Rules › Rulesets › *New branch ruleset*, targeting the default branch
([GitHub Docs — available rules](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets)):

| Rule | Setting |
|---|---|
| Restrict deletions | on |
| Block force pushes | on |
| Require linear history | on (pull requests are squash-merged) |
| Require a pull request before merging | on — 1 approval (2 once there are two maintainers); dismiss stale approvals when new commits are pushed; require review from Code Owners; require approval of the most recent reviewable push; require conversation resolution before merging |
| Require status checks to pass | on, with the branch up to date — the jobs of `.github/workflows/verify.yml`: `terminology`, `rust`, `desktop`, `generated types`, `licences`, `website`, `hakari` |
| Bypass list | the owner only, for an emergency, said on the pull request afterwards |

Merge options (Settings › General › Pull Requests): allow **squash merging** only, with the pull
request's title as the commit subject; automatically delete head branches.

## Code owners

`.github/CODEOWNERS` names the owner on every path today, laid out by area. A new maintainer is added
to the areas they own; a code owner must have write access to the repository
([GitHub Docs — code owners](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners)).

## Releases

Only the owner releases ([Release](release.md)). Nothing publishes on its own.
