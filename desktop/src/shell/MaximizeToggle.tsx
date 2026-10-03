/**
 * The maximize / restore button a floating panel wears in its header — the
 * Notes and Draw overlays' list headers and editors render this one, so the
 * icon pair, the pressed state and the words are decided once. The wording
 * names the frame `maximizedPanel.ts` gives: the window between the header,
 * the footer and the sidebar; and the corner it comes back to.
 */
import { ICON, Tooltip } from "../ui";
import { t } from "../i18n/l10n.mjs";

export function MaximizeToggle({ maximized, onToggle, size = 14 }: { maximized: boolean; onToggle: () => void; size?: 13 | 14 }) {
  const label = maximized ? t("shell-panel-restore") : t("shell-panel-maximize");
  const Icon = maximized ? ICON.collapse : ICON.expand;
  return (
    <Tooltip label={label}>
      <button type="button" aria-label={label} aria-pressed={maximized} onClick={onToggle} className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
        <Icon size={size} aria-hidden />
      </button>
    </Tooltip>
  );
}
