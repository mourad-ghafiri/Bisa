# Docs and language review

Every pull request. Bisa's documents describe the platform as built, its words are one per concept,
and every sentence a person reads is a message of the catalog.

## Documents
- [ ] Every page that describes what changed is updated in the same pull request — the guide a person
  reads, the architecture page a contributor reads, the reference an integrator reads.
- [ ] Generated pages (`docs/reference/http-api.md`, `settings-keys.md`, `catalog.md`, `keymap.md`) are
  regenerated, never edited.
- [ ] Links and anchors resolve, paths named in code spans exist, settings keys named are registered —
  the docs test holds all three (`scripts/test module core docs`).
- [ ] The pages describe what is built, in the present tense — no plans, no phase names.
- [ ] Every Mermaid diagram parses (`node scripts/check-mermaid.mjs docs`).
- [ ] *Unreleased* in `CHANGELOG.md` has the entry, under the right kind.

## Words
- [ ] One word per concept ([Terminology](../terminology.md)); the vocabulary lint is green
  (`scripts/lint-terminology`).
- [ ] Every sentence a person reads — in the desktop, the CLI, an error, a notification — is a message
  of the catalog with its id, never English written in code
  ([Say something to a person](../recipes.md#26-say-something-to-a-person)).
- [ ] A message id is new for a new meaning, never reused; the English is plain, short and specific.
- [ ] Ending a session, a harness or a process is *terminate*, everywhere.

## Screens
- [ ] A changed screen has screenshots in light and dark; text fits at the narrowest supported width.
- [ ] Settings are named by their path as a person sees it: *Settings › group › panel*.
