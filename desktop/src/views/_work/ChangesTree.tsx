/**
 * The Changes list itself (ide/04): every changed file once, in the
 * project's own tree — the folders as the explorer draws them — or as a
 * flat list, as one kit tree (`TreeList`, ide/03) with one cursor and one
 * tab stop. The rows are `gitTreeModel.mjs`'s; this file paints two kinds
 * and answers the keys the tree has no answer for.
 *
 * **A folder row** is the explorer's: the chevron, the folder glyph, the
 * name, the count of changed files under it; it reveals the verbs that have
 * files to take, each over exactly those (`folderActions`) — *Stage 3*,
 * *Unstage 2*, *Discard changes…*, *Delete files…*; its click folds. **A
 * file row** wears the explorer's glyph, the name — its basename in the
 * tree, its path in the list — and its **standing** as chips, one per side
 * it has (`standingChips`): *added* in the accent for the staged side,
 * *modified* quiet for the unstaged, *untracked*, *conflict*. The chips are
 * the doors to the two patches: the row's body opens the primary side, a
 * chip its own, and the open side's chip is pressed. The verbs on hover and
 * a `⋮` come from the standing too (`rowActions`); reading a file never
 * changes the index.
 *
 * The tree is `unbounded`: the right panel is the scrollport, so the sticky
 * composer under the list keeps working, and a reveal scrolls the cursor
 * row into view through it. Keys: Enter selects a file or folds a folder,
 * Space stages what the cursor holds — or unstages a file staged alone —
 * Delete or Backspace asks to discard or delete it, Shift+F10 opens its
 * menu.
 */

import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import type { GitFileRow } from "../../types";
import { CURSOR_RING, Chip, ContextMenu, ICON, MoreMenu, Tooltip, TreeList, cn, fileIcon, useTokenPx } from "../../ui";
import type { MenuItem, TreeRowState } from "../../ui";
import type { ChangesLayout } from "../_workbench/rightPanelModel.mjs";
import type { GitSelection, GitSide, RowAction } from "./gitFiles.mjs";
import { isSelected, letterPair, rowActions } from "./gitFiles.mjs";
import type { ChangesRow, DirRow, FileRow } from "./gitTreeModel.mjs";
import { actOn, deleteVerb, dirRowTitle, folderActions, selectedIds, spaceVerb } from "./gitTreeModel.mjs";
import { standingChips } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The explorer's indent, so the two trees read alike. */
const INDENT = 14;
const ROW_HEIGHT_FALLBACK = 28;

export interface ChangesTreeProps {
  rows: readonly ChangesRow[];
  layout: ChangesLayout;
  cursor: string | null;
  onCursor: (id: string | null) => void;
  selection: GitSelection | null;
  busy: boolean;
  /** A delete whose disposal is still being read: its rows wait. */
  askingDelete: boolean;
  /** A folder opened or shut. */
  onFold: (id: string, open: boolean) => void;
  /** One side of a file's patch is wanted. */
  onSelect: (row: FileRow, side: GitSide) => void;
  onStage: (paths: string[]) => void;
  onUnstage: (paths: string[]) => void;
  /** Asked first; `under` names the folder in the question. */
  onDiscard: (paths: string[], under: string | null) => void;
  onDelete: (paths: string[], under: string | null) => void;
  menuFor: (row: FileRow) => MenuItem[];
  folderMenuFor: (row: DirRow) => MenuItem[];
  /** Bumped to bring the cursor row into view. */
  revealNonce: number;
}

/**
 * A row's verbs: a cluster that stops the row's click once, the glyphs
 * inside. It **overlays** the right end of the row on hover, focus and the
 * pressed row rather than reserving its width at rest — four glyphs and a
 * `⋮` on every row would squeeze the name column of a list that is meant to
 * fill its panel — and takes no pointer until it shows, so the name under
 * it stays clickable. The standing chips, which are the doors to the two
 * patches, never move.
 */
