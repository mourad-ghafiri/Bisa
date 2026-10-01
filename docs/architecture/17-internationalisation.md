# 17 — Internationalisation

Every word the platform says to a person goes through one translation layer. English is the
language shipped; another language is a folder. This page is the model — what a word is, where it
lives, who renders it, what is never translated — and the guards that hold the sources to it.

## The model

**A sentence is data.** The platform authors prose in many places: a refusal, a setting's label, a
validation problem, a step's summary, a line the CLI prints, a button in the desktop. None of it
travels as a sentence. It travels as a **`Text`** — the **id** of a message in the catalog and the
**arguments** the message interpolates (`bisa_core::text::Text`, `core/text.rs`; on the wire
`{ "id": "error-goal-not-found", "args": { "id": "g1" } }`). An argument is a scalar — a string, an
integer, a number — because that is what a message selects on and formats; a step id, a path, a
person's own title ride as arguments and are shown as they are.

**The catalog is one folder, one format.** `locales/<lang>/` holds every message in
[Fluent](https://projectfluent.org/) (`.ftl`): `settings.ftl`, `errors.ftl`, `problems.ftl`,
`engine.ftl`, `cli.ftl` for the crates, `desktop/*.ftl` for the desktop, one file per area
([`locales/README.md`](../../locales/README.md)). Ids are stable identifiers, kebab-case,
area-prefixed; a widget's several strings are attributes of one message (`.label`, `.help`,
`.placeholder`); plurals are selectors in the message (`{ $n -> [one] … *[other] … }`), never code;
the brand is a term (`-bisa`). Two runtimes read the same files: `fluent-bundle` in the crates
(`crates/bisa-i18n`), `@fluent/bundle` in the desktop (`desktop/src/i18n/`). Isolation marks are off
in both — a placeable is its characters — and direction is the document's (`<html dir>`).

**The shell's own menus** — the menu bar icon's *Open Bisa* · *Show in Dock* · *Quit Bisa*, the Edit
menu's four verbs — are the one place words are drawn outside the webview. The Tauri shell holds no
catalog: it builds its menus in English and the webview pushes the words once at boot
(`shell/shellWords.ts` → the `shell_words` command → `words.rs`), so they follow the window's language.

**The edges render.** A person reads at three places, and each renders a `Text` in the person's
language: the **desktop** (`i18n/l10n.mjs`: `t(id, args)`, `attr(id, name, args)`, `tx(text)`; a
file with a `t` of its own imports the door as `tr`; `i18n/rich.tsx`'s `rich(id, slots, args)` says a
sentence with an element inside it — the message carries `<path/>`, the caller the element), the
**CLI** (`bisa-i18n`, the locale from `--lang` or `LC_ALL` · `LC_MESSAGES` · `LANG`), and the
**node's error body**, rendered by the request's `Accept-Language` as a courtesy to anyone reading
raw HTTP — the desktop sends the header on every request. Everything else the node sends is a
`Text` the reader renders. Logs stay English through `Display`: a log is the developer's, and a
guard keeps every typed refusal covered by the catalog so the two cannot drift apart unseen.

