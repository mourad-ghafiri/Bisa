/**
 * The project rail (layout): every project, its workstreams and
 * the sessions standing in them — grouped, renameable, with a picture —
 * under three tabs that are three views over one list (D5). The rows come from
 * `projectRailModel.mjs`; this file is paint, menus and dialogs.
 */

import { readPref, switchPref, switchWord, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, inDesktopShell, openExternal, revealPath } from "../../api";
import { navigate, replace } from "../../router";
import { IMPORT_PROJECT, NEW_GOAL, NEW_WORKSTREAM, RENAME_SELECTION, fire, onDoor } from "../../shell/shortcuts";
import { setPendingProjects } from "../_work/NewGoalDialog";
import { openHarnessSession, openTerminalTab } from "../../shell/sessionDoors";
import { stopSession, useSessions } from "../../shell/sessionsStore";
import { stopWords } from "../../shell/sessionRosterModel.mjs";
import { usePorts, rescanPorts } from "../../shell/portsStore";
import { openTerminalIn, useTerminals, focusTerminalTab, closeExitedTerminalTabs, restartTerminalTab, reorderTerminalSessionTab } from "../../shell/useTerminals";
import { requestCloseOthers, requestCloseTerminal } from "../../shell/terminalCloseGuard";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { settingOf } from "../../shell/settingsModel.mjs";
import { renameGroup as renameGroupOrder } from "./railOrderModel.mjs";
import { isHeadless, railCanNest, railCanReorder, railDragOf, railDropVerdict } from "./railDragModel.mjs";
import { selectionOf } from "./railSelectionModel.mjs";
import { RAIL_PLACE } from "./idePlacesModel.mjs";
import { useViewScroll } from "../../shell/useViewScroll";
import { useViewState } from "../../shell/viewMemoryStore";
import { textValue } from "../../shell/viewValuesModel.mjs";
import { keepListedSelection, setRailSelection } from "./railSelectionStore";
import type { RailOrder } from "./railOrderModel.mjs";
import { groupPhoto, renameGroupMeta, withGroupPhoto } from "./railGroupsModel.mjs";
import type { RailGroups } from "./railGroupsModel.mjs";
import { stopPort } from "../../terminal/session";
import { attributePorts, portUrl, stopPrompt } from "./portsModel.mjs";
import type { WorkstreamPort } from "./portsModel.mjs";
import { useHarnessLabels, useLaunchableHarnesses } from "../../shell/useHarnesses";
import { agentRowMenuSpec, workstreamMenuSpec } from "./railMenuModel.mjs";
import { terminalTabMenu } from "./tabMenuModel.mjs";
import { useWorkspace } from "../../shell/useWorkspaceData";
import {
  Button,
  ConfirmDialog,
  CountBadge,
  Dialog,
  Field,
  ICON,
  Menu,
  failureText,
  harnessMark,
  Tabs,
  TextInput,
  Tooltip,
  Switch,
  TreeList,
  expandAll,
  toggleCollapsed,
  useCollapsedUnder,
  useToast,
  useTokenPx,
  EmptyState,
  revealLabel,
  PHOTO_PROFILES,
  scalePhoto,
  yieldKeptScroll,
} from "../../ui";
import type { DragData, DropPlan, MenuItem, TreeRowState } from "../../ui";
import type { ProjectRow } from "../../types";
import { boardLabel } from "../_work/types";
import { attempt, useAsync } from "../_work/useAsync";
import { NewProjectDialog } from "../_work/NewProjectDialog";
import { AttachGoalDialog } from "../_work/AttachGoalDialog";
import { attachableGoals, goalLabelOf } from "../_work/attachGoalModel.mjs";
import { RemoveProjectDialog } from "../_work/RemoveProjectDialog";
import { removeProject } from "../_work/removeProject";
import { isAdopted, standsOn } from "../_work/removeProjectModel.mjs";
import { homeRoute } from "./ideHomeModel.mjs";
import { closeWorkstream } from "../_work/closeWorkstream";
import { terminationConsent, terminationCounts } from "../_work/closeWorkstreamModel.mjs";
import { renameBody, renamedWords } from "../_work/workstreamCardModel.mjs";
import { showRightPanel } from "./rightPanelStore";
import { RAIL_ARCHIVED_KEY, RAIL_TAB_KEY, RAIL_TABS, RAIL_TAB_FOLD, RAIL_TAB_ICON, RAIL_TAB_LABEL, collapseKey, currentProjectOf, railTabOf, groupNames, importLanding, newWorkstreamOffer, railCounts, railRows, revealPlan, rowIndexOfProject, rowIndexOfWorkstream, tabOf, treeRowsOf } from "./projectRailModel.mjs";
import { stepOf } from "../_work/projectOriginModel.mjs";
import type { RailRow, RailTab, RailTreeRow } from "./projectRailModel.mjs";
import { useBus } from "../../bus";
import { INDENT_BASE_PX, INDENT_PX, guideInset, rowHeightToken } from "./railLayoutModel.mjs";
import { RailAgentRow, RailHeadingRow, RailProjectRow, RailShellRow, RailWorkstreamRow } from "./rail";

/** The one verb for showing a folder to the OS — the platform's word, read once. */
const REVEAL_LABEL = revealLabel(typeof navigator === "undefined" ? "" : navigator.userAgent);
import { STATUS_POLL_MS, useWorkstreamStatuses } from "../../shell/workstreamStatusStore";
import { useClock } from "../../shell/clock";
import { t as tr } from "../../i18n/l10n.mjs";


/**
 * A pulse worth a line on a rail row: never the bare "N shells open" quiet line
 * (the roster's session rows already say what stands here), unless it
 * carries a port chip that needs a home.
 */
function meaningfulPulse(pulse: { flashKey?: string; ports?: readonly unknown[] } | null | undefined): boolean {
  if (!pulse) return false;
  if ((pulse.ports?.length ?? 0) > 0) return true;
  return !String(pulse.flashKey ?? "").startsWith("shell:");
}

