/**
 * The designer's Runs pane (03-workflows §Runs of the workspace): the
 * workflow's runs of the workspace — the ones still going first, then the
 * newest — with *Run…* on top and, per run, its title, its status in words,
 * when it started or ended, who started it and its verbs: *Stop* while it
 * goes, *Restart*, *Open* (its page, `#/runs/<rid>`). A run of the workspace
 * runs its own copy of the workflow, so nothing here freezes the canvas.
 * Refreshed whenever the bus says a run of this workflow moved
 * (`movesRunsOf`). A workflow that listens keeps every run it made, so the
 * pane draws a bounded list — the runs going, then the newest — and *Show
 * older* brings the next page (`paneWindow`); a workflow put away restarts
 * none. The rules are `workflowRunsModel.mjs`'s; this is paint.
 */

import { useState } from "react";
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { href, navigate } from "../../router";
import type { WorkflowRow } from "../../types";
import { Button, Chip, ErrorNote, ICON, RUN_STATUS_ICON, RelativeTime, SkeletonRows } from "../../ui";
import { useReloadOnReconnect } from "../../ui/useReloadOnReconnect";
import { useAsync } from "../_work/useAsync";
import { useRunVerbs } from "./useRunVerbs";
import { useWorkflowVerbs } from "./WorkflowCard";
import { RUNS_SHOWN, movesRunsOf, paneWindow, readStanding, runsPaneRows } from "./workflowRunsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function WorkflowRunsPane({ row, onChanged }: { row: WorkflowRow; onChanged: () => void }) {
  const id = row.workflow.id;
  const { data, error, loading, reload } = useAsync((s) => api.workflowRuns(id, s), [id]);
  const moved = () => {
    reload();
    onChanged();
  };
  useEngineEvents((e) => {
    if (movesRunsOf(e, id)) moved();
  });
  // What a restarted node resumed, failed or withdrew reaches the pane by no frame.
  useReloadOnReconnect(moved);
  const { verbs, items, dialogs } = useWorkflowVerbs(row, moved);
  // Stop and Restart, one at a time on a run.
  const runVerbs = useRunVerbs(moved);
  const runItem = items.find((item) => item.verb === "run");
  // How many rows the person asked to see: the first page until *Show older*.
  const [shown, setShown] = useState(RUNS_SHOWN);
  const all = runsPaneRows(data?.runs, { archived: Boolean(row.workflow.archived) });
  const { rows, hidden, more } = paneWindow(all, shown);
  // A re-read that failed keeps the rows on screen and says why.
  const read = readStanding({ data, loading, error }, t("workflow-runs-what-runs"));

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <ICON.run size={13} aria-hidden className="text-text-dim" />
        <h3 className="text-2xs font-semibold tracking-wide text-text-dim uppercase">{t("workflow-designer-panel-runs")}</h3>
        <span className="tnum text-2xs text-text-dim">{row.runs.total}</span>
        {runItem && (
          <Button size="sm" variant="primary" className="ml-auto" onClick={runItem.onSelect}>
            <ICON.run size={12} aria-hidden />
            {verbs.run?.label}
          </Button>
        )}
      </div>
      {(row.workspace_problems ?? []).length > 0 && (
        <p className="border-b border-border px-3 py-2 text-2xs text-text-dim">{t("workflow-runs-pane-runs-on-goal-only")}</p>
      )}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {read.standing === "stale" && (
          <p role="status" className="border-b border-border px-3 py-1 text-2xs text-danger">
            {read.line}
          </p>
        )}
        {read.standing === "failed" ? (
          <div className="p-3">
            <ErrorNote error={read.line ?? ""} retry={reload} />
          </div>
        ) : read.standing === "reading" ? (
          <SkeletonRows rows={4} className="p-3" />
        ) : rows.length === 0 ? (
          <p className="p-3 text-2xs text-text-dim">{t("workflow-runs-pane-no-runs-yet")}</p>
        ) : (
          <ul className="flex flex-col">
            {rows.map((r) => {
              const Icon = RUN_STATUS_ICON[r.status] ?? ICON.run;
              return (
                <li key={r.id} className="flex flex-col gap-1 border-b border-border px-3 py-2 text-2xs">
                  <div className="flex items-center gap-2">
                    <Icon size={12} aria-hidden style={{ color: `var(--color-${r.words.tone === "quiet" ? "text-dim" : r.words.tone})` }} />
                    <a href={href({ name: "run", id: r.id })} className="min-w-0 truncate font-medium hover:underline">
                      {r.title}
                    </a>
                    <Chip tone={r.words.tone}>{r.words.word}</Chip>
                  </div>
                  <div className="flex flex-wrap items-center gap-x-2 text-text-dim">
                    {r.words.at != null && (
                      <span>
                        {r.words.atWord} <RelativeTime at={r.words.at} />
                      </span>
                    )}
                    <span>{r.startedBy}</span>
                  </div>
                  <div className="flex items-center gap-1">
                    <Button size="sm" variant="ghost" onClick={() => navigate({ name: "run", id: r.id })}>
                      <ICON.forward size={12} aria-hidden />
                      {t("workflow-runs-pane-open")}
                    </Button>
                    {r.verbs.restart && (
                      <Button size="sm" variant="ghost" disabled={runVerbs.busy(r.id)} onClick={() => runVerbs.restart(r.id)}>
                        <ICON.restart size={12} aria-hidden />
                        {t("workflow-runs-pane-restart")}
                      </Button>
                    )}
                    {r.verbs.stop && (
                      <Button size="sm" variant="ghost" disabled={runVerbs.busy(r.id)} onClick={() => runVerbs.stop(r.id)}>
                        <ICON.stop size={12} aria-hidden />
                        {t("workflow-runs-pane-stop")}
                      </Button>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
        {hidden > 0 && (
          <div className="flex items-center gap-2 px-3 py-2 text-2xs text-text-dim">
            <span>{t("workflow-runs-pane-older-hidden", { n: hidden })}</span>
            <Button size="sm" variant="ghost" onClick={() => setShown((n) => n + RUNS_SHOWN)}>
              {t("workflow-runs-pane-show-older", { n: more })}
            </Button>
          </div>
        )}
      </div>
      {dialogs}
    </div>
  );
}
