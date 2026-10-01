/**
 * One step on the canvas.
 *
 * A kit card, not xyflow's default box, and one size for every kind — the
 * family shows in the card's shape and its glyph, never in its size: an
 * **event** (`start`, `wait`, `emit`, `end`) is a pill whose glyph sits in a
 * ring; a **gateway** (`decide`, `if`, `switch`, `judge`, `parallel`) wears
 * its glyph in a diamond; a loop and a task are the plain card. The name,
 * the id in mono beneath with the kind — or, for a start, the event it
 * begins on — badges for what departs from the defaults (`join: any`, an
 * `on_fail` that is not `fail`, a `max_visits` that is not three), and a
 * count of problems. In run mode the card wears a ring in the step's state
 * role and its state chip, so the same canvas that designs a workflow shows
 * one running.
 *
 * Handles: `in` on top — none on a start, which begins a run; `out` at the
 * bottom — none on an end, which ends one — one per branch for a gateway or
 * a loop, spread across the edge, each with the id `branch:<name>`
 * (`stepKinds.mjs`'s `branchHandle`) so a branch may be named anything, the
 * branch named on the flow's label; `fail` on the left for an `on_fail:
 * then` flow; `loop-out` and `loop-in` on the right, where a flow back to an
 * earlier step leaves and arrives. The side handles are anchors, not drop
 * targets: a flow is drawn from the bottom, and a loop is a flow drawn to a
 * step that runs earlier.
 *
 * Boundary events are chips on the card's lower edge — ⏱ a timeout, ↻ a
 * reminder, ✉ a message, ⚡ a signal (`BOUNDARY_ON_ICON`). A divert's chip
 * carries its own `branch:<name>` handle, the one its path leaves from, and
 * the step's own `out` moves left to share the edge; an act beside the step
 * — ↳ a post, ⚡ an emit — is dashed and handle-less. On a run's canvas, a
 * chip whose boundary fired this visit lights up.
 */

import { memo } from "react";
import type { Boundary, Step, StepState } from "../../types";
import { BOUNDARY_ACT_ICON, BOUNDARY_ON_ICON, Chip, StepStateChip, cn, stepKindIcon } from "../../ui";
import { FlowHandle, type FlowNodeProps } from "../../ui/flow";
import { chipOf, ownOffsets } from "./forms/boundaryModel.mjs";
import { eventPhrase } from "./forms/startForm.mjs";
import { branchHandle, DEFAULT_MAX_VISITS, familyOf } from "./stepKinds.mjs";
import { t } from "../../i18n/l10n.mjs";

export interface StepNodeData extends Record<string, unknown> {
  step: Step;
  problems: number;
  /** In run mode: the record's state and the role its ring wears. */
  state?: StepState | null;
  tone?: string | null;
  label?: string | null;
  /** In run mode: the names of the boundary events that fired this visit, joined by commas — a chip lights for each. */
  fired?: string | null;
  /** In an amendment: whether this step may still change. */
  editable?: boolean;
}

/** The handle ids every step shares. */
export const HANDLE = Object.freeze({ in: "in", out: "out", fail: "fail", loopOut: "loop-out", loopIn: "loop-in" });

/** The glyph, framed as its family draws it: a ring for an event, a diamond for a gateway, bare for the rest. */
function KindGlyph({ step }: { step: Step }) {
  const Icon = stepKindIcon(step.kind);
  const family = familyOf(step.kind);
  if (family === "event") {
    return (
      <span className="inline-flex size-5 shrink-0 items-center justify-center rounded-full border border-border text-text-dim" aria-hidden>
        <Icon size={11} />
      </span>
    );
  }
  if (family === "gateway") {
    return (
      <span className="inline-flex size-4 shrink-0 rotate-45 items-center justify-center rounded-sm border border-border text-text-dim" aria-hidden>
        <Icon size={10} className="-rotate-45" />
      </span>
    );
  }
  return <Icon size={14} aria-hidden className="shrink-0 text-text-dim" />;
}

/** One boundary event, as a chip on the card's lower edge: a divert solid with its path's handle, an act dashed. */
function BoundaryChip({ boundary, lit }: { boundary: Boundary; lit: boolean }) {
  const chip = chipOf(boundary);
  const OnIcon = BOUNDARY_ON_ICON[chip.event as keyof typeof BOUNDARY_ON_ICON] ?? BOUNDARY_ON_ICON.after;
  const ActIcon = chip.act === "divert" ? null : BOUNDARY_ACT_ICON[chip.act as keyof typeof BOUNDARY_ACT_ICON];
  return (
    <span
      title={chip.diverts ? t("workflow-step-node-boundary-diverts", { name: chip.name, words: chip.words }) : t("workflow-step-node-boundary-acts", { name: chip.name, words: chip.words })}
      className={cn(
        "relative inline-flex h-5 max-w-[7.5rem] items-center gap-0.5 rounded-full border bg-surface px-1.5 text-3xs whitespace-nowrap",
        chip.diverts ? "border-warn/70 text-warn" : "border-dashed border-border text-text-dim",
        lit && "bg-warn-soft ring-2 ring-accent",
      )}
    >
      <OnIcon size={10} aria-hidden className="shrink-0" />
      <span className="min-w-0 truncate">{chip.words}</span>
      {ActIcon && <ActIcon size={10} aria-hidden className="shrink-0" />}
      {/* A divert's path leaves from under its chip: the handle is the chip's own. */}
      {chip.diverts && <FlowHandle kind="source" id={branchHandle(chip.name)} side="bottom" />}
    </span>
  );
}

