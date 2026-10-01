# locales/ — every word the platform says to a person

One folder per language, one format — [Fluent](https://projectfluent.org/) (`.ftl`) — read by two
runtimes: `fluent-bundle` in the crates (`crates/bisa-i18n`) and `@fluent/bundle` in the desktop
(`desktop/src/i18n/`). English is the language shipped; another language is a copy of `en/` with
the same file names and message ids, translated, and its tag added to `AVAILABLE` in both runtimes
(`crates/bisa-i18n/src/locale.rs`, `desktop/src/i18n/localeModel.mjs` — a guard holds them equal).
See [17 — Internationalisation](../docs/architecture/17-internationalisation.md).

```
en/
  settings.ftl   the settings registry: setting-<key> · .help · .choice-<value>
  errors.ftl     refusals: error-<crate>-<variant>, the node's own
  problems.ftl   workflow validation: problem-<kind>
  engine.ftl     the engine's sentences: summaries, reasons, stops, notes
  cli.ftl        the CLI's lines and help
  catalog.ftl    the content seam: catalog-<kind>-<slug> · .description · .tagline — the rule, no entries
  desktop/*.ftl  the desktop, one file per area (shell, ui, inbox, goals, …, format)
```

**Ids** are stable identifiers, kebab-case, area-prefixed; a widget's several strings are attributes
of one message (`.label`, `.help`, `.placeholder`, `.aria`). **Plurals** are selectors in the message
(`{ $n -> [one] … *[other] … }`), never code. **Arguments** are the world's words — an id, a name, a
path, a count — and are never translated. **Never here**: a person's own content (titles, bodies,
commit messages), what is for a model (prompts, tool descriptions, intake refusals), what is for a
log, identifiers (settings keys, routes, events, CSS).
