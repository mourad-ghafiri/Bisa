/**
 * Plain text with its paths and links as doors (ide/17): a step's output,
 * an agent's evidence, a check's words — rendered as it was written, with
 * `findLinks`' spans as anchors and the one delegated click.
 */

import { useMemo } from "react";
import { delegateLinkClick, useLinkHandler, useLinkRoots } from "./linkContext";
import { findLinks } from "./linkModel.mjs";

export function LinkedText({ text, className = "", as: Tag = "span" }: { text: string; className?: string; as?: "span" | "pre" | "dd" }) {
  const handler = useLinkHandler();
  const roots = useLinkRoots();
  const parts = useMemo(() => {
    const hits = findLinks(text);
    if (hits.length === 0) return null;
    const out: { key: string; text: string; hit?: (typeof hits)[number] }[] = [];
    let at = 0;
    hits.forEach((h, i) => {
      if (h.start > at) out.push({ key: `t${i}`, text: text.slice(at, h.start) });
      out.push({ key: `l${i}`, text: h.raw, hit: h });
      at = h.end;
    });
    if (at < text.length) out.push({ key: "tail", text: text.slice(at) });
    return out;
  }, [text]);
  if (!parts) return <Tag className={className}>{text}</Tag>;
  return (
    <Tag className={className} onClick={(e: React.MouseEvent) => void delegateLinkClick(e, handler, roots)}>
      {parts.map((p) =>
        p.hit ? (
          p.hit.kind === "url" ? (
            <a key={p.key} data-link="url" href={p.hit.url} className="link-url">
              {p.text}
            </a>
          ) : (
            <a
              key={p.key}
              data-link="path"
              data-path={p.hit.path}
              data-line={p.hit.line ?? undefined}
              data-col={p.hit.col ?? undefined}
              href="#"
              className="link-path"
            >
              {p.text}
            </a>
          )
        ) : (
          <span key={p.key}>{p.text}</span>
        ),
      )}
    </Tag>
  );
}
