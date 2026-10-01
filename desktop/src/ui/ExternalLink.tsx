/**
 * A link to somewhere outside the app — a pull request, a check run, an
 * agent's page — that opens through the handler (ide/17): the card asks,
 * the browser opens, the window stays. Never `target="_blank"`: in the
 * desktop shell that is a navigation of the app itself.
 */

import type { ReactNode } from "react";
import { delegateLinkClick, useLinkHandler } from "./linkContext";

export function ExternalLink({ href, className = "", title, children }: { href: string; className?: string; title?: string; children: ReactNode }) {
  const handler = useLinkHandler();
  return (
    <a
      href={href}
      data-link="url"
      title={title}
      className={className}
      onClick={(e) => {
        if (!delegateLinkClick(e, handler, null)) e.preventDefault();
      }}
    >
      {children}
    </a>
  );
}
