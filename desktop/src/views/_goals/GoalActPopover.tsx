/**
 * Act on a goal without leaving the list. Opened from a strip's *your move*
 * marker; loads the full `GoalView` on demand (one call, only when asked)
 * and mounts the same `StepActions` the Workflow tab uses for the live
 * human/approval/release steps. A draft or a gate that needs the designer —
 * an adoption, a publish — is a link to the goal instead: the popover acts
 * where acting is one control, and navigates where it is a decision surface.
 */
import { useEffect, useState } from "react";
import { api } from "../../api";
import { navigate } from "../../router";
import { Button, ErrorNote, Popover, SkeletonRows } from "../../ui";
import type { GoalView } from "../../types";
import { liveSteps } from "../_goal/progressModel.mjs";
import { StepActions } from "../_workflow/StepActions";
import { t } from "../../i18n/l10n.mjs";

export function GoalActPopover({
  goal,
  open,
  onClose,
  onChanged,
  children,
}: {
  goal: string;
  open: boolean;
  onClose: () => void;
  onChanged: () => void;
  children: React.ReactNode;
}) {
  const [view, setView] = useState<GoalView | null>(null);
  // A read that fails says so in the popover: an empty box would read as
  // nothing to do here, which is not what happened.
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    const ctl = new AbortController();
    setError(null);
    void api
      .goal(goal, ctl.signal)
      .then(setView)
      .catch((e: unknown) => {
        if (ctl.signal.aborted) return;
        setView(null);
        setError(e instanceof Error ? e.message : String(e));
      });
    return () => ctl.abort();
  }, [open, goal]);

  const run = view?.run ?? null;
  const live = liveSteps(run);
  const gate = view?.pending_gates?.[0];

  return (
    <Popover
      open={open}
      onOpenChange={(o) => !o && onClose()}
      trigger={children}
      asChild
      className="w-80 p-2"
    >
      {error ? (
        <ErrorNote error={error} />
      ) : !view ? (
        <SkeletonRows rows={3} />
      ) : (
        <div className="flex flex-col gap-2">
          {run &&
            live.map((s) => (
              <StepActions
                key={s.id}
                run={run.id}
                step={s}
                record={run.steps[s.id]}
                onChanged={() => {
                  onChanged();
                  onClose();
                }}
              />
            ))}
          {(!run || live.length === 0) && (
            <p className="text-2xs text-text-dim">
              {gate
                ? t("goals-goal-act-popover-decision-waiting-needs-goal-s-own")
                : t("goals-goal-act-popover-nothing-here-acts-one-control")}
            </p>
          )}
          <Button
            size="sm"
            onClick={() => {
              onClose();
              navigate({ name: "goal", id: goal }, { tab: "workflow" });
            }}
          >{t("goals-goal-act-popover-open-workflow-tab")}</Button>
        </div>
      )}
    </Popover>
  );
}
