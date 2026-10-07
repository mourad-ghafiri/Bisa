/**
 * The shell's data, loaded once and kept live.
 *
 * Every list used to be component-local state that refetched on mount, so
 * moving between two conversations meant a spinner, a rebuilt list and a lost
 * scroll position — and unread only existed on the screen that owned it. This
 * holds the workspace's shape (goals, channels, DMs, agents, inbox) above
 * the router, subscribes to the bus **once**, and patches in place. Navigating
 * is then free, and a badge is correct wherever it appears.
 */

import { onSessionTransition, sessionRows } from "./sessionsStore";
import { clearedByRow, rebuilt, sameWorking, withReplied, withThinking } from "./workingModel.mjs";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { api, ApiError, nodeFailureReason } from "../api";
import { type ConnState, subscribe as busSubscribe, watchConnection } from "../bus";
import { FIRST_CONN } from "../busModel.mjs";
import { errorFields, log } from "../log";
import { toaster } from "../ui/Toast";
import { photoOfPrincipal } from "../ui/photoModel.mjs";
import { applyDelta, needsOf, joinOf, waitingOf } from "../views/_studio/inboxModel.mjs";
import { withLatest } from "../views/_studio/channelListModel.mjs";
import { nameIn, principalNames } from "./namesModel.mjs";
import { rowRead } from "../views/_studio/readModel.mjs";
import { degradedReads, failedRead, hostedFailure, offlineLine, offlineWords, reconnectWords, reloadOnReconnect, reloadsWorkspace, settleLoads } from "./workspaceLoadModel.mjs";
import { createLatest } from "./latestModel.mjs";
import { channelDefOf, hostName, hostedRead, type HostedEntry, type HostedSection } from "./hostedModel.mjs";
import type {
  AttachmentRef,
  AgentDef,
  ChannelListEntry,
  InboxRow,
  GoalRow,
  MemberRow,
  ProjectRow,
  TeamDef,
  WorkspaceInfo,
  WorkstreamRef,
  HostedChannelRow,
} from "../types";

/** The reads a load makes, in the order they settle — the names the chrome shows. */
/** The hosted sections, and the reads among them that did not answer, named for `degraded`. */
interface HostedLoad {
  sections: HostedSection[];
  failed: string[];
}

/**
 * Every hosted membership with the channels this person reaches there — one
 * read per host beside the list. A host that cannot answer (the node runs no
 * pump, or the membership is not a member's) keeps its section with no rows,
 * and the read is named in `failed` (`hostedFailure`) so the chrome says so
 * rather than showing a host with nothing in it.
 */
async function loadHosted(signal?: AbortSignal): Promise<HostedLoad> {
  const { hosts } = await api.hosts(signal);
  const failed: string[] = [];
  const sections = await Promise.all(
    hosts.map(async (host) => {
      const key = host.host.pubkey;
      const entries = (rows: HostedChannelRow[]): HostedEntry[] =>
        rows.map((r) => ({ channel: channelDefOf(r.channel), unread_count: r.unread_count, latest_at: r.latest_at ?? null }));
      const read = async (what: "channels" | "dms", rows: Promise<HostedChannelRow[]>): Promise<HostedEntry[]> => {
        try {
          return entries(await rows);
        } catch (e) {
          if (signal?.aborted) throw e;
          failed.push(hostedFailure(hostName(host), what, e));
          return [];
        }
      };
      const [channels, dms] = await Promise.all([
        read("channels", api.hostedChannels(key, signal).then((r) => r.channels)),
        read("dms", api.hostedDms(key, signal).then((r) => r.dms)),
      ]);
      return { host, channels, dms };
    }),
  );
  return { sections, failed };
}

const LOAD_NAMES = ["workspace", "agents", "teams", "goals", "channels", "dms", "inbox", "projects", "workstreams", "paused", "hosts"] as const;

