/**
 * The strip above a run's canvas: which run this is, how far it has got,
 * what is live, and how it ended. A legend for the ring colours — a
 * diverted step's detour among them — so the canvas is readable without
 * having learned it; the boundary chips that fired light on their cards.
 * With more than one run, a
 * menu over the goal's runs — the same rows the Runs list draws — picks
 * which one the canvas wears.
 */

import type { RunSummary, WorkflowRun } from "../../types";
import { Chip, ICON, Menu, RUN_STATUS_ICON, RelativeTime, STEP_STATE_ICON, type MenuItem } from "../../ui";
import { runRows } from "../_goal/runControl.mjs";
import { LEGEND_STATES, STEP_TONE_TOKENS, overlayFacts, stepStateWord } from "./runView.mjs";
import { t } from "../../i18n/l10n.mjs";
import { percent } from "../../i18n/format.mjs";
import { rich } from "../../i18n/rich";

export function RunOverlay({ run, runs, onPickRun }: { run: WorkflowRun; runs: RunSummary[]; onPickRun?: (id: string) => void }) {
  // What the strip says is the model's: the run's own status, its place, how far, when.
  const facts = overlayFacts(run, runs);
  const picker: MenuItem[] = runRows(runs).map((r) => ({
    label: t("workflow-run-overlay-run", { index: r.index, word: r.words.word }),
    icon: RUN_STATUS_ICON[r.status] ?? ICON.run,
    disabled: r.id === run.id,
    onSelect: () => onPickRun?.(r.id),
  }));
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-hairline px-3 py-1.5 text-2xs">
      <ICON.run size={13} aria-hidden className="text-text-dim" />
      <span className="font-medium">
        {t("workflow-run-overlay-which", {
          run: facts.index ? t("workflow-run-history-run", { index: facts.index }) : t("workflow-run-overlay-run-2", { run: run.id.slice(-6) }),
          revision: t("workflow-goal-workflow-tab-rev", { revision: facts.revision ?? 0 }),
        })}
      </span>
      <Chip tone={facts.tone}>{facts.word}</Chip>
      {!facts.queued && (
        <>
          <div className="h-1.5 w-32 overflow-hidden rounded-full bg-surface-2" role="progressbar" aria-valuenow={facts.percent} aria-valuemin={0} aria-valuemax={100}>
            {/* The fill wears the run's own tone, as its chip does: the accent only while it runs. */}
            <div className="h-full" style={{ width: `${facts.percent}%`, background: `var(--color-${facts.tone === "quiet" ? "text-dim" : facts.tone})` }} />
          </div>
          <span className="tnum text-text-dim">{percent(facts.percent / 100)}</span>
        </>
      )}
      {facts.live.length > 0 && (
        <span className="text-text-dim">
          {rich("workflow-run-overlay-live-steps", { steps: <code className="font-mono">{facts.live.join(", ")}</code> })}
        </span>
      )}
      <span className="text-text-dim">
        {facts.began.at != null && (
          <>
            {facts.began.word} <RelativeTime at={facts.began.at} />
          </>
        )}
        {facts.queued ? ` ${t("workflow-run-overlay-starts-when-live-run-finishes")}` : null}
        {facts.ended?.at != null ? (
          <>
            {" · "}
            {facts.ended.word} <RelativeTime at={facts.ended.at} />
          </>
        ) : null}
      </span>
      {runs.length > 1 && onPickRun && (
        <Menu
          label={t("workflow-run-overlay-which-run")}
          items={picker}
          trigger={
            <span className="anim ml-auto inline-flex items-center gap-1 rounded-control border border-border bg-surface px-1.5 py-0.5 text-2xs hover:bg-surface-2">
              <ICON.queued size={11} aria-hidden />{t("workflow-run-overlay-runs", { runs: runs.length })}</span>
          }
        />
      )}
      <ul className={`flex items-center gap-2 ${runs.length > 1 ? "" : "ml-auto"}`} aria-label={t("workflow-run-overlay-legend")}>
        {LEGEND_STATES.map((s) => {
          const Icon = STEP_STATE_ICON[s];
          return (
            <li key={s} className="flex items-center gap-1 text-text-dim">
              <Icon size={11} aria-hidden style={{ color: `var(--color-${STEP_TONE_TOKENS[s]})` }} />
              {stepStateWord(s)}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
