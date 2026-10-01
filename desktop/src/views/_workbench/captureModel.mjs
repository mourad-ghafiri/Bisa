/**
 * Marking a device's screen for an agent (ide/19), as facts with no React
 * in them — the mirror's cousin of `annotationModel.mjs`. A person turns
 * the wand on, drags a rectangle over the mirror (or marks the whole
 * screen) and says what should change; the screen is captured at that
 * moment (`POST /mobile-development/devices/{id}/screenshot`, an attachment the node
 * holds) and each capture becomes a numbered badge over the mirror and a
 * line in the tray. The captures are a **draft of the session** on that one
 * device — nothing on disk, nothing on the node but the pictures. Sending
 * posts them as `capture` chips under the edit contract; attaching moves
 * them to the Agent pane's tray; either empties the draft.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * @typedef {{x: number, y: number, width: number, height: number}} Mark
 * @typedef {{sha256: string, name: string, mime: string, size: number}} AttachmentRef
 * @typedef {{id: number, mark: Mark | null, note: string, shot: AttachmentRef, label: string, width: number | null, height: number | null}} Capture
 * @typedef {{seq: number, captures: Capture[], message: string}} CaptureDraft
 */

/** No capture, no words: what a device starts with. */
export const EMPTY_CAPTURE_DRAFT = Object.freeze({ seq: 1, captures: Object.freeze([]), message: "" });

/**
 * Add a capture — the screen as it was, the rectangle drawn (none for the
 * whole screen), the change wanted. A note with no words adds nothing.
 * @param {CaptureDraft} draft
 * @param {{shot: AttachmentRef, mark: Mark | null, label: string, width?: number | null, height?: number | null}} taken
 * @param {string} note
 * @returns {CaptureDraft}
 */
export function addCapture(draft, taken, note) {
  const words = String(note ?? "").trim();
  if (!words) return draft;
  const capture = { id: draft.seq, mark: taken.mark ?? null, note: words, shot: taken.shot, label: taken.label, width: taken.width ?? null, height: taken.height ?? null };
  return { seq: draft.seq + 1, captures: [...draft.captures, capture], message: draft.message };
}

/** Drop one capture; the numbers of the rest follow the list. @param {CaptureDraft} draft @param {number} id */
export function removeCapture(draft, id) {
  const captures = draft.captures.filter((c) => c.id !== id);
  return captures.length === draft.captures.length ? draft : { ...draft, captures };
}

/** @param {CaptureDraft} draft @param {string} message */
export function setCaptureMessage(draft, message) {
  return { ...draft, message };
}

/** A mark as a sentence names it. @param {Mark | null} mark */
export function markWords(mark) {
  if (!mark) return t("workbench-capture-whole-screen");
  return t("workbench-capture-mark-at", { x: mark.x, y: mark.y, width: mark.width, height: mark.height });
}

/** A capture's line in the tray: its number, where, what. @param {number} n @param {Capture} c */
export function captureLabel(n, c) {
  return `${n} · ${markWords(c.mark)} — ${c.note}`;
}

/**
 * The chip a capture becomes: the device by id and name, the picture as the
 * node holds it, the rectangle in device pixels, the change wanted — what
 * the engine frames as `[capture]` and hands the agent as a file to read.
 * @param {string} device
 * @param {string} label
 * @param {AttachmentRef} shot
 * @param {Mark | null} mark
 * @param {string} note
 */
export function captureChip(device, label, shot, mark, note) {
  return { kind: "capture", device, label, shot, mark: mark ?? null, note };
}

/** @param {string} device @param {string} label @param {readonly Capture[]} captures */
export function captureChips(device, label, captures) {
  return captures.map((c) => captureChip(device, label, c.shot, c.mark, c.note));
}

/** What the edit is about, for the request's target. @param {number} count @param {string} label */
export function capturesTarget(count, label) {
  return count === 1 ? t("workbench-capture-marked-spot-screen", { label }) : t("workbench-capture-marked-spots-screen", { count, label });
}

/** The request's text: the person's words, then what the chips are. @param {string} words @param {number} count */
export function capturesContent(words, count) {
  const own = String(words ?? "").trim();
  const where =
    count === 1
      ? t("workbench-capture-captured-screen-attached-chip-device-picture")
      : t("workbench-capture-each-captured-screens-attached-chip-device", { count });
  return own ? `${own}\n\n${where}` : where;
}

/** The session draft's key for a device in a scope. @param {string} scope @param {string} deviceId */
export function capturesKey(scope, deviceId) {
  return `${scope}|captures|${deviceId}`;
}
