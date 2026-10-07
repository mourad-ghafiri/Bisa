/**
 * The desktop shell's node commands — what the shell knows of the node as
 * its supervisor: running or not, the pid it sees, whether a person started
 * the node outside the app, how many unasked-for restarts, how long healthy,
 * whether it is still booting, and why there is none while there is none;
 * and the one verb, a restart. What the node knows of itself is
 * `GET /node`'s. Tauri-only: a browser dev session answers `null`. The
 * shapes mirror `src-tauri/src/sidecar.rs`.
 */

import { inDesktopShell } from "../api";

/** Why the shell has no node — `Failure` in `sidecar.rs`, tagged by `kind`. */
export interface NodeFailure {
  kind: "exited" | "timed_out" | "no_binary" | "held_by_other";
  /** `exited`: how — *exited with code 1*, *was ended by signal 9*. */
  how?: string;
  /** `exited`, `timed_out`: the node's last stderr lines. */
  said?: string[];
  /** `timed_out`: how long the shell waited. */
  secs?: number;
  /** `no_binary`: what each binary answered. */
  tried?: string[];
  /** `held_by_other`: the process holding the workspace. */
  pid?: number;
}

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
  /** The child is up and not yet answering: it is booting. */
  booting?: boolean;
  /** Why there is no node, while there is none. */
  failure?: NodeFailure | null;
}

/** The node as the shell supervises it; `null` off the desktop shell. */
export async function nodeStatus(): Promise<NodeStatus | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<NodeStatus>("node_status");
}

/**
 * Ask the shell to restart the node: stopped gracefully, started again on
 * its port. The answer is the shell's view of the new child, booting; the
 * webview hears `node:boot` and `node:restarted` as it comes up
 * (`nodeBootStore`). `null` off the desktop shell, where there is nothing
 * to restart.
 */
export async function restartNode(): Promise<NodeStatus | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<NodeStatus>("restart_node");
}
