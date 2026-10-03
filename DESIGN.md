---
name: Bisa
description: A local-first agentic IDE whose one colour means "this waits on you".
colors:
  accent: "oklch(0.52 0.17 240)"
  accent-ink: "oklch(0.46 0.16 240)"
  accent-soft: "oklch(0.94 0.035 240)"
  accent-contrast: "oklch(1 0 0)"
  harbor-amber: "oklch(0.54 0.16 58)"
  orchard-blossom: "oklch(0.54 0.18 350)"
  dune-indigo: "oklch(0.52 0.19 265)"
  suede-cobalt: "oklch(0.5 0.17 250)"
  overlay-violet: "oklch(0.53 0.21 295)"
  overlay-teal: "oklch(0.52 0.11 190)"
  overlay-rose: "oklch(0.53 0.19 12)"
  bg: "oklch(0.965 0.006 235 / 0.86)"
  surface: "oklch(0.995 0.003 235 / 0.72)"
  surface-2: "oklch(0.865 0.012 235 / 0.92)"
  border: "oklch(0.55 0.02 235 / 0.28)"
  text: "oklch(0.22 0.015 235)"
  text-dim: "oklch(0.42 0.02 235)"
  selected: "color-mix(in oklch, oklch(0.22 0.015 235) 9%, transparent)"
  hairline: "color-mix(in oklch, oklch(0.55 0.02 235 / 0.28) 65%, transparent)"
  overlay: "oklch(0.2 0.02 235 / 0.35)"
  warn: "oklch(0.56 0.13 90)"
  warn-soft: "oklch(0.95 0.045 90)"
  danger: "oklch(0.49 0.2 22)"
  danger-soft: "oklch(0.955 0.045 22)"
  ok: "oklch(0.53 0.13 155)"
  ok-soft: "oklch(0.95 0.04 155)"
  step-event: "oklch(0.5 0.07 197)"
  step-gateway: "oklch(0.5 0.07 307)"
  step-loop: "oklch(0.5 0.07 122)"
  step-task: "oklch(0.5 0.02 255)"
typography:
  title:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "-0.025em"
  dialog-title:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.5
    letterSpacing: "-0.025em"
  body:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  row:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.45
  bar-title:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.25
    letterSpacing: "-0.025em"
  heading-group:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.5
  prose-h1:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.3
  prose-h2:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "18px"
    fontWeight: 600
    lineHeight: 1.3
  prose-h3:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.3
  prose-body:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.625
  label:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.4
  meta:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.4
  badge:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica Neue, Arial, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: 1.35
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.4
    fontFeature: "\"tnum\""
rounded:
  sm: "4px"
  control: "12px"
  card: "16px"
  pill: "9999px"
spacing:
  grid: "4px"
  row-sm: "28px"
  row: "36px"
  row-lg: "44px"
  chrome: "40px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-contrast}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "32px"
  button-default:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.body}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "32px"
  button-default-hover:
    backgroundColor: "{colors.surface-2}"
  button-default-active:
    backgroundColor: "{colors.selected}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-dim}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "32px"
  button-ghost-hover:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.text}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.danger}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "32px"
  button-danger-hover:
    backgroundColor: "{colors.danger-soft}"
  button-sm:
    typography: "{typography.row}"
    padding: "0 8px"
    height: "28px"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.row}"
    rounded: "{rounded.control}"
    padding: "6px 10px"
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.card}"
    padding: "12px"
  chip-neutral:
    backgroundColor: "{colors.surface-2}"
    textColor: "{colors.text-dim}"
    typography: "{typography.meta}"
    rounded: "{rounded.pill}"
    padding: "0 8px"
    height: "20px"
  chip-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.text-dim}"
    typography: "{typography.meta}"
    rounded: "{rounded.pill}"
    padding: "0 8px"
    height: "20px"
  wait-badge:
    backgroundColor: "{colors.accent-soft}"
    textColor: "{colors.accent-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "0 8px 0 6px"
    height: "20px"
  count-badge:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-contrast}"
    typography: "{typography.label}"
    rounded: "{rounded.pill}"
    padding: "0 4px"
    height: "16px"
  nav-row:
    backgroundColor: "transparent"
    textColor: "{colors.text-dim}"
    typography: "{typography.row}"
    rounded: "{rounded.control}"
    padding: "0 8px"
    height: "{spacing.row}"
  nav-row-active:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.text}"
  menu-item:
    textColor: "{colors.text}"
    typography: "{typography.row}"
    rounded: "{rounded.sm}"
    padding: "6px 10px"
  menu-item-highlighted:
    backgroundColor: "{colors.selected}"
  tooltip:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    typography: "{typography.meta}"
    rounded: "{rounded.control}"
    padding: "4px 8px"
  dialog:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.card}"
    padding: "16px 20px"
---

# Design System: Bisa

## Overview

**Creative North Star: "One Light On The Desk"**

Bisa is an Operate surface: a window a developer sits in for hours, running several coding harnesses beside terminals, an editor and an embedded browser. The system is built so that exactly one thing in the window glows, and that thing is whatever waits on the person. Everything else (the place you stand, the tab you are on, the chosen option, the identity of an agent) is drawn in neutrals that lean toward the family's hue. The accent is a summons, never decoration.

The visual world is a role contract, not a stylesheet (`desktop/src/theme/tokens.css`). Components ask for `surface`, `text-dim` or `danger-soft`, and whichever of the five families is mounted answers. Glass is the default and owns bare `:root`: cool translucent panes frosted over the desktop behind the window. Harbor, Orchard and Dune are opaque siblings that differ by the hue of their neutrals; Suede is the matte family with no drop shadows. Six orthogonal dials compose on `<html>`: theme, accent, density, UI font, mono font and type scale. Every family is held to a contrast promise and the role contract by tests (`theme/themes.test.mjs`, `theme/roles.test.mjs`).

Density is calm rather than packed: a 14px body, 12px as the reading floor, a 4px grid, three structural row heights that the Compact setting moves, sentence-case labels and open empty states. Motion is short, flat and skippable, and reduced motion is honoured at the token, so a component written next year inherits it.

