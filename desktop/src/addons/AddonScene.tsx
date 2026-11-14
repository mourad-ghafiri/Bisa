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
 * pointer everywhere under the chrome and above the footer, and `Esc` leaves.
 * Leaving a hunt or a solid region blurs the frame, so a chord typed next
 * reaches the app.
 *
 * A hunt is left from the page's own strip. The host draws nothing over the
 * app for it; what it keeps is for a page that stopped answering: `Esc`, a
 * footer no hunt covers (the Addons control puts the scene away), and a
 * state that falls back to work whenever the page starts again (`sceneAfter`).
 *
 * The scene never publishes its whole box as a browser clear — a
 * viewport-sized hole would blank every native tab — only its solid regions
 * (`useBrowserClearBoxes`); the page is told where the tabs stand through the
 * `scene` event and keeps off them. Panels (z-40), dialogs and menus (z-50)
 * stand above the scene as they stand above every window.
 */

import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../api";
import { useBrowserClearBoxes, useBrowserCutsAround } from "../shell/browserClear";
import type { SlotState } from "../shell/layerSlots";
import type { Addon } from "../types";
import { toaster, useDockViewport } from "../ui";
import { useAddonBridge } from "./addonBridge";
import type { AddonBridgeHandlers, AddonLoad, AddonSummary } from "./addonBridge";
import type { AddonEvent, SceneMode, SceneRect } from "./addonBridgeModel.mjs";
import { entryOf, frameKeyOf, huntStatus, sceneNotStartedWords } from "./addonsModel.mjs";
import { SCENE_REST, footerKept, frameTaken, pointInSolid, sceneAfter, sceneEvent, sceneKey, solidClears } from "./addonSceneModel.mjs";
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
  // The mode, the solid regions and the hover are one value the model moves
  // (`sceneAfter`): every way out of a hunt is an event there.
  const [{ mode, solid, hover }, dispatch] = useReducer(sceneAfter, SCENE_REST);
  const modeRef = useRef<SceneMode>("work");
  modeRef.current = mode;
  const solidRef = useRef<ReadonlyArray<SceneRect>>([]);
  solidRef.current = solid;
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

  // A hunt ends: the keyboard goes back to the app, and the last point the
  // host saw is forgotten — the frame had every move since, so that point is
  // wherever the hunt began (the strip, usually), and judging it again would
  // hand the frame the pointer for a region the pointer left long ago.
  const giveBack = useCallback(() => {
    lastPoint.current = null;
    blurFrame();
  }, [blurFrame]);

  const handlers: AddonBridgeHandlers = useMemo(
    () => ({
      onResize: () => {},
      onClose: () => setAddonHidden(addon.id, true),
      onTitle: () => {},
      onOpenUrl,
      onMediaEmbed: () => {},
      onMediaBox: () => {},
      onMediaRemove: () => {},
      onReady: () => {
        // A page that says hello again started again: it is at work, whatever the last one was doing.
        if (modeRef.current === "hunt") giveBack();
        dispatch({ type: "hello" });
        setReady((n) => n + 1);
      },
      onStartMissed: () => toaster.error(sceneNotStartedWords(name)),
      onSceneMode: (asked) => {
        if (asked === "work" && modeRef.current === "hunt") giveBack();
        dispatch({ type: "ask", mode: asked });
      },
      onSceneSolid: (rects) => dispatch({ type: "solid", rects }),
      onSceneRelease: () => {
        blurFrame();
        dispatch({ type: "release" });
      },
    }),
    [addon.id, name, onOpenUrl, blurFrame, giveBack],
  );
  const { sendEvent } = useAddonBridge(frame, addon, { visible: true, summary, load, handlers, epoch: frameKey });
  sendRef.current = sendEvent;

  // Another frame is another page — the record moved, so it loads again:
  // nothing of the last one stands, a hunt least of all. A page that failed
  // to load would otherwise inherit a pane that takes every click.
  useEffect(() => {
    dispatch({ type: "frame" });
  }, [frameKey]);

  // Work mode: the frame is click-through, so the page under it sees every
  // move; the pointer over a solid region hands the frame the pointer. A move
  // the host still sees while the frame has it comes from a layer above the
  // scene or from the chrome, and the region test says which.
  useEffect(() => {
    if (mode !== "work" || shielded) {
      dispatch({ type: "hover", on: false });
      return;
    }
    const onMove = (e: PointerEvent) => {
      lastPoint.current = { x: e.clientX, y: e.clientY };
      dispatch({ type: "hover", on: pointInSolid(lastPoint.current, solidRef.current, vp.top) });
    };
    // `document.hasFocus()` stays true when focus only moved into the scene's own frame.
    const onBlur = () => dispatch({ type: "blur", focused: document.hasFocus() });
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
    dispatch({ type: "hover", on: pointInSolid(lastPoint.current, solid, vp.top) });
  }, [solid, mode, shielded, vp.top]);

  // Hunt mode: the frame has the pointer everywhere under the chrome — but
  // not over the footer (`footerKept`). The page's own strip is where a hunt
  // is left; the host draws nothing over the app for it. What the host keeps
  // is for a page that stopped answering mid-hunt: `Esc` while the app has
  // the keyboard, and the footer's Addons control, which a hunt never covers.
  const leave = useCallback(() => {
    giveBack();
    dispatch({ type: "leave" });
  }, [giveBack]);
  useEffect(() => {
    if (mode !== "hunt") return;
    const onKey = (e: KeyboardEvent) => {
      // A key a dialog took is the dialog's: one Esc closes it, the next leaves the hunt.
      if (e.key !== "Escape" || e.defaultPrevented) return;
      leave();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mode, leave]);

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
  // The footer is as tall as the chrome — one token (`h-chrome`) — so the
  // chrome's height is the band a hunt leaves at the bottom.
  const kept = footerKept(mode, vp.top);

  return createPortal(
    <div
      role="region"
      aria-label={name}
      data-scene-mode={mode}
      style={{ left: 0, right: 0, bottom: 0, top: vp.top, pointerEvents: taken ? "auto" : "none", clipPath: kept > 0 ? `inset(0 0 ${kept}px 0)` : undefined }}
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
        // `light`, said outright: a page that declares no scheme is a light canvas, and a frame
        // whose scheme differs from its page's is given an opaque ground. `normal` left that to
        // the engine — under a dark app root one of them reads it as dark, and the pane went white.
        style={{ colorScheme: "light" }}
      />
      {/* Who has the pointer, for a screen reader: always here, so the change is what is said. Nothing is drawn. */}
      <p role="status" className="sr-only">{huntStatus(name, mode)}</p>
      {/* The shield: while anything in the layer is dragged or resized, the frame takes nothing — even under a root that takes nothing itself. */}
      {shielded && <div aria-hidden className="pointer-events-auto absolute inset-0" />}
    </div>,
    document.body,
  );
}