function Verbs({ actions, subject, disabled, act, children }: { actions: RowAction[]; subject: string; disabled: boolean; act: (id: RowAction["id"]) => void; children?: ReactNode }) {
  return (
    <span
      className="row-actions anim pointer-events-none absolute inset-y-0 right-0 flex items-center rounded-control bg-surface-2 pl-2 group-hover:pointer-events-auto group-focus-within:pointer-events-auto"
      onClick={(e) => e.stopPropagation()}
    >
      {actions.map((a) => {
        const Glyph = ICON[a.icon];
        return (
          <Tooltip key={a.id} label={a.label(subject)}>
            <button
              type="button"
              tabIndex={-1}
              disabled={disabled}
              onClick={() => act(a.id)}
              aria-label={a.label(subject)}
              className={cn("anim shrink-0 rounded px-1 py-0.5 text-text-dim disabled:opacity-45", a.tone === "danger" ? "hover:text-danger" : "hover:text-text")}
            >
              <Glyph size={13} aria-hidden />
            </button>
          </Tooltip>
        );
      })}
      {children}
    </span>
  );
}

/** The disclosure column: a chevron on a row that opens, its blank on a file. */
function Twisty({ open, onToggle }: { open: boolean | null; onToggle?: () => void }) {
  if (open === null) return <span aria-hidden className="w-4 shrink-0" />;
  const Glyph = open ? ICON.expanded : ICON.collapsed;
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-label={open ? t("work-changes-tree-collapse") : t("work-changes-tree-expand")}
      onClick={(e) => {
        e.stopPropagation();
        onToggle?.();
      }}
      className="anim flex h-4 w-4 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface hover:text-text"
    >
      <Glyph size={12} aria-hidden />
    </button>
  );
}

const ROW = "group anim relative mr-1 flex h-row-sm items-center gap-1 rounded-control pr-1";
/** The disclosure column is the tree's: a flat list has no folder to open, so its rows start at the edge. */
const twistyFor = (layout: ChangesLayout) => (layout === "tree" ? <Twisty open={null} /> : null);

