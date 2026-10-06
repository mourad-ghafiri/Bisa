/**
 * Rendered markdown for messages, notes, results — and for `.md` files read
 * out of somebody's repository.
 *
 * The content here is authored by agents and remote collaborators, so it is
 * untrusted input, not our own copy. In a message, a note or a card raw HTML
 * is *escaped* — micromark's default — and shows as text, so `Vec<T>` in an
 * agent's prose keeps its brackets. In a **document** — a file at a path,
 * `relativeLinks` — raw HTML renders as GitHub renders it: through DOMPurify's
 * prose profile (`markdownHtmlModel.PROSE_PROFILE`): a README's `<details>`,
 * its row of badges in `<p align="center">`, a `<br>`, a `<kbd>` draw;
 * nothing that runs, styles, frames, submits, plays or hides is kept, no
 * `data-*` a document could mint, and a comment is never a tag, so it is
 * gone; a YAML front matter block is left out (`withoutFrontMatter`). On top
 * of that we restrict protocols the same way in both modes.
 *
 * # `relativeLinks`, and why it is off by default
 *
 * A message that says `see ./notes.md` is pointing at nothing: there is no
 * document a conversation is relative *to*, so the safe rendering is a link
 * that goes nowhere. A markdown **file** is different — it sits at a path, and
 * `./notes.md` beside it is the most useful link in the document. So the mode
 * is per-caller rather than global, and the caller that turns it on is the one
 * that knows what the link is relative to.
 *
 * Turning it on widens the sanitizer by exactly one shape: a target with no
 * protocol. `on*` handlers are still stripped, and `javascript:` is still not
 * a protocol we vouch for. What resolves such a link, and what refuses one
 * that climbs out of the tree, is `views/_workbench/docLink.mjs`, where it is
 * pure and tested.
 *
 * The HTML goes into the page through `dangerouslySetInnerHTML` with an
 * object memoised on its string: react-dom sets `innerHTML` again whenever
 * that object is new, which would replace every text node on every render —
 * and a find over the rendering (`ui/find/useDomFind.ts`) holds ranges over
 * those nodes.
 */

import DOMPurify from "dompurify";
import { useMemo } from "react";
import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";
import { withHeadingIds } from "./headingAnchorsModel.mjs";
import { delegateLinkClick, useLinkHandler, useLinkRoots } from "./linkContext";
import { linkifyHtml } from "./linkModel.mjs";
import { PROSE_PROFILE, isGfmTaskBox, withoutFrontMatter } from "./markdownHtmlModel.mjs";
import { cn } from "./cn";
import { MermaidView } from "./MermaidView";
import { mermaidBlocks } from "./mermaidModel.mjs";
import { placeholderChips } from "./placeholderChips.mjs";

const MERMAID_BLOCK = /<pre><code class="language-mermaid">([\s\S]*?)<\/code><\/pre>/g;

/**
 * The sanitizer for a document, an instance of this renderer's own, so its
 * hook reaches no other — the docx view keeps the library's default. The
 * hook keeps the one `<input>` a document may draw, GFM's task-list box, and
 * removes any other control (`isGfmTaskBox`).
 */
const purifier = typeof window === "undefined" ? null : DOMPurify(window);
purifier?.addHook("uponSanitizeElement", (node, data) => {
  if (data.tagName !== "input") return;
  const el = node as Element;
  if (!isGfmTaskBox(el.getAttribute("type"), el.getAttribute("disabled"))) el.parentNode?.removeChild(el);
});

function decodeEntities(html: string): string {
  return html
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, "&");
}

const SAFE_PROTOCOL = /^(https?:|mailto:|#|\/)/i;
/** A protocol, or a protocol-relative URL. Anything else is relative. */
const HAS_PROTOCOL = /^([a-z][a-z0-9+.-]*:|\/\/)/i;

/**
 * Strip any link or image target that isn't one we vouch for.
 *
 * A relative image under `relativeLinks` keeps its path as `data-doc-src`
 * with the `src` itself neutered: nothing here can fetch it, but the
 * document that sits at a path can — `EditorDoc`'s rendered view reads each
 * one through the node's byte route (`GET /ide/raw`, ide/03) into a blob
 * URL, so a `./diagram.png` written inside a README draws. Anywhere else a
 * relative image stays a broken icon by design: an attachment is rendered by
 * the message that carries it, where its hash is known. Runs after the
 * sanitizer in a document, whose serializer double-quotes every attribute,
 * so the regex sees each URL.
 */
function sanitizeUrls(html: string, relativeLinks: boolean): string {
  return html
    .replace(/(href|src)="([^"]*)"/gi, (whole, attr: string, url: string) => {
      if (SAFE_PROTOCOL.test(url)) return whole;
      if (relativeLinks && !HAS_PROTOCOL.test(url)) {
        if (attr.toLowerCase() === "href") return whole;
        return `src="#" data-doc-src="${url}"`;
      }
      return `${attr}="#"`;
    })
    .replace(/\son[a-z]+="[^"]*"/gi, "");
}

