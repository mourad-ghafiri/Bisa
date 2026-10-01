/**
 * A page artifact, live (ide/12): a sandboxed frame with the page as its
 * `srcdoc` and the page's own Content-Security-Policy in its head
 * (`pageDocument.mjs`). No `src`, ever — the node serves nothing an agent
 * wrote as a page — and no `allow-same-origin`: the frame's origin is
 * opaque, so the page cannot reach the app's origin, its token or its storage.
 *
 * With `inspector` the document also carries the IDE's inspector script
 * (ide/03 §Annotate), dressed in the app's theme as the document is built,
 * and the frame lends its element and its `load` to the caller —
 * `usePageInspector` is the app's half of that wire, and says the theme again
 * when it moves rather than rebuilding the document: a reload would close a
 * box open in the page and drop the note typed in it. The walls do not change.
 */

import { useMemo } from "react";
import type { Ref } from "react";
import { cn } from "../cn";
import { useInspectorTheme } from "./inspectorTokens";
import { PAGE_SANDBOX, pageDocument } from "./pageDocument.mjs";

export function PageFrame({
  html,
  title,
  libraries,
  className,
  inert = false,
  inspector = false,
  frameRef,
  onLoad,
}: {
  html: string;
  title: string;
  /** Whether the page may load libraries from the three CDNs (`artifacts.html.libraries`). */
  libraries: boolean;
  className?: string;
  /** A preview under a message: the frame runs but the pointer goes to the card. */
  inert?: boolean;
  /** Carry the IDE's inspector script — a page being annotated, never an artifact's. */
  inspector?: boolean;
  /** The frame's element, for the inspector's wire. */
  frameRef?: Ref<HTMLIFrameElement>;
  /** The document loaded — again, on every `srcdoc` change. */
  onLoad?: () => void;
}) {
  const theme = useInspectorTheme();
  // The theme at build only: a later switch is said to the page (`bisa:theme`), never a new document.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const doc = useMemo(() => pageDocument(html, { libraries, scheme: theme.scheme, inspector: inspector ? theme : null }), [html, libraries, inspector]);
  return (
    <iframe
      ref={frameRef}
      title={title}
      sandbox={PAGE_SANDBOX}
      srcDoc={doc}
      referrerPolicy="no-referrer"
      onLoad={onLoad}
      className={cn("block h-full w-full border-0 bg-white", inert && "pointer-events-none", className)}
    />
  );
}
