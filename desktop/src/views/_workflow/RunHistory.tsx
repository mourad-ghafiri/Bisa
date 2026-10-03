/**
 * The goal's runs, newest first: the number, the status with its place in
 * the queue or its cause, when it was made or started and when it ended,
 * *View* into the Workflow tab wearing that run, and *Withdraw* on a queued
 * row — no confirm: a queued run did nothing yet, and a withdrawal is undone
 * by a new run.
 *
 * A goal that listens for a year keeps every run it ever made, and a list
 * is never drawn without a bound: a page of rows — never fewer than the
 * runs still to end, which lead it — and *Show older* for the next
 * (`workflowRunsModel.paneWindow`, the Runs pane's own rule).
 */
import { useState } from "react";
import { api } from "../../api";
import { setSearch } from "../../router";
import type { RunSummary } from "../../types";
import { Button, Chip, ICON, RUN_STATUS_ICON, RelativeTime, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { runRows } from "../_goal/runControl.mjs";
import { RUNS_SHOWN, paneWindow } from "./workflowRunsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function RunHistory({ goal, runs, onChanged }: { goal: string; runs: RunSummary[]; onChanged: () => void }) {
  const toast = useToast();
  const [shown, setShown] = useState(RUNS_SHOWN);
  const all = runRows(runs);
  const { rows, hidden, more } = paneWindow(all, shown);
  if (all.length === 0) return null;
  return (
    <section className="rounded-card border border-border" aria-label={t("workflow-run-history-runs")}>
      <h3 className="flex items-center gap-2 border-b border-hairline px-3 py-2 text-sm font-semibold text-text">
        <ICON.run size={12} aria-hidden className="text-text-dim" />
        {t("workflow-run-history-runs")}
        <span className="tnum text-2xs font-normal text-text-dim">{all.length}</span>
      </h3>
      <ul className="flex flex-col">
        {rows.map((r) => {
          const Icon = RUN_STATUS_ICON[r.status] ?? ICON.run;
          return (
            <li key={r.id} className="flex min-h-row flex-wrap items-center gap-2 border-b border-hairline px-3 py-1.5 text-2xs last:border-b-0">
              <Icon size={12} aria-hidden style={{ color: `var(--color-${r.words.tone === "quiet" ? "text-dim" : r.words.tone})` }} />
              <span className="font-medium">{t("workflow-run-history-run", { index: r.index })}</span>
              <Chip tone={r.words.tone}>{r.words.word}</Chip>
              <span className="text-text-dim">{t("workflow-goal-workflow-tab-rev", { revision: r.revision })}</span>
              {r.words.at != null && (
                <span className="text-text-dim">
                  {r.words.atWord} <RelativeTime at={r.words.at} />
                </span>
              )}
              <span className="ml-auto flex items-center gap-1">
                <Button size="sm" variant="ghost" onClick={() => setSearch({ tab: "workflow", run: r.id })}>{t("workflow-run-history-view")}</Button>
                {r.withdraw && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      void attempt(() => api.withdrawRun(goal, r.id), toast.error, () => {
                        toast.ok(t("workflow-run-history-withdrawn"));
                        onChanged();
                      })
                    }
                  >{t("workflow-run-history-withdraw")}</Button>
                )}
              </span>
            </li>
          );
        })}
      </ul>
      {hidden > 0 && (
        <div className="flex items-center gap-2 border-t border-hairline px-3 py-1.5 text-2xs text-text-dim">
          <span>{t("workflow-runs-pane-older-hidden", { n: hidden })}</span>
          <Button size="sm" variant="ghost" onClick={() => setShown((n) => n + RUNS_SHOWN)}>
            {t("workflow-runs-pane-show-older", { n: more })}
          </Button>
        </div>
      )}
    </section>
  );
}
