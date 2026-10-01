/**
 * The one dialog that attaches a project the workspace already has to a goal.
 *
 * Opened from the project's About › Goals (*Attach to a goal…*) and from the
 * rail's project menu, and from nowhere else: creating a project asks nothing
 * about goals (`NewProjectDialog.tsx`), and a capture's hand-over (*New goal
 * with this project*, `NewGoalDialog.tsx`) attaches with no dialog at all.
 * It was two dialogs once — About's and the rail's, the same form written
 * twice with different words, one of them offering an empty select when there
 * was nothing to attach.
 *
 * Attaching is additive, reversible and moves nothing, so it asks nothing
 * beyond the goal: the only candidate is chosen for you, several are an
 * explicit pick, and nothing to offer is said in words
 * (`attachGoalModel.mjs`). The toast names both ends. Detaching stays where
 * it is — About's confirmed verb.
 */

import { useState } from "react";
import { api } from "../../api";
import { Button, Dialog, Field, Select, useToast } from "../../ui";
import { attempt } from "./useAsync";
import { attachChoice, attachEmptyWords, attachableGoals, attachedWords, goalLabelOf } from "./attachGoalModel.mjs";
import type { GoalChoice } from "./attachGoalModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** What the dialog needs of the project: its id, its name for the toast, the goals it is on. */
export interface AttachSubject {
  id: string;
  name: string;
  goals: readonly string[];
}

export function AttachGoalDialog({
  project,
  goals,
  onClose,
  onAttached,
}: {
  /** Open while a project is given; `null` closes it. */
  project: AttachSubject | null;
  /** Every goal of the workspace, labelled. */
  goals: GoalChoice[];
  onClose: () => void;
  /** The record is written: the caller reloads what it shows. */
  onAttached: () => void;
}) {
  const toast = useToast();
  const candidates = project ? attachableGoals(goals, project.goals) : [];
  /**
   * The person's own pick, remembering which project it was made for: a pick
   * for one project is no pick for the next, with no effect to reset it. The
   * model falls back when the pick is no longer offered.
   */
  const [pick, setPick] = useState<{ project: string; goal: string } | null>(null);
  const picked = project && pick?.project === project.id ? pick.goal : null;
  const chosen = attachChoice(candidates, picked);

  const attach = () => {
    if (!project || !chosen) return;
    const { id, name } = project;
    const label = goalLabelOf(goals, chosen);
    onClose();
    void attempt(() => api.attachProject(id, chosen), toast.error, () => {
      toast.ok(attachedWords(name, label));
      onAttached();
    });
  };

  return (
    <Dialog
      open={project !== null}
      onClose={onClose}
      title={t("work-attach-goal-dialog-attach-goal")}
      description={t("work-attach-goal-dialog-goal-s-agents-start-seeing-project")}
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={!chosen} onClick={attach}>{t("work-attach-goal-dialog-attach")}</Button>
        </>
      }
    >
      {candidates.length === 0 ? (
        <p className="text-xs text-text-dim">{attachEmptyWords(goals.length)}</p>
      ) : (
        <Field label={t("work-attach-goal-dialog-goal")}>
          <Select value={chosen} onChange={(e) => project && setPick({ project: project.id, goal: e.target.value })}>
            <option value="">{t("work-attach-goal-dialog-pick-goal")}</option>
            {candidates.map((g) => (
              <option key={g.id} value={g.id}>
                {g.label}
              </option>
            ))}
          </Select>
        </Field>
      )}
    </Dialog>
  );
}
