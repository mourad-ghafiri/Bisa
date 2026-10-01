/**
 * The desktop shell's node command — what the shell knows of the node as
 * its supervisor: running or not, the pid it sees, whether a person started
 * the node outside the app, how many unasked-for restarts, how long healthy.
 * What the node knows of itself is `GET /node`'s. Tauri-only: a browser dev
 * session answers `null`. The shape mirrors `src-tauri/src/sidecar.rs`.
 */

import { inDesktopShell } from "../api";

export interface NodeStatus {
  running: boolean;
  pid?: number | null;
  port: number;
  /** A node a person started (`BISA_API_BASE`), not this app's child. */
  external: boolean;
  /** Unasked-for restarts in a row — reset once the node stays healthy. */
  restarts: number;
  /** How long the child has been up and answering, in seconds. */
  healthy_secs?: number | null;
}

/** The node as the shell supervises it; `null` off the desktop shell. */
export async function nodeStatus(): Promise<NodeStatus | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<NodeStatus>("node_status");
}
