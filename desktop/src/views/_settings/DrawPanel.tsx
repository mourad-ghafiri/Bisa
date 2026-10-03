/**
 * What the Draw overlay does on this machine, and how much of it is on
 * screen (19 — Drawings): the dock, its count, whether the panel opens
 * maximized, how long after a stroke the canvas saves, and the dock's place.
 * Every control here is window state on this machine — the panel's own
 * furniture. Who may draw and how wide a snapshot is are the workspace's
 * (`draw.*`), drawn by the registry panel beneath this one.
 */

import { Button, Card, Section, Slider, Switch } from "../../ui";
import { SAVE_MAX_MS, SAVE_MIN_MS, SAVE_STEP_MS } from "../../draw/drawModel.mjs";
import { drawDockMoved, resetDrawDockPosition, setDrawCountBadge, setDrawDockVisible, setDrawMaximized, setDrawSaveDelay, useDrawOverlay } from "../../draw/drawStore";
import { t } from "../../i18n/l10n.mjs";

export function DrawPanel() {
  const { dock, dockVisible, countBadge, maximized, saveDelay } = useDrawOverlay();
  const moved = drawDockMoved(dock);

  return (
    <div className="flex flex-col gap-6">
      <Section title={t("settings-draw-panel-on-screen")}>
        <Card>
          <div className="flex flex-col gap-3">
            <Switch checked={dockVisible} onChange={setDrawDockVisible} label={t("settings-draw-panel-show-floating-dock")} hint={t("settings-draw-panel-dock-hint")} />
            <Switch checked={countBadge} onChange={setDrawCountBadge} disabled={!dockVisible} label={t("settings-draw-panel-show-count")} hint={dockVisible ? t("settings-draw-panel-count-hint") : t("settings-draw-panel-count-hidden-hint")} />
            <Switch checked={maximized} onChange={setDrawMaximized} label={t("settings-draw-panel-open-maximized")} hint={t("settings-draw-panel-maximized-hint")} />
          </div>
        </Card>
      </Section>

      <Section title={t("settings-draw-panel-drawing")}>
        <Card>
          <Slider
            label={t("settings-draw-panel-save-after-stroke")}
            value={saveDelay}
            min={SAVE_MIN_MS}
            max={SAVE_MAX_MS}
            step={SAVE_STEP_MS}
            onChange={setDrawSaveDelay}
            format={(ms) => `${(ms / 1000).toFixed(1)}s`}
            hint={t("settings-draw-panel-save-hint")}
          />
        </Card>
      </Section>

      <Section title={t("settings-draw-panel-layout")}>
        <Card>
          <div className="flex items-center justify-between gap-3">
            <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{moved ? t("settings-draw-panel-dock-dragged") : t("settings-draw-panel-dock-where-started")}</p>
            <Button size="sm" variant="ghost" disabled={!moved} onClick={resetDrawDockPosition}>
              {t("settings-draw-panel-reset-position")}
            </Button>
          </div>
        </Card>
      </Section>
    </div>
  );
}
