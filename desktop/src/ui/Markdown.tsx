/**
 * Rendered markdown for messages, notes, results — and for `.md` files read
 * out of somebody's repository.
 *
 * micromark's default `allowDangerousHtml: false` drops raw HTML, and we
 * additionally restrict protocols — the content here is authored by agents and
 * remote collaborators, so it is untrusted input, not our own copy.
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
 * protocol. Everything else is unchanged — raw HTML is still dropped, `on*`
 * handlers are still stripped, and `javascript:` is still not a protocol we
 * vouch for. What resolves such a link, and what refuses one that climbs out of
 * the tree, is `views/_workbench/docLink.mjs`, where it is pure and tested.
 */

import { useMemo } from "react";
import { micromark } from "micromark";
import { gfm, gfmHtml } from "micromark-extension-gfm";
import { delegateLinkClick, useLinkHandler, useLinkRoots } from "./linkContext";
import { linkifyHtml } from "./linkModel.mjs";
import { MermaidView } from "./MermaidView";
import { mermaidBlocks } from "./mermaidModel.mjs";
import { placeholderChips } from "./placeholderChips.mjs";

const MERMAID_BLOCK = /<pre><code class="language-mermaid">([\s\S]*?)<\/code><\/pre>/g;

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
 * the message that carries it, where its hash is known.
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

export function Markdown({
  text,
  className = "",
  relativeLinks = false,
  onGoToLine,
}: {
  text: string;
  className?: string;
  /** Keep protocol-less `href`s, for a document that sits at a path. */
  relativeLinks?: boolean;
  /** Clicking a diagram error moves an editor to that line, when there is one. */
  onGoToLine?: (line: number) => void;
}) {
  const handler = useLinkHandler();
  const roots = useLinkRoots();
  const html = useMemo(() => {
    if (!text.trim()) return "";
    try {
      // Sanitized first; then every path and link marked as a door
      // (`linkModel.mjs`, ide/17) on a string with no raw HTML left in it;
      // then a redacted secret an agent quoted back rendered as a chip
      // naming its kind (`placeholderChips.mjs`) — the scanner has already
      // refused to link inside it.
      return placeholderChips(
        linkifyHtml(
          sanitizeUrls(
            micromark(text, {
              allowDangerousHtml: false,
              allowDangerousProtocol: false,
              extensions: [gfm()],
              htmlExtensions: [gfmHtml()],
            }),
            relativeLinks,
          ),
        ),
      );
    } catch {
      return "";
    }
  }, [text, relativeLinks]);
  // Delegated: micromark's output is a string, so there is no anchor to
  // attach a handler to at render time. A URL or a path goes to the handler
  // — the window never navigates; a relative link in a document is the
  // caller's, resolved against the file it sits in, and is left to bubble.
  const onClick = (e: React.MouseEvent) => {
    void delegateLinkClick(e, handler, roots);
  };

  if (!html) {
    return <p className={`text-2xs text-text-dim ${className}`}>{text || "—"}</p>;
  }

  // A ```mermaid fence renders as a diagram rather than a code block. The
  // HTML is split around each one and the viewer mounted in its place; the
  // block's line in the source is what an error message is offset by.
  const blocks = mermaidBlocks(text);
  const segments: { html?: string; mermaid?: { source: string; startLine: number } }[] = [];
  let last = 0;
  let n = 0;
  for (const m of html.matchAll(MERMAID_BLOCK)) {
    segments.push({ html: html.slice(last, m.index) });
    const block = blocks[n++];
    segments.push({
      mermaid: { source: block?.source ?? decodeEntities(m[1] ?? ""), startLine: block?.startLine ?? 1 },
    });
    last = (m.index ?? 0) + m[0].length;
  }
  segments.push({ html: html.slice(last) });
  if (segments.length === 1) {
    return (
      <div
        className={`prose-i text-xs leading-relaxed ${className}`}
        onClick={onClick}
        // Sanitized above: no raw HTML, no non-vouched protocols, no handlers.
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }
  return (
    <div className={`prose-i text-xs leading-relaxed ${className}`} onClick={onClick}>
      {segments.map((seg, i) =>
        seg.mermaid ? (
          <MermaidView
            key={`m${i}`}
            source={seg.mermaid.source}
            startLine={seg.mermaid.startLine}
            onGoToLine={onGoToLine}
            className="my-2 rounded-card border border-border bg-surface"
          />
        ) : seg.html ? (
          // Sanitized above, as the single-segment path is.
          <div key={`h${i}`} dangerouslySetInnerHTML={{ __html: seg.html }} />
        ) : null,
      )}
    </div>
  );
}
