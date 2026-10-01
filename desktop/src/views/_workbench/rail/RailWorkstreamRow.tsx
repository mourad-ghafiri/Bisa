/**
 * A workstream's row: the state mark first, where the eye lands; the name
 * (a branch in mono, the primary plain); then the facts as one quiet
 * cluster — *primary*, `+3 · −1` (how far the branch stands from its base,
 * `railFactsModel`; the tree's state is never a word here — that is the
 * Git tab's mark), *no checkout*, who is here when the row is folded (a
 * glyph per harness, one per shell), the ports — and `⋯` on hover. The pill is accent for the current row or one waiting on you,
 * danger for a failed one. No line under it: folded, the glyphs and the
 * mark say who is here; open, the session rows do.
 */

import { Chip, ContextMenu, ICON, SessionMark, TextInput, Tooltip, cn, harnessMark } from "../../../ui";
import type { MenuItem, TreeRowState } from "../../../ui";
import { PortChips } from "../PulseLine";
import type { WorkstreamPort } from "../portsModel.mjs";
import type { RailRow as RailModelRow } from "../projectRailModel.mjs";
import { RailActions, RailMenu } from "./RailActions";
import { RailRow } from "./RailRow";
import { factsWords, workstreamFacts } from "../railFactsModel.mjs";
import { rowTreatment } from "../railStyleModel.mjs";
import type { RailAttention } from "../railStyleModel.mjs";
import { RailGlyph, RailTwisty } from "./RailTwisty";
import { t } from "../../../i18n/l10n.mjs";

type WorkstreamRow = Extract<RailModelRow, { kind: "workstream" }>;

/** The pill's ink from the row's loudest word. */
function attentionOf(row: WorkstreamRow): RailAttention {
  if (row.pulse?.word === "waiting") return "accent";
  if (row.pulse?.word === "failed") return "danger";
  return "none";
}

export function RailWorkstreamRow({
  row,
  rs,
  menu,
  harnessLabels,
  portActions,
  editing,
  onToggle,
  onOpen,
  onRename,
}: {
  row: WorkstreamRow;
  rs: TreeRowState;
  menu: MenuItem[];
  harnessLabels: Record<string, string>;
  portActions: { openPort: (port: number) => void; stopPort: (p: WorkstreamPort) => void };
  editing: { value: string; onChange: (v: string) => void; onCommit: () => void; onCancel: () => void } | null;
  onToggle: () => void;
  onOpen: () => void;
  onRename: () => void;
}) {
  const facts = factsWords(workstreamFacts(row.status));
  const marks = t("workbench-rail-project-row-agents-working-shells", { agents: row.activity.counts.agents, working: row.activity.counts.working, live: row.activity.counts.live });
  const treatment = rowTreatment({ kind: "workstream", current: row.current, attention: attentionOf(row), exists: row.exists });
  return (
    <ContextMenu items={menu} className="block">
      <RailRow rs={rs} treatment={treatment} onClick={onOpen} onDoubleClick={onRename}>
        <RailTwisty open={row.sessions > 0 ? !row.collapsed : null} onToggle={onToggle} />
        <RailGlyph>
          <SessionMark state={row.activity.state} title={marks} />
        </RailGlyph>
        {editing ? (
          <TextInput
            autoFocus
            value={editing.value}
            placeholder={row.label}
            className="h-6 min-w-0 flex-1 text-xs"
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => editing.onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") editing.onCommit();
              if (e.key === "Escape") editing.onCancel();
            }}
            onBlur={editing.onCommit}
          />
        ) : (
          <span className={cn("min-w-0 flex-1 truncate text-xs", !row.primary && "font-mono")}>{row.label}</span>
        )}
        <span className="flex min-w-0 shrink-0 items-center gap-1.5 text-3xs text-text-dim">
          {row.primary && (
            <Tooltip label={t("workbench-rail-workstream-row-project-s-own-root-remove-project")}>
              <span>
                <Chip tone="quiet" className="border-transparent">{t("workbench-rail-workstream-row-primary")}</Chip>
              </span>
            </Tooltip>
          )}
          {facts.text !== "" && (
            <span className="tnum" title={facts.title}>
              {facts.text}
            </span>
          )}
          {!row.exists && <Chip tone="warn">{t("workbench-rail-workstream-row-no-checkout")}</Chip>}
          {row.collapsed &&
            row.harnesses.map((h) => {
              const Glyph = harnessMark(h);
              return (
                <Tooltip key={`h:${h}`} label={harnessLabels[h] ?? h}>
                  <span className="shrink-0 opacity-80">
                    <Glyph size={12} aria-hidden />
                  </span>
                </Tooltip>
              );
            })}
          {row.collapsed &&
            row.shellGlyphs.map((sh, i) => (
              <Tooltip key={`sh:${i}`} label={sh.live ? t("workbench-rail-workstream-row-shell-live") : t("workbench-rail-workstream-row-shell-exited")}>
                <span className={cn("shrink-0", sh.live ? "opacity-80" : "opacity-40")}>
                  <ICON.shell size={12} aria-hidden />
                </span>
              </Tooltip>
            ))}
          {/* The ports are the workstream's: on its own row, folded or open. */}
          {row.pulse && row.pulse.ports.length > 0 && <PortChips ports={row.pulse.ports} harnessLabels={harnessLabels} {...portActions} />}
        </span>
        <RailActions>
          <RailMenu items={menu} label={t("workbench-rail-heading-row-actions", { row: row.label })} />
        </RailActions>
      </RailRow>
    </ContextMenu>
  );
}