**Key Characteristics:**
- One accent per family, spent only on what asks for the person's attention.
- Neutral "you are here": selection, active tabs and pressed toggles use the derived `selected` wash, never the accent.
- Five families, light and dark each, swapped as data on the same role names.
- Elevation is two theme-owned roles (`shadow-raised`, `shadow-floating`); components never name a shadow of their own.
- System UI font stack by default; Inter or Atkinson Hyperlegible and JetBrains Mono or IBM Plex Mono as bundled, offline dials.
- Doors, not prose: empty states are open (no box) and carry their next action.
- A workflow scans by family: each step kind's glyph wears a quiet tinted-neutral ink beside the shape that already tells it apart.
- A run's canvas is a live map: one moving accent flow toward where the run stands, a breathing ring on running work, a twice-called ring on a step waiting on you.

## Colors

A neutral field tinted by each family's hue, one saturated accent per family, and a status ramp (warn, danger, ok) that stays in the same hues across families so a board of states reads as one system. The frontmatter carries the default family, Glass light; the other families answer the same roles.

### Primary
- **Glass Clear Blue** (`accent`, hue 240): the summons. Primary button fills, needs-you and waiting counts, the "waiting on you" badge's dot, an "on" Switch and a checked Checkbox, live/working dots, focus rings, the caret, drag-and-drop targets and find matches. Glass dark answers `oklch(0.75 0.13 240)`.
- **Accent Ink** (`accent-ink`): the accent at text contrast, for accent-coloured words (the wait badge's label, links in running prose via `.prose-i a` and `.link-url`). `accent` alone is a fill, border or dot colour, never body text.
- **Accent Wash** (`accent-soft`): the ground of a summons: `WaitBadge`, `Chip tone="accent"` (running work), banners that ask the person to act now (Inbox's decision banner, the publish gate, `NeedsAction`, `AskCard`, `StepActions`, the proposed-plan landing, People's join requests, the editor's "Changed on disk" bar and the designer's save conflict), drop zones at 30 to 60% and find highlights at 80%.
- **Accent Contrast** (`accent-contrast`): text on an accent fill, and also on a `danger` fill; the two fills sit at the same lightness in every family by construction, so one role covers both.
- **Per-family accents:** Harbor Amber (hue 58), Orchard Blossom (hue 350), Dune Twilight Indigo (hue 265), Suede Cobalt (hue 250), each with its own dark-side values in `theme/themes/*.css`.
- **Accent overlays** (`theme/accents.css`): Violet (295), Teal (190) and Rose (12) replace only the four accent roles, with a light set and a dark set chosen by `data-scheme` (or `prefers-color-scheme` before any script runs). "Amber" in the picker means removing the overlay and using the family's own accent.

### Neutral
- **Frosted Page** (`bg`) and **Frosted Pane** (`surface`): Glass's ground and the panes laid over it, both translucent (alpha 0.86 and 0.72 light; 0.97 and 0.7 dark). Opaque families answer solid values (Harbor light `bg` 0.975, `surface` 0.995; dark 0.20 and 0.235).
- **Pressed Step** (`surface-2`): hover and pressed grounds, inset groups (`bg-surface-2/50` to `/70`), neutral chips, the segmented control's track, code blocks.
- **Edge** (`border`): the outer edge of a pane, card, control or popover.
- **Hairline** (derived, `border` at 65%): dividers inside a surface, such as between rows, under a dialog header, above its footer, under a tab strip.
- **Selected** (derived, `text` at 9%): where you are. The current sidebar row, the pressed toggle, the chosen option, a highlighted menu row, the keyboard cursor row, the active `IconRail` item. It leans with the family's text hue and needs no contrast promise because no word is drawn in it.
- **Ink** (`text`) and **Secondary Ink** (`text-dim`): body and meta. `text-dim` stays above 4.5:1 in every family on its own surfaces.
- **Scrim** (`overlay`): behind dialogs, with a 1px backdrop blur.

### Status
- **Warn** (hue 85 to 90): a condition you should know about, not one that blocks you. Used sparingly; never the summons.
- **Danger** (hue 22 to 25): destructive actions and failures; arrives as `danger-soft` behind `danger` text, or as the solid confirm in a destructive dialog.
- **OK** (hue 155): accepted, done, finished.

### Step-family inks (`theme/tokens.css`, `views/_workflow/familyInk.ts`)
Four quiet inks name what kind of step a glyph stands for: **Steel Cyan** (`step-event`, hue 197, what happens), **Dusty Mauve** (`step-gateway`, hue 307, what routes), **Sage** (`step-loop`, hue 122, what repeats) and **Slate** (`step-task`, hue 255 at near-zero chroma, what works; most steps are tasks, so structure is what the eye finds first). They are shared by every family: light values sit in `@theme`; the dark side answers `oklch(0.78 0.065 …)` (slate `0.78 0.02 255`) in plain blocks keyed on `data-scheme`, or `prefers-color-scheme` before any script runs. An accent overlay that lands near an ink moves the ink, never the accent: teal moves the event ink to hue 222, violet moves the gateway ink to 328, on both sides.

They are worn through `familyInk` / `familyFrame` / `familyColor` only: the kind glyph on canvas nodes, the palette, the inspector header, library thumbnails (SVG `color`), the current-step pill, a run's step rows and plan proposals. An event's ring and a gateway's diamond, the small marks that frame the glyph, take the ink's edge at 45% over a 10% wash. Goal step chips keep the state colour, not the family ink.

### Families (`desktop/src/theme/themes/*.css`)

| Family | Neutral hue | Accent | Opacity | Radii control / card | Raised | Floating |
|---|---|---|---|---|---|---|
| Glass (default, owns `:root`) | 235 cool | Clear blue 240 | translucent, frosted 18px blur, saturate 1.35 light / 1.2 dark | 12 / 16 | inner 1px light line + whisper shadow | inner light line over a soft deep drop |
| Harbor | 240 cool grey | Amber 58 | opaque | 10 / 14 | Tailwind's sm pair | Tailwind's lg pair |
| Orchard | 150 green-grey | Blossom 350 | opaque | 10 / 14 | Tailwind's sm pair | Tailwind's lg pair |
| Dune | 60 to 70 warm sand | Indigo 265 | opaque | 10 / 14 | Tailwind's sm pair | Tailwind's lg pair |
| Suede | 60 to 68 taupe, matte | Cobalt 250 | opaque; light `bg` 0.935 (paper), dark `bg` 0.245 | 12 / 16 | `none` | wide, soft, low-alpha bloom in the neutral's hue |

With no `data-theme` and a dark OS, the `prefers-color-scheme` block answers Glass dark before any script runs.

### Named Rules
**The One Summons Rule.** The accent means your attention. It appears on primary buttons, waiting-on-you badges and the needs-you count, "new" markers, an "on" Switch and a checked Checkbox (a decided state the person set), live/working dots and running states (a run canvas's live flow and step rings included), banners that ask for a decision now, focus rings and the caret, drag-and-drop target feedback, find matches and links in running prose. Nothing else wears it. Unread is news, not a summons: unread counts are neutral, and Inbox's source-tab counts are `quiet`. The sides of a conflict are neutral and told apart by glyph; tag names are neutral.

**The Neutral Here Rule.** The place you stand is not asking for anything. Selection, active tabs, pressed toggles, chosen options and identity chips use `selected`, `text`, or neutral tones; active indicator bars and tab underlines are drawn in `text` (at 70% on rails).

**The Waiting-On-You Rule.** "Waiting on you" is unified on the accent: the goal strip's holder tones are `you` = accent and `agents` = text (`views/_goals/goalStripModel.mjs`); Inbox and Pulse map the activity `wait` tone to accent ink; the NeedsAction, AskCard, YourMoveBand and StepActions treatments are accent-tinted. Warn stays for genuine caution.

**The Quiet Kind Rule.** A step's kind is never a summons or a state. Its ink is a tinted neutral (chroma at most 0.08, against the 0.12 and more of every accent and status), keeps at least 30° from the three status hues and from every family's accent and chosen overlay, and holds 3:1 against a card on its side. It goes on the glyph only, never on a card's ground, edge or text, and always beside the shape (ring, diamond, plain card) that already tells the families apart. `theme/stepInks.test.mjs` holds all of it.

**The Words Stay Solid Rule.** On Glass, `bg`, `surface`, `surface-2` and `border` carry alpha; text, the accent and status colours never do, and every contrast bar is measured over the composite on both white and black.

## Typography

**Body Font:** the system UI stack (`ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, …`), with Inter Variable (cv11, ss01; no root numeral setting, so `.tnum` always wins) or Atkinson Hyperlegible as opt-in dials.
**Mono Font:** the system mono stack (`ui-monospace, SFMono-Regular, Menlo, Consolas`), with JetBrains Mono Variable or IBM Plex Mono as opt-in dials.

**Character:** a tool's type: the face the person's OS already tuned, set small and steady, with hierarchy carried by weight and ink rather than size jumps. The bundled faces ship in the app and load on demand (`theme/fonts.css`, `shell/theme.ts`); there is no display face.

Every size is a multiple of `--type-rem` (`1rem × --type-scale`), so the text-size setting moves type and the row heights together, and composes with webview zoom. Each step carries its own line height (`theme/tokens.css`).

### Hierarchy
- **Bar title** (600, 16px `text-base`, tight leading, tracking-tight): the top bar's screen name, the page's only `h1` (`ui/PageHeader.tsx`, `level="h1"`).
- **Title** (600, 18px `text-lg`, tight leading, tracking-tight): the thing a detail is about (a goal, a team, a run) via `PageHeader`.
- **Dialog title** (600, 16px `text-base`, tracking-tight): dialog and alert headers.
- **Content heading** (600, 14px `text-sm`, 1.5, full `text`): the one heading tier inside content: `Section` titles (`ui/Card.tsx`) and the h2/h3 of views.
- **Body** (400, 14px `text-sm`, 1.5): the document default on `body`; empty-state titles at 600.
- **Document** (400, 14px `text-sm`, `leading-relaxed`): rendered Markdown (`ui/Markdown.tsx`); its headings are 22 / 18 / 16 at 600 and 1.3 (`.prose-i`). A fold that reads smaller (`ThinkingBlock`) passes 13px through `StreamedMarkdown`'s `textClass`.
- **Row** (400, 13px `text-xs`, 1.45): dense rows, sidebar rows, menu rows, inputs, tabs, empty-state hints.
- **Label** (600, 12px `text-2xs`, 1.4, `text-dim`): group and section labels in panes and lists (`SectionHeader`), field labels at 500.
- **Meta** (400, 12px `text-2xs`, `text-dim`): counts, timestamps, hints and explanatory paragraphs (`leading-relaxed`, `max-w-measure`). By far the most used step in the build.
- **Badge** (500, 11px `text-3xs`, 1.35): badges, glyph labels and the status bar's stat labels. Never running text.

### Named Rules
**The Sentence Case Rule.** Labels and headings are sentence case with no `uppercase` and no wide tracking; lowercase data (a kind, a tag, a step family) gets `capitalize`. A chip that stands alone at the head of its row (the goal holder chip, `views/_goals/HolderBadge.tsx`) capitalises only its first letter (`first-letter:uppercase`), while the model keeps the word as it reads mid-sentence. No eyebrows or kickers above headings, and no section numbers.

**The Twelve Floor Rule.** 12px (`text-2xs`) is the floor for anything a person reads; 11px is reserved for badges and glyph labels.

**The Measure Rule.** Running text is held to `max-w-measure` (`--container-measure: 36em`, about 70 characters of the face at any scale step), never Tailwind's `max-w-prose` (65ch ran past 85 characters). Every `h1` to `h3` wraps balanced and every `p` wraps pretty (`styles.css`); in a document, paragraphs, list items and quotes hold the measure while tables and code keep their width.

**The Tabular Rule.** Timestamps, counters, ids and code use tabular numerals (`.tnum`, `time`, `code`).

## Layout

The shell is a fixed frame: a frosted sidebar on the left (`shell/Sidebar.tsx`, gutter `px-2`), a top bar of `chrome` height (40px; 36px compact) holding the screen title and a 256px search door, a content column, optional right panes with a 40px `IconRail`, and a footer status bar of the same `chrome` height. Panes are marked `data-pane` so a glass family frosts them.

Spacing sits on Tailwind's 4px grid: tight inside a group (`gap-1` to `gap-2`), clearly more between groups (`gap-5` to `gap-8`). Gutters are `px-6` on index screens, `px-4` on panes, inspectors and page headers, `px-2` to `px-3` in rails and trees; dialogs use 20px sides.

**The One Band Rule** (`ui/ScreenBar.tsx`). Every index screen (Inbox, Agents, Teams, Workflows, Goals, Pulse, Channels, Messages) opens with the same band under the chrome: at least 44px tall, on the `px-6` gutter, one hairline under it, never scrolling with the list. Its view switch is a `Tabs` with `bare`, standing the band's height so the underline lands on the band's hairline (one line, never a strip's short line plus the band's); a tab's own inset is given back so its word starts on the gutter. Then what narrows the list (search, filters), wrapping among themselves on a narrow screen. At the right edge: the count, quiet tools (re-read, the Browser door), then the screen's one primary create action, always last. A band that wraps keeps its tabs on its base (their underline still on the hairline) and its count and primary on its first line. Tags sit at the head of the list under the band (`TagFilterBar`, `mb-3` to `mb-4`), right above what they narrow. A screen that is empty or still reading returns its empty state or skeleton without the band.

Components respond to their container, not the viewport: Agents and Teams' list-detail splits at `@2xl`, Settings › Appearance tiles at `@xl` / `@2xl`, the workflow inspector's forms at `@xs` / `@sm` / `@lg` under the Inspector's `@container` root, a goal's step rows show their holder and time columns only from `@md` of the step list (`StepRow`; narrower, the name keeps the row), and every dialog body is an `@container` (`ui/dialogLayout.mjs`), so a form reads the same in a dialog, a pane or a full screen. The Inbox list takes `clamp(16rem, 38%, 22rem)` of its screen. Card grids add columns from their own room, never the window's: the workflow library and the template gallery are `repeat(auto-fill, minmax(13.75rem, 1fr))` (`libraryLayout.mjs`), three columns beside a 1024px window's sidebar, four at 1440, six at 1920. Lists of rows fill the body's width, left on the gutter, never a centred column; a goal's progress list caps at `max-w-4xl` on the page's edge so a step's state stays near its name. Placeholders are open empty states, not boxed panels.

**A detail beside its list stays in view.** On Agents and Teams the detail column is sticky beside the roster (`@2xl`), with a scroll of its own when it is the taller: the roster ran past two screens while the detail fit in a third of one, and opening the eleventh agent scrolled its detail out of sight. Stacked under a narrow roster it scrolls with the page.

**Room for the work.** Side columns give way before the work does. The Details pane (`shell/auxPaneModel.mjs`) takes up to seven tenths of the room but never leaves the screen beside it under `SCREEN_MIN_WIDTH` (420px): on a 1024px window the default pane gave a goal's page 364px and its steps lost their names. The workflow designer shares its row the way the Project IDE does (`ideColumnsModel.fitColumns`): past the canvas's least, the right column narrows to its own least, then the Steps palette folds to its glyphs (`Palette` `compact`: each kind's name moves into its tooltip and accessible name, the groups parted by hairlines). Both are drawn widths of the moment; the widths a person stored are never touched.

Three structural heights carry every list, and density moves only these (`theme/density.css`): `row-sm` 28px (trees, graphs), `row` 36px (dense lists and every sidebar row; 32px compact), `row-lg` 44px (Inbox rows; 38px compact). Pulse rows sit on `row`. Padding and gaps do not shrink in Compact, so dense rows stay legible.

**Dock clearance.** The notes and draw docks float over content at a placement the person chose and are never moved. Each dock publishes its painted box (`ui/dockClearance.ts`); the content column stamps `--dock-clear-bottom` and `data-dock-clear` only while a dock is in its lower half, and `styles.css` adds that much room after every `[data-scroll-keep]` scroll region and `[data-dock-room]` virtual list, with an 8px gap. Sideways strips (`.overflow-x-auto`) are left alone; a bottom-pinned composer reads `useDockOverlap` to keep its right-end controls reachable.

**Settings** (`views/Settings.tsx`): one content width, `max-w-3xl`, for every panel, so moving between panels never moves the column. Registry settings are rows in one Card split by hairlines (`views/_settings/RegistryPanel.tsx`), each key's own spelling behind a copy glyph whose tooltip is the key. Explicit-save forms end in the shared `SaveFooter` (`views/_settings/SaveFooter.tsx`): right-aligned under a hairline, "Unsaved changes" beside Discard and Save while the form holds edits, and leaving the panel with unsaved edits asks first (`unsavedModel.mjs`). Rows that write as you go show a quiet `SavedNote` ("Saved", `text-dim`, `role="status"`, gone after 2s); typed settings keep a draft and commit it on blur, Enter or unmount, never per keystroke (`settingDraftModel.mjs`).

**Status bar** (`shell/StatusBar.tsx`): the footer is an `@container`, and its read-outs are glyphs with values, never words: the terminals, harnesses, ports and browser-tab counts draw a glyph and a number, and the network draws its glyph and a dot in its tone (danger down, ok up, quiet before the first read) with the word *VPN* only while a tunnel is up (`networkStatModel.barWord`). Each trigger's name and tooltip say what it counts. Every stat opens from a real button trigger (toggles carry `aria-pressed`), with no button nested inside another; a row in a count's list that cannot open says why the kit's way (a Tooltip on a focusable wrapper and words for a screen reader), never in a `title`. Toasts sit above the footer (`ui/Toast.tsx` offsets by the `chrome` height).

**Project IDE columns** (`views/_workbench/ideColumnsModel.mjs`): the rail and the right panel keep the widths the person set while the centre keeps 360px; past that the right panel gives way first, to its least, then the rail to its, and then the rail folds — for as long as the window is that narrow, never in the stored widths or switches. A folded rail's toggle reads shut, and asking for it (the button or `⌘B`) makes its room by shutting the right panel: the last ask wins. The centre's landing shrinks with its column and its path ends in an ellipsis.

**Element defaults sit in the base layer** (`styles.css`): `h1`–`h3` balance and `p` wraps pretty under `@layer base`, because `text-wrap` sets the longhand `white-space: nowrap` does and an unlayered element rule would outrank every layered utility — a `truncate` paragraph would wrap.

**Footer usage** (`shell/UsageStat.tsx`, `shell/HarnessUsageLine.tsx`) is capped at `max-w-[60vw]`; on a narrow line the reset note is what gives way (it truncates first) so every meter stays readable.

### Known limits (decisions, not rules)
- **Canvas zoom:** the workflow canvas opens with `fitView` (`ui/FlowCanvas.tsx`), so on a tall graph the whole flow fits and node glyphs, inks and rings draw small until the person zooms.
- **Workflow thumbnails** (`views/_workflow/thumbnailModel.mjs`, `WorkflowThumbnail.tsx`) draw the designer's own layout at canvas coordinates and fit it into the card with `preserveAspectRatio="xMidYMid meet"`. There is no fixed thumbnail scale: library thumbnails are small, a tall or wide workflow draws smaller still, and the stroke compensates from the scale the card's box fits the picture at (`max(1.5, 1.25 / fitted)`), so a line is never thinner than about a pixel and a tall flow reads as its shape.
- **Atkinson Hyperlegible** ships only 400 and 700, so medium (500) and semibold (600) fall back to the nearest weight; closing it needs a new font dependency.
- **Font-load reflow:** choosing a bundled, non-system face loads it on demand, and text reflows once when it arrives.

## Elevation & Depth

A hybrid, owned per family. Depth is first tonal (`bg` under `surface`, `surface-2` for pressed and inset), then two shadow roles that Tailwind's `shadow-sm`, `shadow-lg` and `shadow-xl` resolve to, so a component asks for "raised" or "floating" and never names a shadow. On Glass, a third layer is the material itself: panes blur what is behind them.

### Shadow Vocabulary
- **Raised** (`--shadow-raised`, via `shadow-sm`): primary and default buttons, cards, the pressed segment. Glass: `inset 0 1px 0 0 oklch(1 0 0 / 0.55), 0 1px 2px 0 oklch(0.2 0.02 235 / 0.06)`; Harbor, Orchard, Dune: `0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)`; Suede: `none`.
- **Floating** (`--shadow-floating`, via `shadow-lg` / `shadow-xl`): menus, popovers, tooltips, dialogs. Glass: `inset 0 1px 0 0 oklch(1 0 0 / 0.6), 0 12px 32px -8px oklch(0.2 0.02 235 / 0.28), 0 2px 6px -2px oklch(0.2 0.02 235 / 0.1)`; opaque families: `0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)`; Suede: `0 16px 40px -12px oklch(0.27 0.014 60 / 0.22)`.
- **Material** (`--material-blur`, `--material-saturate`): Glass `blur(18px) saturate(1.35)` light, `saturate(1.2)` dark, applied only to `[data-pane]` elements and the page's ground (`theme/material.css`); opaque families answer `0px` and `1` and never match the rule. The ground frosts on a layer under the content (`body::before`, laid in `styles.css`), never on `body` itself: an element with a backdrop-filter is the Backdrop Root of everything inside it (drafts.csswg.org/filter-effects-2), so a frosted `body` would leave every dialog, menu and pane nothing to frost, and the page's words would read through them (`theme/themes.test.mjs` holds it).

### Named Rules
**The Two Shadows Rule.** Raised or floating, from the family; nothing else. Interaction-specific lifts (the Board card in flight, the tree's nest ring) live as named classes in `styles.css`, not in components.

**The Panes Frost Rule.** Only panes frost. A dialog over a right panel stacks two blurs, which is the most there is; a surface read for minutes (the notes overlay) lays its own near-opaque ground. Code and terminals stay solid.

**The Live Page Rule.** A browser tab is a native view that paints over every DOM element, so the app decides what may stand over it. A *surface* — a dialog, a menu, a popover, the palette, a maximized Notes or Draw panel — makes the page step aside while it is open (`ui/openSurfaces.ts`). A *floating overlay* — the Notes and Draw panels while floating, their docks, the pet, an addon window — never does: it says its painted box (`shell/browserClear.ts`), and the browser layer cuts a hole for it (`browser.rs`, `BisaBrowserLayer`: a Core Animation mask for what shows, `hitTest:` for what is clicked), so the overlay shows and takes its own clicks while the page stays full size and live around it. Never freeze a page into a picture or shrink it to make room for an overlay. Off macOS the layer cannot cut yet; an addon window still hides by intersection there.

## Shapes

Soft, consistent corners from two theme-owned radii: `radius-control` for buttons, inputs, rows, menus, popovers and tooltips, and `radius-card` for cards, dialogs and tiles (Glass and Suede 12 / 16; Harbor, Orchard, Dune 10 / 14). Small 4px corners (`rounded`) are for menu-item highlights inset from the panel's edge, checkboxes and the focus ring; pills (`rounded-full`) for chips, badges, tags and dots. Pixel radii (`rounded-[Npx]`) are forbidden by a test. Borders are 1px; a dashed border means "absent" or "add here" (an unbound agent slot, a missing artifact, a drop zone, a placeholder chip), never a frame for content.

## Components

### Buttons
Decisive and ranked: the split is consequence, not looks (`ui/Button.tsx`).
- **Shape:** the control radius, 1px border; `md` 32px tall with 12px sides, `sm` 28px with 8px, `icon` 28px square.
- **Primary:** accent fill, accent-contrast text, raised. Hover lowers opacity to 90%, press to 80%.
- **Default:** surface with the edge border, raised; hover `surface-2`, press `selected`.
- **Ghost:** transparent, `text-dim`; hover `surface-2` with full ink. For actions inside a row.
- **Danger:** transparent outline with danger text, `danger-soft` on hover. A destructive confirmation fills its one button solid danger (`SOLID_DANGER`).
- **States:** a press darkens the ground and never moves the button; disabled is 45% opacity with no pointer events. All transitions use `.anim` (colour, border, opacity, transform at `motion-fast`).
- **Disabled with a reason** (`disabledReason`): the reason rides a focusable wrapper with a Tooltip and sr-only words beside the button, so pointer, keyboard and screen reader all learn why. The wrapper stands whenever the prop is passed (the Tooltip goes `active` only while disabled), so a button that disables under the keyboard is never remounted.
- **Destructive confirmations:** what cannot be taken back asks first in a danger `ConfirmDialog`: stopping a run (`views/_workflow/StopRunDialog.tsx`), Undo all (its title names the file count), discarding a goal's workflow drawing that differs from the stored one (`draftChanged`), rotating a hook secret, retracting a message.
- **Delete from a list row:** a record you would otherwise have to open to delete (a note, a drawing) wears a trash on its row: a 24px ghost glyph at the row's right edge, `text-dim` at rest and `danger` on `danger-soft` under the pointer, shown on hover or focus (`.row-actions`). It is the row button's sibling in a `group relative` wrapper, never nested inside it, its name says which record (*Delete "Roadmap"*), the row reserves its width so text never runs under it, and it asks in the same danger `ConfirmDialog` as the open record's Delete (`notes/NoteOverlay.tsx`, `draw/DrawOverlay.tsx`).

**The Irreversible Asks Rule.** What cannot be taken back is asked first, in a danger `ConfirmDialog` with one solid danger confirm that names what will go. What can be undone, or an untouched draft, goes at once.

### Chips and badges
- **Neutral chip** (default tone): a borderless pill in `surface-2/70` with `text-dim` words, 20px tall, 12px medium. A row of tags reads as words, not buttons.
- **Quiet chip:** outlined in `border`, transparent. The state that has not happened yet (pending, skipped, cancelled).
- **Status chips:** `accent` for running, claimed or in-progress work; `warn` for waiting-at-a-condition and review; `danger` for failed, rejected or blocked; `ok` for done and accepted. Each carries a glyph from `ui/icons` as well as colour. A step that is still *pending* is no chip at all, only dim words (`StepStateChip`): pending is the absence of a state, and a column of ten bordered "pending" chips buried the one step that had moved.
- **Tag filters** (`TagFilterBar`): the six most-used tags, then *N more* to unfold the rest (*Show less* folds them back) once two or more would hide; a tag that is on always shows. Fourteen tags in a row were a wall to read past before the first card.
- **Avatars** (`ui/avatarModel.mjs`): an identicon's ground is OKLCH lightness 0.52 at chroma 0.13 in ten hues (never violet), so its white initials read at 4.5:1 or better in every hue; the test measures each one.
- **Wait badge:** the one badge that always catches the eye: accent wash, accent-ink 12px semibold label, a 6px accent dot as its mark.
- **Count badge:** accent (or danger) fill with accent-contrast numerals for what needs the person; `neutral` (edge-bordered `surface-2`, `text-dim`) for unread; `quiet` for counts that are facts, not calls (Inbox's source tabs). Dots for "that", numbers for "how many"; the working dot pings only under `motion-safe`.

### Cards / Containers
- **Corner Style:** the card radius.
- **Background:** `surface` on the page's `bg`.
- **Shadow Strategy:** raised (see Elevation).
- **Border:** 1px `border`; clickable cards brighten the border toward `text-dim/30` and move to `surface-2` on hover. A clickable card is one `<button>` and contains no other buttons.
- **Internal Padding:** 12px.
- **Nesting:** never a bordered card inside a bordered card; inner groups are `surface-2/50` with the control radius, or rows split by hairlines.
- **Section:** a heading at 14px semibold full ink (the content-heading tier) over its content, with an optional right-hand action.

### Inputs / Fields
- **Style:** native controls with one class (`CONTROL`): surface ground, 1px edge, control radius, 6px by 10px padding, 13px text, `text-dim` placeholder; hover nudges the border toward `text-dim/40`.
- **Focus:** the border turns accent and nothing else; no ring, no halo (`styles.css` removes the keyboard outline on fields). Grouped controls with a glyph (`.control-group`) turn the outer box's border accent.
- **Disabled:** 60% opacity, not-allowed cursor. Field labels are 12px medium `text-dim` above the control; hints 12px below.
- **Error:** `Field`'s `error` is the reason a value was refused or put back, in 12px `danger` text under the control with `role="alert"`; the wrapped control gets `aria-invalid` and the error in its `aria-describedby`. Said where the value was typed, never only in a tooltip.
- **Hints sit outside the label:** for Field, Checkbox and Switch the hint is a sibling of the `<label>`, tied to the control by `aria-describedby`, so it describes the control rather than naming it.
- **Labelled** (`ui/Labelled.tsx`): the same caption-over-content shape built from a `<div>`, for a label over a group of controls. Never wrap a group of controls or a copy button in `Field`.
- **A field's own action** (`Field`'s `action`): a small control that works on the field's value — the pull request dialog's *Suggest* — sits at the end of the label's line, `h-6` so the line does not grow, outside the `<label>` so it is never part of the field's name. An agent-drafted value wears the agent glyph, lands in the field to be read and edited, never writes over what was typed while it was asked, and says what it did under the fields with *Undo* (`prFormModel.applyPrSuggestion`), as the commit composer's *Suggest* does for a message.
- **Checkbox:** a drawn 14px box with the small radius, accent fill and contrast check when on.
- **Switch** (`ui/Switch.tsx`): a setting that takes effect now (a Checkbox is a value to submit). On, it fills with the accent, like a checked Checkbox; off, `surface-2` with the edge. `hideLabel` keeps the name for assistive technology but draws none, for a switch at the end of a row whose words already say what it turns on.

### Navigation
- **Sidebar rows** (`shell/SidebarSection.tsx`): `row` height, control radius, 13px; resting `text-dim`, hover `surface-2`, active `selected` ground (a shared-layout marker that glides between rows) with medium full ink. Section labels are 12px semibold `text-dim` with a rotating chevron and a quiet count; row actions appear on hover or focus (`.row-actions`). The Inbox door shows two counts, never one sum (`sidebarModel.inboxBadge`): what needs the person in the accent, and beside it what is merely unread, neutral. The collapsed rail has room for one number: the needs count with a neutral dot while anything is also unread, or the unread count alone, neutral, when nothing is owed (`shell/SidebarRail.tsx`). Only the needs count replays `motion-pop`, and only when it rises (`useArrivals`), never when it falls or a screen mounts with it standing. Channel, DM and hosted unread counts are neutral. A list that could not be read shows no create door.
- **Tabs** (`ui/Tabs.tsx`): 13px medium on a hairline; the active tab's underline is a 2px `text` bar that slides between tabs (skipped under reduced motion). Strips that do not fit fold to glyphs in declared stages; nothing wraps.
- **Tab strip** (`ui/TabStrip.tsx`, editor and terminal tabs): the open tab is `selected/50` with a `text/70` edge. `role="tablist"` sits on the scroller, and the strip is one tab stop (the open tab, or the first); ← → move focus, Home and End jump to the ends, nothing wraps, and Enter or Space opens (`ui/tabStripModel.mjs`). The unsaved mark is a drawn 6px dot, not a glyph, in the tab's own ink (`bg-current`): unsaved is a fact, not a summons, so neither the dot (the tab strip's, the file tree's) nor the editor's *Unsaved* chip wears the accent.
- **Segmented control:** a `surface-2/70` track with a hairline edge; the pressed segment becomes `surface` with full ink and the raised shadow. For two to four options; past that, a select.
- **Icon rail:** 40px column; the showing item is `selected` with a 2px `text/70` indicator bar.
- **Menus and popovers** (`ui/surfaces.ts`): one floating surface (control radius, edge border, surface, floating shadow, `motion-pop`); menu rows are 13px, inset 4px from the panel with a small corner, highlighted with `selected` (danger rows with `danger-soft`).
- **Tooltips:** 12px on the floating surface, never interactive, for names of glyph-only controls and the reason a button is disabled.
- **Inbox keys:** the list's single-key commands act only when focus is on a list row or nothing is focused, never while a field or another control holds focus.

### Dialog
A column capped at 84vh, 8vh from the top, card radius, edge border, floating shadow, over the scrim. Header 16px semibold with a hairline under it; the body is the one scrollport (20px sides, 16px vertical); the footer right-aligns its actions on `surface-2/40` above a hairline. One primary in the footer.

### Tile
A choice read at a glance (`ui/Tile.tsx`): preview at the card radius, name and blurb below. Selected wears the accent border with `ring-2 ring-accent/40` and a `text` check beside the name; resting has the edge border. The Appearance accent and font tiles match this ring (pinned by `scenarios/pet.test.mjs`).

### Empty State
Open, never boxed (`ui/EmptyState.tsx`): a 36px soft tile in `surface-2/70` holding the concept's glyph, a 14px semibold title, a 13px `text-dim` hint up to `max-w-sm`, and its action below. The `action` prop is required; `null` is a stated decision. The door is never a ghost: a ghost button is an action that lives inside a row, and alone under a sentence it read as more sentence, so the empty state draws any ghost it is handed raised, as the default button (`Button` says its variant as `data-variant`). `cleared` (driven by `useCleared`, used by the Inbox when its list empties while watched in the same filter) pops the tile once and rests it in `ok-soft` with `ok` ink: the work is done.

### Workflow canvas: the live run map
The designer's canvas (`ui/FlowCanvas.tsx`, `views/_workflow/Designer.tsx`, `theme/flow.css`) shows where a run stands, not only where it has been.
- **Edges:** a taken flow is `text` at 72%, 2 wide (the record of the path); a skipped one is `text-dim` at 45%, dashed 4/4; an on-fail flow is dashed `danger`; a boundary divert is dashed `warn`; a loop is dotted. A selected edge is full `text` at 3.5, so a selection never reads as the route.
- **The live flow** (`.is-live`, the `live` edge prop): the flow the run came along to the step it stands on, in the accent, 2.5 wide, round caps, dashed 7/5 and moving toward that step (900ms linear). It is the only moving thing and the only accent flow on the canvas.
- **Step rings:** each `.step-card` carries an accent `::after` ring whose distance is the registered `@property --ring`, so it grows concentric with the card's own radius (`--step-radius` for an event's pill), never by scale. A running step breathes: a one-way ripple from 2px to 7px that fades and rests (2.6s, repeating). A step waiting on the person (`data-yours`, drawn on the accent tone) calls: the same ripple to 10px, twice, then rests on its static accent ring. Card ring state changes morph over `motion-slow`.
- **Selection:** a selected node is a 2px `text` ring at 60%, never the accent.
- **Reduced motion:** the live flow turns solid and the rings stop; colour and ring still say all three states.
- **Keyboard:** a focused node draws the kit's one ring on the card itself (2px accent, 2px off, following the card's radius; `theme/flow.css`). The canvas carries an aria-label, xyflow's own controls speak the app's language through `ariaLabelConfig`, and edge labels read at `--text-2xs`. Fit lives once, in the designer's toolbar, not in xyflow's controls.
- **Inspector** (`views/_workflow/Inspector.tsx`): a step reads identity first (id, name), then its kind's own form, then a collapsed "Flow and failure" fold (Then, join, on failure, route, retries, max visits, boundaries) that stays as the person left it from step to step; forms go two columns from `@sm`. The "Then" field (`forms/ThenField.tsx`) draws a step's flows without a pointer through the canvas's own connect rules.

### Activity lines
A Pulse row under its subject's heading never says the subject again by its id: under the goal's title it reads *goal captured*, under the agent's name *replied* (`activityModel.pulseLine`). The live line, which has no heading, keeps the id. A note or drawing changed reads as what changed and what it is about (*a workspace note changed*), never the fact's own type name. Setting keys keep their own spelling, the words Settings shows beside their copy glyph.

### Errors
People read sentence-case catalog messages that point to the diagnostic log ("…could not be read. The diagnostic log has the detail."), never a raw `String(e)`. A failed act goes through the kit's one door (`ui/failure.ts`): `sayFailure(target, what, e)` leads with the catalog sentence naming what failed, follows it with the node's own sentence when the error is an `ApiError`, and otherwise points to the log and writes the detail there; `failureReason` is the same for a `{ $reason }` slot in a sentence that already names what failed, and `failureText` is the one sentence a shared door hands on where nothing names the act (`attempt`, a read): the node's words, or *Something unexpected stopped it. The diagnostic log has the detail.* What a person typed and the app could not parse — a JSON definition, a pattern, a payload — keeps its parser's words: that is feedback on their own text, not an exception. `ErrorNote` (`ui/Card.tsx`) is `danger-soft` behind `danger` words with `role="alert"` and an optional Retry; a failed read shows it with Retry instead of an empty state (Goals, Pulse, Channels, Direct messages), because a list that could not be read is unknown, not empty; a list not yet read shows its skeleton, never *No channels yet*, and the sidebar offers no create door for either.

### Focus and the browser's own surfaces
One visible ring everywhere: 2px accent outline, 2px offset, 4px radius (`styles.css`); kit rings for compact controls use `ring-1 ring-accent` (`ui/rings.ts`). Containers Radix focuses programmatically show no ring for pointer users. Text selection is the accent at 26%; the caret is the accent; scrollbars are thin, `text-dim` at 40%. Find matches use the accent wash at 80%, and the current match `warn-soft`.

### Page annotation overlay
The inspector drawn into a page to annotate it (`ui/artifact/pageInspector.mjs`, dressed by `inspectorTheme.mjs` from the app's roles, the same core in a rendered file's frame and a browser tab) must never hide what it points at. The hover outline is a 2px accent edge with **no fill** and a 1px surface ring outside it, so the element stays readable under it. The tag label rides outside the element — above its edge, else under it, inside its top edge only when the element is the whole height of the view — and never past the right edge. The note box stands above the element, else below, else beside it (right, then left), and in the view's farthest corner only when the element is the size of the view. Every part that only marks (outline, tag, badges) is `pointer-events:none`, restated after `all:initial`.

The note box speaks the kit's popover language inside a page that has none of the app's CSS: the floating surface (card radius, edge, floating shadow) on a **solid** ground — the theme's `surface` with its alpha taken off (`inspectorTheme.opaque`), since nothing frosts behind a box drawn in a page; one line of where the element sits (its three nearest parents as quiet mono crumbs that re-pick, a parent's crumb giving way to an ellipsis before the element's, the element itself a neutral chip in full ink, never the accent); a ghost close drawn as two strokes in the text's colour, never a glyph from a font; the element's text as one dim line; the kit's field; and a foot with how the keys answer (*Enter adds it · Esc closes*) beside the one primary button (*Add*, *Change* when the element has a note). Its words are catalog messages baked into the page's script with its dress (`inspectorWords`).

**Annotating is the Project IDE's alone** (`useBrowserAnnotation`'s `offered`): a tab in the IDE's centre, or in the Details pane while it stands beside the IDE, has the wand, the note box and the tray that sends to an agent; beside any other screen the Browser pane only shows the page — no wand — and a wand left on, or badges left drawn, are taken back from the page.

### Marks
A mark is a thing's own glyph, not a concept's (`docs/contributing/terminology.md`). The harness marks (`ui/harnessMarks.tsx`), the platform's (`ui/PlatformMark.tsx`) and the **Git mark** (`ui/GitMark.tsx`: the official Git logo redrawn as an outline in the kit's lucide stroke — 24-unit grid, a stroke of 2, round caps — so it stands among the glyphs it sits beside; `currentColor`; credited in `NOTICES.md`). The Git mark stands wherever git is the *tool* — the Project IDE's Git tab, Settings › Git & code hosts › Identity, initialising a repository, a remote repository line, a commit or a failed push in the activity feed. A git *concept* keeps its kit glyph: branch, merge, commit, pull request, stash, tag, compare. Icon slots that may hold one are typed `LucideIcon | Mark`.

