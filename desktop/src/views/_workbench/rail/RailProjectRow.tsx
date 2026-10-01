/**
 * The project card — the rail's one tall row: the avatar in its ring, the
 * name set strong and never cut before the activity is, what the model says
 * of it (*missing*; a folded project's loudest line, in the row's slack after
 * the name), `+` and `⋯` on hover, the state mark at the edge in its own
 * column so every card's mark aligns. The current project wears the wash,
 * the pill and the accent ink (`railStyleModel.rowTreatment`).
 */

import { Avatar, Chip, ContextMenu, ICON, SessionMark, TextInput, cn, usePhotoThumb } from "../../../ui";
import type { MenuItem, TreeRowState } from "../../../ui";
import { PulseLine } from "../PulseLine";
import type { RailRow as RailModelRow } from "../projectRailModel.mjs";
import { avatarSize, rowTreatment } from "../railStyleModel.mjs";
import { RailAction, RailActions, RailMenu } from "./RailActions";
import { RailRow } from "./RailRow";
import { RailTwisty } from "./RailTwisty";
import { t } from "../../../i18n/l10n.mjs";

type ProjectRow = Extract<RailModelRow, { kind: "project" }>;

export function RailProjectRow({
  row,
  rs,
  menu,
  harnessLabels,
  showPulse,
  editing,
  onToggle,
  onOpen,
  onRename,
  onNewWorkstream,
}: {
  row: ProjectRow;
  rs: TreeRowState;
  menu: MenuItem[];
  harnessLabels: Record<string, string>;
  /** Whether the folded project's pulse is worth a line. */
  showPulse: boolean;
  /** The rename field, while this row is being renamed. */
  editing: { value: string; onChange: (v: string) => void; onCommit: () => void; onCancel: () => void } | null;
  onToggle: () => void;
  onOpen: () => void;
  onRename: () => void;
  onNewWorkstream: () => void;
}) {
  const p = row.project;
  // One small square per photo, shared with every other site drawing it (ide/14 §Photos).
  const photo = usePhotoThumb(p.photo?.sha256 ?? null);
  const marks = t("workbench-rail-project-row-agents-working-shells", { agents: row.activity.counts.agents, working: row.activity.counts.working, live: row.activity.counts.live });
  const treatment = rowTreatment({ kind: "project", current: row.current, archived: !!p.archived, exists: row.exists });
  return (
    <ContextMenu items={menu}>
      <RailRow rs={rs} tall treatment={treatment} onClick={onOpen} onDoubleClick={onRename}>
        <RailTwisty open={!row.collapsed} onToggle={onToggle} />
        <Avatar
          id={p.id}
          name={p.name}
          url={photo}
          size={avatarSize("project")}
          className={cn("shrink-0 rounded-control ring-1", row.current ? "ring-accent/40" : "ring-border/60")}
        />
        {editing ? (
          <TextInput
            autoFocus
            value={editing.value}
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
          <span className="min-w-0 truncate text-xs font-semibold" title={p.name}>
            {p.name}
          </span>
        )}
        {/* No origin chip here: the tab is the origin and the section
            header names the goal or workflow; About keeps the Origin line. */}
        {p.archived && (
          <Chip tone="quiet" icon={ICON.archive} title={t("workbench-rail-project-row-put-away-refused-attachment-step-every")}>{t("workbench-rail-project-row-archived")}</Chip>
        )}
        {!row.exists && <Chip tone="warn">{t("workbench-rail-project-row-missing")}</Chip>}
        {/* The row's slack: the folded project's loudest line when there is
            one, cut before the name ever is — the name is the row. */}
        <span className="flex min-w-0 flex-1 items-center">
          {row.pulse && showPulse && <PulseLine pulse={row.pulse} harnessLabels={harnessLabels} />}
        </span>
        <RailActions>
          {/* A put-away project takes no new work: the menu says why, the `+` is not offered. */}
          {!p.archived && (
            <RailAction label={t("workbench-rail-project-row-new-workstream", { p: p.name })} onClick={onNewWorkstream}>
              <ICON.add size={14} aria-hidden />
            </RailAction>
          )}
          <RailMenu items={menu} label={t("workbench-rail-project-row-actions", { p: p.name })} />
        </RailActions>
        <span className="flex w-4 shrink-0 items-center justify-center">
          <SessionMark state={row.activity.state} title={marks} />
        </span>
      </RailRow>
    </ContextMenu>
  );
}
