/**
 * A screenshot of a tab, taken and delivered (ide/18): the shell's own
 * snapshot of the webview (`browser/session.ts`), as PNG bytes — uploaded
 * to the workspace's attachments for an agent (the engine answers the path
 * of a named copy), copied to the clipboard or saved where the person
 * chooses for the bar's camera. A shot is of a tab that renders: `takeShot`
 * waits for the tab to show (`browserShown.ts`) before it asks, since a
 * hidden webview renders nothing and a snapshot asked of one never comes,
 * and refuses as {@link NotShownError} when it does not show in time — the
 * one door, so neither the camera nor the bridge can forget. The webview
 * never names a path: a save goes through the named copy the node makes,
 * as an artifact's does; a copy goes through the shell's clipboard, which
 * needs no gesture (`ui/clipboard.ts`).
 */

import { api, saveArtifactCopy } from "../api";
import type { AttachmentRef } from "../types";
import { screenshotBrowserView } from "../browser/session";
import { copyImage } from "../ui/clipboard";
import { browserPrefs } from "./browserPrefsStore";
import { CLIPBOARD_REFUSED, NOT_SHOWN_WORDS, pngSize, shotName } from "./browserShotModel.mjs";
import { untilShown } from "./browserShown";

export interface Shot {
  readonly bytes: Uint8Array;
  readonly name: string;
  readonly width: number;
  readonly height: number;
}

/** How long the shell gets to render the snapshot before the ask is given up. */
const SHOT_MS = 15_000;

/** The tab did not show in time, so nothing was asked of it — the person's words; the bridge answers the agent's. */
export class NotShownError extends Error {
  override readonly name = "NotShownError";
  constructor() {
    super(NOT_SHOWN_WORDS);
  }
}

/** Snapshot a tab at the workspace's width, once it shows; {@link NotShownError} when it does not. */
export async function takeShot(key: string): Promise<Shot> {
  if (!(await untilShown(key))) throw new NotShownError();
  const bytes = await Promise.race([
    screenshotBrowserView(key, browserPrefs().shotWidth),
    new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error("the snapshot took too long")), SHOT_MS)),
  ]);
  const size = pngSize(bytes);
  if (!size) throw new Error("the snapshot is not a PNG");
  return { bytes, name: shotName(key), ...size };
}

/** The shot's bytes as a `File`, the shape the upload takes. */
function fileOf(shot: Shot): File {
  return new File([shot.bytes.slice().buffer as ArrayBuffer], shot.name, { type: "image/png" });
}

/** How long an upload may take before the tab stops reading busy: a stalled node ends in a refusal, never in a dot that pulses for good. */
export const UPLOAD_MS = 30_000;

/** Upload a shot to the workspace's attachments; what an agent's answer names. */
export function uploadShot(shot: Shot): Promise<AttachmentRef> {
  return api.uploadAttachment(fileOf(shot), AbortSignal.timeout(UPLOAD_MS));
}

/** Put a shot on the clipboard as a picture — through the kit's one door, which needs no gesture in the desktop. */
export async function copyShot(shot: Shot): Promise<void> {
  if (!(await copyImage(shot.bytes))) throw new Error(CLIPBOARD_REFUSED);
}

/**
 * Save a shot where the person chooses: uploaded first, named by the node,
 * then copied from that named copy — the one route by which a file leaves
 * the store for a place the person picked. Answers the path, or `null`
 * when the dialog was dismissed.
 */
export async function saveShot(shot: Shot): Promise<string | null> {
  const uploaded = await uploadShot(shot);
  const named = await api.attachmentFile(uploaded.sha256, shot.name);
  return saveArtifactCopy(named.path, shot.name, () => Promise.resolve(shot.bytes), "image/png");
}
