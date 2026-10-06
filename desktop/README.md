# Bisa Desktop

Tauri 2 + React 19 shell over the Bisa engine. The app spawns the `bisa` binary as a
**sidecar node** (`bisa node --listen 127.0.0.1:<free port>`) and talks to it over the HTTP API
in [`docs/reference/http-api.md`](../docs/reference/http-api.md).

This file is how the app is built and the rules its code keeps. What each surface is *for* is
[`docs/guide/the-desktop.md`](../docs/guide/the-desktop.md) and [`docs/guide/the-ide.md`](../docs/guide/the-ide.md);
the Project IDE's architecture is [`docs/architecture/ide/`](../docs/architecture/ide/README.md);
every directory, store, model and localStorage key is mapped in
[`docs/architecture/crates/desktop.md`](../docs/architecture/crates/desktop.md).

## Development

```sh
cargo build -p bisa-cli                            # the sidecar
cd desktop && npm install
BISA_BIN=../target/debug/bisa cargo tauri dev  # full app
BISA_API_BASE=http://127.0.0.1:4477 npm run dev     # UI only, against a node you run
```

`BISA_API_BASE` also makes the Tauri shell adopt that node instead of spawning one; give it the
node's token too — `BISA_API_TOKEN=$(cat ~/.bisa/run/token)` — because every route but
`/health` and the node's other named exceptions — a public hook, the OAuth callback, the A2A card
and endpoint, an installed addon's files, a terminal session's four doors — answers `401` without one. When the shell spawns the sidecar it mints the token itself,
hands it over through the same variable, and the webview reads it with the `api_token` command;
`fetch` sends it as a header, and an `EventSource` or an `<img>` gets it in the query through
`withToken()`. Sidecar resolution: `BISA_BIN` → the node the bundle carries beside the executable
(`Contents/MacOS/bisa`, universal and signed with the app by `scripts/macos/lib.sh`, the build both `scripts/bundle-macos.sh` and `scripts/release-macos.sh` share) → `bisa` on
`PATH` → `target/debug/bisa` near the executable. Readiness is a `GET /health` poll, which ends
the moment the node does: a node refused its workspace costs under a second, not twenty.

The shell runs once (`tauri-plugin-single-instance`, first among the plugins): a second process of
the app hands its arguments to the running one, which brings its window forward, and ends inside
`build` — before it has a node. A dev run and the installed `Bisa.app` share the identifier
`dev.bisa.bisa`, so quit the installed app before `cargo tauri dev`, or the dev run hands off to it.
The node is started from `setup` (`sidecar::boot`), after that door, never from `main`.

```sh
npm run build          # typecheck + bundle — run this, not tsc alone: tsc does not catch module resolution
npm test               # node --test over every src/**/*.test.mjs, the theme contract included
cargo tauri build      # the app bundle
```

## The source tree

| Directory | Holds |
|---|---|
| `src/theme/` | `tokens.css` (the role contract), the theme stylesheets, accents, density, motion, `flow.css` for the canvas |
| `src/ui/` | **the kit** — every primitive a view may use, `icons.ts`, the one wrapper per library, and the platform's mark (`PlatformMark.tsx` shows the one file `logo/logo.svg`; `just app-icon` rasterises the same file into the icon set) |
| `src/views/` | the screens; `_work/`, `_studio/`, `_workflow/`, `_workbench/`, `_settings/` beside them |
| `src/shell/` | chrome that outlives every screen: sidebar, top chrome, the window footer (`StatusBar`), omnibox, the terminal layer, keymap, workspace data |
| `src/terminal/` | xterm and the typed Tauri commands |
| `src/notes/` · `src/draw/` · `src/pet/` | the three overlays — notes, drawings ([19](../docs/architecture/19-drawings.md)), the pet |
| `src-tauri/src/` | the shell: sidecar, PTYs, the login environment |
| `*.mjs` + `*.d.mts` + `*.test.mjs` | pure models beside their components |

## The four rules

**A view imports from `../ui` and nothing else** — never from `lucide-react`, `@radix-ui/*`,
`motion`, `monaco-editor`, `mermaid` or `@xyflow/react` directly. Each library is wrapped once, so
swapping an implementation is a one-file change; `src/ui/imports.test.mjs` fails the build otherwise.

