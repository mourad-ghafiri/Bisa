/**
 * Every addon window on screen (18 — Addons).
 *
 * Mounted in `App.tsx` beside the pet, outside `<Screen>` and its error
 * boundary, at `z-30` — under the notes panel and every dialog. It draws one
 * `AddonWindow` per addon that is active and not put away, while the layer
 * is shown and the machine's `addons.enabled` switch is on (`visibleAddons`,
 * the one rule the footer counts by too); keeps the list current
 * (`useAddonsSync`, mounted here and nowhere else); hands every window the facts an addon may ask
 * for — the workspace's counts, the machine's load — and asks the person
 * before any link an addon wants opened leaves the app.
 */

import { useCallback, useMemo, useRef, useState } from "react";
import { openExternal } from "../api";
import { t } from "../i18n/l10n.mjs";
import { useLayerSlot } from "../shell/layerSlots";
import { useSettledSessions } from "../shell/useSettledSessions";
import { useDiskUsage, useHostLoad } from "../shell/statsStore";
import { useWorkspace } from "../shell/useWorkspaceData";
import { ConfirmDialog } from "../ui";
import { counts as sessionCounts } from "../ui/sessionState.mjs";
import { AddonWindow } from "./AddonWindow";
import type { AddonLoad, AddonSummary } from "./addonBridge";
import { workSummary } from "../shell/workSummaryModel.mjs";
import { slotOf, visibleAddons } from "./addonsModel.mjs";
import { useAddons, useAddonsSync } from "./addonsStore";

export function AddonLayer() {
  // The one mount of the sync: the footer and the panel read the same store.
  useAddonsSync();
  const { addons, hidden, layerShown, switchedOn } = useAddons();
  const center = useLayerSlot("center");
  const aux = useLayerSlot("aux");
  const slots = useMemo(() => [center, aux], [center, aux]);

  // The facts a widget may read — the same the pet reads, the roster as the tabs say it.
  const ws = useWorkspace();
  const sessions = useSettledSessions();
  const summary: AddonSummary = useMemo(() => {
    const { waiting, review, working } = workSummary(ws, sessionCounts(sessions));
    return { waiting, review, working };
    // Keyed on the three facts `workSummary` reads: `ws` is a new object on
    // every workspace fact, and following it would wake every listening
    // addon for facts they never see.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ws.inbox, ws.waiting, ws.working, sessions]);
  const { load: hostLoad } = useHostLoad();
  const { disk } = useDiskUsage();
  // The footer's sample, plus the volume — one object, so an addon that
  // listens hears every figure the footer shows and nothing more.
  const load: AddonLoad | null = useMemo(() => {
    if (!hostLoad) return null;
    const volume = disk?.volume ?? null;
    return { ...hostLoad, disk: volume ? { used: volume.used, total: volume.total, mount: volume.mount, workspace_bytes: disk?.total ?? 0 } : null };
  }, [hostLoad, disk]);

  // One shield for the whole layer: a drag over one window must not hand the
  // pointer to another window's frame.
  const [shields, setShields] = useState(0);
  const onShield = useCallback((on: boolean) => setShields((n) => Math.max(0, n + (on ? 1 : -1))), []);

  // A link an addon asks to open: the person decides, in this window.
  const [ask, setAsk] = useState<{ name: string; url: string } | null>(null);
  const answer = useRef<((opened: boolean) => void) | null>(null);
  const openFor = useCallback(
    (name: string) => (url: string) =>
      new Promise<boolean>((resolve) => {
        answer.current?.(false);
        answer.current = resolve;
        setAsk({ name, url });
      }),
    [],
  );
  const settle = (opened: boolean) => {
    const url = ask?.url;
    setAsk(null);
    const resolve = answer.current;
    answer.current = null;
    if (opened && url) void openExternal(url);
    resolve?.(opened);
  };

  const windows = visibleAddons(addons, hidden, layerShown, switchedOn);
  return (
    <>
      {windows.map((addon) => (
        <AddonWindow
          key={addon.id}
          addon={addon}
          slot={slotOf(addons, addon.id)}
          slots={slots}
          layerVisible
          shielded={shields > 0}
          onShield={onShield}
          summary={summary}
          load={load}
          onOpenUrl={openFor(addon.manifest.name)}
        />
      ))}
      <ConfirmDialog
        open={ask !== null}
        onClose={() => settle(false)}
        onConfirm={() => settle(true)}
        title={ask ? t("addons-addon-layer-open-link-title", { name: ask.name }) : ""}
        body={ask ? <span className="break-all font-mono text-2xs">{ask.url}</span> : ""}
        confirmLabel={t("addons-addon-layer-open-link")}
      />
    </>
  );
}
