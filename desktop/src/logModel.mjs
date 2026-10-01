/**
 * The webview's side of the diagnostic log (`bisa-log`), as facts: the
 * level words in the order the setting offers them, whether a line passes
 * the level in force, the shape of one event as the shell's `log_event`
 * command takes it, and the `logging.*` settings read into a configuration.
 *
 * An event is **bounded here** so the shell and the file never see a
 * runaway: a message is cut, a field is stringified and cut, and only so
 * many fields ride. Nothing here formats a body, a prompt or a token — the
 * sites that call the logger hand over ids, statuses and a typed error's
 * words, and `errorFields` keeps to an error's name, message, status, path
 * and the first lines of its stack.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it with no
 * build step.
 */

import { boolOf, choiceOf, numberOf } from "./shell/settingsModel.mjs";
import { t } from "./i18n/l10n.mjs";

/** The levels, quietest first — the crate's `LogLevel::WORDS`. */
export const LEVELS = Object.freeze(["error", "warn", "info", "debug", "trace"]);

/** The rotations — the crate's `LogRotation::WORDS`. */
export const ROTATIONS = Object.freeze(["hourly", "daily"]);

/** The four keys, in the registry's order. */
export const KEYS = Object.freeze({
  enabled: "logging.enabled",
  level: "logging.level",
  rotation: "logging.rotation",
  keepFiles: "logging.keep_files",
});

/** What the registry defaults resolve to: errors only, daily, fourteen. */
export const DEFAULT_CONFIG = Object.freeze({
  enabled: true,
  level: "error",
  rotation: "daily",
  keep_files: 14,
});

/** How much of a message and of a field survives. */
export const MAX_MESSAGE = 2000;
export const MAX_FIELD = 1000;
export const MAX_FIELDS = 16;
/** How many lines of a stack ride with an error. */
export const STACK_LINES = 4;

/**
 * Whether a line at `level` passes when `threshold` is in force: the
 * threshold admits itself and everything quieter. An unknown word on
 * either side passes nothing.
 * @param {string} level
 * @param {string} threshold
 */
export function passes(level, threshold) {
  const a = LEVELS.indexOf(level);
  const b = LEVELS.indexOf(threshold);
  return a >= 0 && b >= 0 && a <= b;
}

/**
 * The resolved `logging.*` settings as a configuration; anything absent or
 * out of its bounds is the default's.
 * @param {readonly import("./shell/settingsModel.mjs").ResolvedRow[] | null | undefined} resolved
 */
export function configFrom(resolved) {
  return {
    enabled: boolOf(resolved, KEYS.enabled, DEFAULT_CONFIG.enabled),
    level: choiceOf(resolved, KEYS.level, LEVELS, DEFAULT_CONFIG.level),
    rotation: choiceOf(resolved, KEYS.rotation, ROTATIONS, DEFAULT_CONFIG.rotation),
    keep_files: numberOf(resolved, KEYS.keepFiles, DEFAULT_CONFIG.keep_files, { min: 1, max: 366 }),
  };
}

/**
 * Cut a string to `max` characters, marking the cut.
 * @param {string} text
 * @param {number} max
 */
function cut(text, max) {
  return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/**
 * One value as a field: a string, number or boolean as it is; `null` as
 * `null`; anything else through JSON, cut. A value JSON refuses (a cycle)
 * reads as its type.
 * @param {unknown} value
 */
function fieldValue(value) {
  if (value === null || value === undefined) return null;
  if (typeof value === "string") return cut(value, MAX_FIELD);
  if (typeof value === "number" || typeof value === "boolean") return value;
  try {
    return cut(JSON.stringify(value) ?? String(value), MAX_FIELD);
  } catch {
    return `<${typeof value}>`;
  }
}

/**
 * What an error is, as fields: its name and message, the status, path and
 * code an `ApiError` carries, and the first lines of its stack — never the
 * body it may hold.
 * @param {unknown} error
 */
export function errorFields(error) {
  if (!(error instanceof Error)) {
    return { error: fieldValue(error) };
  }
  /** @type {Record<string, unknown>} */
  const out = { error: error.name, message: error.message };
  const any = /** @type {Record<string, unknown>} */ (/** @type {unknown} */ (error));
  if (typeof any.status === "number") out.status = any.status;
  if (typeof any.path === "string") out.path = any.path;
  if (typeof any.code === "string") out.code = any.code;
  if (typeof error.stack === "string") {
    out.stack = error.stack.split("\n").slice(0, STACK_LINES).join("\n");
  }
  return out;
}

/**
 * One event as the shell takes it: the level and target as words, the
 * message cut, at most `MAX_FIELDS` fields each a JSON-safe value.
 * @param {string} level
 * @param {string} target
 * @param {string} message
 * @param {Record<string, unknown> | undefined} [fields]
 */
export function shapeEvent(level, target, message, fields) {
  /** @type {Record<string, unknown>} */
  const shaped = {};
  let n = 0;
  for (const [key, value] of Object.entries(fields ?? {})) {
    if (n >= MAX_FIELDS) {
      shaped["…"] = t("app-log-more-fields-dropped");
      break;
    }
    shaped[key] = fieldValue(value);
    n += 1;
  }
  return {
    level: LEVELS.includes(level) ? level : "error",
    target: cut(target || "webview", 64),
    message: cut(String(message), MAX_MESSAGE),
    fields: shaped,
  };
}
