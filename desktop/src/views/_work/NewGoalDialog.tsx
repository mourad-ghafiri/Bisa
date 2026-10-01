/**
 * Capturing a goal is one text box, and one choice about how it moves.
 *
 * **Auto** hands the goal to the Workflow Agent, which designs a workflow the
 * platform adopts, starts and repairs by itself. **Guided** has the agent
 * propose and you adopt from the Inbox. **Manual** opens the Workflow tab in
 * the designer for you to draw it, with the agent on request. The choice
 * starts on the workspace's `goals.default_mode`.
 *
 * Nothing here asks who carries the goal or which workflow it runs: the
 * Workflow Agent assigns every step from the enabled staff, and a manual
 * goal's person designs. The extra controls are the ones no agent can decide
 * for you: the **documents** the work starts from — a brief, a spec, a
 * screenshot, kept in the goal's `documents/` folder and read by whoever
 * works on it — and the **tags** it files under. A document is uploaded the
 * moment it is chosen (`useUploads`); *Capture* waits for the last upload, so
 * a goal is never captured without its context. The decisions live in
 * `newGoalModel.mjs` and `goalMode.mjs`.
 */

import { useEffect, useRef, useState } from "react";
import { api } from "../../api";
import { errorFields, log } from "../../log";
import { navigate } from "../../router";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { choiceOf } from "../../shell/settingsModel.mjs";
import type { GoalMode } from "../../types";
import { Button, Dialog, ICON, Labelled, PendingFiles, SegmentedControl, TAG_VOCABULARY, TagInput, TextArea, usePastedImages, useToast, useUploads, type Segment } from "../../ui";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, GOAL_MODES, MODE_MEANING, afterCapture, captureHint, modeSegments } from "../_goal/goalMode.mjs";
import { showHookSecrets } from "../_workflow/hookSecretsStore";
import { AssigneeTags } from "./AssigneePicker";
import { attachRefusedWords, canSubmit, captureAssignees, captureLabel, captureToast, documentsHint, goalBody } from "./newGoalModel.mjs";
import { attempt } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

const MODES: readonly Segment<GoalMode>[] = modeSegments().map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon] }));

/**
 * A team chosen elsewhere ("New goal for this team") that this dialog
 * should open with. Set immediately before firing the create-request; the
 * dialog consumes it once so a later plain "New goal" is not pre-assigned.
 * It is shown, never edited: the dialog offers no picker.
 */
let pendingTeam: string | null = null;
export function setPendingTeam(id: string | null): void {
  pendingTeam = id;
}

/**
 * Projects chosen elsewhere ("New goal from these projects" on the rail) that
 * the goal should be attached to once it exists. Consumed once, like the team.
 */
let pendingProjects: string[] = [];
export function setPendingProjects(ids: string[]): void {
  pendingProjects = [...ids];
}

