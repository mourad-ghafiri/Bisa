/**
 * A canvas nobody sees (19 — Drawings): mounted by `DrawPanel` after the
 * first request that needs it, standing off the viewport in a laid-out box
 * — text is measured against a real layout, and `display: none` would give
 * it none — in view mode with every piece of chrome off. The bridge loads a
 * drawing into it, draws, reads the scene back and saves.
 */

import { useEffect } from "react";
import { offscreenGone, offscreenMounted } from "./offscreen";

type ExcalidrawModule = typeof import("@excalidraw/excalidraw");

const UI_OPTIONS = {
  canvasActions: {
    loadScene: false,
    saveToActiveFile: false,
    export: false,
    saveAsImage: false,
    toggleTheme: false,
    clearCanvas: false,
    changeViewBackgroundColor: false,
  },
  tools: { image: false },
} as const;

export function OffscreenScene({ mod }: { mod: ExcalidrawModule }) {
  useEffect(() => () => offscreenGone(), []);
  const { Excalidraw } = mod;
  return (
    <div aria-hidden style={{ position: "fixed", left: -10000, top: 0, width: 1600, height: 1000, visibility: "hidden", pointerEvents: "none" }} data-draw-offscreen>
      <Excalidraw excalidrawAPI={offscreenMounted} viewModeEnabled theme="light" UIOptions={UI_OPTIONS} initialData={{ elements: [], appState: {} }} />
    </div>
  );
}
