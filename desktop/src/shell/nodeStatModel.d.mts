import type { ConnState } from "../bus";
import type { NodeInfo, RelayCheck, RelayHealth, SyncReport, WorkspaceInfo } from "../types";
import type { NodeStatus } from "./nodeApi";
import type { ProcessShare } from "./statsApi";

export type Tone = "ok" | "warn" | "danger" | "quiet";

export interface StatWords {
  tone: "ok" | "warn" | "danger";
  word: "connected" | "connecting" | "unreachable";
  title: string;
}
export declare function statWords(conn: ConnState): StatWords;
export declare function sinceWords(secs: number): string;
export declare function sinceOf(at: number, now: number): number;

export interface OverlaySection {
  key: string;
  title: string;
  tone?: Tone;
  label?: string;
  sentence: string | null;
  rows: { label: string; value: string }[];
}
export declare function overlaySections(
  conn: ConnState,
  facts: {
    info: NodeInfo | null;
    status: NodeStatus | null;
    workspace: WorkspaceInfo | null;
    sync: SyncReport | null;
    checks: readonly RelayCheck[] | null;
    paused: boolean | null;
    share: ProcessShare | null;
    appVersion: string;
    apiBase: string;
    now: number;
  },
): OverlaySection[];
export declare function nodeShare(processes: readonly ProcessShare[] | null | undefined): ProcessShare | null;
export declare function footnote(readAt: number | null, now?: number): string;