export function NewGoalDialog({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: () => void }) {
  const toast = useToast();
  const { resolved } = useResolvedSettings(null);
  const defaultMode = choiceOf(resolved, DEFAULT_MODE_KEY, GOAL_MODES, DEFAULT_MODE);
  const [statement, setStatement] = useState("");
  const [mode, setMode] = useState<GoalMode>(defaultMode);
  const [touched, setTouched] = useState(false);
  const [team, setTeam] = useState<string | null>(null);
  const [tags, setTags] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [projects, setProjects] = useState<string[]>([]);
  const uploads = useUploads();
  // A paste anywhere in the dialog lands in the Documents well: a picture named first, a file as it is.
  const pasted = usePastedImages(uploads.take);
  const fileInput = useRef<HTMLInputElement>(null);

  // The workspace's default, until the person picks for this capture.
  useEffect(() => {
    if (!touched) setMode(defaultMode);
  }, [defaultMode, touched]);

  // Consume a team or projects handed over from elsewhere, once.
  useEffect(() => {
    if (!open) return;
    if (pendingTeam) {
      setTeam(pendingTeam);
      pendingTeam = null;
    }
    if (pendingProjects.length > 0) {
      setProjects(pendingProjects);
      pendingProjects = [];
    }
  }, [open]);

  const reset = () => {
    setProjects([]);
    setStatement("");
    setMode(defaultMode);
    setTouched(false);
    setTeam(null);
    setTags([]);
    uploads.clear();
  };

  const submit = async () => {
    if (!canSubmit({ statement, busy, uploading: uploads.uploading })) return;
    setBusy(true);
    let created: string | null = null;
    const body = goalBody({
      statement,
      mode,
      assignees: captureAssignees(team),
      tags,
      documents: uploads.uploaded,
    });
    await attempt(async () => {
      const { goal, secrets } = await api.createGoal(body);
      created = goal.id;
      // A capture that armed a public hook answers its secret this once.
      showHookSecrets(secrets ?? []);
    }, toast.error);
    // Attaching is additive and moves nothing. Each project is tried on its
    // own: one that refuses is said and logged, the goal still stands, and
    // the ones after it are attached all the same.
    if (created) {
      const goal: string = created;
      const refused: { project: string; reason: string }[] = [];
      for (const pid of projects) {
        try {
          await api.attachProject(pid, goal);
        } catch (e) {
          log.warn("goal", "a project handed over with a capture could not be attached", { goal, project: pid, ...errorFields(e) });
          refused.push({ project: pid, reason: e instanceof Error ? e.message : String(e) });
        }
      }
      const words = attachRefusedWords(refused);
      if (words) toast.error(words);
    }
    setBusy(false);
    if (!created) return;
    // Say it landed, and what happens next: the agent designs for a while,
    // or the designer opens for a manual goal.
    toast.ok(captureToast(mode));
    const landing = afterCapture(mode);
    reset();
    onCreated();
    onClose();
    navigate({ name: "goal", id: created }, landing ?? undefined);
  };

  const close = () => {
    reset();
    onClose();
  };

  return (
    <Dialog
      open={open}
      onClose={close}
      title={t("work-new-goal-dialog-what-do-want-make-real")}
      description={captureHint(mode)}
      footer={
        <>
          <Button variant="ghost" onClick={close} disabled={busy}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" onClick={() => void submit()} disabled={!canSubmit({ statement, busy, uploading: uploads.uploading })}>
            {captureLabel({ busy, uploading: uploads.uploading })}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3" onPaste={pasted.onPaste}>
        {pasted.dialog}
        <TextArea
          rows={4}
          autoFocus
          value={statement}
          placeholder={t("work-new-goal-dialog-ship-landing-page-bookmark-product")}
          disabled={busy}
          onChange={(e) => setStatement(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              void submit();
            }
          }}
        />
        <Labelled label={t("work-new-goal-dialog-how-moves")} hint={MODE_MEANING[mode]}>
          <SegmentedControl
            options={MODES}
            value={mode}
            onChange={(next) => {
              setTouched(true);
              setMode(next);
            }}
            label={t("work-new-goal-dialog-how-moves")}
          />
        </Labelled>
        {team && (
          <Labelled label={t("work-new-goal-dialog-carried")} hint={t("work-new-goal-dialog-team-goal-opened-agents-take-steps")}>
            <AssigneeTags value={captureAssignees(team)} />
          </Labelled>
        )}
        <Labelled label={t("work-agent-editor-tags")} hint={t("work-new-goal-dialog-how-files-optional")}>
          <TagInput value={tags} onChange={setTags} suggestions={[...TAG_VOCABULARY]} />
        </Labelled>
        <Labelled label={t("work-documents-card-documents")} hint={documentsHint(uploads.uploaded.length)}>
          <input
            ref={fileInput}
            type="file"
            multiple
            hidden
            onChange={(e) => {
              uploads.take(e.target.files);
              // Cleared so choosing the same file twice in a row fires again.
              e.target.value = "";
            }}
          />
          <div
            className="flex flex-col gap-1.5 rounded-control border border-dashed border-border p-2"
            // The OS's own file drop — a browser event, not the app's drag world.
            onDragOver={(e) => {
              if (e.dataTransfer.types.includes("Files")) e.preventDefault();
            }}
            onDrop={(e) => {
              if (!e.dataTransfer.files.length) return;
              e.preventDefault();
              uploads.take(e.dataTransfer.files);
            }}
          >
            <div className="flex items-center gap-2 text-2xs text-text-dim">
              <Button variant="ghost" size="sm" disabled={busy} onClick={() => fileInput.current?.click()}>
                <ICON.attach size={11} aria-hidden />{t("work-new-goal-dialog-add-files")}</Button>
              <span>{t("work-new-goal-dialog-drop-them-here")}</span>
            </div>
            <PendingFiles files={uploads.pending} onRemove={uploads.remove} />
          </div>
        </Labelled>
      </div>
    </Dialog>
  );
}
