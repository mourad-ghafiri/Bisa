/**
 * Where a goal is, in one pill: the current step's kind glyph and name, and
 * the holder word — or, with no live step, the holder alone. The one
 * replacement for the retired status chip on every row that names a goal.
 */
import { STEP_KIND_ICON, Tooltip } from "../../ui";
import type { GoalRow, StepKind } from "../../types";
import { HOLDER_LABEL, KIND_LABEL, currentStepOf } from "./goalStripModel.mjs";
import { HolderBadge } from "./HolderBadge";

export function CurrentStepPill({
  row,
  withHolder = true,
}: {
  row: Pick<GoalRow, "holder" | "strip">;
  /** Off when the row already wears the holder badge elsewhere. */
  withHolder?: boolean;
}) {
  const step = currentStepOf(row.strip);
  if (!step) return withHolder ? <HolderBadge holder={row.holder} strip={row.strip} /> : null;
  const Icon = STEP_KIND_ICON[step.kind as StepKind["kind"]];
  return (
    <Tooltip label={`${KIND_LABEL[step.kind] ?? step.kind} · ${HOLDER_LABEL[row.holder] ?? row.holder}`}>
      <span className="inline-flex min-w-0 items-center gap-1 text-2xs text-text-dim">
        {Icon && <Icon size={11} aria-hidden className="shrink-0" />}
        <span className="max-w-40 truncate">{step.name}</span>
        {step.more > 0 && <span className="tnum">+{step.more}</span>}
        {withHolder && <HolderBadge holder={row.holder} strip={row.strip} />}
      </span>
    </Tooltip>
  );
}
