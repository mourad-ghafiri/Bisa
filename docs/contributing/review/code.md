# Code review

Every pull request, every time. Each question is answered *yes*, or *not applicable* with a reason.

## The change itself
- [ ] It does what its issue asked — no more. Anything else is a separate pull request.
- [ ] It sits where its rule lives: a domain rule in `bisa-core`, a write through the engine, a fact of
  the desktop in a model ([07 — Layering](../../architecture/07-layering.md)).
- [ ] It keeps the architecture's rules ([Architecture](../../architecture/README.md)): the files are the
  truth and the index a cache; one place per invariant; only the engine has effects; only
  `WorkflowRun::apply` changes a run.
- [ ] It keeps the numbered invariants it touches ([02 — Domain model](../../architecture/02-domain-model.md),
  [14 — Collaboration](../../architecture/14-collaboration.md)), and the page that states one is updated
  when the change reaches it.
- [ ] The crate graph is unchanged, or the change to it is argued on the issue (the layering test holds
  the edges).
- [ ] It stays generic: no code, agent, template or start event made for one scenario or one customer —
  generic reach, and documentation of how to use it.

## Errors and failure
- [ ] A refusal is a typed error that says what was refused and why, in a message of the catalog —
  never a panic, never `unwrap`/`expect` in production code of `bisa-core`.
- [ ] No error is parsed from prose; no `Result` is dropped (`warn_on_err`/`debug_on_err` are the only
  sanctioned ways) — a discarded write is a lie.
- [ ] Every write is atomic or compare-and-swap as the store requires
  ([08 — Persistence](../../architecture/08-persistence.md)); nothing can make a change unrecoverable.
- [ ] Nothing waits for ever: a lock is taken the sanctioned way, a stream has an end, a loop has a bound,
  a queue has a cap.

## Tests
- [ ] A fix has a test that fails without it.
- [ ] Every new rule has a unit test beside it; every new route, verb or tool has its node, CLI or MCP
  test; a feature the binary reaches has its journey step.
- [ ] The tests follow [the testing rules](../testing-rules.md): nothing destructive, temporary
  directories only, fakes for every remote, git isolated.
- [ ] No test was removed, skipped or loosened — or the pull request says why it was wrong.
- [ ] The author ran the commands they list, and `just verify` before marking the pull request ready.

## The desktop
- [ ] Logic lives in a `.mjs` model with its `.d.mts` and its test; the component only draws.
- [ ] A view imports from `../ui` only; a new surface uses the kit; colours and radii come from tokens.
- [ ] A new poller or clock is gated on visibility; a cache is bounded
  ([Performance](../performance.md)).
- [ ] Screenshots of every screen it changes, light and dark, are in the pull request.

## Generated and shared files
- [ ] Generated files (`desktop/api-schema.json`, `desktop/src/types.gen.ts`, the generated reference
  pages, `THIRD-PARTY-NOTICES.md`, the hakari table) are regenerated in this change, never edited.
- [ ] A new route, setting, tool, verb, kind, path or locale followed its recipe ([Recipes](../recipes.md))
  to the last file.

## Hygiene
- [ ] No secret, key, token, personal path or e-mail address — in code, tests, fixtures, docs or
  screenshots.
- [ ] No new `unsafe`, lint `allow`, `eslint-disable` or `#[ignore]` without a written reason.
- [ ] `cargo fmt` on the files it touched; no new warning.
- [ ] Names say what they are, in the project's vocabulary ([Terminology](../terminology.md)).
