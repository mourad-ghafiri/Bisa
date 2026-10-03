/**
 * A session under its workstream — a harness a person opened in a terminal
 * or an engine session — and, nested under it, its sub-agents. The harness's
 * own glyph (a sub-agent the nested mark), the name, the model after it
 * with the effort it runs at, the sub-agent count as a badge, what it is doing in the state's ink, how
 * long, *Answer* when it waits on you, the one stop mark while it runs or
 * waits, the state mark. What the harness's account has left is the
 * footer's, not a session row's.
 */

import { ContextMenu, CountBadge, ICON, LiveDuration, RelativeTime, SessionMark, absolute, cn, durationPrecise, harnessMark, modelWords, relative } from "../../../ui";
import type { MenuItem, TreeRowState } from "../../../ui";
import { isAttention, isLive, isStoppable, stateOf } from "../../../ui/sessionState.mjs";
import type { RailRow as RailModelRow } from "../projectRailModel.mjs";
import { rowTreatment } from "../railStyleModel.mjs";
import { RailAction, RailActions } from "./RailActions";
import { RailRow } from "./RailRow";
import { RailGlyph, RailTwisty } from "./RailTwisty";
import { t } from "../../../i18n/l10n.mjs";

type AgentRow = Extract<RailModelRow, { kind: "agent" }>;

export function RailAgentRow({
  row,
  rs,
  menu,
  harnessLabels,
  folded,
  onToggle,
  onOpen,
  onAnswer,
  onStop,
}: {
  row: AgentRow;
  rs: TreeRowState;
  menu: MenuItem[];
  harnessLabels: Record<string, string>;
  /** Whether this harness's sub-agents are folded under it. */
  folded: boolean;
  onToggle: () => void;
  onOpen: () => void;
  onAnswer: () => void;
  onStop: () => void;
}) {
  const attention = isAttention(row.state);
  // A harness with sub-agents folds them under its own chevron.
  const foldable = row.parent === null && (row.childCount ?? 0) > 0;
  const Glyph = harnessMark(row.harness);
  // The model, then the effort the session runs at when the row carries one.
  const model = modelWords(row.model, row.effort);
  const stopLabel = row.terminalKey ? t("workbench-rail-agent-row-terminate-harness") : t("workbench-rail-agent-row-abort-session");
  const waiting = attention && stateOf(row.state) === "waiting";
  const treatment = rowTreatment({ kind: "agent", attention: attention ? (waiting ? "accent" : "danger") : "none" });
  return (
    <ContextMenu items={menu}>
      <RailRow
        rs={rs}
        treatment={treatment}
        className="text-2xs"
        title={t("workbench-rail-agent-row-words", { row: row.label, full: model?.full ?? "", flag: model ? "yes" : "no", harness: harnessLabels[row.harness] ?? row.harness, flag2: (row.harness) ? "yes" : "no", activity: row.activity })}
        onClick={onOpen}
      >
        <RailTwisty open={foldable ? !folded : null} onToggle={onToggle} />
        <RailGlyph className={row.parent ? "text-text-dim" : undefined}>{row.parent ? <ICON.subagent size={12} aria-hidden /> : <Glyph size={12} aria-hidden />}</RailGlyph>
        <span className="min-w-0 truncate font-medium">{row.label}</span>
        {model && (
          // The rail's own width decides (its root is an `@container`), not the window's.
          <span className="hidden min-w-0 shrink truncate text-text-dim @xs:inline" title={model.full}>
            {model.short}
          </span>
        )}
        {foldable && <CountBadge count={row.childCount ?? 0} tone="neutral" title={t("workbench-rail-agent-row-sub-agents", { n: row.childCount ?? 0 })} />}
        <span className={cn("min-w-0 flex-1 truncate", attention && (stateOf(row.state) === "waiting" ? "text-accent-ink" : "text-danger"))}>{row.activity}</span>
        {row.started !== null ? (
          <span className="tnum hidden shrink-0 text-2xs text-text-dim @sm:inline" title={t("workbench-rail-agent-row-started-at", { at: absolute(row.started) })}>
            {isLive(row.state) ? <LiveDuration since={row.started} /> : row.since > row.started ? durationPrecise(row.since - row.started) : relative(row.since)}
          </span>
        ) : (
          <RelativeTime at={row.since} className="hidden shrink-0 text-2xs @sm:inline" />
        )}
        {row.gateId && !row.terminalKey && (
          <button
            type="button"
            className="anim shrink-0 rounded px-1 text-accent-ink hover:underline"
            onClick={(e) => {
              e.stopPropagation();
              onAnswer();
            }}
          >{t("workbench-rail-agent-row-answer")}</button>
        )}
        {/* One stop mark, only while something runs or waits on you
            (`isStoppable` — never a session idle between turns); the verb —
            Terminate a harness, Abort a session — is the tooltip's. */}
        {row.parent === null && isStoppable(row.state) && (
          <RailActions>
            <RailAction label={stopLabel} danger onClick={onStop}>
              <ICON.terminate size={11} aria-hidden />
            </RailAction>
          </RailActions>
        )}
        <SessionMark state={row.state} />
      </RailRow>
    </ContextMenu>
  );
}
