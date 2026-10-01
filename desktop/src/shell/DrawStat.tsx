/**
 * The footer's Draw switch (19 — Drawings): shows or hides the floating
 * drawings button — opening the panel is that button's job, or the keymap's
 * `toggle_draw`, never the footer's — and wears a working dot while an agent
 * draws into a drawing, the one place the footer says so.
 */

import { useBusyDrawings } from "../draw/drawActivityStore";
import { drawingWords } from "../draw/drawRequestModel.mjs";
import { setDrawDockVisible, useDrawOverlay } from "../draw/drawStore";
import { ICON, WorkingDot } from "../ui";
import { FooterToggle } from "./FooterToggle";
import { t } from "../i18n/l10n.mjs";

export function DrawStat() {
  const { dockVisible } = useDrawOverlay();
  const busy = useBusyDrawings();
  return (
    <span className="relative inline-flex" data-draw-stat>
      <FooterToggle icon={ICON.draw} on={dockVisible} label={dockVisible ? t("shell-status-bar-hide-draw-button") : t("shell-status-bar-show-draw-button")} onClick={() => setDrawDockVisible(!dockVisible)} />
      {busy.length > 0 && (
        <span className="pointer-events-none absolute -top-0.5 -right-0.5">
          <WorkingDot title={drawingWords(busy.length)} />
        </span>
      )}
    </span>
  );
}
