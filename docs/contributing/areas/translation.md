# Translation

Every word the platform says to a person goes through one catalog. A sentence travels as a `Text` —
the id of a message and the arguments it interpolates — and is rendered in the person's language at
the edge that talks to them: the desktop, the command line, the node's error body. The catalog is
[Fluent](https://projectfluent.org/) under `locales/<lang>/`, read by `fluent-bundle` in the crates
(`bisa-i18n`) and `@fluent/bundle` in the desktop. English is the only language shipped; another
language is a folder ([Add a locale](../recipes.md#25-add-a-locale)).

## Where it lives

- `locales/en/` — the catalog: `settings.ftl`, `errors.ftl`, `problems.ftl`, `engine.ftl`, `cli.ftl`, `catalog.ftl` (the content seam's rule, no entries) and `locales/en/desktop/`, one file per area.
- `crates/bisa-core/src/text.rs` — `Text`, `Arg` and the `text!` macro.
- `crates/bisa-i18n/src/` — `locale.rs` (`AVAILABLE`, negotiation), `catalog.rs` (the namespaces, compiled in), `ratchet.rs` (the guard's scanner, and the command's body as `ratchet::run`) and `crates/bisa-i18n/src/bin/i18n-ratchet.rs` (the thin command); the baseline is `crates/bisa-i18n/tests/ratchet.baseline.json`.
- `desktop/src/i18n/` — `l10n.mjs` (the door: `t`, `attr`, `tx`), `localeModel.mjs` (the languages and the setting), `format.mjs` (numbers, dates and spans), `ratchetModel.mjs` and `ratchet.baseline.json`.
- `desktop/src-tauri/src/words.rs` — the menu bar icon's and the Edit menu's words, pushed by the webview once it has started.

## Read first

- [locales/README.md](../../../locales/README.md) — the folder's rules: ids, attributes, plurals, arguments, what is never there.
- [17 — Internationalisation](../../architecture/17-internationalisation.md) — the model, [what is never translated](../../architecture/17-internationalisation.md#what-is-never-translated), the guards, [adding a language](../../architecture/17-internationalisation.md#adding-a-language).
- [crates/i18n](../../architecture/crates/i18n.md) — the crate, its invariants and its tests.
- [Say something to a person](../recipes.md#26-say-something-to-a-person) and [Add a locale](../recipes.md#25-add-a-locale).

## Rules a change must keep

- A sentence for a person is a message of the catalog, said by its id — `t("area-thing")` in the desktop, `text!("error-…")` in the crates — never a literal ([Say something to a person](../recipes.md#26-say-something-to-a-person)).
- The ratchet: the bare sentences of each source file equal the committed baseline, and both baselines are empty — `{}` — and stay so ([17 § Guards](../../architecture/17-internationalisation.md#guards)).
- A word not for a person — for the agent, for the machine, for the log, or a person's own content — is marked on its line or the line above (`// for the agent`), and the ratchet leaves it out ([17 § What is never translated](../../architecture/17-internationalisation.md#what-is-never-translated)).
- Never translated: a person's own content, what a model reads (prompts, skills, tool descriptions), logs, identifiers (settings keys, routes, events, keymap ids), and the documentation ([17 § What is never translated](../../architecture/17-internationalisation.md#what-is-never-translated)).
- Ids are stable, kebab-case and area-prefixed; a widget's several strings are attributes of one message; a plural is a selector in the message, never code; an argument is never translated ([locales/README.md](../../../locales/README.md)).
- Every `text!` names a message with exactly its arguments, and every message is said ([crates/i18n § Invariants held here](../../architecture/crates/i18n.md#invariants-held-here)).
- A shipped language ships whole — every file, every id — and the two `AVAILABLE` lists agree ([Add a locale](../recipes.md#25-add-a-locale)).
- A library entry's display fields are a seam: English ships no message for them, and a language may translate one by its entry's id ([17 § What is never translated](../../architecture/17-internationalisation.md#what-is-never-translated)).
- A sentence that names a Settings panel spells the path as the rail shows it (`desktop/src/scenarios/settingsPaths.test.mjs`).

## Testing a change

- `scripts/test crate i18n` — every file parses, every `text!` names a message with its arguments, every message is said, a language ships whole, the ratchet.
- `scripts/test desktop i18n`, then from `desktop/`: `node --test --import ./src/i18n/preload.mjs src/scenarios/i18n.test.mjs` — the same for `t()` and `attr()`, and the desktop's ratchet.
- A model's test asserts the English sentence, which `npm test` preloads; one file runs with `node --test --import ./src/i18n/preload.mjs` from `desktop/`.
- To find a line the ratchet counts: `cargo run -q -p bisa-i18n --bin i18n-ratchet -- --list` and, from `desktop/`, `node src/i18n/ratchet.mjs --list`.
- `node --test desktop/src/i18n/localeModel.test.mjs` — the setting's choices equal the desktop's languages.
- One module at a time; `just verify` closes the pass ([Testing rules § Running](../testing-rules.md#running)).

## Common changes

- [Say something to a person](../recipes.md#26-say-something-to-a-person) — in the desktop, the node or the engine, the CLI.
- [Add a locale](../recipes.md#25-add-a-locale) — a copy of `locales/en/`, every file and every id, translated; the tag in both `AVAILABLE` lists and among the `appearance.language` choices.
- [Add a setting](../recipes.md#2-add-a-setting) and [Add a CLI verb](../recipes.md#12-add-a-cli-verb) — each brings its own messages.
- The engine's sentences still read in English are listed by file in [17 § What is not yet in the catalog](../../architecture/17-internationalisation.md#what-is-not-yet-in-the-catalog).

## Compatibility

- The words of messages are not part of the contract ([Not promised in 0.x](../../reference/compatibility.md#not-promised-in-0x)), nor is the command line's human-readable output ([The command line](../../reference/compatibility.md#the-command-line)). A client switches on an error's `code`, never on its words, so a sentence may be reworded in any release; an id stays stable all the same.
- `appearance.language` and its choices are [settings keys](../../reference/compatibility.md#settings-keys): a new language is a new choice beside the old ones — additive.
- Declare your change's compatibility in the pull request ([Keeping compatibility](../compatibility.md)).

## Review focus

- Every sentence a person reads goes through the catalog, in the namespace of the area that says it, and the baselines stay empty ([Docs and language](../review/docs-and-language.md)).
- A mark is honest: *for the agent* only on what a model reads, *for the machine* only on a word another program parses ([Code review](../review/code.md)).
- A translation keeps every id, every attribute and every placeable, and the plural in the message.
- A new language is offered only when it ships whole ([Compatibility review](../review/compatibility.md)).
