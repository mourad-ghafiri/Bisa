/**
 * A template's picture in a tile (19 — Drawings): the shapes
 * `templatePreview` reads off the skeleton, painted as one inline SVG — the
 * fills the template chose, every stroke in the text colour, so the preview
 * reads on every theme. Decorative: the tile's name says what it is.
 */

import { useMemo } from "react";
import { templatePreview } from "./templates/preview.mjs";

export function TemplatePreview({ skeleton }: { skeleton: () => Record<string, unknown>[] }) {
  const { viewBox, shapes } = useMemo(() => templatePreview(skeleton()), [skeleton]);
  if (shapes.length === 0) return <span className="block h-full w-full" aria-hidden />;
  const stroke = Math.max(1, viewBox.w / 400);
  return (
    <svg viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`} preserveAspectRatio="xMidYMid meet" aria-hidden className="block h-full w-full text-text" style={{ strokeWidth: stroke }}>
      <defs>
        <marker id="tpl-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="currentColor" />
        </marker>
      </defs>
      {shapes.map((s, i) => {
        switch (s.kind) {
          case "frame":
            return <rect key={i} x={s.x} y={s.y} width={s.w} height={s.h} rx={8} fill="none" stroke="currentColor" strokeDasharray={`${stroke * 6} ${stroke * 6}`} opacity={0.5} />;
          case "rect":
            return <rect key={i} x={s.x} y={s.y} width={s.w} height={s.h} rx={6} fill={s.fill ?? "none"} stroke="currentColor" strokeDasharray={s.dashed ? `${stroke * 6} ${stroke * 6}` : undefined} />;
          case "ellipse":
            return <ellipse key={i} cx={s.x + s.w / 2} cy={s.y + s.h / 2} rx={s.w / 2} ry={s.h / 2} fill={s.fill ?? "none"} stroke="currentColor" />;
          case "diamond": {
            const cx = s.x + s.w / 2;
            const cy = s.y + s.h / 2;
            return <polygon key={i} points={`${cx},${s.y} ${s.x + s.w},${cy} ${cx},${s.y + s.h} ${s.x},${cy}`} fill={s.fill ?? "none"} stroke="currentColor" />;
          }
          case "line":
            return <line key={i} x1={s.x1} y1={s.y1} x2={s.x2} y2={s.y2} stroke="currentColor" markerEnd={s.arrow ? "url(#tpl-arrow)" : undefined} />;
          case "text":
            return (
              <text key={i} x={s.x} y={s.y} fontSize={s.size} textAnchor={s.anchor} dominantBaseline={s.anchor === "middle" ? "middle" : undefined} fill="currentColor" stroke="none" fontFamily="var(--font-sans)">
                {s.text}
              </text>
            );
          default:
            return null;
        }
      })}
    </svg>
  );
}
