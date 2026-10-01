/**
 * One of the footer's resource read-outs — a glyph and its value, the word
 * only as the accessible name, the metric's sentence as the tooltip — and,
 * on a click, its overlay (`ResourceOverlay`). Controlled, so a row that
 * opened something can close the panel behind it. The values are the stats
 * store's host part; the overlay reads the rest only while it is open.
 */

import { useState } from "react";
import { ICON, Popover, Tooltip, cn } from "../ui";
import { ResourceOverlay } from "./ResourceOverlay";
import type { Metric } from "./resourceModel.mjs";

const GLYPH: Record<Metric, (typeof ICON)[keyof typeof ICON]> = { cpu: ICON.cpu, gpu: ICON.gpu, memory: ICON.memory, disk: ICON.disk };
const NAME: Record<Metric, string> = { cpu: "CPU", gpu: "GPU", memory: "memory", disk: "disk" };

export function ResourceStat({ metric, value, title }: { metric: Metric; value: string; title: string }) {
  const [open, setOpen] = useState(false);
  const Icon = GLYPH[metric];
  return (
    <Popover
      label={title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      // The width is the tab row's: six dimensions, each its glyph and its
      // whole word, never cut. In the type unit so the panel grows with the
      // text (`theme/tokens.css`) instead of trapping the row at a larger scale.
      className="w-[calc(var(--type-rem)*40)] max-w-[90vw]"
      trigger={
        <Tooltip label={title}>
          <span aria-label={`${NAME[metric]} ${value}`.trim()} className={cn("anim flex h-6 shrink-0 cursor-pointer items-center gap-1 rounded-control px-1 text-text-dim hover:bg-surface-2")}>
            <Icon size={13} aria-hidden />
            <span className="tnum text-2xs text-text">{value}</span>
          </span>
        </Tooltip>
      }
    >
      {open && <ResourceOverlay metric={metric} close={() => setOpen(false)} />}
    </Popover>
  );
}
