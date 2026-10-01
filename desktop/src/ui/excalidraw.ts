/**
 * The kit's third entry: the canvas, alone (19 — Drawings).
 *
 * `ui/index.ts` deliberately does **not** carry Excalidraw, for the reason
 * `ui/flow.ts` gives about the graph library: one re-export would pull the
 * largest chunk the app ships into every screen. The Draw overlay and the
 * drawing bridge import from here, and only they may (`ui/imports.test.mjs`).
 *
 * Everything the canvas needs at load is decided here once: its fonts are
 * this app's own copy (`desktop/public/excalidraw/fonts`, copied from the
 * package by `scripts/sync-excalidraw-assets.mjs` at `dev` and `build`), so
 * `window.EXCALIDRAW_ASSET_PATH` names the app's origin before the module is
 * asked for — the canvas asks no CDN for anything; its stylesheet rides in
 * with the module; and the module is loaded once, lazily, shared by every
 * caller (`ui/monaco.ts`'s `mod`/`loading` pattern).
 */

import { lazy } from "react";
import "./excalidraw.css";

type ExcalidrawModule = typeof import("@excalidraw/excalidraw");
type MermaidModule = typeof import("@excalidraw/mermaid-to-excalidraw");

export type { ExcalidrawImperativeAPI, AppState, BinaryFiles, LibraryItem, LibraryItems, ExcalidrawInitialDataState } from "@excalidraw/excalidraw/types";
export type { ExcalidrawElement, NonDeletedExcalidrawElement, OrderedExcalidrawElement } from "@excalidraw/excalidraw/element/types";
export type { ExcalidrawElementSkeleton } from "@excalidraw/excalidraw/data/transform";

declare global {
  interface Window {
    EXCALIDRAW_ASSET_PATH?: string | string[];
  }
}

/** Where the canvas's fonts and assets are served from: the app's own origin, never a CDN. */
const ASSET_PATH = "/excalidraw/";

/** Point the canvas at this app's copy of its assets. Idempotent; runs before the module is asked for. */
function pointAssetsHome(): void {
  if (typeof window === "undefined") return;
  if (window.EXCALIDRAW_ASSET_PATH === undefined) window.EXCALIDRAW_ASSET_PATH = `${window.location.origin}${ASSET_PATH}`;
}

let mod: ExcalidrawModule | null = null;
let loading: Promise<ExcalidrawModule> | null = null;

/** Load the canvas once; every caller shares the one import. Also imports its stylesheet. */
export function loadExcalidraw(): Promise<ExcalidrawModule> {
  if (mod) return Promise.resolve(mod);
  if (!loading) {
    pointAssetsHome();
    loading = Promise.all([import("@excalidraw/excalidraw"), import("@excalidraw/excalidraw/index.css")]).then(([m]) => {
      mod = m;
      return m;
    });
  }
  return loading;
}

let mermaid: MermaidModule | null = null;
let mermaidLoading: Promise<MermaidModule> | null = null;

/** Load the Mermaid-to-canvas converter once, on the first request that needs it. */
export function loadMermaidToExcalidraw(): Promise<MermaidModule> {
  if (mermaid) return Promise.resolve(mermaid);
  if (!mermaidLoading) {
    mermaidLoading = import("@excalidraw/mermaid-to-excalidraw").then((m) => {
      mermaid = m;
      return m;
    });
  }
  return mermaidLoading;
}

/** The canvas component, lazy: mounted inside a `Suspense`, its chunk arrives on first use. */
export const Excalidraw = lazy(() => loadExcalidraw().then((m) => ({ default: m.Excalidraw })));
