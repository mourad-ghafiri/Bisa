/**
 * The door a path or a link opens through (ide/17), as a contract the kit
 * defines and the shell provides: `Markdown`, `LinkedText`, `ExternalLink`
 * and the terminal read a handler from context and hand it what was
 * clicked, where, and which roots the surface knows. No view threads a
 * callback; a surface that knows its roots wraps its text in `<LinkRoots>`.
 */

import { createContext, createElement, useContext, type ReactNode } from "react";
import type { FileScope } from "./linkModel.mjs";
import { endsSelection } from "./selectionModel.mjs";

/** A root a surface knows a path may be relative to, the most likely first. */
export interface LinkRootRef {
  scope: FileScope;
  id: string;
}

export type LinkHit =
  | { kind: "url"; url: string }
  | { kind: "path"; path: string; line: number | null; col: number | null; raw: string }
  /** A document the surface already resolved — a relative link in a rendered file. */
  | { kind: "doc"; scope: FileScope; id: string; path: string; line: number | null };

export interface LinkPointer {
  x: number;
  y: number;
}

export interface LinkHandler {
  /**
   * `direct`: the person held ⌘ — open without a menu when the door is plain.
   * `from`: the absolute directory a relative path is read from, when the
   * surface stands in one — a shell's current directory; absent, a relative
   * path is read from the roots alone.
   */
  onLink: (hit: LinkHit, at: LinkPointer, roots: readonly LinkRootRef[] | null, opts?: { direct?: boolean; from?: string | null }) => void;
}

export const LinkHandlerContext = createContext<LinkHandler | null>(null);
export const LinkRootsContext = createContext<readonly LinkRootRef[] | null>(null);

export function useLinkHandler(): LinkHandler | null {
  return useContext(LinkHandlerContext);
}

export function useLinkRoots(): readonly LinkRootRef[] | null {
  return useContext(LinkRootsContext);
}

/** The roots a surface's text is read against — a conversation's checkout, a goal's projects, a document's own root. */
export function LinkRoots({ roots, children }: { roots: readonly LinkRootRef[]; children: ReactNode }) {
  return createElement(LinkRootsContext.Provider, { value: roots }, children);
}

/** The hit an anchor carries in its data attributes, or null for an anchor that is not a door. */
export function hitOfAnchor(anchor: HTMLElement): LinkHit | null {
  const kind = anchor.getAttribute("data-link");
  if (kind === "url") {
    const url = anchor.getAttribute("href");
    return url ? { kind: "url", url } : null;
  }
  if (kind === "path") {
    const path = anchor.getAttribute("data-path");
    if (!path) return null;
    const line = anchor.getAttribute("data-line");
    const col = anchor.getAttribute("data-col");
    return { kind: "path", path, line: line ? Number(line) : null, col: col ? Number(col) : null, raw: anchor.textContent ?? path };
  }
  return null;
}

/**
 * The one click handler every text surface delegates to: the closest anchor
 * that is a door goes to the handler; anything else is left alone. Answers
 * whether it took the click.
 */
export function delegateLinkClick(e: React.MouseEvent, handler: LinkHandler | null, roots: readonly LinkRootRef[] | null): boolean {
  const anchor = (e.target as HTMLElement).closest("a");
  if (!anchor || !handler) return false;
  // A drag that ended here, or a double-click on a word of the path: the
  // person was selecting text, and a card would take the selection away.
  // Swallowed, not ignored: an anchor's own default is to navigate the
  // window, and an outer handler would open a relative link.
  if (endsSelection(window.getSelection(), e.currentTarget as HTMLElement)) {
    e.preventDefault();
    e.stopPropagation();
    return true;
  }
  const hit = hitOfAnchor(anchor);
  if (!hit) return false;
  e.preventDefault();
  e.stopPropagation();
  handler.onLink(hit, { x: e.clientX, y: e.clientY }, roots, { direct: e.metaKey || e.ctrlKey });
  return true;
}
