# Keeping compatibility

[Compatibility](../reference/compatibility.md) is the promise users read. This page is how a change
keeps it. The rule underneath every row below:

> **Forward only; nothing breaks inside a major.** A shape grows by addition — a new field has a
> default, a new value sits beside the old, a number is never reused. What one release reads, every
> later release of that major reads. A change that cannot be made that way waits for the next major,
> which brings its migration ([Migrations](migrations.md)).

Nothing checks this automatically. **You declare it** in the pull request — *no contract change*,
*adds to the contract*, *deprecates*, or *breaks* — and **review decides** with
[the compatibility checklist](review/compatibility.md). A change declared *breaks* is not merged into
0.x: it is labelled `compat: breaking` and parked for the next major.

## The additive way, surface by surface

| You want to | Do | Never |
|---|---|---|
| add a field to a stored or synced record | make it optional, with `#[serde(default)]` (or a default that means "as before"); document it | make an existing field required, rename it, remove it, or change its type or meaning |
| add a value to an enum that is stored or travels | add the variant; keep every existing word; if the index lists the words in a `CHECK`, bump `SCHEMA_VERSION` (the index is a cache) and say in the changelog that the next open rebuilds it | rename or remove a variant; reuse a word for a new meaning |
| add to a request body | an optional field, with a default that keeps the old behaviour | make a field required; remove one; tighten what it accepts |
| add to a response or an event | a new field | remove, rename or retype a field; drop an event kind or a topic |
| add a route | a new path | rename or remove a route; change its method or the status a known case answers |
| add an error | a new `ErrorCode` | change the status of a code; reuse a code for another case |
| add to the CLI | a new verb or flag; a new field in a `--json` answer | rename or remove a verb or flag; change an exit status; print anything on standard output under `--json` when it fails |
| add an MCP tool | a new tool, or a new optional input | rename or remove a tool; make an input required |
| add a setting | a new key in the registry | rename or remove a key; narrow its scopes; remove a choice |
| change a default | do it in a minor, and name it in the changelog's *Changed* | change one in a patch |
| add a keymap command | a new id | rename an id (`keymap.overrides` is keyed by it) |
| add a GEP kind | the next free number ([GEP](../reference/gep.md)) | reuse a retired number; change a kind's content incompatibly |
| add to the addon API | a new method, permission or event | remove or rename one; widen what an existing permission grants |
| add to a workflow or connector definition | an optional key; a new step kind, start event or condition | rename a key; change what an existing key means |
| retire something | deprecate it ([below](#deprecating-something)) | remove it before the next major |

Two consequences that surprise people:

- **Records are read strictly.** A key a release does not know is refused by name, never dropped
  ([08 — Persistence](../architecture/08-persistence.md)). That is why a new field must be *optional*:
  the files earlier releases wrote do not have it.
- **The catalog is content, not contract.** An agent, skill, team, template, connector, addon or pet
  can change or leave the catalog in a minor; what a person installed is theirs and an upgrade never
  touches it.

## Deprecating something

1. Mark it deprecated where it is documented — its reference page, and *Deprecated* in
   `CHANGELOG.md` *Unreleased* — saying what replaces it.
2. Keep it working, unchanged, for the rest of the 0.x line. A deprecated setting is still read; a
   deprecated flag still works; a deprecated route still answers.
3. Where a person meets it, say so in words a person reads (a message of the catalog — [Say something
   to a person](recipes.md#26-say-something-to-a-person)).
4. Open an issue labelled `compat: breaking` for its removal, with the 1.0.0 milestone.

## Declaring your change

The pull request template asks one question, with four answers:

| Answer | Means | Then |
|---|---|---|
| **No contract change** | nothing under [Surface by surface](../reference/compatibility.md#surface-by-surface) changes | nothing more |
| **Adds to the contract** | something new, nothing changed or removed | list what, and add it under *Added* in the changelog |
| **Deprecates** | something keeps working and is marked for removal | follow [Deprecating something](#deprecating-something) |
| **Breaks** | something that worked no longer would | the change waits for 1.0.0 — open or join the `compat: breaking` issue instead |

When you are unsure, declare *breaks* and say why: review would rather lower the class than discover it.

## Patches

A patch release fixes behaviour and nothing else: no new field, key, route, verb, kind or default. A
fix that needs any of them goes into the next minor.