function StepNodeInner({ data, selected }: FlowNodeProps<StepNodeData>) {
  const { step, problems, state, tone, label, fired, editable } = data;
  const family = familyOf(step.kind);
  const own = ownOffsets(step);
  const ring = tone ? { boxShadow: `0 0 0 2px var(--color-${tone})` } : undefined;
  const frozen = editable === false;
  const failsOn = step.on_fail && step.on_fail.on_fail !== "fail";
  const boundaries = step.boundaries ?? [];
  const lit = new Set((fired ?? "").split(",").filter(Boolean));
  const begins = step.kind === "start";
  const ends = step.kind === "end";
  // A start says the event it begins on where another step says its kind.
  const second = step.kind === "start" ? eventPhrase(step.on) : step.kind;
  return (
    <div
      className={cn(
        "relative w-[220px] border border-border bg-surface px-3 py-2 text-left",
        family === "event" ? "rounded-[2.25rem] px-4" : "rounded-card",
        frozen && "opacity-70",
        selected && "border-accent",
      )}
      style={ring}
    >
      {/* A start begins a run: nothing flows into it. */}
      {!begins && <FlowHandle kind="target" id={HANDLE.in} side="top" />}
      {/* Where a loop arrives. Not a target to drop a flow on: a loop is a
          flow drawn to the top handle of a step that runs earlier. */}
      {!begins && <FlowHandle kind="target" id={HANDLE.loopIn} side="right" offset={35} connectable={false} />}
      <div className="flex items-center gap-2">
        <KindGlyph step={step} />
        <span className="min-w-0 flex-1 truncate text-xs font-medium">{step.name || step.id}</span>
        {problems > 0 && (
          <Chip tone="danger" title={t("workflow-step-node-problem-problems", { problems })}>
            {problems}
          </Chip>
        )}
      </div>
      <div className="mt-0.5 flex items-center gap-1.5">
        <code className="truncate font-mono text-2xs text-text-dim">{step.id}</code>
        <span className="text-2xs text-text-dim">·</span>
        <span className="truncate text-2xs text-text-dim">{second}</span>
      </div>
      {((step.join ?? "all") !== "all" || failsOn || (step.max_visits ?? DEFAULT_MAX_VISITS) !== DEFAULT_MAX_VISITS || (step.retries ?? 0) > 0) && (
        <div className="mt-1 flex flex-wrap gap-1">
          {(step.join ?? "all") !== "all" && <Chip tone="quiet">{t("workflow-step-node-join", { step: step.join })}</Chip>}
          {failsOn && (
            <Chip tone="quiet">
              {step.on_fail?.on_fail === "then" ? t("workflow-step-node-fail", { step: step.on_fail.step }) : t("workflow-step-node-fail-skip")}
            </Chip>
          )}
          {(step.retries ?? 0) > 0 && <Chip tone="quiet">{t("workflow-step-node-retries", { retries: step.retries })}</Chip>}
          {(step.max_visits ?? DEFAULT_MAX_VISITS) !== DEFAULT_MAX_VISITS && (
            <Chip tone="quiet">{t("workflow-step-node-visits", { max_visits: step.max_visits })}</Chip>
          )}
        </div>
      )}
      {state && (
        <div className="mt-1.5 flex items-center gap-1.5">
          <StepStateChip state={state} />
          {label && <span className="truncate text-2xs text-text-dim">{label}</span>}
        </div>
      )}
      {own.branches.size > 0
        ? [...own.branches.entries()].map(([b, at]) => <FlowHandle key={b} kind="source" id={branchHandle(b)} side="bottom" offset={at} />)
        : !ends && <FlowHandle kind="source" id={HANDLE.out} side="bottom" offset={own.out === 50 ? undefined : (own.out ?? undefined)} />}
      {/* The side anchors a loop and an on-fail route are drawn from. Not
          handles to drag a flow out of: a flow leaves the bottom, and an
          on-fail target is set in the inspector. */}
      {!ends && <FlowHandle kind="source" id={HANDLE.loopOut} side="right" offset={65} connectable={false} />}
      {!ends && <FlowHandle kind="source" id={HANDLE.fail} side="left" offset={65} connectable={false} />}
      {/* Right-anchored on the lower edge, in declaration order: the step's own flow keeps the left. */}
      {boundaries.length > 0 && (
        <div className="absolute -bottom-2.5 right-2 flex gap-1" aria-label={t("workflow-step-node-boundary-events")}>
          {boundaries.map((b) => (
            <BoundaryChip key={b.name} boundary={b} lit={lit.has(b.name)} />
          ))}
        </div>
      )}
    </div>
  );
}

export const StepNode = memo(StepNodeInner);
export const NODE_TYPES = { step: StepNode };