interface WorkspaceData {
  ready: boolean;
  /** Set when the node is unreachable; the shell still renders. */
  offline: string | null;
  /**
   * The reads of the last load that did not answer, each as `name: message`
   * — the lists keep their last values, and the chrome says so. Empty when
   * every read answered.
   */
  degraded: string[];
  /**
   * The engine is paused, so nothing new will start — read from
   * `GET /pause` with every load, so a window that opens on an
   * already-paused node says so, and moved by the bus's `paused` and
   * `resumed` frames between loads.
   */
  paused: boolean;
  me: string;
  info: WorkspaceInfo | null;
  members: MemberRow[];
  agents: AgentDef[];
  /** Every team once — what a conversation offers under `@` beside the agents. */
  teams: TeamDef[];
  goals: GoalRow[];
  /** The goals put away — what the Goals screen shows only when asked. */
  archivedGoals: GoalRow[];
  /** Every project once (04-workspace-project-goal: one list, never two) — the ones in the lists. */
  projects: ProjectRow[];
  /** The projects put away — what the rail shows only when asked. */
  archivedProjects: ProjectRow[];
  /** Every workstream, each project's primary first. */
  workstreams: WorkstreamRef[];
  channels: ChannelListEntry[];
  dms: ChannelListEntry[];
  /** The workspaces this node is a guest of, each with the channels this person reaches there (14-collaboration). */
  hosted: HostedSection[];
  /** The last read of the hosts answered: `hosted` is the node's word on the memberships, not the list a failed read left standing. */
  hostsRead: boolean;
  inbox: InboxRow[];
  /** Total needs-action across every conversation — the Inbox badge. */
  waiting: number;
  /** scope → unread messages. */
  unread: Record<string, number>;
  /** scope → agent ids currently mid-turn there. */
  working: Record<string, string[]>;
  nameOf: (pubkey: string) => string;
  /** The face a principal wears — a member's (the owner's own row included) or an agent's photo, by content hash (ide/14 §Photos). */
  photoOf: (pubkey: string) => AttachmentRef | null;
  agentByPubkey: (pubkey: string) => AgentDef | undefined;
  /**
   * Mark a conversation read — a scope of this node's, or one on the host
   * named — clearing what the shell shows of it at once (its unread, its
   * inbox row, a hosted entry), then confirming with the node. The thread
   * that shows it is the one caller (`useReadAsShown`); the Inbox's own
   * verbs go through the API and reload.
   */
  markRead: (scope: string, host?: string | null) => void;
  refresh: () => void;
}

const EMPTY: WorkspaceData = {
  ready: false,
  offline: null,
  degraded: [],
  paused: false,
  me: "",
  info: null,
  members: [],
  teams: [],
  agents: [],
  goals: [],
      archivedGoals: [],
      archivedProjects: [],
  projects: [],
  workstreams: [],
  channels: [],
  dms: [],
  hosted: [],
  hostsRead: false,
  inbox: [],
  waiting: 0,
  unread: {},
  working: {},
  nameOf: (p) => `${p.slice(0, 8)}…`,
  photoOf: () => null,
  agentByPubkey: () => undefined,
  markRead: () => {},
  refresh: () => {},
};

export const WorkspaceContext = createContext<WorkspaceData>(EMPTY);

export function useWorkspace(): WorkspaceData {
  return useContext(WorkspaceContext);
}

/** The provider's own reload, registered while it is mounted — for the doors that run outside a component (`retire.ts`). */
let refresher: (() => void) | null = null;

/** Re-read the workspace now; nothing when no provider is mounted. */
export function refreshWorkspace(): void {
  refresher?.();
}

/**
 * The two frames the working dot reads (`agent_thinking` / `agent_replied`,
 * typed in `types.hand.ts`), read structurally here: the handler below
 * switches on `type` across every engine frame, and a loose shape keeps it
 * one `if` chain.
 */
interface LoosePayload {
  type: string;
  scope?: string;
  agent?: string;
}

