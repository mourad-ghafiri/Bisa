/**
 * A section of the rail — a project group, a goal, a workflow: a quiet
 * header in sentence case (the label as the person or the goal wrote it),
 * with its photo or glyph, its label, who is working under it — one mark per
 * harness open in its projects, with a tip naming each one's place
 * (`railHarnessesModel`) — and its count as a quiet badge, a section's space
 * above it so sections read apart (`headingSpacing`) and a softer hover,
 * since a heading only folds. A real group renames on double-click and
 * carries its `⋯`; a goal's or workflow's heading does not.
 */

import { Avatar, ContextMenu, CountBadge, ICON, Tooltip, harnessMark, usePhotoThumb } from "../../../ui";
import type { MenuItem, TreeRowState } from "../../../ui";
import type { RailRow as RailModelRow } from "../projectRailModel.mjs";
import { headingHarnessWords, headingMarks } from "../railHarnessesModel.mjs";
import type { HeadingHarness } from "../railHarnessesModel.mjs";
import { avatarSize, headingSpacing, rowTreatment } from "../railStyleModel.mjs";
import { RailActions, RailMenu } from "./RailActions";
import { RailRow } from "./RailRow";
import { RailGlyph, RailTwisty } from "./RailTwisty";
import { t } from "../../../i18n/l10n.mjs";

type Heading = Extract<RailModelRow, { kind: "group" | "goal" }>;

/**
 * Who is working under the heading: one mark per distinct harness, quiet —
 * identity, never attention — and a tip of one line per harness naming its
 * project, its workstream and what it is doing. The same sentence is the
 * marks' accessible name, so the fact is read without a pointer too.
 */
function HeadingHarnesses({ harnesses, labels }: { harnesses: readonly HeadingHarness[]; labels: Record<string, string> }) {
  const words = headingHarnessWords(harnesses, labels);
  const more = words.more > 0 ? t("workbench-rail-harnesses-model-more", { more: words.more }) : null;
  const said = [`${words.title}: ${words.lines.map((l) => l.text).join("; ")}`, more].filter(Boolean).join(" ");
  return (
    <Tooltip
      label={
        <span className="flex flex-col gap-0.5">
          <span className="font-semibold">{words.title}</span>
          {words.lines.map((l, i) => {
            const Mark = harnessMark(l.harness);
            return (
              <span key={i} className="flex items-center gap-1">
                <Mark size={10} aria-hidden className="shrink-0" />
                <span className="min-w-0 truncate">{l.text}</span>
              </span>
            );
          })}
          {more && <span className="text-text-dim">{more}</span>}
        </span>
      }
    >
      <span role="img" aria-label={said} className="inline-flex shrink-0 items-center gap-0.5 opacity-80">
        {headingMarks(harnesses).map((h) => {
          const Mark = harnessMark(h);
          return <Mark key={h} size={12} aria-hidden />;
        })}
      </span>
    </Tooltip>
  );
}

export function RailHeadingRow({
  row,
  rs,
  first,
  photo,
  editable,
  menu,
  groupMenu,
  harnessLabels,
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
  /** What each harness is called, by id — the tip's word for a mark. */
  harnessLabels: Record<string, string>;
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
        {row.harnesses.length > 0 && <HeadingHarnesses harnesses={row.harnesses} labels={harnessLabels} />}
        <CountBadge count={row.count} tone="quiet" title={t("workbench-rail-heading-row-project-projects", { row: row.count })} />
      </RailRow>
    </ContextMenu>
  );
}
