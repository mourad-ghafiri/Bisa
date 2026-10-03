/**
 * A section of the rail — a project group, a goal, a workflow: a quiet
 * header in sentence case (the label as the person or the goal wrote it),
 * with its photo or glyph, its label and its count as a quiet badge,
 * a section's space above it so sections read apart (`headingSpacing`) and
 * a softer hover, since a heading only folds. A real group renames on
 * double-click and carries its `⋯`; a goal's or workflow's heading does not.
 */

import { Avatar, ContextMenu, CountBadge, ICON, usePhotoThumb } from "../../../ui";
import type { MenuItem, TreeRowState } from "../../../ui";
import type { RailRow as RailModelRow } from "../projectRailModel.mjs";
import { avatarSize, headingSpacing, rowTreatment } from "../railStyleModel.mjs";
import { RailActions, RailMenu } from "./RailActions";
import { RailRow } from "./RailRow";
import { RailGlyph, RailTwisty } from "./RailTwisty";
import { t } from "../../../i18n/l10n.mjs";

type Heading = Extract<RailModelRow, { kind: "group" | "goal" }>;

export function RailHeadingRow({
  row,
  rs,
  first,
  photo,
  editable,
  menu,
  groupMenu,
  onToggle,
  onRename,
}: {
  row: Heading;
  rs: TreeRowState;
  /** The first row of the list wears no space above. */
  first: boolean;
  /** The group's photo, when it has one. */
  photo: { sha256: string } | null;
  /** A real, named project group: renamable, with a menu of its own. */
  editable: boolean;
  /** The context menu — the same for right-click and `⋯`. */
  menu: MenuItem[];
  groupMenu: MenuItem[];
  onToggle: () => void;
  onRename: () => void;
}) {
  const treatment = rowTreatment({ kind: row.kind });
  const thumb = usePhotoThumb(photo?.sha256 ?? null);
  return (
    <ContextMenu items={menu}>
      <RailRow rs={rs} treatment={treatment} className={headingSpacing(first) === "section" ? "mt-3" : undefined} onDoubleClick={editable ? onRename : undefined}>
        <RailTwisty open={!row.collapsed} onToggle={onToggle} />
        <RailGlyph className="text-text-dim">
          {photo ? (
            <Avatar id={row.id} name={row.label} url={thumb} size={avatarSize(row.kind)} className="shrink-0 rounded" />
          ) : row.kind === "goal" ? (
            <ICON.goal size={12} aria-hidden />
          ) : (
            <ICON.folder size={12} aria-hidden />
          )}
        </RailGlyph>
        <span className="min-w-0 flex-1 truncate text-2xs font-semibold">{row.label}</span>
        {editable && (
          <RailActions>
            <RailMenu items={groupMenu} label={t("workbench-rail-heading-row-actions", { row: row.label })} />
          </RailActions>
        )}
        <CountBadge count={row.count} tone="quiet" title={t("workbench-rail-heading-row-project-projects", { row: row.count })} />
      </RailRow>
    </ContextMenu>
  );
}
