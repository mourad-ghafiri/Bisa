/**
 * The desktop shell's permission commands — the Tauri side of Settings ›
 * Capabilities › System (ide/01: a grant is the machine's, so the shell asks).
 * Tauri-only: a browser dev session has no `#[tauri::command]`s, so these
 * answer `null` and the panel says so instead of guessing.
 */

import { inDesktopShell } from "../api";

/** The grants `permissions.rs` knows, spelled as it spells them. */
export type PermissionKind = "full_disk_access" | "microphone";

export type PermissionStatus = "granted" | "denied" | "not_determined" | "unsupported";

/** What macOS said about one grant, mirrored from the Rust `PermissionReport`. */
export interface PermissionReport {
  kind: PermissionKind;
  status: PermissionStatus;
  /** The reason behind `unsupported` or `not_determined`. */
  detail?: string | null;
}

/** Read a grant's status without prompting; `null` off the desktop shell. */
export async function permissionStatus(kind: PermissionKind): Promise<PermissionReport | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PermissionReport>("system_permission_status", { kind });
}

/**
 * Ask for a grant — macOS's own prompt for the microphone, the System Settings
 * pane for Full Disk Access (which has none) — and answer the status after.
 */
export async function requestPermission(kind: PermissionKind): Promise<PermissionReport | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PermissionReport>("system_permission_request", { kind });
}
