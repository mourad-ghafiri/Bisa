/**
 * The row above the Changes tree (ide/04): the layout switch — the changed
 * files as folders or as a flat list, remembered once for every checkout —
 * the **filter** beside it (a compact menu: *All · Staged · Unstaged ·
 * Tracked · Untracked · Modified*, each with its count, the one on wearing
 * a check, a word with nothing to show off; it narrows the rows and never
 * the verbs), *Collapse all* for the folders, and **Stage all** with the other scopes
 * behind its `▾`: *Stage tracked* (the modified tracked files alone — `git
 * add -u`'s meaning), *Stage untracked*, *Unstage all* — and, last and
 * apart, the two throw-aways, each asked first: *Discard all changes…* over
 * every working-tree change, and *Delete all untracked files…* over every
 * file git has never seen, through the IDE's disposal. The items are
 * `bulkMenu`'s, every scope's paths from one place (`stageScopes`), each off
 * at zero; this binds the ids to the panel's acts. A conflicted path is in
 * no stage scope — staging it would mark it resolved.
 */

import { Button, ICON, Menu, SegmentedControl, Tooltip } from "../../ui";
import type { ChangesFilter, ChangesLayout } from "../_workbench/rightPanelModel.mjs";
import { CHANGE_FILTERS, CHANGE_FILTER_LABEL, CHANGES_LAYOUTS, CHANGES_LAYOUT_LABEL } from "../_workbench/rightPanelModel.mjs";
import { filterWords } from "./changesFilterModel.mjs";
import { bulkMenu } from "./changesBulkModel.mjs";
import type { BulkVerb } from "./changesBulkModel.mjs";
import type { StageScopes } from "./gitFiles.mjs";
import { bulkLabel } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

const LAYOUT_ICON = { tree: ICON.tree, list: ICON.list } as const;

export function ChangesToolbar({
  layout,
  onLayout,
  filter,
  counts,
  onFilter,
  canCollapse,
  onCollapseAll,
  scopes,
  busy,
  onStage,
  onUnstage,
  onDiscard,
  onDeleteUntracked,
}: {
  layout: ChangesLayout;
  onLayout: (layout: ChangesLayout) => void;
  /** Which changed files the tree draws. */
  filter: ChangesFilter;
  /** How many rows each word would keep (`filterCounts`). */
  counts: Record<ChangesFilter, number>;
  onFilter: (filter: ChangesFilter) => void;
  /** Some folder is open to fold. */
  canCollapse: boolean;
  onCollapseAll: () => void;
  scopes: StageScopes;
  busy: boolean;
  onStage: (paths: string[]) => void;
  onUnstage: (paths: string[]) => void;
  /** Asked first, by the panel's discard confirmation. */
  onDiscard: (paths: string[]) => void;
  /** Every untracked file — asked first, by the panel's delete confirmation, through the IDE's disposal. */
  onDeleteUntracked: (paths: string[]) => void;
}) {
  const { all, tracked, untracked } = scopes;
  const reach = [tracked.count > 0 && t("work-changes-toolbar-modified", { tracked: tracked.count }), untracked.count > 0 && t("work-changes-toolbar-untracked", { untracked: untracked.count })].filter(Boolean).join(", ");
  const filterItems = CHANGE_FILTERS.map((f) => ({
    label: `${CHANGE_FILTER_LABEL[f]} (${counts[f]})`,
    icon: f === filter ? ICON.check : undefined,
    // A word with nothing to show is off — except the one on, which stays a door back.
    disabled: f !== "all" && f !== filter && counts[f] === 0,
    onSelect: () => onFilter(f),
  }));
  const act = (id: BulkVerb, paths: string[]) => {
    switch (id) {
      case "stage_tracked":
      case "stage_untracked":
        return onStage(paths);
      case "unstage_all":
        return onUnstage(paths);
      case "discard_all":
        return onDiscard(paths);
      case "delete_untracked":
        return onDeleteUntracked(paths);
    }
  };
  const scopeItems = bulkMenu(scopes).map((item) => ({
    label: item.label,
    disabled: busy || item.disabled,
    separatorBefore: item.separatorBefore,
    danger: item.danger,
    onSelect: () => act(item.id, item.paths),
  }));
  return (
    <div className="flex items-center gap-1.5">
      <SegmentedControl
        label={t("work-changes-toolbar-changes-layout")}
        size="sm"
        iconOnly
        value={layout}
        onChange={onLayout}
        options={CHANGES_LAYOUTS.map((l) => ({ id: l, label: CHANGES_LAYOUT_LABEL[l], icon: LAYOUT_ICON[l] }))}
      />
      <Menu
        label={t("work-changes-toolbar-which-changes-show")}
        items={filterItems}
        align="start"
        trigger={
          <span
            title={t("work-changes-toolbar-which-changes-show")}
            className={`anim inline-flex h-6 items-center gap-1 rounded-control px-1.5 text-2xs hover:bg-surface-2 hover:text-text ${filter === "all" ? "text-text-dim" : "bg-accent-soft text-accent-ink"}`}
          >
            <ICON.filter size={11} aria-hidden />
            {filterWords(filter, counts[filter])}
          </span>
        }
      />
      {layout === "tree" && (
        <Tooltip label={t("work-changes-toolbar-collapse-every-folder")}>
          <span className="inline-flex">
            <Button size="icon" variant="ghost" className="h-6 w-6" aria-label={t("work-changes-toolbar-collapse-every-folder")} disabled={!canCollapse} onClick={onCollapseAll}>
              <ICON.collapseAll size={12} aria-hidden />
            </Button>
          </span>
        </Tooltip>
      )}
      <span className="flex-1" />
      <Tooltip label={all.count > 0 ? t("work-changes-toolbar-stage-every-change-git-has-not", { reach }) : t("work-changes-toolbar-nothing-stage")}>
        <span className="inline-flex">
          <Button size="sm" variant="default" className="h-6" disabled={busy || all.count === 0} onClick={() => onStage(all.paths)}>
            <ICON.stage size={12} aria-hidden />
            {bulkLabel("stage")}
          </Button>
        </span>
      </Tooltip>
      <Menu
        label={t("work-changes-toolbar-other-ways-stage-discard-all-delete")}
        items={scopeItems}
        trigger={
          <span aria-label={t("work-changes-toolbar-other-ways-stage-discard-all-delete")} className="anim flex h-6 w-5 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
            <ICON.expanded size={12} aria-hidden />
          </span>
        }
      />
    </div>
  );
}
