# Compatibility review

For any change declared anything but *no contract change* — and for any change whose declaration the
reviewer doubts. The promise is [Compatibility](../../reference/compatibility.md); how a change keeps it
is [Keeping compatibility](../compatibility.md).

## The declaration
- [ ] The class declared — no contract change, adds, deprecates, breaks — is the right one. When in
  doubt the reviewer chooses the stricter class.
- [ ] A change declared *breaks* is not merged into 0.x: it is labelled `compat: breaking` and moved
  to the next major's milestone.

## Each surface it touches
- [ ] **Records on disk or on the wire:** a new field is optional with a default; nothing is renamed,
  removed, retyped or given a new meaning; an earlier workspace still opens.
- [ ] **Enums:** a new variant only; every existing word kept; the index's `CHECK` lists and
  `SCHEMA_VERSION` updated if they name the words.
- [ ] **HTTP API:** routes, methods and statuses unchanged; request fields optional; response fields
  only added; `ErrorCode` only added.
- [ ] **CLI:** verbs and flags only added; exit statuses unchanged; `--json` fields only added; nothing
  on standard output when a `--json` call fails.
- [ ] **MCP tools, settings keys, keymap ids, GEP kinds, the addon API:** only added; no number reused.
- [ ] **Workflow and connector definitions:** a definition that parsed still parses and means the same.
- [ ] **Defaults:** a changed default is in a minor, and in the changelog's *Changed*.

## Deprecations
- [ ] What is deprecated keeps working, is marked on its reference page and in *Deprecated*, says so
  where a person meets it, and has its removal issue for the next major.

## Records of the change
- [ ] The changelog's *Unreleased* names every addition, deprecation and changed default.
- [ ] The generated references were regenerated, so the contract on the page is the contract in code.
