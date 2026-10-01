/**
 * One step of the run, read downward: a line that says what it is, where it
 * stands, who holds it and when it moved; open, the body — what it produced,
 * what went wrong, the work item it became, the project it made — and the
 * verbs the step admits right now, the same `StepActions` the canvas offers,
 * addressed to the run by its id. A step of a definition not started yet
 * (`run` null) has no body to act on.
 */
import type { Step, StepRecord } from "../../types";
import { Button, ICON, LinkedText, RelativeTime, STEP_KIND_ICON, StepStateChip, cn } from "../../ui";
import type { StepKind } from "../../types";
import { answerSummary } from "../../askModel.mjs";
import { HOLDER_LABEL } from "../_goals/goalStripModel.mjs";
import { StepActions } from "../_workflow/StepActions";
import type { ProgressRow } from "./progressModel.mjs";
import { navigate } from "../../router";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { projectsMadeByStep } from "../_work/projectOriginModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function StepRow({
  run,
  row,
  step,
  record,
  expanded,
  onToggle,
  onChanged,
  onOpenItem,
  onDecide,
}: {
  /** The run the step belongs to — a goal's or the workspace's; `null` for a ghosted step. */
  run: string | null;
  row: ProgressRow;
  step: Step | null;
  record: StepRecord | null;
  expanded: boolean;
  onToggle: () => void;
  onChanged: () => void;
  onOpenItem: (item: string) => void;
  onDecide: () => void;
}) {
  const Icon = STEP_KIND_ICON[row.kind as StepKind["kind"]];
  const said = answerSummary(row.answer);
  // The project this step of this run made, when it made one — the goal's
  // own design's or a library workflow's; either way, the IDE opens on it.
  const ws = useWorkspace();
  const made = run ? projectsMadeByStep(ws.projects, run, row.id) : [];
  const hasBody = row.error || row.output !== null || row.workItem || said || row.actions.length > 0 || made.length > 0;
  return (
    <li className={cn("rounded-card border border-transparent", row.current && "border-l-2 border-l-accent bg-surface-2/60", expanded && "border-border")}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        className="grid min-h-row w-full grid-cols-[auto_1fr_auto_auto_auto] items-center gap-3 px-3 py-1.5 text-left hover:bg-surface-2"
      >
        {Icon ? <Icon size={14} aria-hidden className="shrink-0 text-text-dim" /> : <span className="size-3.5" />}
        <span className="min-w-0 truncate text-sm">{row.name}</span>
        <StepStateChip state={row.state} />
        <span className="w-14 text-2xs text-text-dim">{row.holder ? HOLDER_LABEL[row.holder] : ""}</span>
        <span className="tnum w-24 text-right text-2xs text-text-dim">
          {row.startedAt !== null ? (
            <>
              <RelativeTime at={row.startedAt} />
              {row.duration !== null && ` · ${row.duration}`}
            </>
          ) : null}
        </span>
      </button>
      {expanded && hasBody && (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3 text-2xs">
          {row.error && <p className="text-danger">{row.error}</p>}
          {said && <p className="text-text-dim">{t("goal-step-row-answered", { said })}</p>}
          {row.output !== null && row.output !== undefined && (
            <LinkedText as="pre" className="max-h-64 overflow-auto rounded-control bg-surface-2 p-2 font-mono text-3xs" text={JSON.stringify(row.output, null, 2)} />
          )}
          {row.workItem && (
            <Button size="sm" variant="ghost" className="self-start" onClick={() => onOpenItem(row.workItem!)}>
              <ICON.workItem size={12} aria-hidden />{t("goal-step-row-open-work-item")}</Button>
          )}
          {made.map((p) => (
            <Button key={p.project.id} size="sm" variant="ghost" className="self-start" onClick={() => navigate({ name: "workbench", scope: "workstream", id: p.project.id })}>
              <ICON.project size={12} aria-hidden />{t("goal-step-row-open-project-step-made", { project: p.project.name })}</Button>
          ))}
          {run && step && row.actions.length > 0 && (
            <StepActions run={run} step={step} record={record} onChanged={onChanged} onOpenItem={onOpenItem} onDecide={onDecide} />
          )}
        </div>
      )}
    </li>
  );
}
