/**
 * A workflow's picture in a card (03-workflows §The library): the shapes
 * `thumbnail` reads off the definition, painted as one inline SVG — a card
 * per step in the kit's surface with the kind's glyph and the step's name,
 * an event's card a pill as on the canvas, every flow a line with an
 * arrowhead — a `then` flow in the dim text colour, a divert's path dashed
 * in the warning role, an on-fail route dashed in the danger role, a loop
 * dotted — the strokes the designer's canvas gives the same edges
 * (`theme/flow.css`), so a thumbnail reads as the canvas would on every
 * theme. Decorative: the card's name says what it is. No canvas is loaded
 * here; the graph library stays in the designer's chunk.
 */

import { useMemo } from "react";
import type { Definition } from "./workflowGraph.mjs";
import { ICON, STEP_KIND_ICON } from "../../ui";
import { thumbnail, type ThumbNode } from "./thumbnailModel.mjs";

/** The glyph's box inside a card, and the name's baseline, in canvas pixels. */
const GLYPH = 16;
const INSET = 12;
const NAME_SIZE = 15;

/** A card's corner: an event is a pill, as on the canvas; every other family a card. */
function cornerOf(n: ThumbNode): number {
  return n.family === "event" ? n.h / 2 : 8;
}

export function WorkflowThumbnail({ definition }: { definition: Definition }) {
  const { viewBox, nodes, edges } = useMemo(() => thumbnail(definition), [definition]);
  if (nodes.length === 0) {
    return (
      <span className="flex h-full w-full items-center justify-center text-text-dim/40" aria-hidden>
        <ICON.workflow size={28} aria-hidden />
      </span>
    );
  }
  // One stroke and one arrowhead whatever the picture's size: scaled to the
  // viewBox so a tall workflow's lines do not vanish and a short one's do not
  // thicken.
  const stroke = Math.max(1.5, viewBox.w / 260);
  const marker = `wf-arrow-${nodes[0]?.id ?? "x"}-${nodes.length}-${Math.round(viewBox.w)}`;
  return (
    <svg viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`} preserveAspectRatio="xMidYMid meet" aria-hidden className="block h-full w-full" style={{ strokeWidth: stroke }}>
      <defs>
        <marker id={marker} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="context-stroke" stroke="none" />
        </marker>
        {nodes.map((n) => (
          <clipPath key={n.id} id={`${marker}-clip-${n.id}`}>
            <rect x={n.x} y={n.y} width={n.w} height={n.h} rx={cornerOf(n)} />
          </clipPath>
        ))}
      </defs>
      {edges.map((e) => (
        <polyline
          key={e.id}
          points={e.points.map((p) => `${p.x},${p.y}`).join(" ")}
          fill="none"
          stroke={e.kind === "on_fail" ? "var(--color-danger)" : e.kind === "boundary" ? "var(--color-warn)" : "var(--color-text-dim)"}
          strokeDasharray={e.kind === "on_fail" || e.kind === "boundary" ? `${stroke * 4} ${stroke * 3}` : e.kind === "loop" ? `${stroke * 1.5} ${stroke * 4}` : undefined}
          strokeLinejoin="round"
          markerEnd={`url(#${marker})`}
        />
      ))}
      {nodes.map((n) => {
        const Icon = STEP_KIND_ICON[n.kind as keyof typeof STEP_KIND_ICON] ?? ICON.workflow;
        return (
          <g key={n.id} clipPath={`url(#${marker}-clip-${n.id})`}>
            <rect x={n.x} y={n.y} width={n.w} height={n.h} rx={cornerOf(n)} fill="var(--color-surface)" stroke="var(--color-border)" />
            <Icon x={n.x + INSET} y={n.y + (n.h - GLYPH) / 2} width={GLYPH} height={GLYPH} color="var(--color-text-dim)" strokeWidth={1.75} aria-hidden />
            <text x={n.x + INSET + GLYPH + 8} y={n.y + n.h / 2} dominantBaseline="middle" fontSize={NAME_SIZE} fontFamily="var(--font-sans)" fill="var(--color-text)" stroke="none">
              {n.name}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
