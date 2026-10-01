/**
 * The footer's node read-out — the node glyph and a dot in the connection's
 * tone, no text; the sentence as the tooltip and the accessible name — and,
 * on a click, its overlay (`NodeOverlay`). Controlled, so the overlay's door
 * can close the panel behind it. The connection is the SSE bus's word,
 * threaded from `App`; every word is `nodeStatModel.mjs`'s.
 */

import { useState } from "react";
import type { ConnState } from "../bus";
import { t } from "../i18n/l10n.mjs";
import { Dot, ICON, Popover, Tooltip, cn } from "../ui";
import { NodeOverlay } from "./NodeOverlay";
import { statWords } from "./nodeStatModel.mjs";

export function NodeStat({ conn }: { conn: ConnState }) {
  const [open, setOpen] = useState(false);
  const words = statWords(conn);
  return (
    <Popover
      label={words.title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      className="w-[30rem] max-w-[90vw]"
      trigger={
        <Tooltip label={words.title}>
          <span aria-label={t("shell-node-stat-node-word", { word: words.word })} className={cn("anim flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-control px-1 text-text-dim hover:bg-surface-2")}>
            <ICON.node size={13} aria-hidden />
            <Dot tone={words.tone} title={words.word} />
          </span>
        </Tooltip>
      }
    >
      {open && <NodeOverlay conn={conn} close={() => setOpen(false)} />}
    </Popover>
  );
}