export function ChangesTree(p: ChangesTreeProps) {
  const rowHeight = useTokenPx("--spacing-row-sm", ROW_HEIGHT_FALLBACK);
  const wrap = useRef<HTMLDivElement>(null);
  const selected = selectedIds(p.selection);

  // A reveal — the sync bar pointing at a conflicted file — scrolls the row
  // into view through the panel, the one scrollport there is.
  useEffect(() => {
    if (p.revealNonce === 0) return;
    wrap.current?.querySelector('[data-cursor="true"]')?.scrollIntoView({ block: "nearest" });
  }, [p.revealNonce]);

  const toggle = (row: DirRow) => p.onFold(row.id, !row.expanded);

  /** A verb over what the cursor holds — a file, or the files under a folder the verb applies to. */
  const verb = (id: RowAction["id"] | null, rowId: string) => {
    if (!id || p.busy) return;
    const target = actOn(p.rows, rowId, id);
    if (!target) return;
    if (id === "stage") p.onStage(target.paths);
    else if (id === "unstage") p.onUnstage(target.paths);
    else if (id === "discard") p.onDiscard(target.paths, target.under);
    else p.onDelete(target.paths, target.under);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const row = p.rows.find((r) => r.id === p.cursor);
    if (!row) return;
    switch (e.key) {
      case "Enter":
        if (row.kind === "file") p.onSelect(row, row.standing.primary);
        else toggle(row);
        break;
      case " ":
        verb(spaceVerb(row), row.id);
        break;
      case "Delete":
      case "Backspace": {
        const what = deleteVerb(row);
        if (!what) return;
        verb(what, row.id);
        break;
      }
      case "F10":
        if (!e.shiftKey) return;
        wrap.current?.querySelector('[data-cursor="true"]')?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  const renderDir = (row: DirRow, rs: TreeRowState) => {
    // The one group above the folders: the unmerged paths, wearing the warning.
    const group = row.group === "conflicted";
    const Glyph = group ? ICON.warn : row.expanded ? ICON.folderOpen : ICON.folder;
    return (
      <ContextMenu items={p.folderMenuFor(row)} className="block" selected={rs.cursor}>
        <div style={{ marginLeft: rs.indent }} className={cn(ROW, "cursor-pointer hover:bg-surface-2", rs.cursor && CURSOR_RING, group && "text-warn")} onMouseDown={() => p.onCursor(row.id)} onClick={() => toggle(row)} title={dirRowTitle(row)}>
          <Twisty open={row.expanded ?? false} onToggle={() => toggle(row)} />
          <Glyph size={13} aria-hidden className={cn("shrink-0", group ? "text-warn" : "text-text-dim")} />
          {/* The name and the count, with the verbs overlaid on their tail when wanted. */}
          <span className="relative flex min-w-0 flex-1 items-center gap-1">
            <span className="min-w-0 flex-1 truncate font-mono text-2xs text-text">{row.label}</span>
            <span className="tnum text-2xs text-text-dim/70">{row.count}</span>
            <Verbs actions={folderActions(row)} subject={row.path} disabled={p.busy || p.askingDelete} act={(id) => verb(id, row.id)}>
              <MoreMenu label={t("work-changes-tree-more", { row: row.path })} items={p.folderMenuFor(row)} />
            </Verbs>
          </span>
        </div>
      </ContextMenu>
    );
  };

  const renderFile = (row: FileRow, rs: TreeRowState) => {
    const f: GitFileRow = row.row;
    const full = f.old_path ? `${f.old_path} → ${f.path}` : f.path;
    const name = p.layout === "tree" && f.old_path ? `${basename(f.old_path)} → ${row.label}` : row.label;
    const Glyph = fileIcon(basename(f.path));
    return (
      <ContextMenu items={p.menuFor(row)} className="block" selected={rs.selected}>
        <div style={{ marginLeft: rs.indent }} className={cn(ROW, rs.cursor && CURSOR_RING)} onMouseDown={() => p.onCursor(row.id)}>
          {twistyFor(p.layout)}
          {/* The row body opens the primary side's patch; the chips open
              theirs; the trailing controls stage, discard or delete — acts
              apart, buttons apart, so reading a file can never change the
              index by accident. */}
          {/* The body and, overlaid on its tail when wanted, the verbs — so the
              name has the row's width at rest and the chips never move. */}
          <span className="relative flex h-full min-w-0 flex-1 items-center">
            <button type="button" tabIndex={-1} onClick={() => p.onSelect(row, row.standing.primary)} aria-pressed={rs.selected} className={cn("anim flex h-full min-w-0 flex-1 items-center gap-1.5 rounded-control px-1 text-left font-mono text-2xs hover:bg-surface-2", rs.selected ? "bg-selected text-text" : "text-text-dim")}>
              <Glyph size={13} aria-hidden className="shrink-0 text-text-dim" />
              <span className="min-w-0 flex-1 truncate text-text" title={full}>
                {name}
              </span>
            </button>
            <Verbs actions={rowActions(row.standing)} subject={f.path} disabled={p.busy || p.askingDelete} act={(id) => verb(id, row.id)}>
              <MoreMenu label={t("work-changes-tree-more-2", { f: f.path })} items={p.menuFor(row)} />
            </Verbs>
          </span>
          {/* The standing, a chip per side — each a button of its own, beside
              the body rather than inside it. */}
          <span className="flex shrink-0 items-center gap-1">
            {standingChips(f).map((chip) => {
              const open = isSelected(p.selection, f, chip.side);
              return (
                <Tooltip key={chip.id} label={`${chip.hint} · ${letterPair(f)}`}>
                  <button type="button" tabIndex={-1} aria-pressed={open} aria-label={t("work-changes-tree-open-patch", { word: chip.word, side: chip.side })} onClick={() => p.onSelect(row, chip.side)} className={cn("anim rounded-full", open && "ring-1 ring-text/50")}>
                    <Chip tone={chip.tone} className="font-sans">
                      {chip.word}
                    </Chip>
                  </button>
                </Tooltip>
              );
            })}
          </span>
        </div>
      </ContextMenu>
    );
  };

  return (
    <div ref={wrap}>
      <TreeList
        rows={p.rows}
        rowHeight={rowHeight}
        measure
        indent={INDENT}
        guides={p.layout === "tree"}
        unbounded
        cursor={p.cursor}
        selected={selected}
        onCursor={(id) => p.onCursor(id)}
        onAction={(a) => {
          const ids = a.kind === "expandAll" ? a.ids : [a.id];
          for (const id of ids) p.onFold(id, a.kind !== "collapse");
        }}
        onKeyDown={onKeyDown}
        label={t("work-changes-tree-changed-files")}
        className="rounded-control focus-visible:ring-1 focus-visible:ring-accent/50"
        render={(row, rs) => (row.kind === "dir" ? renderDir(row, rs) : renderFile(row, rs))}
      />
    </div>
  );
}

function basename(path: string): string {
  const i = path.lastIndexOf("/");
  return i === -1 ? path : path.slice(i + 1);
}
