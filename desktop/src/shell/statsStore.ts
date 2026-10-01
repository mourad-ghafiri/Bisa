/**
 * The footer's resources — one module store, polled, on the `portsStore`
 * pattern (awake-gated, `inFlight`-guarded, paused when nothing reads it).
 *
 * Two cadences, deliberately apart: the host's load and every named
 * process's share are cheap and move, so they refresh every 5 s; the data
 * directory's sizes are a walk of the whole tree and must never ride the
 * fast tick, so they refresh on their own slow timer. The data-dir path is
 * learned once from the node (`GET /workspace`); what the machine *is* is
 * read once from the shell.
 *
 * Three parts, each compared on its own, so a reading that did not move
 * re-renders nothing: the host (the bar's four values), the processes (an
 * open overlay's rows) and the disk. The bar subscribes to the host alone;
 * an overlay to what it shows. What a reading changes, and what a failed one
 * keeps, is `statsModel.mjs`'s; this file polls and notifies.
 */

import { errorFields, log } from "../log";
import { useEffect, useSyncExternalStore } from "react";
import { api } from "../api";
import { dataDirUsage, hostInfo, resourceUsage, type DiskUsage, type HostInfo, type HostLoad, type ProcessShare } from "./statsApi";
import { rootPids } from "./portsStore";
import { useSessions } from "./sessionsStore";
import { terminalAvailable } from "../terminal/session";
import { isHidden, onVisibilityChange } from "./visibility";
import { EMPTY_DISK, EMPTY_HOST, EMPTY_PROCESSES, afterFailure, afterReading, failureIsNews, processesAfter, samePart, withInfo } from "./statsModel.mjs";

/** The host and the processes — cheap, and they move. Tunable via
 *  `cache.desktop.stats_poll_ms` / `cache.desktop.disk_poll_ms`. */
let statsPollMs = 5_000;
/** The data-dir walk — slow and cached. */
let diskPollMs = 60_000;

/** Retune the two footer cadences; restarts the timers at the new intervals. */
export function setStatsPollMs(stats: number, disk: number): void {
  let changed = false;
  if (Number.isFinite(stats) && stats > 0 && stats !== statsPollMs) {
    statsPollMs = stats;
    changed = true;
  }
  if (Number.isFinite(disk) && disk > 0 && disk !== diskPollMs) {
    diskPollMs = disk;
    changed = true;
  }
  if (changed && fastTimer) {
    clearInterval(fastTimer);
    fastTimer = null;
    if (slowTimer) clearInterval(slowTimer);
    slowTimer = null;
    schedule();
  }
}

/** The fast cadence in force — the overlay's footnote says it. */
export function statsPoll(): number {
  return statsPollMs;
}

export interface HostPart {
  readonly info: HostInfo | null;
  readonly load: HostLoad | null;
  /** Why the last read failed, while the last reading stands; `null` while the reads answer. */
  readonly stale: string | null;
}

export interface ProcessPart {
  readonly processes: readonly ProcessShare[];
  readonly intervalSecs: number | null;
  readonly readMs: number | null;
}

export interface DiskPart {
  readonly disk: DiskUsage | null;
  /** The data directory, for a tooltip. */
  readonly dataDir: string | null;
}

let host: HostPart = EMPTY_HOST;
let procs: ProcessPart = EMPTY_PROCESSES;
let diskPart: DiskPart = EMPTY_DISK;

const listeners = new Set<() => void>();
let fastTimer: ReturnType<typeof setInterval> | null = null;
let slowTimer: ReturnType<typeof setInterval> | null = null;
let roots: readonly number[] = [];
let dataDir: string | null = null;
let awake = false;
let inFlight = false;
let diskInFlight = false;

function notify() {
  for (const l of listeners) l();
}

function setHost(next: HostPart) {
  if (samePart(next, host)) return;
  host = next;
  notify();
}

