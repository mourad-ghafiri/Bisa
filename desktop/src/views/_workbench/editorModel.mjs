/**
 * The state of one editable buffer, and the autosave policy over it (ide/03).
 *
 * A buffer is loaded from the node with the hash of what was read; edits make
 * it dirty; a save is compare-and-swap against that hash; a `409` — the file
 * changed since it was read — becomes a conflict the person resolves, never a
 * clobber. A change on disk while the buffer is clean reloads silently; while
 * it is dirty it raises the same conflict, without touching the buffer.
 *
 * A buffer's **source** is a file under the root at a relative path, a
 * **loose** file anywhere on this machine at an absolute path (read and
 * saved by the shell, ide/03 §Loose files), or an untitled document (⌘N):
 * the same buffer, with no file behind it until a save names one. What the
 * name may be is `savePathProblem`'s word.
 *
 * Pure: every transition returns a new state, so `node --test` can walk them.
 */

import { bytesWords } from "../../ui/artifact/artifactModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** How long after the last keystroke a save may fire. Below this the timer fires between two words. */
export const AUTOSAVE_FLOOR_MS = 300;
export const AUTOSAVE_CEILING_MS = 10000;
const AUTOSAVE_MODES = ["off", "after_delay", "on_focus_change"];

/**
 * The size a file is drawn plain above (ide/03): the editable size when
 * nobody set one (the engine's `EditorCaps::UNSET`, the parity test holds the
 * two equal). Whether a file is *editable* is the node's word on the file —
 * `editable`, under `editor.large_file.editable_mib` — never this number: a
 * machine that raised the bound edits a larger file, drawn plain.
 */
export const EDITABLE_BYTES = 2 * 1024 * 1024;
/** Whether a source is an untitled document rather than a file. */
export function isUntitled(source) {
  return source?.kind === "untitled";
}

/** Whether a source is a loose file — on this machine, under no root. */
export function isLoose(source) {
  return source?.kind === "loose";
}

/** What a source is called: the file's path (relative under a root, absolute for a loose one), or `Untitled-<seq>`. */
export function sourceName(source) {
  return isUntitled(source) ? `Untitled-${source.seq}` : String(source?.path ?? "");
}

/** The last segment of a path — what a save dialog offers as the name. */
export function basenameOf(path) {
  const s = String(path ?? "");
  const at = s.lastIndexOf("/");
  return at === -1 ? s : s.slice(at + 1);
}

/**
 * The buffer an untitled document starts from: nothing read, nothing to
 * read — empty, clean, editable, with no hash, so its first save is a
 * create the node refuses when the name is taken.
 */
export function untitledBuffer(source) {
  return { ...emptyBuffer(sourceName(source)), status: "clean" };
}

/**
 * What the path a new document is saved under must satisfy: relative to the
 * root, forward slashes, no `.` or `..` segment, and a file rather than a
 * folder. Whether the name is taken is the node's answer. Null when it is fine.
 * @param {string} value
 * @returns {string | null}
 */
export function savePathProblem(value) {
  const v = String(value ?? "").trim();
  if (!v) return t("workbench-editor-path-needed");
  if (v.includes("\\")) return t("workbench-editor-use-forward-slashes");
  if (v.startsWith("/") || v.startsWith("~")) return t("workbench-editor-path-relative-root");
  if (v.endsWith("/")) return t("workbench-editor-file-not-folder");
  if (v.split("/").some((part) => part === "" || part === "." || part === "..")) return t("workbench-editor-not-path-inside-root");
  return null;
}

export function emptyBuffer(path) {
  return {
    path,
    status: "loading",
    text: "",
    savedText: "",
    savedHash: null,
    dirtySince: null,
    readOnly: false,
    readOnlyWhy: null,
    plain: false,
    binary: false,
    size: 0,
    conflict: null,
    error: null,
    refusal: null,
  };
}

/**
 * The node answered `GET /ide/file`. Whether the file may be edited is the
 * node's word on it (`editable`, under this machine's
 * `editor.large_file.editable_mib`) and why it may not is kept beside it
 * (`readOnlyWhy`): cut where the read stops, or over the editable size —
 * never a size compared here.
 */
