/**
 * Every addon window on screen (18 — Addons).
 *
 * Mounted by `AddonsHost` beside the pet, at `z-30` — under the notes
 * panel and every dialog. It draws one `AddonWindow` per addon that is
 * active and not put away — or an `AddonScene` for one whose manifest says
 * `frame: "scene"`, mounted first so the windows stand above it — while the
 * layer is shown and the machine's `addons.enabled` switch is on
 * (`visibleAddons`, the one rule the footer counts by too). Each addon
 * stands in a boundary of its own: one that throws is put away with a toast
 * naming it, and the others stand; the eye brings it back as a fresh page.
 * The layer hands every window the facts an addon may ask for — the
 * workspace's counts, the machine's load — and asks the person before any
 * link an addon wants opened leaves the app. The list's sync is the host's,
 * outside this boundary.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { openExternal } from "../api";
import { t } from "../i18n/l10n.mjs";
import { errorFields, log } from "../log";
import { useLayerSlot } from "../shell/layerSlots";
import { useSettledSessions } from "../shell/useSettledSessions";
import { useDiskUsage, useHostLoad } from "../shell/statsStore";
import { useWorkspace } from "../shell/useWorkspaceData";
import type { Addon } from "../types";
import { ConfirmDialog, ErrorBoundary, toaster } from "../ui";
import { counts as sessionCounts } from "../ui/sessionState.mjs";
import { AddonScene } from "./AddonScene";
import { AddonWindow } from "./AddonWindow";
import type { AddonLoad, AddonSummary } from "./addonBridge";
import { workSummary } from "../shell/workSummaryModel.mjs";
import { isScene, slotOf, stoppedWords, visibleAddons } from "./addonsModel.mjs";
import { setAddonHidden, useAddons } from "./addonsStore";

/** A closed addon draws nothing. */
const nothing = (): ReactNode => null;

/**
 * One addon's boundary: a throw in its window, its scene or their hooks
 * puts that addon away — said in a toast with its name and in the log — and
 * costs nobody else. No reset key: put away unmounts it, and the eye mounts
 * a fresh one.
 */
function AddonBoundary({ addon, children }: { addon: Addon; children: ReactNode }) {
  return (
    <ErrorBoundary
      fallback={nothing}
      onError={(error, info) => {
        log.error("addons", "an addon crashed and was put away", { addon: addon.id, ...errorFields(error), component_stack: info.componentStack ?? undefined });
        toaster.error(stoppedWords(addon.manifest.name));
        setAddonHidden(addon.id, true);
      }}
    >
      {children}
    </ErrorBoundary>
  );
}

export function AddonLayer() {
  const { addons, hidden, layerShown, switchedOn } = useAddons();
  const center = useLayerSlot("center");
  const aux = useLayerSlot("aux");
  const slots = useMemo(() => [center, aux], [center, aux]);

  // The facts a widget may read — the same the pet reads, the roster as the tabs say it.
  const ws = useWorkspace();
  const sessions = useSettledSessions();
  const summary: AddonSummary = useMemo(() => {
    const { waiting, review, working } = workSummary(ws, sessionCounts(sessions), sessions);
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

  // A link an addon asks to open: the person decides, in this window. The
  // ask is the addon's by id — two addons may share a name — and leaves
  // with it.
  const [ask, setAsk] = useState<{ id: string; name: string; url: string } | null>(null);
  const answer = useRef<((opened: boolean) => void) | null>(null);
  const openFor = useCallback(
    (addon: Addon) => (url: string) =>
      new Promise<boolean>((resolve) => {
        answer.current?.(false);
        answer.current = resolve;
        setAsk({ id: addon.id, name: addon.manifest.name, url });
      }),
    [],
  );
  const settle = useCallback(
    (opened: boolean) => {
      setAsk((current) => {
        if (opened && current) {
          void openExternal(current.url).catch((e: unknown) => log.warn("addons", "the link an addon asked for could not be opened", { addon: current.id, ...errorFields(e) }));
        }
        return null;
      });
      const resolve = answer.current;
      answer.current = null;
      resolve?.(opened);
    },
    [],
  );

  const shown = visibleAddons(addons, hidden, layerShown, switchedOn);
  // The addon that asked left — put away, switched off, removed: its question goes with it.
  const askedBy = ask?.id ?? null;
  const askerShown = askedBy !== null && shown.some((a) => a.id === askedBy);
  useEffect(() => {
    if (askedBy !== null && !askerShown) settle(false);
  }, [askedBy, askerShown, settle]);

  // A scene is the whole desktop under the chrome; it is mounted first, so
  // every window keeps standing above it (portal order is paint order).
  const scenes = shown.filter(isScene);
  const windows = shown.filter((addon) => !isScene(addon));
  return (
    <>
      {scenes.map((addon) => (
        <AddonBoundary key={addon.id} addon={addon}>
          <AddonScene addon={addon} slots={slots} shielded={shields > 0} summary={summary} load={load} onOpenUrl={openFor(addon)} />
        </AddonBoundary>
      ))}
      {windows.map((addon) => (
        <AddonBoundary key={addon.id} addon={addon}>
          <AddonWindow
            addon={addon}
            slot={slotOf(addons, addon.id)}
            slots={slots}
            layerVisible
            shielded={shields > 0}
            onShield={onShield}
            summary={summary}
            load={load}
            onOpenUrl={openFor(addon)}
          />
        </AddonBoundary>
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
