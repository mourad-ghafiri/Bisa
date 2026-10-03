# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The desktop app is a Tauri shell around a React webview (`desktop/`); its design language is the web's, not a native one.

## Users

Developers who carry work from a stated want to a running outcome with coding agents, on their own machine. Two situations matter equally:

- **Solo:** one developer at the window for hours, running several harnesses in parallel beside terminals, the IDE and the embedded browser.
- **Small team:** a few people and their agents in shared channels over relays, where gates, questions and approvals are the main loop.

## Product Purpose

Bisa is a local-first, account-free agentic IDE. It orchestrates the coding harnesses a person already has (Claude Code, Codex CLI, OpenCode, GitHub Copilot CLI, Grok Build, pi, Oh My Pi, anything speaking ACP) rather than shipping its own. A goal is the durable object; its workflow is how it runs. Success is a person who can see at a glance what waits on them, decide it, and get back to the work.

## Positioning

Local-first and account-free: the person's keys, repositories and workspace stay on their machine. One engine reached three ways (desktop app, `bisa` CLI, headless node). Every fact about a run is a signed event in its journal.

## Operating Context

- The sidebar's destinations: Inbox, Agents, Teams, Projects (the Project IDE), Workflows, Goals, Pulse, then Channels and Direct messages.
- A footer status bar with harness usage, live terminals, harnesses, ports, browser tabs, the node, machine resources and overlay toggles.
- Floating overlays: notes, drawings, pets, addons.
- A setup gate on first launch checks git, a harness and the three core agents (General, Workflow, Decision-Making).
- Settings is one registry at three scopes, with five theme families (Glass default, Harbor, Orchard, Dune, Suede), accents, density, text size and two font dials.

## Capabilities and Constraints

- Inside 0.x nothing that worked may stop working: features, routes, shortcuts and settings are preserved.
- Every sentence a person reads is a catalog message with an id (`locales/`); no English literals in code.
- The desktop's facts live in `.mjs` models with tests; components only draw.
- Generated files (`desktop/api-schema.json`, `desktop/src/types.gen.ts`, the pages under `website/`) are never hand-edited.
- No new dependency without agreement.
- Themes are held to a contrast promise and a role contract by tests (`theme/themes.test.mjs`, `theme/roles.test.mjs`).

## Brand Commitments

- Name: Bisa. Mark: `logo/logo.svg`.
- The accent means **your attention**: "waiting on you" wears it, because that is the app's primary call to action. It is not spent on surfaces that ask for nothing.
- Voice: plain, exact, sentence case; one word per concept (docs/contributing/terminology.md).

## Evidence on Hand

- Screenshots of the shipped app: `website/screenshots/*.webp`.
- Product and architecture docs: `README.md`, `docs/guide/`, `docs/architecture/`.

## Product Principles

1. What waits on the person is never out of sight, and nothing else competes with it.
2. Calm for hours: legible, low-glare, consistent from screen to screen.
3. The person's machine, the person's keys: local-first is visible in how the app behaves, not claimed.
4. Doors, not prose: every empty or blocked state says what to do next.

## Accessibility & Inclusion

- A contrast floor held by tests in every theme, both sides; secondary text stays readable.
- Reduced motion honoured by zeroing the motion tokens.
- Keyboard first: every row is a real link or button; a visible focus ring everywhere.
- A text-size dial and a hyperlegible font option (Atkinson Hyperlegible).
