/**
 * One workflow in the library: the graph's thumbnail, what it is, the one
 * state it is in — marked *On* or *Paused* while it listens — how big,
 * where from — with a `⋮` beside the card
 * (`workflowCardModel.cardMenu`): *Open*; the run verbs — *Run…* when it
 * can start in the workspace (*Test run…* when only events begin it), *Turn
 * on…* / *Turn off* when it begins on events, *Stop every run* and *Restart
 * every run* while a run of it there goes (`workflowVerbs.mjs`); *Delete…*, the designer's own dialog
 * (`RetireDialog`: the holders, the fall-back to Archive while anything
 * uses it) — and, under the card, the projects its steps made, each a door
 * into the Project IDE. The card itself is the door to the designer
 * (`LibraryCard` as an `<a>`); the menu and the footer sit outside it.
 */

import { useState } from "react";
import { api } from "../../api";
import type { ProjectRow, WorkflowRow } from "../../types";
import { href, navigate } from "../../router";
import { ConfirmDialog, MoreMenu, useToast, type MenuItem } from "../../ui";
import { attempt } from "../_work/useAsync";
import { retiredWords } from "../_work/retireModel.mjs";
import { RetireDialog } from "../_work/RetireDialog";
import { Chip, ICON, TagChips } from "./cardHelpers";
import { LibraryCard } from "./LibraryCard";
import { RunWorkflowDialog } from "./RunWorkflowDialog";
import { TurnOnDialog } from "./TurnOnDialog";
import { cardMenu, cardMeta, cardStatus, onMark } from "./workflowCardModel.mjs";
import { restartEveryWords, stopEveryWords, workflowVerbs } from "./workflowVerbs.mjs";
import { t } from "../../i18n/l10n.mjs";

type Verb = "run" | "turn_on" | "turn_off" | "restart" | "stop";

/** A run verb as a menu item, tagged with which verb it is — so a host filters by the verb, never by its words. */
export interface VerbItem extends MenuItem {
  verb: Verb;
}

/**
 * The run verbs of a library workflow, as menu items with their dialogs —
 * shared by the card and the designer's header menu.
 */
export function useWorkflowVerbs(row: WorkflowRow | null | undefined, onChanged?: () => void) {
  const toast = useToast();
  const [pending, setPending] = useState<Verb | null>(null);
  const verbs = workflowVerbs(row);
  // Turning Off is at once, with no confirm: turning On again is one dialog away.
  const turnOff = (wf: WorkflowRow) =>
    void attempt(() => api.turnOffWorkflow(wf.workflow.id), toast.error, () => {
      toast.ok(t("workflow-workflow-card-turned-off", { workflow: wf.workflow.name }));
      onChanged?.();
    });
  const items: VerbItem[] = row
    ? [
        ...(verbs.run ? [{ verb: "run" as const, label: verbs.run.label, icon: ICON.run, onSelect: () => setPending("run") }] : []),
        ...(verbs.turnOn ? [{ verb: "turn_on" as const, label: verbs.turnOn.label, icon: ICON.signal, onSelect: () => setPending("turn_on") }] : []),
        ...(verbs.turnOff ? [{ verb: "turn_off" as const, label: verbs.turnOff.label, icon: ICON.close, onSelect: () => turnOff(row) }] : []),
        ...(verbs.restart ? [{ verb: "restart" as const, label: verbs.restart.label, icon: ICON.restart, onSelect: () => setPending("restart") }] : []),
        ...(verbs.stop ? [{ verb: "stop" as const, label: verbs.stop.label, icon: ICON.stop, danger: true, onSelect: () => setPending("stop") }] : []),
      ]
    : [];
  const close = () => setPending(null);
  const dialogs = row ? (
    <>
      <RunWorkflowDialog open={pending === "run"} row={row} onClose={close} onDone={onChanged} />
      <TurnOnDialog open={pending === "turn_on"} row={row} onClose={close} onDone={onChanged} />
      <ConfirmDialog
        open={pending === "stop"}
        onClose={close}
        title={t("workflow-workflow-card-stop-every-run-workflow")}
        confirmLabel={t("workflow-workflow-card-stop-every-run")}
        danger
        body={stopEveryWords(verbs.stop?.live ?? 0)}
        onConfirm={() => {
          close();
          void attempt(() => api.stopWorkflow(row.workflow.id), toast.error, (r) => {
            toast.ok(r.runs.length === 0 ? t("workflow-workflow-card-nothing-running") : t("workflow-workflow-card-stopped-run-runs", { runs: r.runs.length }));
            onChanged?.();
          });
        }}
      />
      <ConfirmDialog
        open={pending === "restart"}
        onClose={close}
        title={t("workflow-workflow-card-restart-every-run-workflow")}
        confirmLabel={t("workflow-workflow-card-restart-every-run")}
        body={restartEveryWords(verbs.restart?.live ?? 0)}
        onConfirm={() => {
          close();
          void attempt(() => api.restartWorkflow(row.workflow.id), toast.error, (r) => {
            toast.ok(r.runs.length === 0 ? t("workflow-workflow-card-nothing-running") : t("workflow-workflow-card-restarted-run-runs", { runs: r.runs.length }));
            onChanged?.();
          });
        }}
      />
    </>
  ) : null;
  return { verbs, items, dialogs };
}

