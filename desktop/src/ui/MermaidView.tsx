/**
 * The one Mermaid viewer (ide/11). A `.mmd` document, a fenced
 * block in Markdown and a fenced block in a message all render through it,
 * so an agent's diagram looks like yours.
 *
 * `mermaid` is imported lazily on the first diagram — its chunk is not in the
 * startup path. Rendering is debounced; the last good SVG stays on screen
 * while a new source parses, so a half-typed edge does not blank the
 * preview. An error names the line in *your* file: the caller passes the
 * offset of the snippet, and `offsetError` adds it before anything is shown.
 *
 * A diagram wears the theme: the roles are resolved off `<html>`
 * into Mermaid's `themeVariables`, and a switch of palette, accent or font
 * re-renders the last source — the same attribute observer the terminal uses.
 */

import { useEffect, useRef, useState } from "react";
import { exportFile } from "../api";
import { Button } from "./Button";
import { copyText } from "./clipboard";
import { cn } from "./cn";
import { resolvedRoles } from "./cssColor";
import { ICON } from "./icons";
import { DIAGRAM_ROLES, exportScale, mermaidTheme, offsetError, themeVariablesFor } from "./mermaidModel.mjs";
import { useToast } from "./Toast";
import { useThemeNonce } from "./useTokenPx";
import { t as tr } from "../i18n/l10n.mjs";

type MermaidModule = typeof import("mermaid").default;
let loading: Promise<MermaidModule> | null = null;
let initialisedFor: string | null = null;

/** `initialize` is global to the module, so it is keyed on everything it was told. */
async function load(theme: string, themeVariables: Record<string, string | boolean>): Promise<MermaidModule> {
  loading ??= import("mermaid").then((m) => m.default);
  const mermaid = await loading;
  const key = JSON.stringify([theme, themeVariables]);
  if (initialisedFor !== key) {
    mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: theme as never, themeVariables });
    initialisedFor = key;
  }
  return mermaid;
}

let counter = 0;

export function MermaidView({
  source,
  startLine = 1,
  onGoToLine,
  themeSetting,
  exportScale: exportScaleSetting,
  exportName = "diagram",
  className,
}: {
  source: string;
  /** The file line the source's first line sits on, for error messages. */
  startLine?: number;
  /** Clicking an error moves the editor there, when a caller can. */
  onGoToLine?: (line: number) => void;
  /** `diagrams.theme`: `follow_app` or one of Mermaid's own. */
  themeSetting?: unknown;
  /** `diagrams.export.scale`: the PNG rasterisation factor, 1–4. */
  exportScale?: unknown;
  exportName?: string;
  className?: string;
}) {
  const toast = useToast();
  const [svg, setSvg] = useState<string>("");
  const [error, setError] = useState<{ line: number | null; message: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const host = useRef<HTMLDivElement>(null);
  const id = useRef(`mermaid-${++counter}`);
  const themeNonce = useThemeNonce();

  useEffect(() => {
    const root = document.documentElement;
    const scheme = root.getAttribute("data-scheme") === "dark" ? "dark" : "light";
    const theme = mermaidTheme(themeSetting);
    const themeVariables = themeVariablesFor(resolvedRoles(DIAGRAM_ROLES), scheme, getComputedStyle(root).getPropertyValue("--font-sans").trim());
    let cancelled = false;
    setBusy(true);
    const t = window.setTimeout(async () => {
      try {
        const mermaid = await load(theme, themeVariables);
        if (cancelled) return;
        const { svg: rendered } = await mermaid.render(`${id.current}-${Date.now()}`, source.trim() || "graph LR\n  empty[\" \"]");
        if (cancelled) return;
        setSvg(rendered);
        setError(null);
      } catch (e) {
        if (cancelled) return;
        const raw = e instanceof Error ? e.message : String(e);
        setError(offsetError(raw, startLine));
      } finally {
        if (!cancelled) setBusy(false);
      }
    }, 250);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [source, startLine, themeSetting, themeNonce]);

  const svgBytes = () => new TextEncoder().encode(svg);

  const copySvg = async () => {
    if (await copyText(svg)) toast.ok(tr("ui-mermaid-view-svg-copied"));
    else toast.error(tr("ui-mermaid-view-clipboard-refused"));
  };
  const exportSvg = async () => {
    const path = await exportFile(`${exportName}.svg`, "image/svg+xml", svgBytes());
    if (path) toast.ok(tr("ui-mermaid-view-saved", { path }));
  };
  const exportPng = async () => {
    const scale = exportScale(exportScaleSetting);
    const img = new Image();
    const blob = new Blob([svg], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    try {
      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = () => reject(new Error("the SVG did not rasterise"));
        img.src = url;
      });
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, img.naturalWidth * scale);
      canvas.height = Math.max(1, img.naturalHeight * scale);
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("no canvas");
      ctx.scale(scale, scale);
      ctx.drawImage(img, 0, 0);
      const png = await new Promise<Blob | null>((r) => canvas.toBlob(r, "image/png"));
      if (!png) throw new Error("no PNG");
      const path = await exportFile(`${exportName}.png`, "image/png", new Uint8Array(await png.arrayBuffer()));
      if (path) toast.ok(tr("ui-mermaid-view-saved", { path }));
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      URL.revokeObjectURL(url);
    }
  };

  return (
    <div className={cn("flex min-h-0 flex-col", className)}>
      <div className="flex shrink-0 items-center gap-1 px-1 py-0.5 text-2xs text-text-dim">
        <span>{busy ? tr("ui-mermaid-view-rendering") : error ? tr("ui-mermaid-view-last-good-render") : tr("ui-mermaid-view-diagram")}</span>
        <span className="flex-1" />
        <Button size="sm" variant="ghost" disabled={!svg} onClick={() => void copySvg()}>
          <ICON.copy size={11} aria-hidden />{tr("ui-mermaid-view-copy-svg")}</Button>
        <Button size="sm" variant="ghost" disabled={!svg} onClick={() => void exportSvg()}>{tr("ui-mermaid-view-svg")}</Button>
        <Button size="sm" variant="ghost" disabled={!svg} onClick={() => void exportPng()}>{tr("ui-mermaid-view-png")}</Button>
      </div>
      {error && (
        <button
          type="button"
          onClick={() => error.line && onGoToLine?.(error.line)}
          className={cn(
            "mx-1 mb-1 rounded-control border border-danger/40 bg-danger-soft px-2 py-1 text-left font-mono text-2xs text-danger",
            error.line && onGoToLine ? "cursor-pointer hover:bg-danger-soft/70" : "cursor-default",
          )}
        >
          {error.message}
        </button>
      )}
      <div
        ref={host}
        className={cn("min-h-0 flex-1 overflow-auto p-2 [&_svg]:max-w-full", error && "opacity-60")}
        // Mermaid's output under `securityLevel: strict` carries no scripts or handlers.
        dangerouslySetInnerHTML={{ __html: svg }}
      />
    </div>
  );
}
