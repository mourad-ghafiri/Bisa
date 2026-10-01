import type { ListeningPort } from "../../terminal/session";
import type { TerminalSessionState } from "../../shell/terminalsModel.mjs";
import type { SessionRow } from "../../types";

export interface WorkstreamPort {
  port: number;
  pid: number;
  process: string;
  workstream: string;
  via: { kind: "shell"; key: string; harness: string | null } | { kind: "harness"; session: string; harness: string | null };
}

export declare function portUrl(port: number): string;
export declare function attributePorts(
  ports: readonly ListeningPort[],
  terminals: readonly TerminalSessionState[],
  sessions: readonly SessionRow[],
): WorkstreamPort[];
export declare function portsOf(attributed: readonly WorkstreamPort[], workstream: string): WorkstreamPort[];
export declare function portTitle(p: WorkstreamPort, harnessLabels?: Record<string, string>): string;
export declare function stopPrompt(p: WorkstreamPort): string;

export interface PortOwner {
  kind: "goal" | "project" | "workstream" | "work_item" | "shell" | "harness";
  id: string;
  harness: string | null;
  via: "shell" | "harness";
  goal?: string | null;
  project?: string | null;
  workstream?: string | null;
}

export interface PlacedPort {
  port: number;
  pid: number;
  process: string;
  owner: PortOwner;
}

export interface PortGroup {
  kind: PortOwner["kind"];
  id: string;
  owner: PortOwner;
  ports: PlacedPort[];
}

export declare function globalPorts(
  ports: readonly ListeningPort[],
  terminals: readonly TerminalSessionState[],
  sessions: readonly SessionRow[],
): PlacedPort[];
export declare function groupPorts(placed: readonly PlacedPort[]): PortGroup[];
export declare function portsSummary(placed: readonly PlacedPort[]): string;
export declare function portsKeptWords(stale: string | null | undefined): string;
export declare function stopPlacedPrompt(p: PlacedPort): string;
