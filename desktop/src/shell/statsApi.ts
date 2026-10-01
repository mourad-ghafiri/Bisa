/**
 * The desktop shell's resource commands — the Tauri side of the footer's
 * CPU, GPU, memory and disk read-outs. Tauri-only: a browser dev session has
 * no `#[tauri::command]`s, so these answer with `null` and the footer shows
 * nothing.
 */

import { terminalAvailable } from "../terminal/session";

/** What the machine is — read once. */
export interface HostInfo {
  hostname?: string | null;
  os?: string | null;
  /** Logical cores: what a process's per-core `cpu_percent` is divided by. */
  cores: number;
  /** The CPU's brand — the GPU's name too on a machine whose accelerator shares the chip. */
  cpu?: string | null;
}

/** The accelerator's load, where the shell has a reader. */
export interface GpuLoad {
  util_percent: number;
  renderer_percent?: number | null;
  tiler_percent?: number | null;
  mem_used?: number | null;
}

/** What the machine is doing now. */
export interface HostLoad {
  /** System-wide CPU usage, 0–100. */
  cpu_percent: number;
  load: [number, number, number];
  mem_used: number;
  mem_total: number;
  swap_used: number;
  swap_total: number;
  uptime_secs: number;
  gpu?: GpuLoad | null;
}

/** Who a process belongs to — the shell's nearest named root. */
export type Root = { kind: "desktop" } | { kind: "node" } | { kind: "terminal"; terminal_id: string } | { kind: "session"; pid: number };

/** One process the shell can name, and what it takes. */
export interface ProcessShare {
  pid: number;
  name: string;
  root: Root;
  /** Per core — a busy tree can exceed 100. */
  cpu_percent: number;
  mem_bytes: number;
  /** Over the last interval, never cumulative. */
  read_bytes: number;
  written_bytes: number;
  run_secs: number;
}

export interface ResourceUsage {
  host: HostLoad;
  processes: ProcessShare[];
  /** Seconds the deltas span. */
  interval_secs: number;
  /** How long the read took. */
  read_ms: number;
}

/** One directory (or first-level file) under the data dir and its bytes. */
export interface DirSize {
  /** Relative, `/`-joined: `goals/01J…`, `logs`, `index`. */
  path: string;
  bytes: number;
}

export interface VolumeUsage {
  mount: string;
  used: number;
  total: number;
}

export interface DiskUsage {
  total: number;
  dirs: DirSize[];
  volume?: VolumeUsage | null;
  read_ms: number;
}

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T | null> {
  if (!terminalAvailable()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

/** What the machine is; `null` off the desktop shell. */
export function hostInfo(): Promise<HostInfo | null> {
  return invoke<HostInfo>("host_info");
}

/** The machine's load and every named process's share; `roots` are the roster's session pids. */
export function resourceUsage(roots: readonly number[]): Promise<ResourceUsage | null> {
  return invoke<ResourceUsage>("resource_usage", { roots });
}

/** The data directory's sizes by area and the volume it lives on; `null` off the desktop shell. */
export function dataDirUsage(path: string): Promise<DiskUsage | null> {
  if (!path) return Promise.resolve(null);
  return invoke<DiskUsage>("data_dir_usage", { path });
}
