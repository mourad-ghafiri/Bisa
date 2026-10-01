/**
 * Where a setting's value comes from and where Settings writes it (ide/13 —
 * *Visible in the UI*): the origin badge every control carries — `default`,
 * `machine`, `workspace`, `project` — the scopes the Settings screen may
 * write a key at, which one a row starts on, and when *Reset* has something
 * to reset. One module, so the six panels that draw a badge and the registry
 * rows that draw a *set at* choice say a scope in the same words: the ones a
 * person types at the prompt (`bisa settings set machine …`) and the ones the
 * node names when it refuses a scope.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/** The scopes Settings writes, narrowest first: what a person usually means by "set this here". A project's layer is the Project IDE's (ide/13). */
export const SETTINGS_SCOPES = Object.freeze(["workspace", "machine"]);

/**
 * A scope's word, as a person types it and as the node names it in a refusal.
 * A word this desktop does not know — a newer node's — is drawn as it came.
 * @param {string} scope
 */
export function scopeWord(scope) {
  switch (scope) {
    case "machine":
      return t("settings-setting-origin-machine");
    case "workspace":
      return t("settings-setting-origin-workspace");
    case "project":
      return t("settings-setting-origin-project");
    default:
      return String(scope);
  }
}

/**
 * The badge of a resolved value: its word, its tone and the sentence behind
 * it. `default` is the compiled default — no scope holds a value.
 * @param {string} origin `default` or a scope
 * @returns {{ word: string, tone: "quiet" | "neutral", hint: string }}
 */
export function originBadge(origin) {
  if (origin === "default") return { word: t("settings-setting-origin-default"), tone: "quiet", hint: t("settings-browser-access-panel-compiled-default-scope-holds-value") };
  const word = scopeWord(origin);
  return { word, tone: "neutral", hint: t("settings-browser-access-panel-value-screen-comes-from-scope", { origin: word }) };
}

/**
 * The scopes a key may be written at from Settings, narrowest first.
 * @param {{ scopes: readonly string[] }} def
 * @returns {string[]}
 */
export function writableScopes(def) {
  return SETTINGS_SCOPES.filter((s) => def.scopes.includes(s));
}

/** The scope a row starts on: the narrowest the key allows here. @param {{ scopes: readonly string[] }} def */
export function defaultTarget(def) {
  return writableScopes(def)[0] ?? "machine";
}

/**
 * Whether *Reset* has something to reset: the value on screen is held at the
 * scope the row writes to. A value that comes from elsewhere — another scope,
 * the default — is not this row's to remove.
 * @param {string} origin
 * @param {string} target
 */
export function mayReset(origin, target) {
  return origin === target;
}

/** Nearest first: the order a value is resolved in. */
const NEAREST = Object.freeze(["project", "workspace", "machine"]);

/**
 * The scope whose value stands over a write at `target`, or `null`: a value
 * held nearer than the scope a row writes to keeps winning, so the write
 * lands and the control does not move. The row says so rather than looking
 * as if the click did nothing.
 * @param {string} origin where the value on screen comes from
 * @param {string} target the scope the row writes to
 * @returns {string | null}
 */
export function shadowedBy(origin, target) {
  const at = NEAREST.indexOf(origin);
  const to = NEAREST.indexOf(target);
  return at >= 0 && to >= 0 && at < to ? origin : null;
}

/** The line under the *set at* choice when a nearer scope's value wins, or `null`. */
export function shadowWords(origin, target) {
  const over = shadowedBy(origin, target);
  return over === null ? null : t("settings-setting-origin-shadowed", { over: scopeWord(over), target: scopeWord(target) });
}

/** *set at machine* — a key one scope alone may hold. */
export function setAtWords(scope) {
  return t("settings-setting-origin-set-at", { scope: scopeWord(scope) });
}
