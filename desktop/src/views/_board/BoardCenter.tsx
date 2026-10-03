/**
 * The Board (ide/16), as the Project IDE's third centre: every workstream a
 * card in five columns — Backlog, Todo, Doing, Done, Archived — with a due
 * date, drag and drop between and along columns, and one click into the
 * workstream or its project. The workbench draws it in Board Mode, between
 * the project rail and the right panel, the way it draws documents in
 * Project Mode and the conversation in Agent Mode (`Workbench.tsx`).
 *
 * A view, not a place work happens: a card's column is where a person put
 * it (the record's `board`), never the workstream's lifecycle state; an
 * unplaced card follows the lifecycle by `boardModel.columnShown`. The
 * facts come from the rail's own four feeds — the workstream list, the one
 * status store (`workstreamStatusStore`: one poll and one list of facts to
 * re-read on, whoever shows it), the sessions and the terminals, the ports
 * — so a card and the rail's row for the same workstream always agree. A drag is shown as
 * it goes: the card hovering another column is moved there at once
 * (`optimisticMove`, a preview beside the pending move), the ghost under
 * the pointer is the card itself (`useDragGhost`), and a drop settles into
 * the room the preview made. The Board stays
 * workspace-wide — every project's workstreams — narrowed to what the
 * person selected in the rail (`railSelectionStore`: a group's projects,
 * or one project) with *All* as the door back; the root is not a filter.
 * The rules live in `boardModel.mjs`; this file is paint. *New
 * workstream…* is the workbench's own dialog (`NEW_WORKSTREAM`), so the
 * Board holds no second one.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, openExternal } from "../../api";
import { navigate } from "../../router";
import { useClock } from "../../shell/clock";
import { usePorts } from "../../shell/portsStore";
import { useSessions } from "../../shell/sessionsStore";
import { boolOf, numberOf } from "../../shell/settingsModel.mjs";
import { NEW_WORKSTREAM, fire } from "../../shell/shortcuts";
import type { NewWorkstreamRequest } from "../../shell/shortcuts";
import { useHarnessLabels } from "../../shell/useHarnesses";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { useViewScroll } from "../../shell/useViewScroll";
import { useViewState } from "../../shell/viewMemoryStore";
import { textValue } from "../../shell/viewValuesModel.mjs";
import { openTerminalIn, useTerminals } from "../../shell/useTerminals";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { STATUS_POLL_MS, useWorkstreamStatuses } from "../../shell/workstreamStatusStore";
import { stopPort } from "../../terminal/session";
import type { MenuItem } from "../../ui";
import { Button, ConfirmDialog, Dialog, EmptyState, ICON, Switch, TextInput, Tooltip, failureText, harnessMark, useActiveDrag, useDragGhost, useToast } from "../../ui";
import type { SortableHandle } from "../../ui";
import { closeWorkstream } from "../_work/closeWorkstream";
import { terminationConsent, terminationCounts, terminationWords } from "../_work/closeWorkstreamModel.mjs";
import { RenameWorkstreamField } from "../_work/RenameWorkstream";
import { cardTitle } from "../_work/workstreamCardModel.mjs";
import { CommandHint } from "../../shell/CommandHint";
import { attributePorts, portUrl, portsOf, stopPrompt } from "../_workbench/portsModel.mjs";
import type { WorkstreamPort } from "../_workbench/portsModel.mjs";
import { workstreamMenuSpec } from "../_workbench/railMenuModel.mjs";
import { boardScope } from "../_workbench/railSelectionModel.mjs";
import { clearRailSelection, useRailSelection } from "../_workbench/railSelectionStore";
import { pulseOf } from "../_workbench/workstreamPulseModel.mjs";
import { workstreamSessionRows } from "../_workbench/workstreamSessionsModel.mjs";
import { BoardCard } from "./BoardCard";
import { BoardColumnView } from "./BoardColumnView";
import { DueDialog } from "./DueDialog";
import { COLUMNS, COLUMN_LABEL, boardRows, canMoveTo, closeWords, countWords, lastActivity, optimisticMove, placeBody, placeIndex, statusIndex, todayKey, type BoardRow, type Column } from "./boardModel.mjs";
import { BOARD_ARCHIVED_KEY, BOARD_DEFAULTS, BOARD_DUE_SOON_KEY, BOARD_WIP_KEY } from "./boardSettings.mjs";
import { setBoardArchived, toggleColumnFolded, useBoardView } from "./boardStore";
import { t as tr } from "../../i18n/l10n.mjs";

/** How long a card a drop just put here wears its wash — `--motion-slow`'s order. */
const LANDED_MS = 700;
/** The ghost is drawn by nothing sortable: no ref, no listeners. */
const GHOST_HANDLE: SortableHandle = { ref: () => undefined, props: {}, style: undefined, dragging: false };