**The language is one machine setting**, `appearance.language` — `system` (the machine's languages,
negotiated against what ships) or a shipped tag — followed by the desktop (`i18n/localeStore.ts`,
at boot and on `settings_changed`) and mirrored as `bisa.locale` so `index.html` stamps `<html lang
dir>` before the first paint. The catalog is installed before any module of the desktop runs
(`i18n/boot.ts` is `main.tsx`'s first import), so a table a model freezes at import time is in
the right language; a change of language mirrors the tag and opens the window again once. A shipped language is negotiated the same way on both sides
(`AVAILABLE` in `i18n/locale.rs` and `desktop/src/i18n/localeModel.mjs`, held equal by a guard).

**Numbers, dates and spans** are `Intl`'s in the locale's shape, through one module
(`desktop/src/i18n/format.mjs`); the words among them — *just now*, *Today*, a span's unit letters,
*ago*, *in* — are messages of `locales/en/desktop/format.ftl`.

## What is never translated

- A person's own content: a title, a message body, a note, a commit message, a name — and what
  the platform writes *as* content, such as the notes repository's commit subject; such a line is
  marked `// content, never translated` and the ratchet leaves it out, as it leaves out a line
  marked `// for the agent`, `// for the machine` (a wire word another program parses — and an
  example of one in a placeholder: a branch name, a URL, a command) or
  `// for the log`; a marker on its own comment line exempts the statement below it, and in
  markup the same words in a JSX comment (`{/* for the machine */}`) or inside the tag before the
  prop.
- What is for a model: a prompt, a skill's body, a tool's description (`bisa-mcp`), an intake
  refusal (`engine/intake.rs`) — a model reads English, whatever the person's language.
- What is for a log, a panic, an `expect`, a `Display` for the developer.
- An identifier: a settings key, a route, an event name, a keymap id and its chord, an icon name,
  a CSS class, a `data-*` word.
- Documentation and its generators (`RouteDoc`, the settings and catalog pages). Whole files whose
  English is another program's — the A2A protocol (`a2a.rs`), `route_docs.rs`, every `bin/` —
  are named in the ratchet's `NOT_FOR_A_PERSON` and never scanned; the desktop names its own
  (the crash boundary, the review prompt builders, the page inspector) in `ratchetModel.mjs`.

**The catalog's display fields are a seam, not a translation.** What ships in `library/` speaks for
itself — an entry's name and description, a pet's tagline are the file's own words — so English
ships no message for them. A language may translate a display field by its entry's id
(`catalog-<kind>-<slug>`, `.description`; `catalog-pet-<id>`, `.tagline`; `catalog-addon-<id>`, `.description` — `locales/en/catalog.ftl`
documents the rule and holds no entries), and the node renders the field through
`bisa_i18n::content` by the request's language, the file's text when no message ships
(`crates/bisa-node/src/catalog.rs`, `pets.rs`, `addons.rs`). A template's body, a skill's steps, a prompt are
never in reach of it: those are for a model.

## Where things live

| Place | Holds |
|---|---|
| `locales/` | the catalog, one folder per language; `README.md` the rules of the folder; `catalog.ftl` the content seam's ids, with no entries |
| `crates/bisa-core/src/text.rs` | `Text`, `Arg`, the `text!` macro — the sentence as data, no dependency, no I/O |
| `crates/bisa-i18n/` | `Locale` (parse, environment, `Accept-Language`, negotiation), `Catalog` (one bundle per shipped language, compiled in), `render`, `attribute`, `english`; the ratchet ([crates/i18n.md](crates/i18n.md)) |
| `desktop/src-tauri/src/words.rs` | `ShellWords` — the menu bar icon's fixed lines and the Edit menu's verbs, pushed by the webview through the `shell_words` command (`desktop/src/shell/shellWords.ts`); the shell holds no catalog and keeps English until the webview has spoken |
| `desktop/src/i18n/` | `l10n.mjs` the door, `rich.tsx` a sentence with an element in it, `localeModel.mjs` the languages and the setting, `format.mjs` the digits and the time helpers, `catalog.ts` the bundled files, `boot.ts` before any other module runs, `localeStore.ts` the language followed, `testing.mjs` + `preload.mjs` English for `node --test`, `ratchetModel.mjs` + `ratchet.mjs` the guard's scanner ([crates/desktop.md](crates/desktop.md)) |

## Guards

Rules that only exist on this page decay; each is a test.

| Rule | Test |
|---|---|
| every `.ftl` under `locales/` parses, for both runtimes; a shipped language ships every namespace; the desktop and the crates name the same shipped languages | `crates/bisa-i18n/tests/it/catalog.rs`, `desktop/src/scenarios/i18n.test.mjs` |
| every `text!("…")` in the crates names a message, with exactly the arguments the message's placeables read, and every message in a crate-owned file is said (or is a setting's derived id, or a content seam id) | `crates/bisa-i18n/tests/it/catalog.rs` |
| a message in `catalog.ftl` names an entry the library ships and only the display fields its kind has | `crates/bisa-i18n/tests/it/catalog.rs` |
| every `t("…")`, `tr("…")`, `rich("…", …)` and `attr("…", "…")` in the desktop names a message and its attribute, and every desktop message is said | `desktop/src/scenarios/i18n.test.mjs` |
| **the ratchet**: the bare sentences per source file — a literal with a space and a few letters that is not a log, a panic, an attribute, a class list, an identifier; and, in the desktop, **every word a screen draws**: a JSX text node with a letter in it whatever punctuation it holds (`,` `;` `(` `)` `|` `=`) and whatever expression stands beside it, a literal an expression child renders (`{n === 1 ? "agent" : "agents"}` — a plural spelt in code), a sentence-shaped prop, and a template of one word beside a placeable (`${n} more`), each read off the TypeScript parser's tree and excused only by where it stands or by a mark, never by a `className` sharing its line — equal the committed baseline, in `desktop/src` and in the node's and the CLI's crates; a rise fails, a fall asks for the baseline to be lowered (`just i18n-baseline`); both baselines are empty — `{}` — and stay so: a bare sentence anywhere in scope fails, and `i18n-ratchet --list` (or `node src/i18n/ratchet.mjs --list`) prints each one as `path:line: literal` | `crates/bisa-i18n/tests/it/ratchet.rs` over `tests/ratchet.baseline.json`; `desktop/src/scenarios/i18n.test.mjs` over `desktop/src/i18n/ratchet.baseline.json`, the scanner itself held by `desktop/src/i18n/ratchetModel.test.mjs` (each blind spot it once had, seen; a marked exception, still excused) |
| a sentence that sends a person to a Settings panel spells the path as the rail shows it — *Settings › group › panel* — the rail being one model a link's label is read from | `desktop/src/scenarios/settingsPaths.test.mjs` (the desktop's catalogs; one constant widens it to the crates') |
| a message the catalog lacks renders as its id, visibly, and is said once in the log; a placeable is not wrapped in isolation marks | `crates/bisa-i18n/tests/it/render.rs`, `desktop/src/i18n/l10n.test.mjs` |
| the catalog is installed before the first render, the language stamps `<html lang dir>` before the first paint, every request carries `Accept-Language`, `npm test` and `scripts/test desktop` speak English | `desktop/src/scenarios/i18n.test.mjs` |