### Motion
Tokens in `theme/tokens.css`: `motion-fast` 120ms, `motion-base` 160ms, `motion-slow` 240ms, easing `cubic-bezier(0.2, 0, 0, 1)`. Overlays fade (`motion-overlay`), dialogs arrive from 4px above at 98.5% scale (`motion-panel`), popovers grow from the side they are anchored to (`motion-pop`). `.anim` transitions colour, border, opacity and transform only, never `all`. Arrival motion marks a change of state, not a render: `useArrivals` replays a one-shot only when a count rises, and `useCleared` fires only when a watched list empties in the same scope (`ui/useArrival.ts`). Under `prefers-reduced-motion` the durations become 1ms (so Radix's exit still fires), and the repeating motions (pulse, spin, breathe, nudge, pop, shake, lift) stop, as do the canvas's live flow and step rings (`theme/flow.css`).

## Do's and Don'ts

### Do:
- **Do** ask for roles (`surface`, `text-dim`, `accent-ink`, `danger-soft`), never a colour; a new theme is a data change.
- **Do** spend the accent only on a summons: primary buttons, waiting-on-you and the needs-you count, "new" markers, an on Switch or checked Checkbox, live and running, decision banners, focus, drop targets, find matches, links in prose. Unread counts are neutral.
- **Do** ask before anything that cannot be taken back, in a danger `ConfirmDialog` that names what will go (The Irreversible Asks Rule).
- **Do** give a disabled button its reason with `disabledReason`, and a refused value its reason with `Field`'s `error`.
- **Do** draw where-you-are with `selected` and full ink, and indicator bars in `text` (70% on rails).
- **Do** keep one `primary` per region: a dialog's confirm, a form's submit, a screen's main create action, an empty state's door. Per-row and per-card actions are `default` or `ghost`.
- **Do** use `border` for the edge of a thing and `hairline` for dividers inside it.
- **Do** build lists on `row-sm` / `row` / `row-lg` so Compact keeps working.
- **Do** use `shadow-sm` for raised and `shadow-lg` for floating, and mark floating panes `data-pane`.
- **Do** keep 12px (`text-2xs`) as the reading floor and sentence case everywhere.
- **Do** take durations from `--motion-*`, so reduced motion is honoured for free.
- **Do** give every empty state an action, and let the content make room for a floating dock rather than moving the dock.
- **Do** colour a step's kind only through `familyInk` / `familyFrame` / `familyColor`, on its glyph, beside its shape.
- **Do** hold running text to `max-w-measure`, and use one content-heading tier (14px semibold `text`).
- **Do** lay out by container (`@container` with `@xs` to `@2xl`) so a component reads the same in a dialog, a pane or a screen.
- **Do** publish a floating overlay's box with `useBrowserClear` so a browser tab is cut around it, and keep the page live (The Live Page Rule).
- **Do** wear the Git mark where git is the tool, and a kit glyph for a git concept.

### Don't:
- **Don't** put the accent on a selected row, an active tab, a pressed toggle, an identity or kind chip, or an informational banner that asks nothing; those use `selected`, neutral chips, or `surface-2/70` with an edge.
- **Don't** use `warn` for "waiting on you"; warn is caution, and the summons is the accent.
- **Don't** give text, the accent or status colours alpha on a glass family.
- **Don't** name a shadow, a glow or a pixel radius in a component (`rounded-[12px]` is test-forbidden).
- **Don't** nest a bordered card inside a bordered card, or box an empty state in a dashed frame.
- **Don't** add `uppercase` or wide tracking to labels, or eyebrows and section numbers above headings.
- **Don't** stand a unicode glyph in for an icon in markup; use the kit's `ICON.*`.
- **Don't** add coloured side stripes thicker than 1px to cards, rows or callouts.
- **Don't** let `danger` sit beside an accent fill as a bare fill; it arrives as `danger-soft` behind `danger` text.
- **Don't** put a step-family ink on a card's ground, edge or words, or let it stand in for a state; goal step chips keep the state colour.
- **Don't** move anything else on a run's canvas: the live flow is the one moving thing and the one accent flow.
- **Don't** show a person a raw error string, or an empty state for a list that failed to read; use a catalog sentence and `ErrorNote` with Retry.
- **Don't** wrap a group of controls or a copy button in `Field`; use `Labelled`.