export function BoardCenter({
  place,
}: {
  /** Where the Board keeps how it stood — its search, where it was scrolled: one place, whatever root it is opened from (`BOARD_PLACE`). */
  place: string;
}) {
  const ws = useWorkspace();
  const toast = useToast();
  const { resolved } = useResolvedSettings(null);
  const soonDays = numberOf(resolved, BOARD_DUE_SOON_KEY, BOARD_DEFAULTS.dueSoonDays, { min: 1, max: 30 });
  const wipLimit = numberOf(resolved, BOARD_WIP_KEY, BOARD_DEFAULTS.wipLimit, { min: 0, max: 50 });
  const archivedDefault = boolOf(resolved, BOARD_ARCHIVED_KEY, BOARD_DEFAULTS.showArchived);
  const view = useBoardView();
  const archived = view.archived ?? archivedDefault;

  // The four feeds the rail assembles, and nothing per card.
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  const scannedPorts = usePorts();
  const ports = useMemo(() => attributePorts(scannedPorts, terminals, sessions), [scannedPorts, terminals, sessions]);
  const harnessLabels = useHarnessLabels();
  // Every workstream's status, from the store the rail reads: a card's state
  // chip and its column follow the record the node moved — a commit from
  // Git › Changes as the workstream's own — and never a rule of the Board's.
  const statuses = useWorkstreamStatuses();
  const clockTick = useClock(STATUS_POLL_MS);
  // `clockTick` is the signal: the clock's beat is what makes *now* worth reading again.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const now = useMemo(() => Date.now(), [clockTick]);
  const today = todayKey(now);
  const statusById = useMemo(() => statusIndex(statuses), [statuses]);

  // The search and the scroll are how the Board stood: leaving it keeps them, and a restart.
  const [needle, setNeedle] = useViewState(place, "board.q", "", textValue);
  const board = useRef<HTMLElement>(null);
  useViewScroll(board, place);
  // What the rail selected — a group's projects, or one project — is the scope.
  const selection = useRailSelection();
  const scope = useMemo(() => boardScope(selection, (id) => ws.projects.find((p) => p.project.id === id)?.project.name), [selection, ws.projects]);
  const laid = useMemo(
    () => boardRows({ refs: ws.workstreams, statuses: statusById, needle, projects: scope.projects, archived }),
    [ws.workstreams, statusById, needle, scope.projects, archived],
  );
  // The columns as painted: the node's answer, with a move applied at once —
  // as a *preview* while a card is still in the air, as *pending* while the
  // node writes a drop. The workspace's next refresh replaces either.
  const [override, setOverride] = useState<{ base: typeof laid.columns; columns: typeof laid.columns; kind: "preview" | "pending" } | null>(null);
  const columns = override && override.base === laid.columns ? override.columns : laid.columns;
  // A drag that ends anywhere but on a column takes its preview with it.
  const dragging = useActiveDrag();
  useEffect(() => {
    if (dragging === null) setOverride((o) => (o?.kind === "preview" ? null : o));
  }, [dragging]);
  /** The card just dropped, wearing its wash for a beat. */
  const [landed, setLanded] = useState<string | null>(null);
  useEffect(() => {
    if (landed === null) return;
    const t = window.setTimeout(() => setLanded(null), LANDED_MS);
    return () => window.clearTimeout(t);
  }, [landed]);

  /** A card hovering a column: shown there now, where it would land. */
  const hover = useCallback(
    (id: string, column: Column, index: number) => {
      const next = optimisticMove(columns, id, column, index);
      if (next === columns) return;
      setOverride({ base: laid.columns, columns: next, kind: "preview" });
    },
    [columns, laid.columns],
  );

  const placeCard = useCallback(
    async (id: string, column: Column, index: number) => {
      const next = optimisticMove(columns, id, column, index);
      setOverride({ base: laid.columns, columns: next, kind: "pending" });
      setLanded(id);
      // The slot as the node counts it: among the column's placed cards, shown here or not.
      const place = placeBody(column, placeIndex({ refs: ws.workstreams, shown: columns, id, column, index }));
      try {
        await api.placeWorkstreamCard(id, place);
        ws.refresh();
      } catch (e) {
        setOverride(null);
        toast.error(failureText("board", "board-center-failed", e));
      }
    },
    [columns, laid.columns, ws, toast],
  );

  // The dialogs: a due date, a rename, a close, a port to stop.
  const [due, setDue] = useState<BoardRow | null>(null);
  const [renaming, setRenaming] = useState<BoardRow | null>(null);
  const [closing, setClosing] = useState<BoardRow | null>(null);
  const [stoppingPort, setStoppingPort] = useState<WorkstreamPort | null>(null);

  const projects = ws.projects.map((p) => p.project);
  // The workbench's dialog, on the one project selected — else the root's.
  const openWorkstream = () => fire(NEW_WORKSTREAM, { pid: selection?.kind === "project" ? selection.id : undefined } satisfies NewWorkstreamRequest);

  const menuFor = (row: BoardRow): MenuItem[] => {
    const target = { scope: "workstream" as const, id: row.id, label: row.title };
    const act: Record<string, () => void> = {
      open: () => navigate({ name: "workbench", scope: "workstream", id: row.id }),
      rename: () => setRenaming(row),
      "new-shell": () => openTerminalIn(target),
      diff: () => navigate({ name: "workbench", scope: "workstream", id: row.id }, { doc: "diff" }),
      close: () => setClosing(row),
    };
    const icon: Record<string, MenuItem["icon"]> = { open: ICON.workstream, rename: ICON.edit, "new-shell": ICON.harness, close: ICON.delete };
    const own = workstreamMenuSpec({ primary: row.primary, exists: row.ref.exists, harnesses: [], rename: true, close: true }).map((item) => ({
      label: item.label,
      icon: item.harness ? harnessMark(item.harness) : icon[item.id],
      danger: item.danger,
      disabled: item.disabled,
      separatorBefore: item.separatorBefore,
      onSelect: act[item.id] ?? (() => undefined),
    }));
    const board: MenuItem[] = [
      { label: tr("board-board-center-open-project"), icon: ICON.project, separatorBefore: true, onSelect: () => navigate({ name: "workbench", scope: "workstream", id: row.project.id }) },
      { label: row.due ? tr("board-board-center-change-due-date") : tr("board-board-center-set-due-date"), icon: ICON.waiting, onSelect: () => setDue(row) },
      ...(row.due ? [{ label: tr("board-board-center-clear-due-date"), onSelect: () => void patch(row, { due: null }) }] : []),
      { label: row.workstream.pinned ? tr("board-board-center-unpin") : tr("board-board-center-pin"), icon: ICON.pin, onSelect: () => void patch(row, { pinned: !row.workstream.pinned }) },
      ...COLUMNS.map((c, i) => ({
        label: tr("board-board-center-move", { c: COLUMN_LABEL[c] }),
        separatorBefore: i === 0,
        disabled: !canMoveTo(row, c),
        // Last among what the column shows — the same act as a drop on its well.
        onSelect: () => void placeCard(row.id, c, columns[c].length),
      })),
    ];
    return [...own, ...board];
  };

  const patch = async (row: BoardRow, body: Parameters<typeof api.patchWorkstream>[1]) => {
    try {
      await api.patchWorkstream(row.id, body);
      ws.refresh();
    } catch (e) {
      toast.error(failureText("board", "board-center-failed", e));
    }
  };

  const closeCard = async (row: BoardRow) => {
    try {
      const r = await closeWorkstream(row.id, { tree: false });
      const ended = terminationWords(r.terminated);
      toast.ok(tr("board-board-center-closed-record-board-under-archived", { title: row.title, ended: ended ? ended[0].toUpperCase() : "", ended2: ended ? ended.slice(1) : "", flag: ended ? "yes" : "no" }));
      ws.refresh();
    } catch (e) {
      toast.error(failureText("board", "board-center-failed", e));
    } finally {
      setClosing(null);
    }
  };

  const portActions = {
    openPort: (port: number) => void openExternal(portUrl(port)).catch((e: unknown) => toast.error(failureText("board", "board-center-failed", e))),
    stopPort: (p: WorkstreamPort) => setStoppingPort(p),
  };

  /** What a card is drawn from — the column's and the ghost's alike, so the two never drift. */
  const cardProps = (row: BoardRow) => ({
    row,
    sessions: workstreamSessionRows(sessions, terminals, row.id),
    pulse: pulseOf({ sessions, terminals, workstream: row.id, now: Math.floor(now / 1000), ports: portsOf(ports, row.id) }),
    ports: portsOf(ports, row.id),
    harnessLabels,
    today,
    soonDays,
    lastActivity: lastActivity(sessions, row, now),
    items: menuFor(row),
    onOpen: () => navigate({ name: "workbench", scope: "workstream", id: row.id }),
    onOpenProject: () => navigate({ name: "workbench", scope: "workstream", id: row.project.id }),
    openPort: portActions.openPort,
    stopPort: portActions.stopPort,
  });
  const rowOf = (id: string): BoardRow | null => {
    for (const c of COLUMNS) {
      const hit = columns[c].find((r) => r.id === id);
      if (hit) return hit;
    }
    return null;
  };
  // The ghost under the pointer is the card itself, and it settles into the
  // room the preview made when the drag ends.
  useDragGhost(
    "workstream-card",
    (data) => {
      const row = data.type === "workstream-card" ? rowOf(data.id) : null;
      return row ? <BoardCard {...cardProps(row)} handle={GHOST_HANDLE} ghost /> : null;
    },
    { settle: true },
  );

  const renderCard = (row: BoardRow, handle: Parameters<Parameters<typeof BoardColumnView>[0]["children"]>[1]) => (
    <div key={row.id} className="pt-2">
      <BoardCard {...cardProps(row)} handle={handle} landed={landed === row.id} />
    </div>
  );

  const shownColumns = COLUMNS.filter((c) => c !== "archived" || archived);
  const shownCount = laid.total - laid.hidden;

  return (
    <section ref={board} aria-label={tr("board-board-center-board")} className="flex min-h-0 min-w-0 flex-1 flex-col">
      {/* One bar: the filters, what they leave, and the door to a new card. */}
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-hairline px-4 py-2">
        {/* The scope: what the rail selected, and the door back to All. */}
        <span
          aria-label={tr("board-board-center-board-scope")}
          // A narrowed scope is where you are standing, not something waiting on you: the selected ground.
          className={`inline-flex h-7 max-w-64 items-center gap-1 rounded-control px-2 text-2xs ${scope.all ? "text-text-dim" : "bg-selected text-text"}`}
          title={scope.all ? tr("board-board-center-every-workstream-pick-project-group-rail") : tr("board-board-center-rail-s-selection-shows-every-workstream")}
        >
          <ICON.project size={12} aria-hidden className="shrink-0" />
          <span className="min-w-0 truncate">{scope.words}</span>
          {!scope.all && (
            <Tooltip label={tr("board-board-center-all-workstreams")}>
              <button type="button" aria-label={tr("board-board-center-show-all-workstreams")} onClick={clearRailSelection} className="anim ml-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded hover:bg-surface-2">
                <ICON.close size={10} aria-hidden />
              </button>
            </Tooltip>
          )}
        </span>
        <TextInput value={needle} onChange={(e) => setNeedle(e.target.value)} placeholder={tr("board-board-center-find-card-title-branch-project-note")} aria-label={tr("board-board-center-find-card")} className="h-7 w-64 text-2xs" />
        <Switch checked={archived} onChange={setBoardArchived} label={tr("board-board-center-archived")} />
        <span className="flex-1" />
        <span className="text-2xs text-text-dim" title={tr("board-board-center-card-s-column-yours-branch-still")}>
          {countWords(shownCount, laid.total)}
        </span>
        {ws.workstreams.length > 0 && (
          <Button size="sm" onClick={openWorkstream} disabled={projects.length === 0} disabledReason={tr("board-board-center-add-project-first")}>
            <ICON.add size={12} aria-hidden />{tr("board-board-center-new-workstream")}<CommandHint id="new_workstream" />
          </Button>
        )}
      </div>

      {ws.workstreams.length === 0 ? (
        // The door is the empty board's own, so the bar above draws none while it shows.
        <EmptyState
          icon={ICON.board}
          title={tr("board-board-center-no-workstreams-yet")}
          hint={tr("board-board-center-project-s-root-first-workstream-open")}
          action={
            <Button size="sm" variant="primary" onClick={openWorkstream} disabled={projects.length === 0} disabledReason={tr("board-board-center-add-project-first")}>
              <ICON.add size={12} aria-hidden />
              {tr("board-board-center-new-workstream")}
            </Button>
          }
        />
      ) : (
        <div data-scroll-keep="board" className="flex min-h-0 flex-1 items-stretch gap-3 overflow-x-auto p-4">
          {shownColumns.map((c) => (
            <BoardColumnView key={c} column={c} rows={columns[c]} folded={view.collapsed.includes(c)} onToggleFold={() => toggleColumnFolded(c)} today={today} soonDays={soonDays} wipLimit={wipLimit} onPlace={(id, col, i) => void placeCard(id, col, i)} onHover={hover}>
              {renderCard}
            </BoardColumnView>
          ))}
        </div>
      )}

      {due && <DueDialog open wid={due.id} title={due.title} due={due.due} onClose={() => setDue(null)} onSaved={() => ws.refresh()} />}
      <Dialog open={renaming !== null} onClose={() => setRenaming(null)} title={tr("board-board-center-rename-workstream")} description={tr("board-board-center-label-yours-branch-still-branch-empty")} width="max-w-sm">
        {renaming && (
          <RenameWorkstreamField
            wid={renaming.id}
            name={renaming.workstream.name ?? null}
            placeholder={cardTitle({ ...renaming.workstream, name: null }, renaming.status)}
            onDone={() => {
              setRenaming(null);
              ws.refresh();
            }}
          />
        )}
      </Dialog>
      <ConfirmDialog
        open={closing !== null}
        onClose={() => setClosing(null)}
        onConfirm={() => closing && void closeCard(closing)}
        title={closing ? tr("board-board-center-close-2", { title: closing.title }) : tr("board-board-center-close")}
        body={closeWords(closing ? terminationConsent(terminationCounts(sessions, terminals, closing.id)) : null)}
        confirmLabel={tr("board-board-center-close")}
      />
      <ConfirmDialog
        open={stoppingPort !== null}
        onClose={() => setStoppingPort(null)}
        onConfirm={() => {
          const p = stoppingPort;
          setStoppingPort(null);
          if (p) void stopPort(p.pid, p.port).catch((e: unknown) => toast.error(failureText("board", "board-center-failed", e)));
        }}
        title={tr("board-board-center-stop-process")}
        body={stoppingPort ? stopPrompt(stoppingPort) : ""}
        confirmLabel={tr("board-board-center-stop")}
        danger
      />
    </section>
  );
}
