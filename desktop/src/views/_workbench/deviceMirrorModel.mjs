/**
 * The device mirror (ide/19): a `<img>` of the device's screen, read from
 * the node a few times a second while it is worth reading, and a rectangle
 * the person drags over it, mapped to the device's pixels. Facts with no
 * React in them: when to poll and how often, how a drag becomes a mark and
 * a mark a box to draw, and the words of the boot door. Plain `.mjs`, so
 * `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/** Between two frames while all is well: a few a second, never a video. */
export const FRAME_MS = 500;
/** After a frame failed: slower, so a device that is going down is not hammered. */
export const ERROR_MS = 2000;
/** Failures in a row before the mirror stops and says so. */
export const MAX_ERRORS = 5;
/** A mark smaller than this in device pixels is a click, not a rectangle. */
const MIN_MARK = 4;

/**
 * Whether to read a frame now, and how soon the next: only while the device
 * is up, the window is awake and in front — a mirror nobody sees costs the
 * simulator a capture for nothing — and not after too many failures.
 * @param {{up: boolean, awake: boolean, focused: boolean, errors: number}} facts
 * @returns {{poll: boolean, every: number, reason: string | null, stopped: boolean}} `stopped`: the device stopped answering — the one pause a person can end by asking again
 */
export function mirrorCadence({ up, awake, focused, errors }) {
  if (!up) return { poll: false, every: 0, reason: t("workbench-device-mirror-device-shut-down"), stopped: false };
  if (errors >= MAX_ERRORS) return { poll: false, every: 0, reason: t("workbench-device-mirror-device-stopped-answering-boot-again-check"), stopped: true };
  if (!awake) return { poll: false, every: 0, reason: t("workbench-device-mirror-window-hidden"), stopped: false };
  if (!focused) return { poll: false, every: 0, reason: t("workbench-device-mirror-window-background"), stopped: false };
  return { poll: true, every: errors > 0 ? ERROR_MS : FRAME_MS, reason: null, stopped: false };
}

/**
 * Where the picture is drawn inside its element under `object-fit: contain`:
 * scaled to fit, centred, letterboxed on the longer side.
 * @param {{clientWidth: number, clientHeight: number, naturalWidth: number, naturalHeight: number}} box
 */
export function drawnRect(box) {
  const { clientWidth, clientHeight, naturalWidth, naturalHeight } = box;
  if (!(clientWidth > 0 && clientHeight > 0 && naturalWidth > 0 && naturalHeight > 0)) return null;
  const scale = Math.min(clientWidth / naturalWidth, clientHeight / naturalHeight);
  const width = naturalWidth * scale;
  const height = naturalHeight * scale;
  return { left: (clientWidth - width) / 2, top: (clientHeight - height) / 2, width, height, scale };
}

/**
 * A drag over the picture as a rectangle in device pixels, clamped to the
 * screen; `null` for a drag too small to be a rectangle — a click — or a
 * picture not drawn yet.
 * @param {{x: number, y: number}} start the pointer, relative to the element
 * @param {{x: number, y: number}} end
 * @param {{clientWidth: number, clientHeight: number, naturalWidth: number, naturalHeight: number}} box
 * @returns {{x: number, y: number, width: number, height: number} | null}
 */
export function markFromDrag(start, end, box) {
  const drawn = drawnRect(box);
  if (!drawn) return null;
  const clamp = (v, max) => Math.min(Math.max(v, 0), max);
  const toDevice = (p) => ({
    x: clamp(Math.round((p.x - drawn.left) / drawn.scale), box.naturalWidth),
    y: clamp(Math.round((p.y - drawn.top) / drawn.scale), box.naturalHeight),
  });
  const a = toDevice(start);
  const b = toDevice(end);
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  const width = Math.abs(a.x - b.x);
  const height = Math.abs(a.y - b.y);
  if (width < MIN_MARK || height < MIN_MARK) return null;
  return { x, y, width, height };
}

/**
 * A mark as a box to draw over the element, in its pixels.
 * @param {{x: number, y: number, width: number, height: number}} mark
 * @param {{clientWidth: number, clientHeight: number, naturalWidth: number, naturalHeight: number}} box
 * @returns {{left: number, top: number, width: number, height: number} | null}
 */
export function markCss(mark, box) {
  const drawn = drawnRect(box);
  if (!drawn) return null;
  return {
    left: drawn.left + mark.x * drawn.scale,
    top: drawn.top + mark.y * drawn.scale,
    width: mark.width * drawn.scale,
    height: mark.height * drawn.scale,
  };
}

/**
 * The words of the door drawn where the mirror would be while the device
 * is not up.
 * @param {{name: string, kind: "simulator" | "emulator" | "physical", state: string}} device
 * @returns {{title: string, hint: string, verb: string | null}}
 */
export function bootDoorWords(device) {
  if (device.kind === "physical") {
    return { title: t("workbench-device-mirror-offline", { device: device.name }), hint: t("workbench-device-mirror-plug-unlock-trust-computer-mirror-follows"), verb: null };
  }
  return { title: t("workbench-device-mirror-shut-down", { device: device.name }), hint: device.kind === "simulator" ? t("workbench-device-mirror-boot-simulator-see-screen-here-run") : t("workbench-device-mirror-start-emulator-see-screen-here-run"), verb: device.kind === "simulator" ? t("workbench-device-mirror-boot") : t("workbench-device-mirror-start") };
}