/** One stretch of the rendering: HTML, or a diagram standing where its fence was. */
type Segment = { inner?: { __html: string }; mermaid?: { source: string; startLine: number } };

export function Markdown({
  text,
  className = "",
  relativeLinks = false,
  onGoToLine,
}: {
  text: string;
  className?: string;
  /** A document at a path: relative links kept, raw HTML rendered through the prose profile, front matter left out. */
  relativeLinks?: boolean;
  /** Clicking a diagram error moves an editor to that line, when there is one. */
  onGoToLine?: (line: number) => void;
}) {
  const handler = useLinkHandler();
  const roots = useLinkRoots();
  // A document's source: its front matter left out. A message is as typed.
  const source = useMemo(() => (relativeLinks ? withoutFrontMatter(text) : text), [text, relativeLinks]);
  const html = useMemo(() => {
    if (!source.trim()) return "";
    try {
      // A document's raw HTML is kept by micromark for the sanitizer; a
      // message's is escaped by it. Then the URL rules; then, in a
      // document, every heading given GitHub's anchor so a table of
      // contents lands (`headingAnchorsModel.mjs`, ide/03) — a message's
      // HTML stays as it was; then every path and link marked as a door
      // (`linkModel.mjs`, ide/17); then a redacted secret an agent quoted
      // back rendered as a chip naming its kind (`placeholderChips.mjs`) —
      // the scanner has already refused to link inside it.
      const prose = relativeLinks && purifier !== null;
      const raw = micromark(source, {
        allowDangerousHtml: prose,
        allowDangerousProtocol: false,
        extensions: [gfm()],
        htmlExtensions: [gfmHtml()],
      });
      const clean = prose ? purifier.sanitize(raw, PROSE_PROFILE) : raw;
      const sanitized = sanitizeUrls(clean, relativeLinks);
      return placeholderChips(linkifyHtml(relativeLinks ? withHeadingIds(sanitized) : sanitized));
    } catch {
      return "";
    }
  }, [source, relativeLinks]);
  // The HTML as the page takes it, one object per string (see the header).
  const inner = useMemo(() => ({ __html: html }), [html]);
  // A ```mermaid fence renders as a diagram rather than a code block. The
  // HTML is split around each one and the viewer mounted in its place; the
  // block's line in the source is what an error message is offset by. The
  // fences in the HTML and in the source are matched one to one; a count
  // that differs — a document's raw `<pre>` wearing the class — keeps every
  // fence a code block rather than mis-number the lines.
  const segments = useMemo((): Segment[] => {
    const blocks = mermaidBlocks(text);
    const fences = [...html.matchAll(MERMAID_BLOCK)];
    if (fences.length === 0 || fences.length !== blocks.length) return [{ inner }];
    const out: Segment[] = [];
    let last = 0;
    fences.forEach((m, n) => {
      out.push({ inner: { __html: html.slice(last, m.index) } });
      const block = blocks[n];
      out.push({ mermaid: { source: block?.source ?? decodeEntities(m[1] ?? ""), startLine: block?.startLine ?? 1 } });
      last = (m.index ?? 0) + m[0].length;
    });
    out.push({ inner: { __html: html.slice(last) } });
    return out;
  }, [html, text, inner]);
  // Delegated: micromark's output is a string, so there is no anchor to
  // attach a handler to at render time. A URL or a path goes to the handler
  // — the window never navigates; a relative link in a document is the
  // caller's, resolved against the file it sits in, and is left to bubble.
  const onClick = (e: React.MouseEvent) => {
    void delegateLinkClick(e, handler, roots);
  };

  if (!html) {
    return <p className={`text-2xs text-text-dim ${className}`}>{source || "—"}</p>;
  }

  if (segments.length === 1) {
    return (
      <div
        className={cn("prose-i text-sm leading-relaxed", className)}
        onClick={onClick}
        // Sanitized above: a document's HTML through the prose profile, a message's escaped; no non-vouched protocols, no handlers.
        dangerouslySetInnerHTML={inner}
      />
    );
  }
  return (
    <div className={cn("prose-i text-sm leading-relaxed", className)} onClick={onClick}>
      {segments.map((seg, i) =>
        seg.mermaid ? (
          <MermaidView
            key={`m${i}`}
            source={seg.mermaid.source}
            startLine={seg.mermaid.startLine}
            onGoToLine={onGoToLine}
            className="my-2 rounded-card border border-border bg-surface"
          />
        ) : seg.inner ? (
          // Sanitized above, as the single-segment path is.
          <div key={`h${i}`} dangerouslySetInnerHTML={seg.inner} />
        ) : null,
      )}
    </div>
  );
}
