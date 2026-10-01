/**
 * The always-mounted Draw component (19 — Drawings), beside the browser
 * layer in `App.tsx`: it hears the agents' requests (`drawing_request`
 * frames, and the parked list at start), hands them to `drawBridge.ts`,
 * reads the parked list every `DRAW_PRESENCE_MS` so the engine knows a
 * desktop is home, feeds the canvas's settings to `drawPrefsStore.ts`, and
 * mounts the offscreen canvas once a request needs one. Off when the
 * machine's switch says so.
 */

import { useEffect, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { errorFields, log } from "../log";
import { presenceLapsed } from "../shell/browserBridgeModel.mjs";
import { useResolvedSettings } from "../shell/useResolvedSettings";
import { loadExcalidraw } from "../ui/excalidraw";
import { performDrawRequest } from "./drawBridge";
import { drawPrefsOf, setDrawPrefs, useDrawPrefs } from "./drawPrefsStore";
import { DRAW_PRESENCE_MS } from "./drawRequestModel.mjs";
import { useOffscreenWanted } from "./offscreen";
import { OffscreenScene } from "./OffscreenScene";

type ExcalidrawModule = typeof import("@excalidraw/excalidraw");

export function DrawPanel() {
  const { resolved } = useResolvedSettings(null);
  useEffect(() => setDrawPrefs(drawPrefsOf(resolved)), [resolved]);
  const { enabled } = useDrawPrefs();

  // The parked list at start, then each frame as it comes.
  useEffect(() => {
    if (!enabled) return;
    void api
      .drawingRequests()
      .then((r) => {
        for (const pending of r.requests) void performDrawRequest(pending);
      })
      .catch((e: unknown) => log.debug("draw", "the pending drawing requests could not be read", errorFields(e)));
  }, [enabled]);
  useEngineEvents((e) => {
    if (e.payload.type === "drawing_request" && enabled) void performDrawRequest({ id: e.payload.id, request: e.payload.request, scope: e.payload.scope, asked_at: 0 });
  });

  // A desktop that is open says so: the engine refuses a request at once as
  // not available when none has read the list within its presence window.
  useEffect(() => {
    if (!enabled) return;
    let failures = 0;
    const read = () =>
      void api
        .drawingRequests()
        .then((r) => {
          failures = 0;
          for (const pending of r.requests) void performDrawRequest(pending);
        })
        .catch((e: unknown) => {
          failures += 1;
          const fields = { failures, ...errorFields(e) };
          if (presenceLapsed(failures)) log.warn("draw", "the presence read keeps failing: agents are about to be told nobody is home", fields);
          else log.debug("draw", "the pending drawing requests could not be read", fields);
        });
    const timer = window.setInterval(read, DRAW_PRESENCE_MS);
    return () => window.clearInterval(timer);
  }, [enabled]);

  // The offscreen canvas, mounted once the first request needs it.
  const wanted = useOffscreenWanted();
  const [mod, setMod] = useState<ExcalidrawModule | null>(null);
  useEffect(() => {
    if (!wanted || mod) return;
    void loadExcalidraw().then(setMod).catch((e: unknown) => log.warn("draw", "the canvas could not be loaded for an agent's request", errorFields(e)));
  }, [wanted, mod]);
  if (!wanted || !mod) return null;
  return <OffscreenScene mod={mod} />;
}