| Dependency | Wrapped in |
|---|---|
| Radix primitives | `Dialog`, `Menu`, `ContextMenu`, `Popover`, `Tooltip` (read, never hovered: it takes no pointer, and a menu trigger wears none — the menu's `label` and a native `title` instead), `Tabs`, `SegmentedControl`, `Switch`, `ScrollArea`, `Separator`, `Avatar`, `Field`, `Button` (`asChild`) |
| `lucide-react` | `src/ui/icons.ts` |
| `monaco-editor` | `src/ui/monaco.ts` — the worker environment, the theme from the token roles (`editorTheme.mjs`); views use `CodeEditor` and `DiffEditor` |
| `motion` | `Tabs`, `AnimatedList`, `SidebarSection`, `useReducedMotion` |
| `class-variance-authority` · `clsx` + `tailwind-merge` · `sonner` · `micromark` | `Button` · `cn()` · `Toast` · `Markdown` (a ```mermaid fence is handed to `MermaidView`) |
| `mermaid` (lazy) | `src/ui/MermaidView.tsx` — the one viewer for `.mmd` files, fenced blocks in Markdown and in messages |
| `pdfjs-dist` (Apache-2.0, lazy) · `xlsx` (SheetJS Community Edition from npm, Apache-2.0, lazy) · `mammoth` (BSD-2, lazy) + `dompurify` (Apache-2.0/MPL-2.0) · `jszip` (MIT, lazy) | `src/ui/artifact/` — the artifact viewers (ide/12): `PdfView`, `SheetView`, `DocumentView`, `SlidesView`; each library is loaded when its kind is first opened and nowhere else |
| `@xyflow/react` (pinned, MIT) | `src/ui/FlowCanvas.tsx` — the designer's canvas and a goal's run drawn on it; themed by `src/theme/flow.css`, whose `--xy-*` names `flowTheme.test.mjs` checks against the installed stylesheet |
| `happy-dom` (dev) | `scripts/check-mermaid.mjs` only; `src/noHappyDom.test.mjs` asserts nothing under `src/` imports it |

**Nothing in the app names a colour.** `src/theme/` is data; `src/ui/` is behaviour. Six dials
compose on `<html>` — `data-theme` (five families, a light and a dark side each, from
`src/theme/themes/`; absent means *System*), `data-accent`, `data-density`, `data-font-ui`,
`data-font-mono`, `--type-scale` — plus a derived `data-scheme`. Every theme supplies every role
between `/* @roles:start */` and `/* @roles:end */` in `src/theme/tokens.css`;
`src/theme/roles.test.mjs` enforces it, and `src/theme/themes.test.mjs` holds every pairing the app
draws to a contrast floor (text 7:1, secondary text and accent ink 4.5:1, status colours 3:1) and the
three theme-id lists equal. The body is 14px at scale 1 (`--text-sm`); `--text-3xs` is the true meta
size; rows that hold text read `--spacing-row-sm` live. The bundled faces (`src/theme/fonts.css`) are
OFL-1.1 `@fontsource` packages. **The accent means *your attention***; `accent-ink` is the accent at
text contrast. Under `prefers-reduced-motion` the three motion durations collapse to 1ms, not 0,
because Radix waits on `animationend`.

**`src/ui/icons.ts` is the one map from a domain concept to a glyph**, in namespaced maps so a goal
status, a step kind, a step state, a work-item state and a gate cannot borrow each other's glyph.
Lucide identifiers that spell a banned word are excused in place, with the reason beside them.

**Logic whose wrong answer is a wrong *fact* goes in a plain `.mjs` module beside its component**,
with a `.d.mts` for the types and a `.test.mjs` beside it, so `npm test` reaches it without a DOM.
There is no jsdom in this repository: a wrong pixel is visible, a wrong fact is not. A model that
restates something Rust owns — the tag vocabulary, the assignee wire grammar, the pet sheet geometry,
the goal statuses, the step kinds — reads the Rust source in its test. A journey that crosses several
models — glance, keep, split and restart in the workbench; a shell's life and its checkpoint; the
Board switch and the mode memory; the notes repository from empty to shared; retiring a goal; an
afternoon of conversations with the agents in a checkout — is a
**scenario** under `src/scenarios/`: one `.test.mjs` per journey, stepping the reducers the way the
components do and reading after each step what the screen would show. Still no DOM: a scenario tests
facts in sequence, not pixels.

## Types

`just gen-types` writes `api-schema.json` and `src/types.gen.ts` from every Rust type deriving
`schemars::JsonSchema`; `just check-types` fails when they drift. Shapes with no schema — engine bus
payloads, store rows — are mirrored by hand in `src/types.hand.ts`, each annotated with its source
file, and `src/types.ts` re-exports both. A goal carries no status of its own: `status` on a row, a
`GoalView` or an inbox row is the node's projection (`GoalStatus`), and so are `holder` and the run
`strip` beside it — the desktop labels them (`views/_goals/goalStripModel.mjs`) and never recomputes
them. `Assignee` is read
as a tagged object and written as a wire string through `assigneeWire.mjs`.
