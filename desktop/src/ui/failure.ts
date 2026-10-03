/**
 * The one door a surface says a failed act through (`failureModel.mjs`):
 * what failed leads, the node's own reason follows, and a raw exception is
 * never shown — its detail goes to the diagnostic log, which the words
 * point to.
 */

import { errorFields, log } from "../log";
import { failureWords, reasonWords, unexpectedWords } from "./failureModel.mjs";

/**
 * The sentence a person reads for a failed act — for a toast, an
 * `ErrorNote` or a `Field` error. `target` names the surface in the log
 * (`git`, `settings`, `workflow`); `what` is the catalog sentence naming
 * what failed ("The branch could not be created.").
 */
export function sayFailure(target: string, what: string, error: unknown): string {
  const { text, raw } = failureWords(what, error);
  if (raw) log.warn(target, what, errorFields(error));
  return text;
}

/**
 * The reason alone, for a catalog sentence that already names what failed
 * and takes the reason as a variable: the node's words, or — for a raw
 * exception, logged here under `what` — where its detail went.
 */
export function failureReason(target: string, what: string, error: unknown): string {
  const { text, raw } = reasonWords(error);
  if (raw) log.warn(target, what, errorFields(error));
  return text;
}

/**
 * One sentence for a failure where the door does not know what failed — a
 * shared helper hands it to a toast, a note or a sentence's reason: the
 * node's words, or that something unexpected stopped it, the detail logged
 * under `what` (words for the log).
 */
export function failureText(target: string, what: string, error: unknown): string {
  const { text, raw } = unexpectedWords(error);
  if (raw) log.warn(target, what, errorFields(error));
  return text;
}
