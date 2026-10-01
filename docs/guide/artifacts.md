# Artifacts

When an agent makes something for you to *look at* — a page, a chart, a report, a spreadsheet, a
deck, an image, a recording — it posts it as an **artifact**, and the artifact renders live in the
conversation, wherever you read it: a direct channel, a standing channel, a goal's thread, the
Project IDE's Agent panel and Agent Mode, the Inbox. Architecture:
[12 — Artifacts](../architecture/12-artifacts.md).

## Under the message

Each artifact is a card: its kind, its title, its size, and the first thing to see. A picture is
drawn inline. A page runs live, in a sandbox, at a small height — at most three pages run at once
in a thread; the rest wait until you open them. A PDF, a spreadsheet, a document, a deck, a video
show as a card with **Open**. Click the title to open it beside the conversation; ⌘-click opens it
over the whole window. An artifact whose bytes are not on this machine yet shows **Request**, the
same door an attachment has.

## Beside the conversation

An artifact opens in the right-hand pane — the same pane a thread or a profile opens in — so the
conversation stays where it was, and Back closes it. The pane's toolbar has:

- **Save as…** — the save dialog, then a copy where you chose.
- **Reveal in Finder** (or your platform's file manager) — the file under its own name, on disk.
- **Open with the default app** — Numbers for a sheet, Keynote or PowerPoint for a deck, Preview
  for a PDF, whatever claims the file.
- *Copy the text* for a page, a figure, Markdown, code or data; *View the source* for a page or a
  figure; **Open in the IDE** when the agent wrote the file inside a checkout.
- **Expand** fills the window; Esc comes back; ← and → move through the conversation's artifacts.
- In the Project IDE, **Open as a tab** puts it in the centre as a document.

Under the viewer, **In this conversation** lists every artifact the conversation holds, newest
first, grouped by title. An agent that revises something posts it under the same title, and the
group shows *3 versions* — each with who posted it and when — so the latest is on top and the
earlier ones a click away.

## What renders how

| What the agent made | What you see |
|---|---|
| a page (`.html`) | the page, live, in a sandbox: it can run its own scripts and, when Settings › Project IDE › Artifacts allows, load libraries from cdnjs, jsDelivr or unpkg — and reach nothing else. It cannot see the app, its token or your files |
| a figure (`.svg`) or an image | the picture; a click swaps between fit and actual size |
| a PDF | page by page, with a page counter and a zoom |
| a spreadsheet (`.csv`, `.tsv`, `.xlsx`) | a grid with lettered columns and a tab per sheet |
| a document (`.docx`) | the prose, as the app's own Markdown reads |
| a deck (`.pptx`) | an outline: one card per slide with its title, its text, its pictures and its notes. For the deck as designed, *Open with the default app* — or ask the agent for a PDF beside it |
| Markdown, a Mermaid diagram | rendered |
| code, data, text | read-only in the editor, with the language its name says |
| a video, a recording | plays here when the codec is one the app can play (MP4 with H.264, MP3, WAV); otherwise the default app |
| anything else | the card, with Save, Reveal and Open with |

## Sharing one yourself

Attach a file in the composer and click the small **file** word on its chip: it turns to
**artifact**, and the file posts as one — rendered where it is read, titled by its name. From the
terminal:

```sh
bisa msg <scope> "the numbers" --artifact q3.csv:"Q3 numbers" --attach notes.txt
```

`--artifact` shares a file to be looked at; `--attach` hands one over as a file.

## What agents are told

Every session's tools say it: *what you made for the person to look at goes in `artifacts`; a file
handed over as a file goes in `attachments`*. The catalog skill **Artifacts** (Settings › Library)
teaches the rest — one self-contained page, the right format per thing, one title per thing and the
same title when revising — and can be attached to any agent. An agent may publish only from where it
works: its own scratch folder, the checkout its session runs in, or the goal's scratch or a run in
the workspace's; a file found elsewhere on the disk is refused.

## Settings

| Key | What it does |
|---|---|
| `artifacts.html.libraries` | whether a page may load scripts, styles and fonts from the three public CDNs. Off, a page runs with only what it carries. Nothing else on the network is ever reachable from a page |
