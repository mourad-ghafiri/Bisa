/**
 * What a person reads when an act failed. What failed leads — a catalog
 * sentence the caller says ("The branch could not be created.") — and the
 * reason follows only when it is a sentence for a person: the node's own
 * refusal, an `ApiError` from `api.ts`, whose words the node already wrote
 * for a person. Anything else is a raw exception — a shell rejection, a
 * `TypeError`, a bare string — and is never shown: the words point to the
 * diagnostic log instead, and `raw` tells the caller to write the detail
 * there (`ui/failure.ts`, `sayFailure`). Plain `.mjs`, so `node --test`
 * reads the rule itself.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * The node's own sentence, when the error carries one: an `ApiError`, told
 * by its name so the model needs no import of the client, with words in it.
 * @param {unknown} error
 * @returns {string | null}
 */
export function nodeSentence(error) {
  if (!(error instanceof Error) || error.name !== "ApiError") return null;
  const words = error.message.trim();
  return words ? words : null;
}

/**
 * The words for a failed act, led by what failed.
 * @param {string} what the catalog sentence naming what failed
 * @param {unknown} error what the act threw
 * @returns {{ text: string, raw: boolean }}
 */
export function failureWords(what, error) {
  const reason = nodeSentence(error);
  if (reason) return { text: t("ui-failure-node-said", { what, reason }), raw: false };
  return { text: t("ui-failure-see-log", { what }), raw: true };
}

/**
 * The reason alone, for a sentence that already says what failed ("…could
 * not be deleted — a.txt: { $reason }."): the node's own words, or where the
 * detail went when the error is a raw exception (`raw`).
 * @param {unknown} error what the act threw
 * @returns {{ text: string, raw: boolean }}
 */
export function reasonWords(error) {
  const reason = nodeSentence(error);
  return reason ? { text: reason, raw: false } : { text: t("ui-failure-reason-in-log"), raw: true };
}

/**
 * The words for a failure where nothing says what failed — a shared door
 * (`attempt`, a read) that hands one sentence to whoever shows it, alone or
 * after a colon: the node's own sentence, or that something unexpected
 * stopped it and where its detail went (`raw`).
 * @param {unknown} error what the act threw
 * @returns {{ text: string, raw: boolean }}
 */
export function unexpectedWords(error) {
  const reason = nodeSentence(error);
  return reason ? { text: reason, raw: false } : { text: t("ui-failure-unexpected"), raw: true };
}
