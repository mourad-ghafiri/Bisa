/**
 * A device's screen, captured (ide/19) — the mobile cousin of
 * `browserShots.ts`. The node takes it (`POST /mobile-development/devices/{id}/screenshot`)
 * and stores it as an attachment with a named copy; this file copies it to
 * the clipboard, saves it where the person chooses, or attaches it to the
 * Agent panel as a `capture` chip of the whole screen.
 */

import { api, saveArtifactCopy } from "../api";
import type { MobileDevelopmentShot } from "../types";
import { attachContext } from "../views/_workbench/agentPaneStore";
import { captureChip } from "../views/_workbench/captureModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** Capture the screen now — stored by the node, answered as an attachment and its named copy's path. */
export function takeDeviceShot(id: string): Promise<MobileDevelopmentShot> {
  return api.mobileDevelopmentScreenshot(id);
}

function bytesOf(shot: MobileDevelopmentShot): Promise<Uint8Array> {
  return api.attachmentBytes(shot.attachment.sha256);
}

/** Put the capture on the clipboard as an image — the window's own clipboard, from a click. */
export async function copyDeviceShot(shot: MobileDevelopmentShot): Promise<void> {
  const bytes = await bytesOf(shot);
  const blob = new Blob([bytes.slice().buffer as ArrayBuffer], { type: "image/png" });
  await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
}

/** Save the capture where the person chooses, from its named copy. Answers the path, or `null` when dismissed. */
export function saveDeviceShot(shot: MobileDevelopmentShot): Promise<string | null> {
  return saveArtifactCopy(shot.path, shot.attachment.name, () => bytesOf(shot), "image/png");
}

/** Attach the whole screen to the Agent panel of a scope, for the person to write about there. */
export function attachDeviceShot(shot: MobileDevelopmentShot, device: string, label: string, scope: string): void {
  attachContext(captureChip(device, label, shot.attachment, null, t("shell-device-shots-screen-now")), scope);
}
