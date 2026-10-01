/**
 * A bar of bands that share one whole (the footer's resource and browser
 * overlays): each band a tone of its own, a hairline between bands, the
 * track's colour for what is left — and, when the caller gives one, a legend
 * beneath that says each part in words. Text carries the numbers; the bar is
 * the glance. The fill classes live here, once: a band is never the track's
 * colour, or it would read as empty.
 */

import { cn } from "./cn";
import { percent } from "../i18n/format.mjs";
import { t } from "../i18n/l10n.mjs";

export type BandTone = "accent" | "ok" | "warn" | "quiet" | "neutral";
export type LegendTone = BandTone | "track";

const FILL: Record<BandTone, string> = {
  accent: "bg-accent",
  ok: "bg-ok",
  warn: "bg-warn",
  quiet: "bg-text-dim",
  neutral: "bg-border",
};

const SWATCH: Record<LegendTone, string> = {
  ...FILL,
  track: "bg-surface-2 ring-1 ring-inset ring-border",
};

export interface Band {
  key: string;
  label: string;
  percent: number;
  tone: BandTone;
  /** The part's own number in the metric, for the tooltip; optional for a bar that is percents alone. */
  value?: number;
}

export interface LegendEntry {
  key: string;
  label: string;
  tone: LegendTone;
  words: string;
}

export function StackedBar({
  segments,
  legend = [],
  label,
  words,
  className,
}: {
  segments: readonly Band[];
  legend?: readonly LegendEntry[];
  /** The bar's whole sentence, for a reader. */
  label: string;
  /** A band's value in words, for its tooltip; the percent alone without it. */
  words?: (band: Band) => string;
  className?: string;
}) {
  if (segments.length === 0) return null;
  const tip = (b: Band) => t("ui-stacked-bar-band", { label: b.label, words: words ? words(b) : percent(b.percent / 100) });
  return (
    <div className={cn("flex min-w-0 flex-col gap-1", className)}>
      <div role="img" aria-label={label} className="flex h-2.5 w-full gap-px overflow-hidden rounded-full bg-surface-2">
        {segments.map((b) => (
          <span key={b.key} title={tip(b)} className={cn("block h-full", FILL[b.tone])} style={{ width: `${Math.max(0, Math.min(100, b.percent))}%`, minWidth: b.percent > 0 ? 2 : 0 }} />
        ))}
      </div>
      {legend.length > 0 && (
        <ul className="flex flex-wrap items-center gap-x-3 gap-y-0.5 px-0.5" aria-label={t("ui-stacked-bar-legend")}>
          {legend.map((l) => (
            <li key={l.key} className="flex items-center gap-1 text-2xs">
              <span aria-hidden className={cn("inline-block h-2 w-2 shrink-0 rounded-full", SWATCH[l.tone])} />
              <span className="text-text-dim">{l.label}</span>
              <span className="tnum text-text">{l.words}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