export function WorkflowCard({ row, projects = [], onChanged }: { row: WorkflowRow; projects?: readonly ProjectRow[]; onChanged?: () => void }) {
  const toast = useToast();
  const w = row.workflow;
  const { verbs, items: verbItems, dialogs } = useWorkflowVerbs(row, onChanged);
  const [deleting, setDeleting] = useState(false);
  const byVerb = new Map(verbItems.map((item) => [item.verb, item]));
  // The menu's shape is the model's; the card binds each id to its door or dialog.
  const items: MenuItem[] = cardMenu(verbs).map((item) => {
    if (item.id === "open") return { label: item.label, icon: ICON.forward, onSelect: () => navigate({ name: "workflow", id: w.id }) };
    if (item.id === "delete") return { label: item.label, icon: ICON.delete, danger: true, separatorBefore: true, onSelect: () => setDeleting(true) };
    const verb = byVerb.get(item.id);
    return { ...verb, label: item.label, separatorBefore: item.separatorBefore, onSelect: verb?.onSelect ?? (() => {}) };
  });
  return (
    <div className="flex min-h-0 flex-col gap-1">
      <LibraryCard
        definition={w}
        href={href({ name: "workflow", id: w.id })}
        icon={<ICON.workflow size={14} aria-hidden />}
        title={w.name}
        status={cardStatus(row)}
        mark={onMark(row)}
        description={w.description}
        meta={cardMeta(row)}
        dimmed={Boolean(w.archived)}
        menu={<MoreMenu vertical label={t("workflow-workflow-card-menu", { w: w.name })} items={items} />}
        footer={
          <>
            {row.used_by.length > 0 && (
              <Chip tone="accent" title={t("workflow-workflow-card-goals-workflows-use-it")}>{t("workflow-workflow-card-used", { used_by: row.used_by.length })}</Chip>
            )}
            <TagChips tags={[...(w.tags ?? [])]} max={3} />
          </>
        }
      />
      {projects.length > 0 && (
        <ul className="flex flex-wrap items-center gap-1.5 px-1 text-2xs text-text-dim" aria-label={t("workflow-workflow-card-projects-steps-made")}>
          <li>{t("workflow-workflow-card-projects-made-by-steps", { projects: projects.length })}</li>
          {projects.map((p) => (
            <li key={p.project.id}>
              <a href={href({ name: "workbench", scope: "workstream", id: p.project.id })} title={t("workflow-workflow-card-open-project-ide")} className="anim inline-flex items-center gap-1 rounded border border-border px-1 hover:border-accent/50 hover:text-text">
                <ICON.project size={10} aria-hidden />
                {p.project.name}
              </a>
            </li>
          ))}
        </ul>
      )}
      {dialogs}
      {deleting && (
        <RetireDialog
          kind="workflow"
          id={w.id}
          name={w.name}
          wanted="delete"
          open
          onClose={() => setDeleting(false)}
          onRetired={(done, choices) => {
            toast.ok(retiredWords("workflow", choices.thing, done.terminated));
            onChanged?.();
          }}
        />
      )}
    </div>
  );
}