export function useWorkspaceState(): WorkspaceData {
  const [info, setInfo] = useState<WorkspaceInfo | null>(null);
  const [agents, setAgents] = useState<AgentDef[]>([]);
  const [teams, setTeams] = useState<TeamDef[]>([]);
  const [goals, setGoals] = useState<GoalRow[]>([]);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [archivedGoals, setArchivedGoals] = useState<GoalRow[]>([]);
  const [archivedProjects, setArchivedProjects] = useState<ProjectRow[]>([]);
  const [workstreams, setWorkstreams] = useState<WorkstreamRef[]>([]);
  const [channels, setChannels] = useState<ChannelListEntry[]>([]);
  const [dms, setDms] = useState<ChannelListEntry[]>([]);
  const [hosted, setHosted] = useState<HostedSection[]>([]);
  const [hostsRead, setHostsRead] = useState(false);
  const [inbox, setInbox] = useState<InboxRow[]>([]);
  const [unread, setUnread] = useState<Record<string, number>>({});
  const [working, setWorking] = useState<Record<string, string[]>>({});
  const [ready, setReady] = useState(false);
  const [offline, setOffline] = useState<string | null>(null);
  // The bus's word: a stream that ended says the node is away before any load does.
  const [conn, setConn] = useState<ConnState>(FIRST_CONN);
  // The reads that did not answer, in two lists (`degradedReads`): the last
  // load's, and the hosted sections' — which are read again on their own.
  const [loadFailed, setLoadFailed] = useState<string[]>([]);
  const [hostedFailed, setHostedFailed] = useState<string[]>([]);
  const degraded = useMemo(() => degradedReads(loadFailed, hostedFailed), [loadFailed, hostedFailed]);
  const [paused, setPaused] = useState(false);

  const inFlight = useRef<AbortController | null>(null);
  /** Coalesces a burst of frames into one reload. */
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** The reads of the hosted sections, in order (`latestModel`): a load's and a re-read's answers land in whatever order the hosts answer, and only the newest asked for is applied. */
  const hostedReads = useRef(createLatest());
  /** Coalesces a burst of `hosted_changed` frames into one re-read. */
  const pendingHosted = useRef<ReturnType<typeof setTimeout> | null>(null);

  const load = useCallback(async () => {
    inFlight.current?.abort();
    const ac = new AbortController();
    inFlight.current = ac;
    const hostedTicket = hostedReads.current.begin();
    // Each read settles on its own (`workspaceLoadModel`): one route that
    // cannot answer — a goals list that cannot read a run, an inbox that
    // cannot read a goal — costs that list, never the shell. The last
    // values stay up and the chrome names what failed.
    const settled = await Promise.allSettled([
      api.workspace(ac.signal),
      api.agents(ac.signal),
      api.teams(ac.signal),
      api.goals({ archived: true }, ac.signal),
      api.channels(ac.signal),
      api.dms(ac.signal),
      api.inbox(undefined, ac.signal),
      api.allProjects({ archived: true }, ac.signal),
      api.allWorkstreams(ac.signal),
      api.paused(ac.signal),
      loadHosted(ac.signal),
    ]);
    if (ac.signal.aborted) return;
    const { ok, degraded: failed, offline: unreachable } = settleLoads(
      LOAD_NAMES,
      settled as PromiseSettledResult<unknown>[],
      (reason) => reason instanceof ApiError && reason.offline,
    );
    const ws = ok.workspace as WorkspaceInfo | undefined;
    const ag = ok.agents as Awaited<ReturnType<typeof api.agents>> | undefined;
    const tm = ok.teams as Awaited<ReturnType<typeof api.teams>> | undefined;
    const it = ok.goals as Awaited<ReturnType<typeof api.goals>> | undefined;
    const ci = ok.channels as Awaited<ReturnType<typeof api.channels>> | undefined;
    const dm = ok.dms as Awaited<ReturnType<typeof api.dms>> | undefined;
    const ib = ok.inbox as Awaited<ReturnType<typeof api.inbox>> | undefined;
    const pr = ok.projects as Awaited<ReturnType<typeof api.allProjects>> | undefined;
    const wst = ok.workstreams as Awaited<ReturnType<typeof api.allWorkstreams>> | undefined;
    const pz = ok.paused as Awaited<ReturnType<typeof api.paused>> | undefined;
    const hs = ok.hosts as HostedLoad | undefined;
    if (ws) setInfo(ws);
    if (hostedReads.current.lands(hostedTicket)) {
      setHostsRead(!!hs);
      if (hs) {
        setHosted(hs.sections);
        setHostedFailed(hs.failed);
      }
    }
    if (pz) setPaused(pz.paused);
    if (ag) setAgents(ag.agents);
    if (tm) setTeams(tm.teams);
    if (it) {
      // One read, two lists: what every screen sees, and what is put away.
      setGoals(it.goals.filter((g) => !g.archived));
      setArchivedGoals(it.goals.filter((g) => !!g.archived));
    }
    if (pr) {
      setProjects(pr.projects.filter((p) => !p.project.archived));
      setArchivedProjects(pr.projects.filter((p) => !!p.project.archived));
    }
    if (wst) setWorkstreams(wst.workstreams);
    if (ci) setChannels(ci.channels);
    if (dm) setDms(dm.dms);
    if (ib) setInbox(ib.rows);
    // One map keyed by scope id. The two lists are disjoint — `/channels` is
    // standing channels and `/dms` is DMs — so nothing here is written twice;
    // before the route was narrowed every DM landed in this map from both
    // sides, which is the same double-listing the sidebar showed.
    if (ci && dm && ib) {
      const next: Record<string, number> = {};
      for (const c of [...ci.channels, ...dm.dms]) next[c.channel.id] = c.unread_count;
      for (const r of ib.rows) if (r.unread_count) next[r.key] = r.unread_count;
      setUnread(next);
    }
    const degradedNow = [...failed, ...(hs?.failed ?? [])];
    if (degradedNow.length > 0) log.warn("workspace", "some of the workspace could not be read", { degraded: degradedNow });
    setLoadFailed(failed);
    setOffline(unreachable ? offlineWords(nodeFailureReason()) : null);
    setReady(true);
    // The roster as it stands is the truth the working dot is held to: a
    // scope is working where a busy row stands, and a hint whose end was
    // lost stops showing (`workingModel`).
    setWorking((w) => {
      const next = rebuilt(sessionRows());
      return sameWorking(w, next) ? w : next;
    });
  }, []);

  // A row that ended, parked or went idle clears its agent from its scope —
  // whatever frame was lost on the way.
  useEffect(() => onSessionTransition((_prev, row) => setWorking((w) => clearedByRow(w, row))), []);

  /** The hosted sections read again on their own: a host that could not answer is named in the chrome, as at a load, and one that answers again is un-named. */
  const readHosted = useCallback(async () => {
    const ticket = hostedReads.current.begin();
    try {
      const next = await loadHosted();
      if (!hostedReads.current.lands(ticket)) return;
      setHosted(next.sections);
      setHostsRead(true);
      setHostedFailed(next.failed);
      if (next.failed.length > 0) log.warn("workspace", "some hosted lists could not be read", { degraded: next.failed });
    } catch (e) {
      if (!hostedReads.current.lands(ticket)) return;
      log.warn("workspace", "the hosted sections could not be re-read", errorFields(e));
      setHostsRead(false);
      setHostedFailed([failedRead("hosts", e)]);
    }
  }, []);

  const scheduleHosted = useCallback(() => {
    if (pendingHosted.current) return;
    pendingHosted.current = setTimeout(() => {
      pendingHosted.current = null;
      void readHosted();
    }, 250);
  }, [readHosted]);

  const schedule = useCallback(() => {
    if (pending.current) return;
    pending.current = setTimeout(() => {
      pending.current = null;
      void load();
    }, 250);
  }, [load]);

  useEffect(() => {
    void load();
    const reads = hostedReads.current;
    reads.open();
    return () => {
      inFlight.current?.abort();
      if (pending.current) clearTimeout(pending.current);
      if (pendingHosted.current) clearTimeout(pendingHosted.current);
      pending.current = null;
      pendingHosted.current = null;
      reads.close();
    };
  }, [load]);

  useEffect(() => watchConnection(setConn), []);

  // The stream came back after a gap. A node that restarted in it — the
  // sidecar's watchdog after a crash, a person's *Restart* — resumed and
  // failed and withdrew things the frames never carried, so every list is
  // read again once, and the person is told why the screen just moved.
  useEffect(
    () =>
      reloadOnReconnect(watchConnection, () => {
        void load();
        toaster.ok(reconnectWords());
      }),
    [load],
  );

  // One subscription for the whole app.
  useEffect(
    () =>
      busSubscribe({}, (frame) => {
        if (frame.stream === "conversation") {
          const f = frame.payload;
          if (f.snapshot) {
            // A channel/agent definition changed: the lists themselves move.
            schedule();
            return;
          }
          // A message. Patch unread in place rather than refetching every list —
          // this is the frame the views used to drop because it has no `snapshot`.
          if (f.scope) {
            const scope = f.scope;
            setUnread((u) => ({
              ...u,
              [scope]: f.unread_count ?? (u[scope] ?? 0) + 1,
            }));
            // The lists' preview follows the room's last words as the node
            // says them: the frame carries what now stands — after a post, a
            // retraction, a reaction alike — and the row it names takes it as
            // it is (`channelListModel.withLatest`). Nothing is read again; a
            // frame about no room of ours leaves each list the same array.
            setChannels((prev) => withLatest(prev, f));
            setDms((prev) => withLatest(prev, f));
          }
          return;
        }
        if (frame.stream === "inbox") {
          // One conversation's inbox state, patched where it sits. The row
          // set itself only changes when a *new* row appears, and that frame
          // finds no row to patch — so an unrecognised key is the one case
          // that still costs a reload.
          const f = frame.payload;
          setUnread((u) => ({ ...u, [f.key]: f.unread_count }));
          setInbox((prev) => {
            const next = applyDelta(prev, f);
            if (next === prev) schedule();
            return next;
          });
          return;
        }
        // What is left is an engine frame; the bus keeps its own `system`
        // frames to itself, and the type says so here.
        if (frame.stream !== "engine") return;
        const p = frame.payload.payload as unknown as LoosePayload;
        // Idempotent on purpose: a designing agent emits `agent_thinking` on
        // every token, and a new object each time re-rendered the whole app
        // through the context. Same state, same reference, no commit.
        if (p.type === "agent_thinking" && p.scope && p.agent) {
          const { scope, agent } = p;
          setWorking((w) => withThinking(w, scope, agent));
        } else if (p.type === "agent_replied" && p.scope) {
          const { scope, agent } = p;
          setWorking((w) => withReplied(w, scope, agent ?? null));
        } else if (p.type === "paused" || p.type === "resumed") {
          // Not a reload: pausing changes what the engine will do next, not
          // what any list currently holds.
          setPaused(p.type === "paused");
        } else if (p.type === "hosted_changed") {
          // A hosted workspace moved: its sections re-read on their own, a burst
          // of frames as one read.
          scheduleHosted();
        } else if (reloadsWorkspace(p.type)) {
          schedule();
        }
      }),
    [schedule, scheduleHosted],
  );

  const markRead = useCallback((scope: string, host: string | null = null) => {
    if (host) {
      // No frame carries a hosted read back: the section is patched here and stays.
      setHosted((prev) => hostedRead(prev, host, scope) as HostedSection[]);
      void api.markHostedRead(host, scope).catch((e: unknown) => {
        // A mark-read that does not land is not worth a toast; the next load corrects it.
        log.debug("workspace", "the hosted section could not be marked read; the next load corrects it", { scope, host, ...errorFields(e) });
      });
      return;
    }
    setUnread((u) => (u[scope] ? { ...u, [scope]: 0 } : u));
    // The badge drops now; the node's inbox delta confirms the row a moment later.
    setInbox((rows) => rowRead(rows, scope) as InboxRow[]);
    void api.markRead(scope).catch((e: unknown) => {
      // A mark-read that does not land is not worth a toast; the next load corrects it.
      log.debug("workspace", "the scope could not be marked read; the next load corrects it", { scope, ...errorFields(e) });
    });
  }, []);

  const byPubkey = useMemo(() => {
    const m = new Map<string, AgentDef>();
    for (const a of agents) m.set(a.pubkey, a);
    return m;
  }, [agents]);

  // What each principal is called (`namesModel`): a member by the label they
  // gave, an agent by its name, you as *You* until you name yourself.
  const names = useMemo(() => principalNames(info?.pubkey, info?.members, agents), [agents, info]);

  const nameOf = useCallback((pubkey: string) => nameIn(names, pubkey), [names]);
  const photoOf = useCallback(
    (pubkey: string) => photoOfPrincipal(info?.members ?? [], agents, pubkey),
    [agents, info],
  );

  // Above the screen's error boundary: a row without its actions is a
  // count of zero here, never a blank window.
  // What waits on you: the asks, a claim to admit, a harness at its prompt in a terminal.
  const waiting = useMemo(() => inbox.reduce((n, r) => n + needsOf(r).length + (joinOf(r) ? 1 : 0) + (waitingOf(r) ? 1 : 0), 0), [inbox]);

  useWorkspaceRefresher(load);

  return {
    ready,
    offline: offlineLine(offline, conn, nodeFailureReason()),
    degraded,
    paused,
    me: info?.pubkey ?? "",
    info,
    members: info?.members ?? [],
    agents,
    teams,
    goals,
    archivedGoals,
    projects,
    archivedProjects,
    workstreams,
    channels,
    dms,
    hosted,
    hostsRead,
    inbox,
    waiting,
    unread,
    working,
    nameOf,
    photoOf,
    agentByPubkey: (p) => byPubkey.get(p),
    markRead,
    refresh: load,
  };
}

/** Keep `refreshWorkspace` pointed at the mounted provider's load. */
function useWorkspaceRefresher(load: () => void): void {
  useEffect(() => {
    refresher = load;
    return () => {
      if (refresher === load) refresher = null;
    };
  }, [load]);
}
