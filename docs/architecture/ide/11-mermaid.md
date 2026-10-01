# 11 — Mermaid

One viewer, three sources: a `.mmd`/`.mermaid` file, a fenced block in Markdown, and a fenced block
in an agent's message. Errors land on the line in *your* file, and every diagram in this
repository's documentation is checked against the same parser the viewer uses.

---

## The viewer

`mermaid` v11, **lazy-loaded on the first
diagram** so the ~1 MB chunk is not in the startup path. `ui/MermaidView.tsx` is the one component;
the Markdown renderer and the message renderer both delegate to it, so an agent-generated diagram
renders identically to one in a file.

| Source | Surface |
|---|---|
| `.mmd` / `.mermaid` document | the source, the live preview, or both side by side; **Preview · Split · Source** like Markdown (`fileDocModel.docModes`) — no scroll sync between the two |
| a fenced ` ```mermaid ` block in a Markdown document | rendered inline in *Rendered* and *Split*; *Source* shows the fence |
| a fenced block in a message | rendered inline in the conversation |

Rendering runs debounced (250 ms) while the source changes; the last good render stays on screen
while a new one is parsing, so a half-typed edge does not blank the preview. The theme is derived
from the token roles: Mermaid runs its `base` theme with `themeVariables` built from the resolved
roles (`mermaidModel.mjs` `themeVariablesFor` — nodes are `surface` on a `bg` canvas with `border`
edges, the accent kept for the active thing) in the interface face, and a diagram re-renders when
the theme, accent or font dial moves. `diagrams.theme` may pin one of Mermaid's own themes instead.

---

## Errors point at the right line

Mermaid reports a location relative to the snippet it was handed. For a fenced block that is line 3
of an extract the reader never saw. The viewer adds the fence's offset within the file **before** it
shows anything, so the error is *"line 47: expected an arrow"* in the Markdown document, and
clicking it moves the editor's cursor there. For a `.mmd` file the offset is zero and the same path
applies.

---

## Export

*Export SVG* writes the rendered SVG through the desktop shell's save dialog; *Export PNG*
rasterises through a canvas at the `diagrams.export.scale` setting; *Copy* puts the SVG on the
clipboard. Export is a desktop-shell operation because it writes wherever the person chooses — a
machine capability, not a workspace one.

---

## The documentation is parse-checked

Every diagram in `docs/` is Mermaid and renders in this viewer. That is a promise, and a promise is
a test: `scripts/check-mermaid.mjs` extracts every ` ```mermaid ` block under `docs/` and runs
`mermaid.parse` on it. Mermaid needs a DOM even to parse, so the script runs under `happy-dom`
(MIT) — a dev dependency imported by **that one script and nothing else**. The desktop's testing
rule ("no jsdom; models are tested, components are not") is about component tests, and a test
asserts no `.test.mjs` imports `happy-dom`, so the two do not blur. The check runs in `just verify`.