export function loaded(buffer, file) {
  if (file.binary) {
    return { ...buffer, status: "binary", binary: true, size: file.size, text: "", savedText: "", savedHash: null, conflict: null, error: null, refusal: null };
  }
  const text = file.text ?? "";
  return {
    ...buffer,
    status: file.editable ? "clean" : "read_only",
    text,
    savedText: text,
    savedHash: file.hash ?? null,
    dirtySince: null,
    readOnly: !file.editable || file.truncated,
    readOnlyWhy: file.truncated ? "truncated" : file.editable ? null : "size",
    plain: file.size > EDITABLE_BYTES,
    binary: false,
    size: file.size,
    conflict: null,
    error: null,
    refusal: null,
  };
}

/** A whole number of bytes off the wire, or null for anything else. */
function bytesOf(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

/** What an answer carried beside its sentence, when it carried anything. */
function bodyOf(error) {
  return error && typeof error === "object" && error.body && typeof error.body === "object" ? error.body : {};
}

/**
 * What a read that threw means: a `413` is a file over the size this
 * machine reads into the editor — with the size and **the limit the node
 * said**, the bound being this machine's setting
 * (`editor.large_file.refuse_mib`) and never a number of the desktop's — a
 * `404` a file that is not there, anything else a failure in its own words.
 * @param {unknown} error
 * @returns {{kind: "too_large", message: string, size: number | null, limit: number | null} | {kind: "missing", message: string} | {kind: "failed", message: string}}
 */
export function loadFailure(error) {
  const status = error && typeof error === "object" ? error.status : undefined;
  const message = error instanceof Error ? error.message : String(error);
  if (status === 413) {
    const body = bodyOf(error);
    return { kind: "too_large", message, size: bytesOf(body.size), limit: bytesOf(body.limit) };
  }
  if (status === 404) return { kind: "missing", message };
  return { kind: "failed", message };
}

/**
 * A read was refused or failed. A document not read yet, or one with
 * nothing typed into it, shows the refusal; **a buffer holding unsaved
 * work keeps the screen** — a read again that failed under it (the node away
 * for a moment, the file gone from disk) is noted as its `error` and takes
 * nothing the person typed out of their hands.
 * @param {object} buffer
 * @param {ReturnType<typeof loadFailure>} failure
 */
export function loadFailed(buffer, failure) {
  if (isUnsaved(buffer)) return { ...buffer, error: failure.message };
  return { ...buffer, status: "error", error: failure.message, refusal: failure };
}

/**
 * The line under a refused read, or `null` when the node's sentence says it
 * all: a file over the size the editor opens says its size and the limit the
 * node said, and where that limit is set; the way out is the file manager.
 * @param {ReturnType<typeof loadFailure> | null | undefined} refusal
 */
export function refusalWords(refusal) {
  if (refusal?.kind !== "too_large") return null;
  if (refusal.size === null || refusal.limit === null) return t("workbench-editor-too-large-open");
  return t("workbench-editor-too-large-open-sized", { size: bytesWords(refusal.size), limit: bytesWords(refusal.limit) });
}

/**
 * Why a document cannot be typed into, or is drawn plain, as the bar says
 * it — `null` when there is nothing to say. Read-only is the node's word:
 * over the editable size this machine set, or cut where the read stops.
 * Plain is the editor's: above `EDITABLE_BYTES` tokenisation is off, whether
 * or not the file may be edited.
 * @param {{readOnly: boolean, readOnlyWhy?: string | null, plain: boolean}} buffer
 */
export function sizeNote(buffer) {
  if (!buffer.readOnly) return buffer.plain ? t("workbench-editor-drawn-plain") : null;
  if (buffer.readOnlyWhy === "truncated") return t("workbench-editor-read-only-cut");
  if (buffer.readOnlyWhy === "size") return buffer.plain ? t("workbench-editor-doc-read-only-above-editable-size-tokenisation") : t("workbench-editor-read-only-above-editable-size");
  return t("workbench-editor-doc-read-only");
}

/** A keystroke. */
export function edited(buffer, text, now) {
  if (buffer.readOnly || buffer.status === "binary" || buffer.status === "error") return buffer;
  if (text === buffer.savedText) {
    return { ...buffer, text, status: buffer.conflict ? "conflict" : "clean", dirtySince: null };
  }
  return {
    ...buffer,
    text,
    status: buffer.conflict ? "conflict" : "dirty",
    dirtySince: buffer.dirtySince ?? now,
  };
}

export function isDirty(buffer) {
  return buffer.text !== buffer.savedText;
}

/**
 * Whether a buffer holds work a close would lose: text that differs from the
 * disk's, or a conflict nobody settled. The one rule the tab's dot, the close
 * guard and the quit question read — of a document on screen or not.
 */
export function isUnsaved(buffer) {
  return Boolean(buffer) && (isDirty(buffer) || buffer.status === "conflict");
}

/**
 * The keys of the buffers that hold unsaved work, in the map's order.
 * @param {ReadonlyMap<string, object>} buffers
 * @returns {string[]}
 */
export function unsavedKeys(buffers) {
  const out = [];
  for (const [key, buffer] of buffers) if (isUnsaved(buffer)) out.push(key);
  return out;
}

/**
 * The buffer a document on disk starts from when it comes on screen. A tab
 * that was off screen kept its buffer: unsaved work stays exactly as typed —
 * the read that follows compares it with the disk (`changedOnDisk`) rather
 * than replacing it — and anything else is read afresh. A buffer caught
 * mid-save by an unmount is not saving any more.
 * @param {object | null | undefined} kept the buffer the tab left behind
 * @param {string} path
 */
export function remounted(kept, path) {
  if (!isUnsaved(kept)) return emptyBuffer(path);
  return kept.status === "saving" ? { ...kept, status: "dirty" } : kept;
}

export function saveStarted(buffer) {
  return { ...buffer, status: "saving" };
}

/**
 * A save's formatter answered (`editor.format_on_save`): the buffer nobody
 * touched since the save began takes the formatted text — what the save is
 * about to write is what the editor shows, so the save leaves it clean. A
 * buffer that stayed as typed would be dirty against its own save, and an
 * autosave would write it again on every tick. One the person typed in
 * while the formatter was out keeps what they typed: a formatter never
 * overwrites a keystroke.
 * @param {object} buffer
 * @param {string} typed the buffer's text as the save began
 * @param {string} text the formatter's text
 */
export function reformatted(buffer, typed, text) {
  if (buffer.text !== typed || text === typed) return buffer;
  return { ...buffer, text };
}

/** `PUT` answered 200 with the new hash; `text` is what was sent. */
export function saved(buffer, text, hash) {
  const stillDirty = buffer.text !== text;
  return {
    ...buffer,
    savedText: text,
    savedHash: hash,
    status: stillDirty ? "dirty" : "clean",
    dirtySince: stillDirty ? buffer.dirtySince : null,
    conflict: null,
    error: null,
  };
}

/** `PUT` answered 409, or the watcher said the file changed under a dirty buffer. */
export function conflicted(buffer, currentText, currentHash) {
  return {
    ...buffer,
    status: "conflict",
    conflict: { theirs: currentText, theirsHash: currentHash },
  };
}

/** The save failed for a reason that is not a conflict. The buffer keeps its text. */
export function saveFailed(buffer, error) {
  return { ...buffer, status: isDirty(buffer) ? "dirty" : "clean", error: String(error) };
}

/**
 * The file changed on disk. The disk holding what this editor last saved
 * — the same hash — is no change at all: the buffer stands, dirty or not,
 * since a save announces itself like any other write. Otherwise a clean
 * buffer takes the new text silently; a dirty one is told, and keeps every
 * character the person typed.
 */
export function changedOnDisk(buffer, file) {
  if (buffer.status === "binary" || buffer.status === "error" || buffer.status === "loading") {
    return loaded(buffer, file);
  }
  if (file.hash && file.hash === buffer.savedHash) return buffer;
  if (!isDirty(buffer)) return loaded(buffer, file);
  return conflicted(buffer, file.text ?? "", file.hash ?? null);
}

/** Resolve a conflict by keeping what the person typed: the next save uses theirs as the base. */
export function keepMine(buffer) {
  if (!buffer.conflict) return buffer;
  return {
    ...buffer,
    savedHash: buffer.conflict.theirsHash,
    savedText: buffer.conflict.theirs,
    status: "dirty",
    dirtySince: buffer.dirtySince ?? 0,
    conflict: null,
  };
}

/** Resolve a conflict by taking what is on disk. The typed text is gone, and the caller said so. */
export function takeTheirs(buffer) {
  if (!buffer.conflict) return buffer;
  return {
    ...buffer,
    text: buffer.conflict.theirs,
    savedText: buffer.conflict.theirs,
    savedHash: buffer.conflict.theirsHash,
    status: "clean",
    dirtySince: null,
    conflict: null,
  };
}

/** Clamp a stored delay into the policy's bounds; anything unusable is the default. */
/**
 * The autosave settings as the resolved rows give them: a mode off the list
 * is `after_delay`, the delay clamped to the floor and the ceiling.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} rows the resolved settings
 * @returns {{mode: string, delay: number}}
 */
export function autosaveFrom(rows) {
  const get = (k) => rows?.find((r) => r.key === k)?.value;
  const mode = get("editor.autosave.mode");
  return {
    mode: typeof mode === "string" && AUTOSAVE_MODES.includes(mode) ? mode : "after_delay",
    delay: clampAutosaveDelay(get("editor.autosave.delay_ms")),
  };
}

export function clampAutosaveDelay(value, fallback = 1000) {
  const n = typeof value === "number" ? value : typeof value === "string" && value !== "" ? Number(value) : NaN;
  if (!Number.isFinite(n)) return fallback;
  return Math.min(AUTOSAVE_CEILING_MS, Math.max(AUTOSAVE_FLOOR_MS, Math.round(n)));
}

/**
 * Whether an autosave should fire now. `after_delay` waits `delayMs` after
 * the *first* unsaved keystroke was recorded and the buffer is still dirty;
 * `on_focus_change` never fires from the timer; `off` never fires.
 */
export function autosaveDue(buffer, mode, delayMs, now) {
  if (mode !== "after_delay") return false;
  if (buffer.status !== "dirty" || buffer.dirtySince === null) return false;
  return now - buffer.dirtySince >= clampAutosaveDelay(delayMs);
}

/** What the tab shows beside its label. */
/**
 * A path as the crumbs above its editor: every folder, then the file. Each
 * crumb names the path up to it, so a folder crumb can be revealed in Files.
 * @param {string} path
 * @returns {{label: string, path: string, dir: boolean}[]}
 */
export function breadcrumbsOf(path) {
  const parts = String(path ?? "").split("/").filter(Boolean);
  return parts.map((label, i) => ({ label, path: parts.slice(0, i + 1).join("/"), dir: i < parts.length - 1 }));
}

/** The tab width a formatter is told when the setting is not a usable number. */
export const DEFAULT_TAB_SIZE = 4;

/**
 * What a save asks the language server's formatter, or `null` when it asks
 * nothing: `editor.format_on_save` must be exactly on and a server must
 * follow the document. The options are the editor's own settings — a tab
 * size that is no whole number from 1 to 16 is the default, and spaces are
 * inserted unless the setting says exactly `false`.
 * @param {readonly {key: string, value: unknown}[] | null | undefined} resolved
 * @param {string | null | undefined} lspLanguage
 * @returns {{tabSize: number, insertSpaces: boolean} | null}
 */
export function formatOnSave(resolved, lspLanguage) {
  if (!lspLanguage) return null;
  const get = (k) => (resolved ?? []).find((r) => r?.key === k)?.value;
  if (get("editor.format_on_save") !== true) return null;
  const size = get("editor.tab_size");
  const tabSize = Number.isInteger(size) && size >= 1 && size <= 16 ? size : DEFAULT_TAB_SIZE;
  return { tabSize, insertSpaces: get("editor.insert_spaces") !== false };
}

/**
 * What a save that threw means: a `409` is the file having changed on disk —
 * the conflict, with the text and the hash the node read — and anything
 * else is a failure in its own words. A conflict whose body the node did not
 * send is still a conflict, against nothing.
 * @param {unknown} error
 * @returns {{kind: "conflict", text: string, hash: string | null} | {kind: "failed", message: string}}
 *   a `413` — the text is over the size this machine edits — is a failure whose words carry the limit the node said
 */
export function saveFailure(error) {
  const status = error && typeof error === "object" ? error.status : undefined;
  const body = bodyOf(error);
  if (status === 409) {
    return { kind: "conflict", text: typeof body.current_text === "string" ? body.current_text : "", hash: typeof body.current_hash === "string" ? body.current_hash : null };
  }
  const message = error instanceof Error ? error.message : String(error);
  // The text grew past the size this machine edits: the node's own 413, with the limit it holds.
  if (status === 413) return { kind: "failed", message: overBoundWords(bytesOf(body.size), bytesOf(body.limit)) ?? message };
  return { kind: "failed", message };
}

/**
 * A save the node refused for its size, in words that carry **the limit the
 * node said** — or `null` when the answer carried none, and the node's own
 * sentence stands.
 * @param {number | null} size @param {number | null} limit
 */
export function overBoundWords(size, limit) {
  if (size === null || limit === null) return null;
  return t("workbench-editor-save-over-bound", { size: bytesWords(size), limit: bytesWords(limit) });
}

