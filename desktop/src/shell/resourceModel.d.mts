/**
 * Types for `resourceModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

import type { SessionRow } from "../types";
import type { DiskUsage, HostInfo, HostLoad, ProcessShare, VolumeUsage } from "./statsApi";
import type { PlaceIndex } from "./footerSessionsModel.mjs";
import type { TerminalSessionState } from "./terminalsModel.mjs";

export type Metric = "cpu" | "gpu" | "memory" | "disk";
export type Dimension = "platform" | "areas" | "goals" | "projects" | "workflows" | "harnesses" | "terminals" | "activity";

export declare const DIMENSIONS: Readonly<Record<Metric, readonly Dimension[]>>;
export declare function dimensionLabel(dimension: Dimension | string): string;
export type DimensionIcon = "platform" | "folder" | "goal" | "project" | "workflow" | "harness" | "shell" | "pulse";
export declare function dimensionIcon(dimension: Dimension | string): DimensionIcon;
export declare function chosenDimension(metric: Metric, remembered: string | null | undefined): Dimension | null;

/** One process, joined to what the rosters know about its root. */
export interface Share {
  kind: "desktop" | "node" | "session" | "terminal";
  sessionId: string | null;
  terminalKey: string | null;
  terminalScope?: string;
  terminalId?: string;
  harness: string | null;
  agent: string | null;
  goal: string | null;
  /** The run of the workspace a session works on, which no goal holds. */
  run?: string | null;
  project: string | null;
  workstream: string | null;
  /** A session root the roster no longer names. */
  ended?: boolean;
  pid: number;
  name: string;
  /** Per core. */
  cpu: number;
  mem: number;
  read: number;
  written: number;
}

/** Where a row leads. */
export type Door =
  | { kind: "goal"; id: string }
  | { kind: "project"; id: string }
  | { kind: "workflow"; id: string }
  | { kind: "run"; id: string }
  | { kind: "terminal"; key: string; scope: string; id: string }
  | null;

export interface Row {
  key: string;
  label: string;
  sub: string | null;
  door: Door;
  value: number;
  count: number | null;
}

export interface PlatformRow extends Row {
  share: number;
}

export interface Segment {
  key: string;
  label: string;
  percent: number;
  tone: "accent" | "ok" | "warn" | "quiet" | "neutral";
  value: number;
}

export interface LegendItem {
  key: string;
  label: string;
  tone: "accent" | "neutral" | "track";
  words: string;
}

export interface UsageBar {
  segments: Segment[];
  legend: LegendItem[];
  label: string;
}

export interface OverlayContext {
  shares: readonly Share[];
  host?: HostLoad | null;
  info?: HostInfo | null;
  disk?: DiskUsage | null;
  places?: PlaceIndex;
  harnessLabels?: Record<string, string>;
}

export declare function attributeShares(
  processes: readonly ProcessShare[] | null | undefined,
  sessions: readonly SessionRow[] | null | undefined,
  terminals: readonly TerminalSessionState[] | null | undefined,
  places?: PlaceIndex,
): Share[];
export declare function oursOf(shares: readonly Share[], metric: "cpu" | "memory", cores: number): number;
export declare function platformRows(shares: readonly Share[], host: HostLoad | null | undefined, cores: number, metric: "cpu" | "memory"): PlatformRow[];
export declare function usageBar(rows: readonly PlatformRow[], metric: "cpu" | "memory"): UsageBar;
export declare function percentWords(n: number): string;
export declare function dimensionRows(
  dimension: "goals" | "projects" | "workflows" | "harnesses" | "terminals",
  shares: readonly Share[],
  places: PlaceIndex,
  metric: "cpu" | "memory",
  cores: number,
  harnessLabels?: Record<string, string>,
): Row[];
export declare function foldRows<R extends { value: number }>(rows: readonly R[], n?: number): { rows: R[]; more: { count: number; value: number } | null };
export declare function valueWords(metric: Metric, value: number): string;
export declare function moreWords(metric: Metric, more: { count: number; value: number } | null): string;
export declare function rowPercent(row: { value: number }, rows: readonly { value: number }[]): number;
export declare function metricWords(metric: Metric, host: HostLoad | null | undefined, info: HostInfo | null | undefined, ours: number, disk?: DiskUsage | null): { value: string; title: string };
export declare function memWords(used: number | null | undefined, total: number | null | undefined): string;
export declare function volumeWords(volume: VolumeUsage | null | undefined): string;
export declare function footnote(metric: Metric, pollMs: number, readMs: number | null | undefined, intervalSecs: number | null | undefined): string;
export declare function areas(disk: DiskUsage | null | undefined): Row[];
export declare function goalsDisk(disk: DiskUsage | null | undefined, places?: PlaceIndex): Row[];
export declare function projectsDisk(disk: DiskUsage | null | undefined, places?: PlaceIndex): Row[];
export declare function harnessesDisk(disk: DiskUsage | null | undefined, harnessLabels?: Record<string, string>): Row[];
export declare function terminalsDisk(disk: DiskUsage | null | undefined): Row[];
export declare function activityRows(shares: readonly Share[]): Row[];
export declare function overlayRows(metric: "cpu" | "memory" | "disk", dimension: Dimension, ctx: OverlayContext): Row[];
export declare function staleWords(stale: string | null | undefined): string;
