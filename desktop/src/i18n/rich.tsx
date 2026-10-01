/**
 * A sentence with an element inside it — *Nothing is at `<path/>` yet*,
 * *Drop a folder holding `<code>pet.json</code>`* — said as one message
 * (17 — Internationalisation): the catalog's text carries the element as a
 * tag, and the caller hands what takes its place, so a translator moves the
 * element with the words and never sees a sentence cut into fragments.
 *
 * Two shapes of tag: a self-closing `<name/>` is a slot the caller fills
 * with an element; a paired `<name>words</name>` keeps its words in the
 * message and the caller (or a default for `code`, `b`, `strong`, `em`,
 * `i`, `kbd`) wraps them. A slot the message names and the caller does not
 * fill renders as its tag, visible like a missing id.
 */

import { Fragment, createElement, type ReactNode } from "react";
import { t } from "./l10n.mjs";

/** What fills a tag: an element for `<name/>`, a wrapper for `<name>…</name>`. */
export type Slot = ReactNode | ((inner: string) => ReactNode);

const TAG = /<([a-z][\w-]*)\/>|<([a-z][\w-]*)>([^<]*)<\/\2>/;

/** The tags a message may use without the caller naming them. */
const DEFAULTS: Record<string, (inner: string) => ReactNode> = {
  code: (inner) => createElement("code", { className: "font-mono" }, inner),
  b: (inner) => createElement("b", null, inner),
  strong: (inner) => createElement("strong", null, inner),
  em: (inner) => createElement("em", null, inner),
  i: (inner) => createElement("i", null, inner),
  kbd: (inner) => createElement("kbd", null, inner),
};

export function rich(id: string, slots: Record<string, Slot> = {}, args?: Record<string, unknown>): ReactNode {
  const parts = t(id, args).split(TAG);
  const out: ReactNode[] = [];
  // split() with two capture groups alternates: text, selfClosing, paired, inner, text, …
  for (let i = 0; i < parts.length; i += 4) {
    if (parts[i]) out.push(parts[i]);
    const self = parts[i + 1];
    const paired = parts[i + 2];
    const inner = parts[i + 3] ?? "";
    if (self !== undefined) {
      const slot = slots[self];
      out.push(<Fragment key={i}>{slot === undefined ? `<${self}/>` : typeof slot === "function" ? slot("") : slot}</Fragment>);
    } else if (paired !== undefined) {
      const slot = slots[paired] ?? DEFAULTS[paired];
      out.push(<Fragment key={i}>{slot === undefined ? `<${paired}>${inner}</${paired}>` : typeof slot === "function" ? slot(inner) : slot}</Fragment>);
    }
  }
  return out;
}