## What is not yet in the catalog

The ratchet's scope is the node's and the command line's sources; the engine's are outside it, and
the engine says things to people too. Read the same way (`cargo run -q -p bisa-i18n --bin
i18n-ratchet -- --list --scope crates/bisa-engine/src`, 2026-09-30), the engine carries **572 bare
sentences in 61 files** — `intake.rs` 77, `executor.rs` 43, `guided.rs` 39, `codehost.rs` 30,
`ops.rs` 28, `security.rs` 23, `readiness.rs` 23 among them. Most are not for a person and would be
marked so under the ratchet: a prompt or a directive for a model (`intake.rs`, `framing.rs`,
`guided.rs`), the context a `warn_on_err` names for the log. The ones a person reads, and which a
later pass brings into the catalog — the work list, by file:

| Where | What a person reads in English today |
|---|---|
| `security.rs`, `inputs.rs`, `content.rs`, `classifier.rs` | the gate's question (*Allow `Bash`?*), a refusal's reason (*refused by the person who owns the work*, *remembered from your earlier answer on this goal or run*, *the classifier judged it harmful: …*), what a content screen withheld |
| `readiness.rs` | the setup gate's five lines and their fixes (*git was not found on your PATH*, *Run on …*) |
| `presence.rs` | a session's waiting words (*the {gate} gate*, *sign in to {provider}*), why it failed (*budget exhausted*, *wall clock exceeded*) |
| `guided.rs`, `ops.rs`, `executor.rs` | a run's or a step's reason as it reaches the journal and the Inbox (`interruption_note`, a launch failure's sentence) |
| `codehost.rs`, `projects.rs`, `ide/connection.rs` | a publish's or a connection's refusal |

Each is a `String` on a wire type today (`GateEntry.question`, `Decision.reason`, a readiness
check's `detail`), so moving one is a change of shape the desktop follows — the same road the
node's errors took. Until then the desktop and the command line render these as they arrive.

**A platform sentence in a post travels as its message.** A note the platform authors — the
Workflow Agent's in a goal's thread, the reply-cut note — is written as `MessageBody::said(text)`:
its English as the content, the `Text` beside it, and the index carries it on the row (`MessageRow.said`,
the said mark, schema version 26), so a reader renders the message in their language and never
its English (held by `bisa-store` `conversation::a_platform_sentence_rides_its_row_and_survives_a_rebuild`).
The desktop's timeline draws it through `timelineModel.contentOf` — `tx(said)` when the installed
catalog has the message, the content otherwise, never an id
(`desktop/src/views/_studio/timelineModel.test.mjs`); the window's catalog bundles every
namespace but the CLI's, the crates' `engine.ftl` included, so the messages the engine authors a
post with are there to render (`desktop/src/scenarios/i18n.test.mjs`).

## Adding a language

A copy of `locales/en/` under the language's tag, translated, its tag in `AVAILABLE` on both sides
and among the `appearance.language` choices ([recipe 25](../contributing/recipes.md#25-add-a-locale)).
Nothing else changes: the desktop bundles every shipped language's files, the node and the CLI
carry them compiled in.
