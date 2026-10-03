/**
 * What a harness's account has left, as a line: its windows in the adapter's
 * order, each *label · bar · percent* — `5h ▰▰ 24% · resets in 2 h 10 min
 * Weekly ▰▱ 41%  Fable ▱▱ 9%` — the first window's reset in place and every
 * other in its meter's tooltip, the credit balance after a divider on the
 * full line only, and a refresh door; a tooltip names the plan, the login,
 * the source and when it was read. A kept report — the last re-read failed —
 * is dimmed with the reason in that tooltip. A harness that reports nothing
 * says so in one dim sentence. The words are `harnessUsageModel.mjs`'s; the
 * reads are `harnessUsageStore.ts`'s.
 *
 * Two surfaces draw it. `UsageLine` is the paint alone, over an entry the
 * caller reads — the footer's stat (compact: one line, windows only) and its
 * picker. Where the line sits inside a control of its own — the footer's
 * trigger, a picker row — it draws no refresh (`refresh={false}`): a button
 * is never nested in a button, and the caller sets `RefreshUsage` beside it. `HarnessUsageLine` is the folded line Settings › Harnesses shows,
 * with `UsageToggle`, the gauge that shows or hides it per harness there. A
 * harness row in the rail or the Workstreams panel carries none: the footer
 * is where a harness's account is read (ide/07).
 */

import type { HarnessUsageEntry } from "./harnessUsageStore";
import { refreshHarnessUsage, useHarnessUsage } from "./harnessUsageStore";
import { ICON, Meter, Tooltip, cn, useCollapsed } from "../ui";
import type { Meter as MeterWords } from "./harnessUsageModel.mjs";
import { usageKey, usageTitle, usageWords } from "./harnessUsageModel.mjs";
import { t } from "../i18n/l10n.mjs";
import { percent } from "../i18n/format.mjs";

/** Whether a harness's usage line is shown in Settings › Harnesses — shown by default, per harness. */
function useUsageShown(harness: string): [boolean, () => void] {
  const [hidden, toggle] = useCollapsed(usageKey(harness), false);
  return [!hidden, toggle];
}

/** The icon on a harness row that shows or hides its usage line. */
export function UsageToggle({ harness, className }: { harness: string; className?: string }) {
  const [shown, toggle] = useUsageShown(harness);
  const words = shown ? t("shell-harness-usage-line-hide-usage") : t("shell-harness-usage-line-show-usage");
  return (
    <Tooltip label={words}>
      <button
        type="button"
        aria-label={words}
        aria-pressed={shown}
        onClick={(e) => {
          e.stopPropagation();
          toggle();
        }}
        className={cn("anim flex h-4 w-4 shrink-0 items-center justify-center rounded hover:bg-surface-2", shown ? "text-text" : "text-text-dim hover:text-text", className)}
      >
        <ICON.usage size={11} aria-hidden />
      </button>
    </Tooltip>
  );
}

/** The refresh door: ask the harness's source again, now. */
export function RefreshUsage({ harness, loading }: { harness: string; loading: boolean }) {
  return (
    <Tooltip label={t("shell-harness-usage-line-read-harness-s-usage-again")}>
      <button
        type="button"
        aria-label={t("shell-harness-usage-line-refresh-usage")}
        disabled={loading}
        onClick={(e) => {
          e.stopPropagation();
          refreshHarnessUsage(harness);
        }}
        className="anim flex h-5 w-5 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45"
      >
        <ICON.refresh size={10} aria-hidden className={loading ? "motion-safe:animate-spin" : undefined} />
      </button>
    </Tooltip>
  );
}

/** One window: its word, its bar, its percentage in the tone it earns; its own reset on hover. */
function MeterChip({ meter }: { meter: MeterWords }) {
  return (
    <span className="inline-flex shrink-0 items-center gap-1" title={meter.tip}>
      <span className="text-text-dim">{meter.label}</span>
      <Meter percent={meter.percent} tone={meter.tone} />
      <span className={cn("tnum", meter.tone === "danger" ? "text-danger" : meter.tone === "warn" ? "text-warn" : "text-text")}>{percent(meter.percent / 100)}</span>
    </span>
  );
}

/**
 * The line itself, over an entry the caller reads. `compact` keeps it to one
 * line of windows that truncates — the footer's width — where the full line
 * wraps and adds the credit balance.
 */
export function UsageLine({ harness, entry, compact = false, refresh = true, className }: { harness: string; entry: HarnessUsageEntry; compact?: boolean; /** Draw the refresh door; off where the line is inside a control. */ refresh?: boolean; className?: string }) {
  const words = usageWords(entry.state);
  const report = entry.state?.state === "report" ? entry.state.report : null;
  const title = report ? usageTitle(report, Date.now() / 1000, entry.stale) : undefined;
  return (
    <span className={cn("flex min-w-0 items-center gap-2", "text-2xs", entry.stale && "opacity-60", className)} title={title} role="group" aria-label={t("shell-harness-usage-line-usage")}>
      {words.note ? (
        <span className={cn("min-w-0 flex-1 truncate", words.tone === "warn" ? "text-warn" : "text-text-dim")}>{words.note}</span>
      ) : (
        <span className={cn("flex min-w-0 flex-1 items-center gap-x-2.5", compact ? "overflow-hidden whitespace-nowrap" : "flex-wrap gap-y-0.5")}>
          {words.meters.map((m, i) => (
            // The reset note is what gives way on a narrow line: it truncates first, so every
            // meter keeps its whole percentage rather than the last one being cut mid-number.
            <span key={m.id} className={cn("inline-flex items-center gap-1.5", i === 0 && words.inline ? "min-w-0" : "shrink-0")}>
              <MeterChip meter={m} />
              {i === 0 && words.inline && <span className="min-w-0 truncate text-text-dim">{t("shell-harness-usage-line-inline-note", { inline: words.inline })}</span>}
            </span>
          ))}
          {!compact && words.extras.length > 0 && (
            <>
              <span aria-hidden className="h-3 w-px shrink-0 bg-hairline" />
              {words.extras.map((m) => (
                <MeterChip key={m.id} meter={m} />
              ))}
            </>
          )}
        </span>
      )}
      {refresh && <RefreshUsage harness={harness} loading={entry.loading} />}
    </span>
  );
}

/** The folded line under a harness's row: shown or hidden by `UsageToggle`, read only while shown. */
export function HarnessUsageLine({ harness, className }: { harness: string; className?: string }) {
  const [shown] = useUsageShown(harness);
  const entry = useHarnessUsage(harness, shown);
  if (!shown) return null;
  return <UsageLine harness={harness} entry={entry} className={className} />;
}
