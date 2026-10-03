/**
 * The footer bar's words and counts, as facts — no React. The bar
 * paints these; the numbers come from the stats store — the terminals and
 * harnesses are `footerSessionsModel.mjs`'s, the node read-out
 * `nodeStatModel.mjs`'s, the overlays behind the four resource read-outs
 * `resourceModel.mjs`'s. Sizes read through `ui/fileTreeModel.formatSize` so a byte
 * count in the footer reads the same as one in the file tree.
 */

import { formatSize } from "../ui/fileTreeModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";


/** CPU as a whole percent, e.g. `34%`; a missing reading is a dash. */
export function cpuLabel(pct) {
  return typeof pct === "number" && Number.isFinite(pct) ? `${Math.round(pct)}%` : "—";
}

/** Memory as `used / total`, e.g. `9.4 / 16 GB`; both share the larger unit's word. */
export function memLabel(used, total) {
  if (typeof used !== "number" || typeof total !== "number" || total <= 0) return "—";
  const t = formatSize(total); // e.g. "16 GB"
  const unit = t.split(" ")[1] ?? "";
  const usedNum = scaleTo(used, unit);
  return `${usedNum} / ${t}`;
}

/** A byte count scaled to a named unit, one decimal below ten — matching `formatSize`. */
function scaleTo(bytes, unit) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  const power = Math.max(0, units.indexOf(unit));
  const n = bytes / 1024 ** power;
  return n.toFixed(n >= 10 || power === 0 ? 0 : 1);
}

/** GPU as a whole percent, e.g. `30%`; no reading — no reader on this machine — is a dash. */
export function gpuLabel(gpu) {
  return gpu && typeof gpu.util_percent === "number" && Number.isFinite(gpu.util_percent) ? `${Math.round(gpu.util_percent)}%` : "—";
}

/** The platform's disk footprint, e.g. `1.2 GB`; a missing reading is a dash. */
export function diskLabel(bytes) {
  return typeof bytes === "number" && bytes >= 0 ? formatSize(bytes) || "0 B" : "—";
}

/** Where the caret is, the way every editor's footer says it. */
export function caretLabel(status) {
  if (!status) return "";
  return tr("shell-status-bar-ln-col", { line: status.line, column: status.column });
}

/** The document's language for the footer; an unknown one is said plainly. */
export function languageLabel(status) {
  if (!status) return "";
  return status.language ?? tr("shell-status-bar-plain-text");
}

/**
 * A count trigger's name, its number said — `3 open terminals`. The footer's
 * popover takes this as the trigger's accessible name, which stands in for
 * whatever the trigger draws, so the number must be in the words.
 * @param {"terminals" | "harnesses" | "ports"} what
 * @param {number} count
 */
export function countWords(what, count) {
  const n = Number.isFinite(count) ? count : 0;
  switch (what) {
    case "terminals":
      return tr("shell-status-bar-open-terminals", { count: n });
    case "harnesses":
      return tr("shell-status-bar-running-harnesses", { count: n });
    case "ports":
      return tr("shell-status-bar-open-ports", { count: n });
    default:
      return String(n);
  }
}

/** The word a group of ports is headed by: the kind of thing that started them (`portsModel.groupPorts`). */
const PORT_OWNER_WORD = Object.freeze({
  goal: tr("shell-status-bar-port-owner-goal"),
  project: tr("shell-status-bar-port-owner-project"),
  workstream: tr("shell-status-bar-port-owner-workstream"),
  work_item: tr("shell-status-bar-port-owner-work-item"),
  shell: tr("shell-status-bar-port-owner-shell"),
  harness: tr("shell-status-bar-port-owner-harness"),
});

/**
 * A port group's kind in words; a kind this build does not know is said as
 * the node spelled it, never dropped.
 * @param {string | null | undefined} kind
 */
export function portOwnerWord(kind) {
  return PORT_OWNER_WORD[kind] ?? String(kind ?? "").replaceAll("_", " ");
}
