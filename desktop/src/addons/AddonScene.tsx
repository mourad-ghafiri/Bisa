/**
 * A scene addon (18 — Addons): an addon whose manifest says `frame: "scene"`
 * is not a window but the whole desktop under the chrome — see-through,
 * never dragged or resized, in the same sandboxed frame every window runs
 * (`sandbox="allow-scripts"`, the node's token-less files route, the one
 * bridge). Every fact is `addonSceneModel.mjs`'s; this component paints the
 * frame and hands it the pointer.
 *
 * Two modes the host owns: in **work** the frame is click-through, so the
 * app, its terminals, its footer and the pet keep the pointer while the
 * page's creatures walk over them — only the regions the page calls solid
 * (`scene.solid`, its own strip) take it, and the host flips the frame's
 * `pointer-events` while the pointer hovers one and back when the page says
 * it left (`scene.release`); in **hunt** (`scene.mode`) the frame has the
 * pointer everywhere under the chrome, and `Esc` leaves. Leaving a hunt or a
 * solid region blurs the frame, so a chord typed next reaches the app.
 *
 * The scene never publishes its whole box as a browser clear — a
 * viewport-sized hole would blank every native tab — only its solid regions
 * (`useBrowserClearBoxes`); the page is told where the tabs stand through the
 * `scene` event and keeps off them. Panels (z-40), dialogs and menus (z-50)
 * stand above the scene as they stand above every window.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../api";
import { useBrowserClearBoxes, useBrowserCutsAround } from "../shell/browserClear";
import type { SlotState } from "../shell/layerSlots";
import type { Addon } from "../types";
import { Button, toaster, useDockViewport } from "../ui";
import { useAddonBridge } from "./addonBridge";
import type { AddonBridgeHandlers, AddonLoad, AddonSummary } from "./addonBridge";
import type { AddonEvent, SceneMode, SceneRect } from "./addonBridgeModel.mjs";
import { entryOf, frameKeyOf, huntWords, sceneNotStartedWords } from "./addonsModel.mjs";
import { frameTaken, modeAfter, pointInSolid, sceneEvent, sceneKey, solidClears } from "./addonSceneModel.mjs";
import { setAddonHidden } from "./addonsStore";

export function AddonScene({
  addon,
  slots,
  shielded,
  summary,
  load,
  onOpenUrl,
}: {
  addon: Addon;
  /** The native layers' boxes: a browser tab is an obstacle the page keeps off. */
  slots: readonly SlotState[];
  /** A drag or resize runs somewhere in the layer: the frame takes nothing. */
  shielded: boolean;
  summary: AddonSummary;
  load: AddonLoad | null;
  onOpenUrl: (url: string) => Promise<boolean>;
}) {
  const vp = useDockViewport(0);
  const cuts = useBrowserCutsAround();
  const frame = useRef<HTMLIFrameElement>(null);
  const [mode, setMode] = useState<SceneMode>("work");
  const modeRef = useRef<SceneMode>("work");
  modeRef.current = mode;
  const [solid, setSolid] = useState<SceneRect[]>([]);
  const solidRef = useRef<SceneRect[]>([]);
  solidRef.current = solid;
  const [hover, setHover] = useState(false);
  /** The last pointer point the host saw, so a region that appears under a resting pointer takes it at once. */
  const lastPoint = useRef<{ x: number; y: number } | null>(null);
  /** How many hellos the page said: the first `scene` event follows each. */
  const [ready, setReady] = useState(0);
  const sendRef = useRef<(topic: AddonEvent, payload: unknown) => void>(() => {});
  // The frame reloads when the record moves — grants are said in the hello;
  // a scene has no box for a reload note: put away and shown again is one.
  const frameKey = frameKeyOf(addon);
  // content, never translated: the addon's own name.
  const name = addon.manifest.name;

  // The frame loses focus whenever the app gets the pointer back: a chord
  // typed next must reach the app, not a page that cannot hear it.
  const blurFrame = useCallback(() => {
    frame.current?.blur();
    const active = document.activeElement;
    if (active instanceof HTMLElement && active !== document.body) active.blur();
  }, []);

  const handlers: AddonBridgeHandlers = useMemo(
    () => ({
      onResize: () => {},
      onClose: () => setAddonHidden(addon.id, true),
      onTitle: () => {},
      onOpenUrl,
      onMediaEmbed: () => {},
      onMediaBox: () => {},
      onMediaRemove: () => {},
      onReady: () => setReady((n) => n + 1),
      onStartMissed: () => toaster.error(sceneNotStartedWords(name)),
      onSceneMode: (asked) => {
        const next = modeAfter(modeRef.current, asked);
        if (next === modeRef.current) return;
        if (next === "work") {
          setHover(false);
          blurFrame();
        }
        setMode(next);
      },
      onSceneSolid: (rects) => setSolid(rects),
      onSceneRelease: () => {
        setHover(false);
        blurFrame();
      },
    }),
    [addon.id, name, onOpenUrl, blurFrame],
  );
  const { sendEvent } = useAddonBridge(frame, addon, { visible: true, summary, load, handlers, epoch: frameKey });
  sendRef.current = sendEvent;

  // Work mode: the frame is click-through, so the page under it sees every
  // move; the pointer over a solid region hands the frame the pointer. A move
  // the host still sees while the frame has it comes from a layer above the
  // scene or from the chrome, and the region test says which.
  useEffect(() => {
    if (mode !== "work" || shielded) {
      setHover(false);
      return;
    }
    const onMove = (e: PointerEvent) => {
      lastPoint.current = { x: e.clientX, y: e.clientY };
      setHover(pointInSolid(lastPoint.current, solidRef.current, vp.top));
    };
    const onBlur = () => setHover(false);
    window.addEventListener("pointermove", onMove, { capture: true, passive: true });
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("pointermove", onMove, { capture: true });
      window.removeEventListener("blur", onBlur);
    };
  }, [mode, shielded, vp.top]);
  // The regions moved: the resting pointer is judged again.
  useEffect(() => {
    if (mode !== "work" || shielded || !lastPoint.current) return;
    setHover(pointInSolid(lastPoint.current, solid, vp.top));
  }, [solid, mode, shielded, vp.top]);

  // Hunt mode: the frame has the pointer everywhere under the chrome, the
  // footer included, and the host hears `Esc` only while it has the
  // keyboard — so a pill at the top edge says who has the pointer and
  // offers the way out, for a page that stopped answering mid-hunt.
  const leave = useCallback(() => {
    setHover(false);
    blurFrame();
    setMode("work");
  }, [blurFrame]);
  useEffect(() => {
    if (mode !== "hunt") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") leave();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mode, leave]);
  const hunt = huntWords(name);

  // What the page is told, once per change of a fact: the mode, the size,
  // the tabs to keep off, whether the solid regions are cut around.
  const payload = useMemo(() => sceneEvent({ mode, viewport: vp, slots, cuts }), [mode, vp, slots, cuts]);
  const key = sceneKey(payload);
  useEffect(() => {
    if (!ready) return;
    sendRef.current("scene", payload);
    // Keyed on the facts' string, so a re-render that moved nothing sends nothing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, ready]);

  // The solid regions, and only those, are cut around over a browser tab.
  const clears = useMemo(() => solidClears(solid, vp.top), [solid, vp.top]);
  useBrowserClearBoxes(`addon-scene:${addon.id}`, clears);

  // A scene that leaves — put away, the layer off — gives the pointer and the keyboard back.
  useEffect(() => () => blurFrame(), [blurFrame]);

  const taken = frameTaken({ mode, hovering: hover, shielded });

  return createPortal(
    <div
      role="region"
      aria-label={name}
      data-scene-mode={mode}
      style={{ left: 0, right: 0, bottom: 0, top: vp.top, pointerEvents: taken ? "auto" : "none" }}
      className="fixed z-30 bg-transparent"
    >
      <iframe
        key={frameKey}
        ref={frame}
        title={name}
        sandbox="allow-scripts"
        src={api.addonFileUrl(addon.id, entryOf(addon.manifest))}
        referrerPolicy="no-referrer"
        allow=""
        // @ts-expect-error — the attribute WebKit reads for a see-through frame.
        allowtransparency="true"
        className="block h-full w-full border-0 bg-transparent"
        style={{ colorScheme: "normal" }}
      />
      {mode === "hunt" && (
        <div role="status" data-pane className="pointer-events-auto absolute left-1/2 top-2 z-10 flex -translate-x-1/2 items-center gap-2 rounded-control border border-border bg-surface py-1 pl-2.5 pr-1 text-2xs text-text-dim shadow-lg">
          <span>{hunt.note}</span>
          <Button size="sm" variant="ghost" onClick={leave}>{hunt.action}</Button>
        </div>
      )}
      {/* The shield: while anything in the layer is dragged or resized, the frame takes nothing — even under a root that takes nothing itself. */}
      {shielded && <div aria-hidden className="pointer-events-auto absolute inset-0" />}
    </div>,
    document.body,
  );
}
