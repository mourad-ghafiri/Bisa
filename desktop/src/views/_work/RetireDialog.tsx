/**
 * Retiring a goal or a workflow — archiving it or deleting it — with what
 * came from it, on one plan the person reads before pressing. Drawn from
 * the node's preview (`GET …/retirement`): the refusal a deletion would
 * meet, the run that is cancelled, what stops — the engine sessions
 * aborted, the harnesses terminated and the shells closed, counted from
 * this app's own stores — the designs that go or stay, what still uses a
 * workflow (each holder a door), the projects born of the thing with their
 * facts and one choice for all of them, the projects merely attached. The
 * one button reads the plan back and is red only when something is
 * deleted. The words are `retireModel.mjs`'s; the act is `retire.ts`'s.
 *
 * The dialog closes itself the moment the node answers, before the
 * after-effects run: nothing that follows can keep it open on a thing that
 * no longer exists, and the screen that opened it hears the deletion on
 * the bus whatever this dialog does.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { navigate } from "../../router";
import { sessionRows, useSessions } from "../../shell/sessionsStore";
import { terminalSessions, useTerminals } from "../../shell/useTerminals";
import type { Retirement as RetirementFacts } from "../../types";
import { Button, Checkbox, Dialog, Pending, SegmentedControl, failureText, cn, useToast } from "../../ui";
import type { Segment } from "../../ui";
import type { Retirement } from "./retire";
import { retire } from "./retire";
import { confirmWords, defaultChoices, deleteAvailable, destroys, retireSections, terminationOf, titleWords } from "./retireModel.mjs";
import type { ProjectsFate, RetireChoices, RetireKind, ThingFate } from "./retireModel.mjs";
import { attempt } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

const THING: readonly Segment<ThingFate>[] = [
  { id: "archive", label: t("work-project-detail-archive-2") },
  { id: "delete", label: t("work-library-refs-delete") },
];
const PROJECTS: readonly Segment<ProjectsFate>[] = [
  { id: "keep", label: t("work-retire-dialog-keep") },
  { id: "archive", label: t("work-project-detail-archive-2") },
  { id: "delete", label: t("work-library-refs-delete") },
];

// A quiet section is a ground, not a box; only a warning or a deletion keeps an edge.
const TONE = { quiet: "border-transparent bg-surface-2/50", warn: "border-warn/40 bg-warn-soft", danger: "border-danger/40 bg-danger-soft" } as const;

export function RetireDialog({
  kind,
  id,
  name,
  wanted,
  open,
  onClose,
  onRetired,
}: {
  kind: RetireKind;
  id: string;
  name: string;
  /** The fate the door named — *Archive goal…* or *Delete goal…*. */
  wanted: ThingFate;
  open: boolean;
  onClose: () => void;
  /** It happened: what the node did, what this app terminated, and the plan it was done on. Called after the dialog closed. */
  onRetired: (done: Retirement, choices: RetireChoices) => void;
}) {
  const toast = useToast();
  const [preview, setPreview] = useState<RetirementFacts | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<RetireChoices>(() => defaultChoices(wanted, null));
  const [busy, setBusy] = useState(false);
  // The stores, subscribed: the consent line follows a tab that closes or a session that ends while the dialog is open.
  useSessions();
  useTerminals();

  // The facts are read when the dialog opens, and the choices start from
  // them: a used workflow, or a goal whose design is used elsewhere, opens
  // on Archive whatever door was pressed.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setPreview(null);
    setError(null);
    void (kind === "goal" ? api.goalRetirement(id) : api.workflowRetirement(id))
      .then((r) => {
        if (cancelled) return;
        setPreview(r.retirement);
        setChoices(defaultChoices(wanted, r.retirement));
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(failureText("work", "retire-dialog-failed", e));
      });
    return () => {
      cancelled = true;
    };
  }, [open, kind, id, wanted]);

  const confirm = async () => {
    if (!preview || busy) return;
    setBusy(true);
    let done: Retirement | null = null;
    const ok = await attempt(async () => {
      done = await retire(kind, id, choices, preview);
    }, toast.error);
    setBusy(false);
    if (!ok || !done) return;
    // Closed first: what follows is the caller's, on a thing that is gone or put away.
    onClose();
    onRetired(done, choices);
  };

  const terminated = preview ? terminationOf(preview, sessionRows(), terminalSessions(), choices, kind === "goal" ? id : null) : null;
  const sections = preview && terminated ? retireSections(kind, preview, choices, terminated) : [];
  const canDelete = deleteAvailable(preview);
  const born = preview?.projects_born.length ?? 0;

  return (
    <Dialog
      open={open}
      onClose={() => {
        if (!busy) onClose();
      }}
      title={titleWords(choices.thing, name)}
      description={
        kind === "goal"
          ? t("work-retire-dialog-run-cancelled-everything-working-ends-before")
          : t("work-retire-dialog-what-becomes-projects-steps-made-choice")
      }
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant={preview && destroys(choices) ? "danger" : "primary"} disabled={!preview || busy} onClick={() => void confirm()}>
            {busy ? t("work-retire-dialog-stopping-retiring") : preview ? confirmWords(kind, choices, preview) : "…"}
          </Button>
        </>
      }
    >
      {error && <p className="rounded-control border border-danger/40 bg-danger-soft px-2 py-1.5 text-2xs text-danger">{error}</p>}
      {!preview && !error && <Pending what={t("work-retire-dialog-what-retiring-touches", { name })} rows={3} />}
      {preview && (
        <div className="flex flex-col gap-3 text-xs">
          <section className="flex flex-col gap-1">
            <span className="text-2xs font-semibold text-text-dim">{t("work-retire-dialog-words", { kind })}</span>
            <SegmentedControl options={THING} value={choices.thing} onChange={(thing) => setChoices({ ...choices, thing })} label={t("work-retire-dialog-s-fate", { kind })} size="sm" />
            <p className="text-2xs leading-relaxed text-text-dim">
              {choices.thing === "delete"
                ? kind === "goal"
                  ? t("work-retire-dialog-journal-runs-work-items-removed-from")
                  : t("work-retire-dialog-definition-every-revision-forgotten-runs-already")
                : kind === "goal"
                  ? t("work-retire-dialog-closed-put-away-hidden-from-list")
                  : t("work-retire-dialog-out-library-pickers-refused-goal-run")}
              {!canDelete && ` ${t("work-retire-dialog-delete-not-available-while-something-uses")}`}
            </p>
          </section>
          {sections.map((s) => (
            <section key={s.id} className={cn("flex flex-col gap-1 rounded-control border px-2 py-1.5", TONE[s.tone])}>
              <span className="text-2xs font-semibold text-text-dim">{s.title}</span>
              {s.id === "born" && (
                <div className="flex flex-col gap-1">
                  <SegmentedControl options={PROJECTS} value={choices.projects} onChange={(projects) => setChoices({ ...choices, projects })} label={t("work-retire-dialog-projects-fate")} size="sm" />
                  {choices.projects === "delete" && <Checkbox label={t("work-retire-dialog-move-their-folders-trash")} checked={choices.tree} onChange={(tree) => setChoices({ ...choices, tree })} hint={t("work-retire-dialog-managed-folder-only-adopted-one-never")} />}
                </div>
              )}
              <ul className="flex flex-col gap-0.5 text-2xs text-text">
                {s.lines.map((l, i) => (
                  <li key={i} className={i < (s.id === "born" ? born : 0) ? "text-text-dim" : undefined}>
                    {l}
                  </li>
                ))}
              </ul>
              {s.holders && s.holders.some((h) => h.kind === "goal") && (
                <ul className="flex flex-wrap gap-1.5 text-2xs" aria-label={t("work-retire-dialog-goals-use")}>
                  {s.holders
                    .filter((h) => h.kind === "goal")
                    .map((h) => (
                      <li key={h.id}>
                        <button
                          type="button"
                          className="anim rounded-sm text-text underline decoration-text-dim/50 underline-offset-2 hover:decoration-text"
                          onClick={() => {
                            onClose();
                            navigate({ name: "goal", id: h.id });
                          }}
                        >
                          {t("work-retire-dialog-open-harness", { label: h.label, live: h.live ? "yes" : "no" })}
                        </button>
                      </li>
                    ))}
                </ul>
              )}
            </section>
          ))}
        </div>
      )}
    </Dialog>
  );
}
