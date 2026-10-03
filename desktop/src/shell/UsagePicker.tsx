/**
 * The overlay under the footer's usage stat: every installed harness with
 * its usage line, each row a radio that pins it — the one the footer keeps
 * showing. While the list is open every row reads its harness, so every
 * account is read once and polled together; *Refresh all* asks every source
 * again.
 */

import type { HarnessRow } from "../types";
import { ICON, Tooltip, cn, harnessMark } from "../ui";
import { UsageLine } from "./HarnessUsageLine";
import { refreshHarnessUsage, useHarnessUsage } from "./harnessUsageStore";
import { pinUsage } from "./footerUsageStore";
import { t } from "../i18n/l10n.mjs";

function PickerRow({ row, pinned }: { row: HarnessRow; pinned: boolean }) {
  const entry = useHarnessUsage(row.id, true);
  const Mark = harnessMark(row.id);
  return (
    <button
      type="button"
      role="radio"
      aria-checked={pinned}
      onClick={() => pinUsage(row.id)}
      title={pinned ? t("shell-usage-picker-one-footer-shows", { row: row.label }) : t("shell-usage-picker-show-footer", { row: row.label })}
      className={cn("anim flex w-full flex-col gap-1 rounded-control px-2 py-1.5 text-left", pinned ? "bg-selected" : "hover:bg-surface-2")}
    >
      <span className="flex items-center gap-1.5 text-xs">
        <Mark size={13} aria-hidden className="shrink-0" />
        <span className="min-w-0 flex-1 truncate font-medium text-text">{row.label}</span>
        {pinned && <ICON.check size={12} aria-hidden className="shrink-0 text-text" />}
      </span>
      {/* No refresh inside the row: the row is a radio button, and *Refresh all* above reads every one. */}
      <UsageLine harness={row.id} entry={entry} refresh={false} className="pl-5" />
    </button>
  );
}

export function UsagePicker({ rows, pinned }: { rows: readonly HarnessRow[]; pinned: string | null }) {
  return (
    <div role="radiogroup" aria-label={t("shell-usage-picker-harness-footer-shows")} className="flex flex-col gap-1">
      <div className="flex items-center justify-between px-2 pb-1">
        <span className="text-2xs font-semibold text-text-dim">{t("shell-usage-picker-harness-usage")}</span>
        <Tooltip label={t("shell-usage-picker-read-every-harness-s-usage-again")}>
          <button
            type="button"
            aria-label={t("shell-usage-picker-refresh-all")}
            onClick={() => {
              for (const r of rows) refreshHarnessUsage(r.id);
            }}
            className="anim flex h-6 items-center gap-1 rounded-control px-1.5 text-2xs text-text-dim hover:bg-surface-2 hover:text-text"
          >
            <ICON.refresh size={10} aria-hidden />{t("shell-usage-picker-refresh-all")}</button>
        </Tooltip>
      </div>
      {rows.map((r) => (
        <PickerRow key={r.id} row={r} pinned={r.id === pinned} />
      ))}
      <p className="px-2 pt-1 text-2xs leading-relaxed text-text-dim">{t("shell-usage-picker-pick-the-one-to-keep")}</p>
    </div>
  );
}
