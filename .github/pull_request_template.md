<!--
Read CONTRIBUTING.md first. Fill every section; write "n/a — <why>" where one does not apply.
A pull request with an empty section, a red check or no accepted issue is not reviewed.
-->

## What and why

<!-- What this changes, and why — the problem it solves. Not a list of files. -->

Closes #

## Kind and area

- **Kind:** <!-- fix · feature · improvement · performance · docs · translation · catalog · build -->
- **Area:** <!-- from docs/contributing/areas/README.md -->

## Compatibility

<!-- docs/contributing/compatibility.md — check exactly one. -->

- [ ] **No contract change** — nothing in docs/reference/compatibility.md § Surface by surface changes.
- [ ] **Adds to the contract** — what is added:
- [ ] **Deprecates** — what, what replaces it, and the removal issue:
- [ ] **Breaks** — this cannot be merged into 0.x; it waits for 1.0.0 (say why no additive way exists):

## Tests

<!-- What proves it. For a fix: the test that fails without it. -->

- Tests added or changed:
- Commands run, and their result:

```sh
scripts/test …
just verify
```

## Impact

- **Security** (docs/contributing/review/security.md): <!-- none · or what it touches: secrets, keys, doors, guards, what leaves the machine, inputs from outside -->
- **Performance** (docs/contributing/review/performance.md): <!-- none · or the path, with numbers before and after -->
- **Licences** (docs/contributing/review/licences.md): <!-- none · or each dependency or asset added, with its licence -->

## Screens

<!-- For a change a person sees: screenshots, light and dark. -->

## Coding agents

- [ ] No coding agent helped with this change.
- [ ] A coding agent helped — which one, and with what:

## Checklist

- [ ] It closes an accepted issue (`status: accepted`), or it is a typo fix.
- [ ] It does one thing; unrelated changes are in their own pull requests.
- [ ] The change sits where its rule lives, and keeps the layering and the invariants it touches.
- [ ] Every new behaviour has its tests; a fix has a test that fails without it; no test was removed or loosened.
- [ ] `just verify` passes on my machine; every CI check is green.
- [ ] Every sentence a person reads is a message of the catalog; the vocabulary lint passes.
- [ ] The docs that describe this change are updated; `CHANGELOG.md` *Unreleased* has its entry.
- [ ] Generated files are regenerated, never edited by hand.
- [ ] No secret, key, token, personal path or e-mail address anywhere in the change.
- [ ] I read and understand every line of this change, and I answer for it.
