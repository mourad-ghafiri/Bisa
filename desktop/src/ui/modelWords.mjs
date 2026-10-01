/**
 * The model a session runs on, as a row's words (ide/07, ide/09): the
 * harness-native id with its provider prefix folded away — a row reads
 * *claude-opus-5-5[1m]*, not *anthropic/claude-opus-5-5[1m]* — and the full
 * id kept for the tooltip. The effort the session runs at follows the model
 * — *claude-opus-5-5[1m] · high* — only when the row carries one; it is the
 * wire's own word, shown as the model's id is. Nothing when neither is
 * known: a row never guesses. `node --test` checks.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * @param {string | null | undefined} model the harness-native id
 * @param {string | null | undefined} [effort] the level the session runs at
 * @returns {{short: string, full: string} | null}
 */
export function modelWords(model, effort) {
  const full = String(model ?? "").trim();
  const level = String(effort ?? "").trim();
  if (!full) return level ? { short: level, full: level } : null;
  const slash = full.lastIndexOf("/");
  const short = slash >= 0 && slash < full.length - 1 ? full.slice(slash + 1) : full;
  if (!level) return { short, full };
  return { short: t("ui-model-words-model-with-effort", { model: short, effort: level }), full: t("ui-model-words-model-with-effort", { model: full, effort: level }) };
}

/** *<agent> · <model>* — the words after a session's name, or the name alone. @param {string} name @param {string | null | undefined} model @param {string | null | undefined} [effort] */
export function nameWithModel(name, model, effort) {
  const words = modelWords(model, effort);
  return words ? t("ui-model-words-name-with-model", { name, model: words.short }) : name;
}
