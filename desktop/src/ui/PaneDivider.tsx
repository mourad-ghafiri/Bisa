/**
 * A draggable line between two panes, turning pixels back into a ratio
 * (ide/06 §Splits). The terminal layer and the document panes both draw pane
 * trees; this is the one divider between them. The geometry is the pane
 * model's (`shell/paneTreeModel.mjs` `dividers`), passed in — the kit never
 * reads the model.
 */

import type { CSSProperties, PointerEvent, RefObject } from "react";
import { t } from "../i18n/l10n.mjs";

/** One divider's place in its body, in fractions of the body's box. */
export interface DividerGeometry {
  splitId: string;
  dir: "row" | "col";
  /** Where the split sits along its axis. */
  at: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export function PaneDivider({
  d,
  body,
  onRatio,
}: {
  d: DividerGeometry;
  /** The element the fractions are of. */
  body: RefObject<HTMLDivElement | null>;
  onRatio: (splitId: string, ratio: number) => void;
}) {
  const onPointerDown = (e: PointerEvent) => {
    const box = body.current?.getBoundingClientRect();
    if (!box) return;
    e.preventDefault();
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    const move = (ev: globalThis.PointerEvent) => {
      const ratio = d.dir === "row" ? ((ev.clientX - box.left) / box.width - d.x) / d.w : ((ev.clientY - box.top) / box.height - d.y) / d.h;
      onRatio(d.splitId, ratio);
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      target.removeEventListener("pointercancel", up);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
    target.addEventListener("pointercancel", up);
  };
  const style: CSSProperties =
    d.dir === "row"
      ? { left: `calc(${d.at * 100}% - 3px)`, top: `${d.y * 100}%`, width: 6, height: `${d.h * 100}%`, cursor: "col-resize" }
      : { top: `calc(${d.at * 100}% - 3px)`, left: `${d.x * 100}%`, height: 6, width: `${d.w * 100}%`, cursor: "row-resize" };
  return <div role="separator" aria-orientation={d.dir === "row" ? "vertical" : "horizontal"} aria-label={t("ui-pane-divider-resize-panes")} style={style} className="absolute z-20 hover:bg-accent/40" onPointerDown={onPointerDown} />;
}