function readTab(): RailTab {
  return readPref(webStorage(), RAIL_TAB_KEY, railTabOf, "workspace");
}

/**
 * Collapsed rows — the one fold memory (`ui/collapsedStore`), read under the
 * `rail.` prefix: a harness folded here is folded on the Workstreams panel too.
 */
function useCollapsedSet(): [Set<string>, (key: string) => void, (keys: readonly string[]) => void] {
  return [useCollapsedUnder("rail."), toggleCollapsed, expandAll];
}

export function ProjectRail({ current }: { current: { scope: string; id: string } | null }) {
  const ws = useWorkspace();
  const toast = useToast();
  const sessions = useSessions();
  const { sessions: terminals } = useTerminals();
  const launchable = useLaunchableHarnesses();
  const [tab, setTabState] = useState<RailTab>(readTab);
  const setTab = (t: RailTab) => {
    setTabState(t);
    writePref(webStorage(), RAIL_TAB_KEY, t);
  };
  // The filter and the scroll are how the rail stood: leaving the IDE keeps
  // them, and a restart. The scroll is put back first and then gives way to
  // the reveal that follows the route, which moves only if the routed row is
  // still out of sight.
  const [filter, setFilter] = useViewState(RAIL_PLACE, "filter", "", textValue);
  const rail = useRef<HTMLDivElement>(null);
  useViewScroll(rail, RAIL_PLACE);
  // The projects put away join the rail only when asked — remembered on this machine.
  const [showArchived, setShowArchivedState] = useState<boolean>(() => readPref(webStorage(), RAIL_ARCHIVED_KEY, switchPref, false));
  const setShowArchived = (on: boolean) => {
    setShowArchivedState(on);
    writePref(webStorage(), RAIL_ARCHIVED_KEY, switchWord(on));
  };
  const railProjects = useMemo(() => (showArchived ? [...ws.projects, ...ws.archivedProjects] : ws.projects), [ws.projects, ws.archivedProjects, showArchived]);
  const [collapsed, toggleCollapsed, expandCollapsed] = useCollapsedSet();
  // Every workstream's status, from the one store the rail marks read too —
  // polled while shown, refreshed on the record's frames and once per burst
  // of file changes, paused while the window is hidden.
  const statuses = useWorkstreamStatuses();
  // The rail model needs a clock only for the linger cutoff and an ended run's
  // words — coarse things — so it ticks once per status poll, shared and paused
  // when hidden. The live "open 12s" counters no longer ride this clock: they
  // tick themselves in `<LiveDuration>` leaves, so a running session no longer
  // rebuilds the whole rail every few seconds.
  // Read whenever the rows are rebuilt — on the tick, and on every roster or
  // terminal change — so a frame-driven rebuild never reuses a tick-old clock.
  const clockTick = useClock(STATUS_POLL_MS);
  // The three are signals for when *now* is read again, not values the memo reads.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const now = useMemo(() => Date.now(), [clockTick, sessions, terminals]);
  const harnessLabels = useHarnessLabels();
  // A tree row's height from the theme, live: the list's estimate; a project
  // card measures taller and the list takes its word.
  const rowHeight = useTokenPx(rowHeightToken("workstream"), 28);
  const scannedPorts = usePorts();
  // Each listening port tied to the workstream that opened it — a shell of
  // its own, or a harness session's pid.
  const ports = useMemo(() => attributePorts(scannedPorts, terminals, sessions), [scannedPorts, terminals, sessions]);
  /** What a port chip's verbs do; the rail owns the calls and the confirmation. */
  const portActions = {
    openPort: (port: number) => void openExternal(portUrl(port)).catch((e: unknown) => toast.error(failureText("workbench", "project-rail-failed", e))),
    stopPort: (p: WorkstreamPort) => setStoppingPort(p),
  };
  useBus({ stream: "engine" }, (f) => {
    if (f.stream !== "engine") return;
    const t = (f.payload.payload as { type?: string }).type;
    if (t === "workflow_changed" || t === "workflow_proposed" || t === "workflow_deleted" || t === "workflow_archived") workflowRows.reload();
  });

  const goals = useMemo(() => ws.goals.map((g) => ({ id: g.id, label: boardLabel(g) })), [ws.goals]);
  // Every workflow, library and goal designs alike: the Workflows view files a
  // project under the one whose step made it, and a design is as likely a
  // maker as a library workflow.
  const workflowRows = useAsync((s) => api.workflows({ scope: "all" }, s), []);
  const workflows = useMemo(
    () => (workflowRows.data?.workflows ?? []).map((r) => ({ id: r.workflow.id, label: r.workflow.name })),
    [workflowRows.data],
  );
  // The person's manual rail order, a machine-scoped `rail.order`
  // setting; the rail re-reads it on `settings_changed`.
  const { resolved } = useResolvedSettings(null);
  const order = useMemo(() => settingOf(resolved ?? [], "rail.order", {}) as RailOrder, [resolved]);
  // A group's adornment: a photo keyed by group name, in the
  // workspace-scoped `rail.groups` setting so it syncs like a project's photo.
  const groupsMeta = useMemo(() => settingOf(resolved ?? [], "rail.groups", {}) as RailGroups, [resolved]);
  const rows = useMemo(
    () =>
      railRows({
        tab,
        projects: railProjects,
        workstreams: ws.workstreams,
        statuses,
        sessions,
        terminals,
        ports,
        goals,
        workflows,
        collapsed,
        current,
        filter,
        now,
        order,
      }),
    [tab, railProjects, ws.workstreams, statuses, sessions, terminals, ports, goals, workflows, collapsed, current, filter, now, order],
  );
  // The strip's badges: every tab's project count, and the red count on the
  // tab whose projects have a session waiting on you or failed.
  const counts = useMemo(() => railCounts({ projects: ws.projects }), [ws.projects]);

  // The rows as the tree draws them: ids unique across kinds, open/closed
  // from the collapsed set.
  const treeRows = useMemo(() => treeRowsOf(rows, (k) => collapsed.has(k)), [rows, collapsed]);

  // Drag-to-reorder, through the kit's tree (`ui/tree`): the tree projects
  // where a row would land and `railDragModel` decides what that means —
  // groups among groups; projects among a group's projects, or into
  // *another* group, which re-groups the project; workstreams within their
  // project; shells within their workstream. Agent rows and goal headings do
  // not move. Projects, groups and workstreams persist to `rail.order`;
  // shells reorder the session order the rail and the centre strip both read.
  const persistOrder = (next: unknown) => {
    void api.setSettings("machine", { "rail.order": next }).catch((e: unknown) => toast.error(failureText("workbench", "project-rail-failed", e)));
  };
  const headless = useMemo(() => isHeadless(rows), [rows]);
  const dragOf = (t: RailTreeRow): DragData | null => railDragOf(t.row, { renaming });
  const canNest = (data: DragData, target: RailTreeRow): boolean => railCanNest(data, target.row);
  const canReorder = (data: DragData, parent: RailTreeRow | null): boolean => railCanReorder(data, parent?.row ?? null, { headless });
  const onDrop = (data: DragData, plan: DropPlan) => {
    const verdict = railDropVerdict(order, data, plan, rows, treeRows);
    if (!verdict) return;
    switch (verdict.kind) {
      case "order":
        persistOrder(verdict.order);
        return;
      case "regroup":
        // Into another group: the project moves house, then takes its slot there.
        act(verdict.words, async () => {
          await api.patchProject(verdict.project, { group: verdict.group });
          if (verdict.order !== order) await api.setSettings("machine", { "rail.order": verdict.order });
        });
        return;
      case "sessions":
        reorderTerminalSessionTab(verdict.key, verdict.before);
        return;
      default:
        return;
    }
  };

  // Dialogs and the row being renamed.
  const [newProject, setNewProject] = useState(false);
  /** The import dialog, with the goal the door named — a goal heading's, or none. The tab never guesses one. */
  const [importing, setImporting] = useState<{ goal: string | null } | null>(null);
  const [renaming, setRenaming] = useState<{ kind: "project" | "workstream"; id: string; value: string } | null>(null);
  const [stoppingPort, setStoppingPort] = useState<WorkstreamPort | null>(null);
  const [grouping, setGrouping] = useState<{ project: ProjectRow; value: string } | null>(null);
  /** Renaming a group: the group name being changed, and the field value. */
  const [renamingGroup, setRenamingGroup] = useState<{ name: string; value: string } | null>(null);
  /** The project *Attach to a goal…* was asked for — the one attach dialog, About's too. */
  const [attaching, setAttaching] = useState<ProjectRow | null>(null);
  const [removing, setRemoving] = useState<ProjectRow | null>(null);
  const [closing, setClosing] = useState<{ wid: string; label: string } | null>(null);
  const photoInput = useRef<HTMLInputElement>(null);
  /** What the next picked image is for — a project's photo, or a group's. */
  const photoFor = useRef<{ kind: "project" | "group"; id: string } | null>(null);

  // The project the workbench is rooted in, for the keyboard and the toolbar.
  const currentProject = useMemo(() => currentProjectOf(current, ws.workstreams, ws.projects), [current, ws.workstreams, ws.projects]);
  // The palette's import names no goal: only a goal heading's door does. (A
  // new workstream is the workbench's dialog: every door fires
  // `NEW_WORKSTREAM`, the rail's too.)
  useEffect(() => {
    return onDoor(IMPORT_PROJECT, () => setImporting({ goal: null }));
  }, []);
  // The keyboard cursor over the rail's rows — a tree row id — roving, never a second selection.
  const [cursor, setCursor] = useState<string | null>(null);
  /** A row the rail must bring into view once the rows hold it. */
  const [reveal, setReveal] = useState<{ pid: string; wid: string; nonce: number } | null>(null);
  const [scrollTo, setScrollTo] = useState<{ id: string | null; nonce: number }>({ id: null, nonce: 0 });
  // The rail follows the route: whatever `#/projects/workstream/<id>`
  // names is shown — its tab, its section unfolded, its row under the cursor —
  // so a link from a goal or a workflow lands, and so does a pasted address.
  useEffect(() => {
    if (!current || current.scope !== "workstream" || !currentProject) return;
    const plan = revealPlan(currentProject.project);
    if (plan.tab !== tab) setTab(plan.tab);
    expandCollapsed(plan.expand);
    setReveal((prev) => ({ pid: currentProject.project.id, wid: current.id, nonce: (prev?.nonce ?? 0) + 1 }));
    // Once per routed row: `tab`, `currentProject` and `expandCollapsed` are
    // read as they stand, and following them would reveal — and re-fold —
    // on every change of the rail.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.scope, current?.id, currentProject?.project.id]);
  useEffect(() => {
    if (!reveal) return;
    const at = (() => {
      const w = rowIndexOfWorkstream(rows, reveal.wid);
      return w >= 0 ? w : rowIndexOfProject(rows, reveal.pid);
    })();
    if (at < 0) return; // the rows have not caught up yet: the next render tries again
    const id = treeRows[at]?.id ?? null;
    setCursor(id);
    // The rail follows the route: a scroll being put back gives way first.
    yieldKeptScroll(rail.current);
    setScrollTo((prev) => ({ id, nonce: prev.nonce + 1 }));
    setReveal(null);
  }, [reveal, rows, treeRows]);
  // A selection kept from the last window may name a project that went
  // while the app was closed: once the workspace has said what it lists,
  // what is not there selects nothing.
  useEffect(() => {
    if (ws.ready) keepListedSelection([...ws.projects, ...ws.archivedProjects].map((r) => r.project.id));
  }, [ws.ready, ws.projects, ws.archivedProjects]);
  // A person's act on a row — a click, the keyboard's cursor — is the
  // selection the Board narrows to (`railSelectionStore`); the reveal that
  // follows the route is not, so the Board opens on every workstream.
  const choose = (treeId: string | null) => {
    const t = treeId ? treeRows.find((r) => r.id === treeId) : null;
    setRailSelection(selectionOf(t?.row ?? null));
  };
  const openRow = (row: RailRow) => {
    switch (row.kind) {
      case "project":
        navigate({ name: "workbench", scope: "workstream", id: row.project.id });
        return;
      case "workstream":
        navigate({ name: "workbench", scope: "workstream", id: row.id });
        return;
      case "terminal":
        openTerminalTab({ key: row.id, scope: "workstream", id: row.workstream });
        return;
      case "agent":
        // A sub-agent row chooses the session it nests under; the pet
        // follows sessions, not their children.
        openHarnessSession({ id: row.parent ?? row.id, workstream: row.workstream, terminalKey: row.terminalKey ?? null });
        return;
      default:
        return;
    }
  };

  // F2: rename the row the workbench is rooted at.
  useEffect(() => {
    const fn = () => {
      if (!current || current.scope !== "workstream") return;
      const ref = ws.workstreams.find((w) => w.workstream.id === current.id);
      if (!ref) return;
      if (ref.workstream.kind.kind === "primary") {
        const p = ws.projects.find((r) => r.project.id === ref.workstream.project);
        if (p) setRenaming({ kind: "project", id: p.project.id, value: p.project.name });
      } else {
        setRenaming({ kind: "workstream", id: ref.workstream.id, value: ref.workstream.name ?? "" });
      }
    };
    return onDoor(RENAME_SELECTION, fn);
  }, [current, ws.workstreams, ws.projects]);

  const act = (label: string | null, fn: () => Promise<unknown>) =>
    void attempt(fn, toast.error, () => {
      if (label) toast.ok(label);
      ws.refresh();
    });

  /** Stop an engine session by its row, through the store's one door: a row the node no longer has is dropped, and said. */
  const stop = (id: string) =>
    void attempt(() => stopSession(id), toast.error, (how) => {
      toast.ok(stopWords(how, tr("workbench-project-rail-session-aborted")));
      ws.refresh();
    });

  const commitRename = () => {
    if (!renaming) return;
    const { kind, id, value } = renaming;
    setRenaming(null);
    if (kind === "project") {
      if (!value.trim()) return;
      act(tr("workbench-project-rail-project-renamed"), () => api.patchProject(id, { name: value.trim() }));
    } else {
      // The one rule of a rename — trimmed, `null` for an empty name — is the card model's.
      const body = renameBody(value);
      act(renamedWords(body), () => api.patchWorkstream(id, body));
    }
  };

  // Renaming a group re-groups its member projects, then re-keys the
  // group's photo and its manual rail position — a group's identity is its name.
  const commitGroupRename = () => {
    if (!renamingGroup) return;
    const { name: oldName, value } = renamingGroup;
    const newName = value.trim();
    setRenamingGroup(null);
    if (!newName || newName === oldName) return;
    act(tr("workbench-project-rail-group-renamed", { newName }), async () => {
      const members = ws.projects.filter((r) => (r.project.group?.trim() || "") === oldName);
      await Promise.all(members.map((r) => api.patchProject(r.project.id, { group: newName })));
      await api.setSettings("workspace", { "rail.groups": renameGroupMeta(groupsMeta, oldName, newName) });
      const nextOrder = renameGroupOrder(order, oldName, newName);
      if (nextOrder !== order) await api.setSettings("machine", { "rail.order": nextOrder });
    });
  };

  /** A real project group's menu: rename and photo. */
  const groupMenu = (name: string): MenuItem[] => [
    { label: tr("workbench-project-rail-rename-group"), icon: ICON.edit, onSelect: () => setRenamingGroup({ name, value: name }) },
    {
      label: groupPhoto(groupsMeta, name) ? tr("workbench-project-rail-change-photo") : tr("workbench-project-rail-set-photo"),
      icon: ICON.attach,
      // The file dialog opens only inside the gesture, never after the menu has left.
      immediate: true,
      onSelect: () => {
        photoFor.current = { kind: "group", id: name };
        photoInput.current?.click();
      },
    },
    ...(groupPhoto(groupsMeta, name)
      ? [{ label: tr("workbench-project-rail-remove-photo"), onSelect: () => act(tr("workbench-project-rail-group-photo-removed"), () => api.setSettings("workspace", { "rail.groups": withGroupPhoto(groupsMeta, name, null) })) }]
      : []),
  ];

  const projectMenu = (row: Extract<RailRow, { kind: "project" }>): MenuItem[] => {
    const p = row.project;
    const groups = groupNames(ws.projects);
    const attachable = attachableGoals(goals, row.goals);
    return [
      { label: tr("workbench-rail-menu-open"), icon: ICON.project, onSelect: () => navigate({ name: "workbench", scope: "workstream", id: p.id }) },
      { label: tr("workbench-project-rail-rename"), icon: ICON.edit, onSelect: () => setRenaming({ kind: "project", id: p.id, value: p.name }) },
      {
        label: tr("workbench-project-rail-new-goal-project"),
        icon: ICON.goal,
        onSelect: () => {
          setPendingProjects([p.id]);
          fire(NEW_GOAL);
        },
      },
      {
        label: p.photo ? tr("workbench-project-rail-change-photo") : tr("workbench-project-rail-set-photo"),
        icon: ICON.attach,
        immediate: true,
        onSelect: () => {
          photoFor.current = { kind: "project", id: p.id };
          photoInput.current?.click();
        },
      },
      ...(p.photo ? [{ label: tr("workbench-project-rail-remove-photo"), onSelect: () => act(tr("workbench-project-rail-photo-removed"), () => api.patchProject(p.id, { photo: null })) }] : []),
      {
        label: p.group ? tr("workbench-project-rail-move-another-group-now", { group: p.group }) : tr("workbench-project-rail-move-group"),
        separatorBefore: true,
        onSelect: () => setGrouping({ project: row, value: p.group ?? groups[0] ?? "" }),
      },
      ...(p.group ? [{ label: tr("workbench-project-rail-remove-from-group"), onSelect: () => act(tr("workbench-project-rail-left", { p: p.name, group: p.group }), () => api.patchProject(p.id, { group: null })) }] : []),
      {
        label: tr("workbench-project-rail-attach-goal-2"),
        icon: ICON.attach,
        separatorBefore: true,
        disabled: attachable.length === 0,
        onSelect: () => setAttaching(row),
      },
      {
        label: newWorkstreamOffer(p).disabled ? tr("workbench-project-rail-new-workstream", { reason: newWorkstreamOffer(p).reason }) : tr("workbench-project-rail-new-workstream-2"),
        icon: ICON.workstream,
        disabled: newWorkstreamOffer(p).disabled,
        onSelect: () => fire(NEW_WORKSTREAM, { pid: row.project.id }),
      },
      // The way back to what made it: the goal, and the library
      // workflow when a library step did.
      ...(p.origin.origin !== "workspace"
        ? [{ label: tr("workbench-project-rail-open-goal-born-of"), icon: ICON.goal, separatorBefore: true, onSelect: () => navigate({ name: "goal", id: p.origin.goal as string }) }]
        : []),
      ...(p.origin.origin === "step" && stepOf(p.origin)
        ? [{ label: tr("workbench-project-rail-open-workflow-step-made"), icon: ICON.workflow, onSelect: () => navigate({ name: "workflow", id: stepOf(p.origin)?.workflow ?? "" }) }]
        : []),
      ...(inDesktopShell() && row.exists
        ? [
            {
              label: REVEAL_LABEL,
              separatorBefore: true,
              onSelect: () => void revealPath(row.path).catch((e: unknown) => toast.error(failureText("workbench", "project-rail-failed", e))),
            },
          ]
        : []),
      ...(p.archived
        ? [{ label: tr("workbench-project-rail-unarchive"), icon: ICON.archive, separatorBefore: true, onSelect: () => act(tr("workbench-project-rail-back", { p: p.name }), () => api.archiveProject(p.id, false)) }]
        : []),
      { label: p.archived ? tr("workbench-project-rail-remove-project") : tr("workbench-project-rail-archive-remove-project"), icon: ICON.delete, danger: true, separatorBefore: !p.archived, onSelect: () => setRemoving(row) },
    ];
  };

  /** The workstream row's verbs, from the spec the Workstreams panel shares (`railMenuModel.mjs`). */
  const workstreamMenu = (row: Extract<RailRow, { kind: "workstream" }>): MenuItem[] => {
    const target = { scope: "workstream" as const, id: row.id, label: row.label };
    const harnesses = launchable.filter((h) => h.installed).map((h) => ({ id: h.id, label: h.label ?? h.id }));
    const act: Record<string, () => void> = {
      open: () => navigate({ name: "workbench", scope: "workstream", id: row.id }),
      rename: () => setRenaming({ kind: "workstream", id: row.id, value: row.workstream.name ?? "" }),
      "new-shell": () => openTerminalIn(target),
      diff: () => navigate({ name: "workbench", scope: "workstream", id: row.id }, { doc: "diff" }),
      close: () => setClosing({ wid: row.id, label: row.label }),
    };
    const icon: Record<string, MenuItem["icon"]> = { open: ICON.workstream, rename: ICON.edit, "new-shell": ICON.harness, close: ICON.delete };
    return workstreamMenuSpec({ primary: row.primary, exists: row.exists, harnesses, rename: true, close: true }).map((item) => ({
      label: item.label,
      icon: item.harness ? harnessMark(item.harness) : icon[item.id],
      danger: item.danger,
      disabled: item.disabled,
      separatorBefore: item.separatorBefore,
      onSelect: item.harness ? () => openTerminalIn({ ...target, harness: item.harness }) : (act[item.id] ?? (() => undefined)),
    }));
  };

  /**
   * End a harness a person opened in a terminal. The tab is the row: closing
   * the tab — through the one close guard, which confirms for a live one —
   * has the host end the process and tell the roster, so the row leaves the
   * list with the process. One verb, *Terminate*, whether or not the harness
   * reports.
   */
  const terminateHarness = (terminalKey: string) => requestCloseTerminal(terminalKey);

  /** A shell row's verbs, worded by the tab menu (`tabMenuModel.mjs`) so the rail and the strip say the same thing. */
  const terminalMenu = (row: Extract<RailRow, { kind: "terminal" }>): MenuItem[] => {
    // A harness in a shell that does not report — launched with reporting
    // off, or typed into the shell — has one verb while it runs, like any harness.
    const live = row.liveness.status === "live";
    if (row.harness && live) return [{ label: tr("workbench-project-rail-terminate"), danger: true, onSelect: () => terminateHarness(row.id) }];
    const here = terminals.filter((t) => t.scope === "workstream" && t.id === row.workstream);
    const spec = terminalTabMenu({ live, harness: row.harness !== null, others: here.length - 1, exited: here.filter((t) => t.liveness.status !== "live").length });
    const act: Record<string, () => void> = {
      focus: () => openTerminalTab({ key: row.id, scope: "workstream", id: row.workstream }),
      restart: () => restartTerminalTab(row.id),
      close: () => requestCloseTerminal(row.id),
      "close-others": () => requestCloseOthers(row.id),
      "close-exited": () => closeExitedTerminalTabs("workstream", row.workstream),
    };
    const icon: Record<string, MenuItem["icon"]> = { restart: ICON.refresh };
    return spec
      .filter((item) => item.id in act)
      .map((item) => ({ label: item.label, icon: icon[item.id], danger: item.danger, disabled: item.disabled, separatorBefore: item.separatorBefore, onSelect: act[item.id] }));
  };

  /** A session row's verbs, from the spec (`railMenuModel.agentRowMenuSpec`): Abort only while it runs or waits. */
  const agentMenu = (row: Extract<RailRow, { kind: "agent" }>): MenuItem[] => {
    const key = row.terminalKey;
    const act_: Record<string, () => void> = {
      show: () => {
        navigate({ name: "workbench", scope: "workstream", id: row.workstream });
        showRightPanel("agents", `workstream:${row.workstream}`);
      },
      answer: () => navigate({ name: "inbox" }),
      abort: () => stop(row.id),
      terminate: () => key && terminateHarness(key),
    };
    const icon: Record<string, MenuItem["icon"]> = { show: ICON.agent, answer: ICON.inbox };
    return agentRowMenuSpec(row).map((item) => ({ label: item.label, icon: icon[item.id], danger: item.danger, separatorBefore: item.separatorBefore, onSelect: act_[item.id] }));
  };

  /** The rename field's wiring for the row being renamed, or null. */
  const editingOf = (kind: "project" | "workstream", id: string) =>
    renaming?.kind === kind && renaming.id === id
      ? { value: renaming.value, onChange: (value: string) => setRenaming({ ...renaming, value }), onCommit: commitRename, onCancel: () => setRenaming(null) }
      : null;

  /** One component per kind over one shell; the state and the verbs stay here. */
  const renderRow = (row: RailRow, rs: TreeRowState, first: boolean) => {
    const key = collapseKey(row);
    const toggle = () => {
      if (key) toggleCollapsed(key);
    };
    switch (row.kind) {
      case "group":
      case "goal": {
        // The projects this heading gathers — the model says which.
        const under = ws.projects.filter((r) => row.projects.includes(r.project.id));
        // A *real* project group (workspace tab, named) can be renamed and wear
        // a photo; the "Other projects" bucket, goal sections and
        // workflow sections cannot.
        const isRealGroup = tab === "workspace" && row.kind === "group" && row.id !== "";
        const photo = isRealGroup ? groupPhoto(groupsMeta, row.id) : null;
        const menu: MenuItem[] = [
          ...(under.length > 0
            ? [
                {
                  label: tr("workbench-project-rail-new-goal-from-project-these-projects", { under: under.length }),
                  icon: ICON.goal,
                  onSelect: () => {
                    setPendingProjects(under.map((r) => r.project.id));
                    fire(NEW_GOAL);
                  },
                },
              ]
            : []),
          // A goal's heading imports into that goal; a group's has no goal to give.
          ...(row.kind === "goal" ? [{ label: tr("workbench-project-rail-import-into-goal"), icon: ICON.import, separatorBefore: under.length > 0, onSelect: () => setImporting({ goal: row.id }) }] : []),
          ...(isRealGroup ? groupMenu(row.id).map((m, i) => (i === 0 ? { ...m, separatorBefore: under.length > 0 } : m)) : []),
        ];
        return (
          <RailHeadingRow
            row={row}
            rs={rs}
            first={first}
            photo={photo}
            editable={isRealGroup}
            menu={menu}
            groupMenu={isRealGroup ? groupMenu(row.id) : []}
            harnessLabels={harnessLabels}
            onToggle={toggle}
            onRename={() => setRenamingGroup({ name: row.id, value: row.id })}
          />
        );
      }
      case "project":
        return (
          <RailProjectRow
            row={row}
            rs={rs}
            menu={projectMenu(row)}
            harnessLabels={harnessLabels}
            showPulse={meaningfulPulse(row.pulse)}
            editing={editingOf("project", row.project.id)}
            onToggle={toggle}
            onOpen={() => navigate({ name: "workbench", scope: "workstream", id: row.project.id })}
            onRename={() => setRenaming({ kind: "project", id: row.project.id, value: row.project.name })}
            onNewWorkstream={() => fire(NEW_WORKSTREAM, { pid: row.project.id })}
          />
        );
      case "workstream":
        return (
          <RailWorkstreamRow
            row={row}
            rs={rs}
            menu={workstreamMenu(row)}
            harnessLabels={harnessLabels}
            portActions={portActions}
            editing={editingOf("workstream", row.id)}
            onToggle={toggle}
            onOpen={() => navigate({ name: "workbench", scope: "workstream", id: row.id })}
            onRename={() => setRenaming({ kind: "workstream", id: row.id, value: row.workstream.name ?? "" })}
          />
        );
      case "terminal":
        return (
          <RailShellRow
            row={row}
            rs={rs}
            menu={terminalMenu(row)}
            harnessLabels={harnessLabels}
            onOpen={() => {
              focusTerminalTab(row.id);
              navigate({ name: "workbench", scope: "workstream", id: row.workstream }, { doc: `terminal:${row.id}` });
            }}
            onRestart={() => restartTerminalTab(row.id)}
          />
        );
      case "agent":
        return (
          <RailAgentRow
            row={row}
            rs={rs}
            menu={agentMenu(row)}
            harnessLabels={harnessLabels}
            folded={key ? collapsed.has(key) : false}
            onToggle={toggle}
            // A harness in a terminal is answered in that terminal: the row
            // opens its tab. An engine session opens the Agents pane.
            onOpen={() => openHarnessSession({ id: row.parent ?? row.id, workstream: row.workstream, terminalKey: row.terminalKey ?? null })}
            // The Inbox on the row the ask lives under — its goal, else its workstream — as the Agents screen's *Answer*.
            onAnswer={() => navigate({ name: "inbox" }, { item: row.goal ?? row.workstream })}
            onStop={() => {
              if (row.terminalKey) terminateHarness(row.terminalKey);
              else stop(row.id);
            }}
          />
        );
      default:
        return null;
    }
  };

  const removingAdopted = isAdopted(removing?.project);
  return (
    // An `@container`: a row's model and its durations show by the rail's width, not the window's.
    <div ref={rail} className="@container flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-1 px-2 pt-1">
        {/* One row at every width: each tab its glyph and its word while they fit; a rail too
            narrow folds Workflows to its glyph first, then Goals, then Workspace (`RAIL_TAB_FOLD`). */}
        <Tabs
          className="min-w-0 flex-1"
          tabs={RAIL_TABS.map((id) => ({
            id,
            label: RAIL_TAB_LABEL[id],
            icon: ICON[RAIL_TAB_ICON[id]],
            fold: RAIL_TAB_FOLD[id],
            count: counts[id],
            badge: <CountBadge count={counts[id]} tone="neutral" title={tr("workbench-project-rail-project-projects", { counts: counts[id] })} />,
          }))}
          active={tab}
          onChange={(t) => setTab(t as RailTab)}
        />
        <Menu
          label={tr("workbench-capture-tray-add")}
          items={[
            { label: tr("workbench-project-rail-new-project"), icon: ICON.project, onSelect: () => setNewProject(true) },
            {
              label: currentProject
                ? newWorkstreamOffer(currentProject.project).disabled
                  ? `${newWorkstreamOffer(currentProject.project).label} (${newWorkstreamOffer(currentProject.project).reason})`
                  : newWorkstreamOffer(currentProject.project).label
                : tr("workbench-project-rail-new-workstream-open-project-first"),
              icon: ICON.workstream,
              disabled: !currentProject || newWorkstreamOffer(currentProject.project).disabled,
              onSelect: () => currentProject && fire(NEW_WORKSTREAM, { pid: currentProject.project.id }),
            },
          ]}
          trigger={
            <span className="anim inline-flex h-7 w-7 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
              <ICON.add size={13} aria-hidden />
            </span>
          }
        />
        {/* Import beside "+": a folder from this machine or a clone, landing
            where the goal picked in the dialog says — the workspace with none. */}
        <Tooltip label={tr("workbench-project-rail-import-folder-clone-repository-into-workspace")}>
          <button
            type="button"
            aria-label={tr("workbench-project-rail-import-folder-clone-repository")}
            onClick={() => setImporting({ goal: null })}
            className="anim inline-flex h-7 w-7 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
          >
            <ICON.import size={13} aria-hidden />
          </button>
        </Tooltip>
      </div>
      <div className="flex shrink-0 items-center gap-2 px-2 py-1">
        <TextInput value={filter} aria-label={tr("workbench-project-rail-filter-projects-workstreams")} placeholder={tr("workbench-project-rail-filter-projects-workstreams")} className="h-6 min-w-0 flex-1 text-2xs" onChange={(e) => setFilter(e.target.value)} />
        <Tooltip label={showArchived ? tr("workbench-project-rail-hide-projects-put-away") : tr("workbench-project-rail-show-projects-put-away")}>
          <span className="inline-flex">
            <Switch checked={showArchived} onChange={setShowArchived} label={tr("workbench-project-rail-archived")} />
          </span>
        </Tooltip>
      </div>
      <TreeList
        rows={treeRows}
        rowHeight={rowHeight}
        measure
        indent={INDENT_PX}
        indentBase={INDENT_BASE_PX}
        guides
        guideInset={guideInset()}
        guideClassName="bg-border/40"
        cursor={cursor}
        onCursor={(id) => {
          setCursor(id);
          choose(id);
        }}
        onAction={(a) => {
          // Open and close are the collapsed set's; the tree only asks.
          const keys = a.kind === "expandAll" ? a.ids : [a.id];
          for (const id of keys) {
            const t = treeRows.find((r) => r.id === id);
            const k = t ? collapseKey(t.row) : null;
            if (!k) continue;
            if (a.kind === "collapse" ? !collapsed.has(k) : collapsed.has(k)) toggleCollapsed(k);
          }
        }}
        onKeyDown={(e) => {
          const t = treeRows.find((r) => r.id === cursor);
          if (e.key === "Enter" && t) {
            e.preventDefault();
            openRow(t.row);
          }
        }}
        render={(t, rs) => (
          <div
            onMouseDown={() => {
              setCursor(t.id);
              choose(t.id);
            }}
          >
            {renderRow(t.row, rs, t.id === treeRows[0]?.id)}
          </div>
        )}
        drag={{ data: dragOf, canNest, canReorder, onDrop }}
        label={tr("workbench-project-rail-projects")}
        scrollTo={scrollTo}
        keepScroll="rail"
        fill
        className="min-h-0 flex-1"
        empty={
          ws.projects.length === 0 ? (
            <EmptyState
              icon={ICON.project}
              title={tr("workbench-project-rail-no-projects-yet")}
              hint={tr("workbench-project-rail-import-folder-clone-repository-start")}
              action={
                // The landing's own door, its words and its dialog: the centre beside
                // it wears the one primary, so this one stays default.
                <Button size="sm" onClick={() => setNewProject(true)}>
                  <ICON.add size={12} aria-hidden />{tr("workbench-center-landing-new-project")}</Button>
              }
            />
          ) : (
            <EmptyState
              icon={ICON.search}
              title={tr("workbench-project-rail-nothing-matches-filter")}
              action={
                <Button size="sm" variant="ghost" onClick={() => setFilter("")}>{tr("workbench-project-rail-clear-filter")}</Button>
              }
            />
          )
        }
      />

      <input
        ref={photoInput}
        type="file"
        accept="image/png,image/jpeg,image/gif,image/webp"
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          const target = photoFor.current;
          e.target.value = "";
          if (!file || !target) return;
          act(tr("workbench-project-rail-photo-set"), async () => {
            // Scaled to a small square here, before it is uploaded (ide/14 §Photos).
            const ref = await api.uploadAttachment(await scalePhoto(file, PHOTO_PROFILES.picture));
            if (target.kind === "project") {
              await api.patchProject(target.id, { photo: ref });
            } else {
              await api.setSettings("workspace", { "rail.groups": withGroupPhoto(groupsMeta, target.id, ref) });
            }
          });
        }}
      />

      <NewProjectDialog
        open={newProject}
        onClose={() => setNewProject(false)}
        onCreated={(created) => {
          ws.refresh();
          // The new row is in the tab its origin names: show that tab.
          setTab(tabOf(created.project));
          navigate({ name: "workbench", scope: "workstream", id: created.project.id });
        }}
      />
      {importing && (
        <NewProjectDialog
          open
          mode="import"
          onClose={() => setImporting(null)}
          goal={importing.goal ?? undefined}
          note={importLanding(tab, importing.goal ? goalLabelOf(goals, importing.goal) : null)}
          onCreated={(created) => {
            ws.refresh();
            setTab(tabOf(created.project));
            navigate({ name: "workbench", scope: "workstream", id: created.project.id });
          }}
        />
      )}
      <Dialog
        open={grouping !== null}
        onClose={() => setGrouping(null)}
        title={grouping ? tr("workbench-project-rail-group-2", { project: grouping.project.project.name }) : ""}
        description={tr("workbench-project-rail-projects-same-group-sit-together-rail")}
        width="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setGrouping(null)}>{tr("workbench-capture-tray-cancel")}</Button>
            <Button
              variant="primary"
              onClick={() => {
                if (!grouping) return;
                const g = grouping.value.trim();
                act(g ? tr("workbench-project-rail-moved", { g }) : tr("workbench-project-rail-removed-from-group"), () => api.patchProject(grouping.project.project.id, { group: g || null }));
                setGrouping(null);
              }}
            >{tr("workbench-project-rail-move")}</Button>
          </>
        }
      >
        <Field label={tr("workbench-project-rail-group")}>
          <TextInput autoFocus list="rail-groups" value={grouping?.value ?? ""} onChange={(e) => grouping && setGrouping({ ...grouping, value: e.target.value })} />
          <datalist id="rail-groups">
            {groupNames(ws.projects).map((g) => (
              <option key={g} value={g} />
            ))}
          </datalist>
        </Field>
      </Dialog>
      <Dialog
        open={renamingGroup !== null}
        onClose={() => setRenamingGroup(null)}
        title={renamingGroup ? tr("workbench-project-rail-rename-2", { renamingGroup: renamingGroup.name }) : ""}
        description={tr("workbench-project-rail-every-project-group-moves-new-name")}
        width="max-w-sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setRenamingGroup(null)}>{tr("workbench-capture-tray-cancel")}</Button>
            <Button variant="primary" disabled={!renamingGroup?.value.trim()} onClick={commitGroupRename}>{tr("workbench-project-rail-rename")}</Button>
          </>
        }
      >
        <Field label={tr("workbench-project-rail-group-name")}>
          <TextInput
            autoFocus
            value={renamingGroup?.value ?? ""}
            onChange={(e) => renamingGroup && setRenamingGroup({ ...renamingGroup, value: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitGroupRename();
              if (e.key === "Escape") setRenamingGroup(null);
            }}
          />
        </Field>
      </Dialog>
      {/* The one attach dialog — About › Goals opens the same one. */}
      <AttachGoalDialog
        project={attaching ? { id: attaching.project.id, name: attaching.project.name, goals: attaching.goals } : null}
        goals={goals}
        onClose={() => setAttaching(null)}
        onAttached={ws.refresh}
      />
      <RemoveProjectDialog
        project={removing ? { name: removing.project.name, archived: Boolean(removing.project.archived), adopted: removingAdopted } : null}
        onClose={() => setRemoving(null)}
        onChoose={(chosen, trash) => {
          if (!removing) return;
          const pid = removing.project.id;
          setRemoving(null);
          // The one door (`removeProject.ts`); a person standing on what went is
          // moved to the next home once the node has answered, never before —
          // by `replace`, so Back never returns to a root that is gone.
          void attempt(() => removeProject(pid, chosen, trash, ws.workstreams, ws.projects), toast.error, ({ said, roots, home }) => {
            if (said.tone === "warn") toast.error(said.text);
            else toast.ok(said.text);
            if (standsOn(current, roots)) replace(homeRoute(home));
          });
        }}
      />
      <ConfirmDialog
        open={closing !== null}
        onClose={() => setClosing(null)}
        title={closing ? tr("workbench-project-rail-close-2", { closing: closing.label }) : ""}
        body={[
          tr("workbench-project-rail-close-workstream-body"),
          ...(closing ? [terminationConsent(terminationCounts(sessions, terminals, closing.wid))].filter((s): s is string => s !== null) : []),
        ].join(" ")}
        confirmLabel={tr("workbench-project-rail-close")}
        onConfirm={() => {
          if (!closing) return;
          const { wid } = closing;
          setClosing(null);
          // Moved off the root once it is closed — a close the node refused moves nobody.
          void attempt(() => closeWorkstream(wid, { tree: false }), toast.error, () => {
            toast.ok(tr("workbench-project-rail-workstream-closed"));
            ws.refresh();
            if (current?.scope === "workstream" && current.id === wid) navigate({ name: "projects" });
          });
        }}
      />
      <ConfirmDialog
        open={stoppingPort !== null}
        onClose={() => setStoppingPort(null)}
        title={stoppingPort ? tr("workbench-project-rail-stop-process", { port: stoppingPort.port }) : ""}
        body={stoppingPort ? stopPrompt(stoppingPort) : ""}
        confirmLabel={tr("workbench-project-rail-stop")}
        danger
        onConfirm={() => {
          if (!stoppingPort) return;
          const { pid, port } = stoppingPort;
          setStoppingPort(null);
          act(tr("workbench-project-rail-stopped-process", { port }), async () => {
            await stopPort(pid, port);
            // The chip should go with the process, not five seconds later.
            rescanPorts();
          });
        }}
      />
    </div>
  );
}
