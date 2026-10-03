/**
 * The popover under the footer's Addons read-out: the layer's switch, then
 * **every installed addon** — its name, what state it is in, its on/off
 * switch (the node's record, optimistic) and, while it runs, *Show* / *Put
 * away* for its window (this machine's) — then the way back to where every
 * window opened, and a door to Settings › Addons. Every fact is the addons
 * store's and every sentence `addonsModel.mjs`'s; this file draws.
 */

import { useState } from "react";
import { t } from "../i18n/l10n.mjs";
import { errorFields, log } from "../log";
import { navigate } from "../router";
import { Button, Chip, ICON, Switch, Tooltip, useToast } from "../ui";
import { settingsPath, settingsSearch } from "../views/_settings/settingsLink.mjs";
import { overlayRows, rowStateWords, switchWords, titleWords, visibleAddons } from "../addons/addonsModel.mjs";
import type { OverlayRow } from "../addons/addonsModel.mjs";
import { resetAddonPositions, setAddonEnabled, setAddonHidden, toggleAddonLayer, useAddons } from "../addons/addonsStore";

function Row({ row, disabled }: { row: OverlayRow; disabled: boolean }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const { addon } = row;
  const flip = async (on: boolean) => {
    setBusy(true);
    try {
      await setAddonEnabled(addon.id, on);
    } catch (e) {
      log.warn("addons", "an addon could not be switched", { addon: addon.id, on, ...errorFields(e) });
      toast.error(t("shell-addons-overlay-could-not-switch", { name: addon.manifest.name }));
    } finally {
      setBusy(false);
    }
  };
  const eye = row.shown ? t("shell-addons-overlay-put-away", { name: addon.manifest.name }) : t("shell-addons-overlay-show-window", { name: addon.manifest.name });
  return (
    <li className="flex items-center gap-2">
      {/* content, never translated: the addon's own name. */}
      <span className="min-w-0 flex-1 truncate text-xs text-text">{addon.manifest.name}</span>
      <Chip tone={row.running && row.shown ? "neutral" : "quiet"}>{rowStateWords(row)}</Chip>
      {row.running && (
        <Tooltip label={eye}>
          <button
            type="button"
            aria-label={eye}
            aria-pressed={row.shown}
            disabled={disabled}
            onClick={() => setAddonHidden(addon.id, row.shown)}
            className="anim flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45"
          >
            {row.shown ? <ICON.eye size={13} aria-hidden /> : <ICON.eyeOff size={13} aria-hidden />}
          </button>
        </Tooltip>
      )}
      <Switch checked={addon.enabled} disabled={busy || (!addon.enabled && !row.canEnable)} onChange={(on) => void flip(on)} label={t("shell-addons-overlay-on", { name: addon.manifest.name })} hideLabel />
    </li>
  );
}

export function AddonsOverlay({ close }: { close: () => void }) {
  const { addons, hidden, layerShown, switchedOn, loaded, failed } = useAddons();
  const rows = overlayRows(addons, hidden, layerShown, switchedOn);
  const showing = visibleAddons(addons, hidden, layerShown, switchedOn).length;
  const words = switchWords(layerShown);
  const running = rows.filter((r) => r.running).length;
  return (
    <div className="flex min-w-0 flex-col gap-3" role="group" aria-label={titleWords(showing, addons.length)}>
      <p className="px-1 text-xs font-medium text-text">{titleWords(showing, addons.length)}</p>
      <Switch checked={layerShown} onChange={(on) => toggleAddonLayer(on)} label={words.label} hint={words.hint} disabled={!switchedOn || !loaded} />
      {rows.length > 0 && (
        <div className="flex flex-col gap-1 px-1">
          <p className="text-2xs font-semibold text-text-dim">{t("shell-addons-overlay-installed", { n: rows.length })}</p>
          <ul className="flex max-h-72 flex-col gap-1.5 overflow-y-auto">
            {rows.map((row) => (
              <Row key={row.addon.id} row={row} disabled={!layerShown || !switchedOn} />
            ))}
          </ul>
        </div>
      )}
      {failed && <p className="px-1 text-2xs text-danger">{failed}</p>}
      {loaded && rows.length === 0 && <p className="px-1 text-2xs text-text-dim">{t("shell-addons-overlay-none-installed")}</p>}
      {!switchedOn && loaded && <p className="px-1 text-2xs text-text-dim">{t("shell-addons-overlay-machine-off")}</p>}
      <div className="flex items-center justify-between gap-2 px-1">
        <Button size="sm" variant="ghost" disabled={running === 0} onClick={resetAddonPositions}>{t("shell-addons-overlay-reset-positions")}</Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            navigate({ name: "settings" }, settingsSearch("addons"));
            close();
          }}
        >{settingsPath("addons")}</Button>
      </div>
    </div>
  );
}
