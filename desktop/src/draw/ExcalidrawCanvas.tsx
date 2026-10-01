/**
 * The canvas itself (19 — Drawings): Excalidraw mounted on a drawing, with
 * the platform's shape libraries in its sidebar and the little of its own
 * chrome this app offers. Everything that reaches outside is off — *Open*,
 * *Save to*, the export dialog, live collaboration, the public library site
 * — and the image tool is off because a drawing is vector only.
 *
 * The theme follows the app's (`data-scheme`, re-read on a change) and the
 * language the app's locale. The parent owns the scene: it hands the
 * elements and the two facts of the app state in, hears every change, and
 * gets the imperative API to drive the canvas from outside.
 */

import { useMemo } from "react";
import { locale } from "../i18n/l10n.mjs";
import { t } from "../i18n/l10n.mjs";
import { useThemeNonce } from "../ui";
import type { AppState, BinaryFiles, ExcalidrawElement, ExcalidrawImperativeAPI, LibraryItems, OrderedExcalidrawElement } from "../ui/excalidraw";
import { canvasAppState } from "./drawModel.mjs";
import { buildLibrary } from "./library/index.mjs";
import type { DrawingDetail } from "../types";

type ExcalidrawModule = typeof import("@excalidraw/excalidraw");

/** Everything the canvas may not offer: nothing leaves the machine from here, and nothing is an image. */
const UI_OPTIONS = {
  canvasActions: {
    loadScene: false,
    saveToActiveFile: false,
    export: false,
    saveAsImage: true,
    toggleTheme: false,
    clearCanvas: true,
    changeViewBackgroundColor: true,
  },
  tools: { image: false },
} as const;

function scheme(): "light" | "dark" {
  return document.documentElement.getAttribute("data-scheme") === "dark" ? "dark" : "light";
}

export function ExcalidrawCanvas({
  mod,
  detail,
  onApi,
  onChange,
}: {
  /** The loaded module, awaited by the parent so the scene can be restored before the first paint. */
  mod: ExcalidrawModule;
  detail: DrawingDetail;
  onApi: (api: ExcalidrawImperativeAPI) => void;
  onChange: (elements: readonly OrderedExcalidrawElement[], appState: AppState, files: BinaryFiles) => void;
}) {
  const nonce = useThemeNonce();
  const theme = useMemo(scheme, [nonce]);
  // Built once per mount: the shape libraries are data, and the converter is the module's.
  const libraryItems = useMemo<LibraryItems>(
    () => buildLibrary((skeleton) => mod.convertToExcalidrawElements(skeleton as Parameters<typeof mod.convertToExcalidrawElements>[0], { regenerateIds: true })),
    [mod],
  );
  const initialData = useMemo(
    () => ({
      elements: mod.restoreElements(detail.scene.elements as ExcalidrawElement[], null, { repairBindings: true }),
      appState: canvasAppState(detail.scene.app_state),
      libraryItems,
      scrollToContent: true,
    }),
    // The initial scene is the mount's: a later change comes through `updateScene`, never a remount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [detail.id, libraryItems, mod],
  );
  const { Excalidraw, MainMenu } = mod;
  return (
    <div className="h-full min-h-0 w-full" data-draw-canvas>
      <Excalidraw
        excalidrawAPI={onApi}
        initialData={initialData}
        onChange={onChange}
        theme={theme}
        langCode={locale()}
        UIOptions={UI_OPTIONS}
        name={detail.title}
      >
        <MainMenu>
          <MainMenu.DefaultItems.SaveAsImage />
          <MainMenu.DefaultItems.ClearCanvas />
          <MainMenu.Separator />
          <MainMenu.DefaultItems.ChangeCanvasBackground />
          <MainMenu.Separator />
          <MainMenu.ItemCustom>
            <span className="px-2 text-xs text-text-dim">{t("draw-canvas-menu-note")}</span>
          </MainMenu.ItemCustom>
        </MainMenu>
      </Excalidraw>
    </div>
  );
}
