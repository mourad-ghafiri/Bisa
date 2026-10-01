/**
 * A small bar for a percentage — a harness account's window, a budget —
 * in the tone the number earns. Text carries the number; the bar is the
 * glance. Fills past 100 stay full and keep their tone.
 */

import { cn } from "./cn";

export type MeterTone = "ok" | "warn" | "danger" | "quiet";

const FILL: Record<MeterTone, string> = {
  ok: "bg-ok",
  warn: "bg-warn",
  danger: "bg-danger",
  quiet: "bg-text-dim",
};

export function Meter({ percent, tone = "quiet", width = "w-8", title, className }: { percent: number; tone?: MeterTone; width?: string; title?: string; className?: string }) {
  const fill = Math.max(0, Math.min(100, percent));
  return (
    <span
      role="meter"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(percent)}
      title={title}
      className={cn("inline-block h-1.5 shrink-0 overflow-hidden rounded-full bg-surface-2", width, className)}
    >
      <span className={cn("block h-full rounded-full", FILL[tone])} style={{ width: `${fill}%` }} />
    </span>
  );
}