function setProcesses(next: ProcessPart) {
  // The model says whether a read is a change (`statsModel.processesAfter`):
  // `readMs` moving alone is not one.
  const after = processesAfter(procs, next);
  procs = after.procs;
  if (after.changed) notify();
}

function setDisk(next: DiskPart) {
  if (samePart(next, diskPart)) return;
  diskPart = next;
  notify();
}

async function tick(): Promise<void> {
  if (inFlight || !terminalAvailable()) return;
  inFlight = true;
  try {
    if (!host.info) {
      const info = await hostInfo();
      if (info) setHost(withInfo(host, info));
    }
    const usage = await resourceUsage(roots);
    if (usage) {
      setHost(afterReading(host, usage.host));
      setProcesses({ processes: usage.processes, intervalSecs: usage.interval_secs, readMs: usage.read_ms });
    }
  } catch (e) {
    // The last reading stands, marked as kept; the next tick asks again.
    const why = e instanceof Error ? e.message : String(e);
    if (failureIsNews(host, why)) log.debug("stats", "the resources could not be read; the last reading stands", { reason: why });
    setHost(afterFailure(host, why));
  } finally {
    inFlight = false;
  }
}

async function ensureDataDir(): Promise<void> {
  if (dataDir !== null) return;
  try {
    const info = await api.workspace();
    dataDir = info.data_dir ?? "";
    setDisk({ ...diskPart, dataDir: dataDir || null });
  } catch (e) {
    // No node yet; the slow tick asks again.
    log.debug("stats", "the data directory is not known yet", errorFields(e));
  }
}

async function diskTick(): Promise<void> {
  if (diskInFlight || !terminalAvailable()) return;
  diskInFlight = true;
  try {
    await ensureDataDir();
    if (dataDir) {
      const disk = await dataDirUsage(dataDir);
      if (disk) setDisk({ ...diskPart, disk });
    }
  } catch (e) {
    // The last sizes stand; a disk walk that keeps failing is a line, not a blank.
    log.debug("stats", "the disk could not be walked; the last sizes stand", errorFields(e));
  } finally {
    diskInFlight = false;
  }
}

function schedule(): void {
  const run = awake && listeners.size > 0 && !isHidden();
  if (run && !fastTimer) {
    void tick();
    void diskTick();
    fastTimer = setInterval(() => void tick(), statsPollMs);
    slowTimer = setInterval(() => void diskTick(), diskPollMs);
  } else if (!run && fastTimer) {
    clearInterval(fastTimer);
    fastTimer = null;
    if (slowTimer) clearInterval(slowTimer);
    slowTimer = null;
  }
}

// A hidden window reads nothing; on becoming visible it refreshes at once.
onVisibilityChange(schedule);

function subscribe(l: () => void) {
  listeners.add(l);
  schedule();
  return () => {
    listeners.delete(l);
    schedule();
  };
}

/**
 * Keep the store awake for a reader. Awake only in the desktop shell (a
 * browser has no command to read); the harness roots follow the roster so
 * every running session's tree is named.
 */
function useAwake(): void {
  const sessions = useSessions();
  const nextRoots = rootPids(sessions);
  const key = nextRoots.join(",");
  useEffect(() => {
    roots = nextRoots;
    awake = terminalAvailable();
    schedule();
    // `nextRoots` is a fresh array every render; `key` is its text.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
}

/** The machine — what it is and its load — for the bar's read-outs. */
export function useHostLoad(): HostPart {
  useAwake();
  return useSyncExternalStore(subscribe, () => host, () => host);
}

/** Every named process's share — for an open overlay. */
export function useProcessShares(): ProcessPart {
  useAwake();
  return useSyncExternalStore(subscribe, () => procs, () => procs);
}

/** The data directory's sizes — for the bar's disk value and its overlay. */
export function useDiskUsage(): DiskPart {
  useAwake();
  return useSyncExternalStore(subscribe, () => diskPart, () => diskPart);
}
