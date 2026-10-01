/**
 * Amend (ide/04 §Amend), as facts — no React: the words the composer and its
 * confirmation say when the last commit is rewritten, and what the switch
 * does to the draft. The rewrite itself is consented and pinned in Safety by
 * the node; what is decided here is only what the person reads and types.
 */

import { VERB } from "./gitWords.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The message a commit holds, as the box shows it: the subject, a blank line, the body. */
export function headMessage(subject, body) {
  const head = (subject ?? "").trim();
  const rest = (body ?? "").trim();
  return rest ? `${head}\n\n${rest}` : head;
}

/**
 * The draft after the switch moves: switching on fills an empty box with
 * HEAD's message, so a reword starts from what is there; switching off
 * empties a box that still holds exactly that message, so nothing is
 * committed by accident under the old words. A message the person wrote is
 * never touched either way.
 * @param {boolean} on
 * @param {string} message
 * @param {string | null} head HEAD's message, when it has been read
 */
export function amendDraft(on, message, head) {
  if (head === null) return message;
  if (on) return message.trim() === "" ? head : message;
  return message === head ? "" : message;
}

/** The box's placeholder while the switch is on. */
export function amendPlaceholder(short, subject) {
  const what = (subject ?? "").trim();
  return what ? `${VERB.amend} ${short} — ${what}` : `${VERB.amend} ${short}`;
}

/**
 * The confirmation before an amend: the commit named, what happens to it,
 * and — when it is already on its upstream — the warning that the branch
 * then needs the force push, since an ordinary push is refused.
 * @param {{short: string, subject?: string | null, staged: number, upstream?: string | null, ahead: number}} facts
 */
export function amendWords({ short, subject = null, staged, upstream = null, ahead }) {
  const what = (subject ?? "").trim();
  const pushed = upstream !== null && upstream !== undefined && ahead === 0;
  return {
    title: what ? `${VERB.amend} ${short} — ${what}?` : `${VERB.amend} ${short}?`,
    body:
      staged > 0
        ? t("work-amend-what-staged-file-files-folds-into", { staged })
        : t("work-amend-only-message-changes-commit-s-id"),
    warning: pushed ? t("work-amend-already-once-amended-branch-needs-lease", { short, upstream, forcePush: VERB.forcePush }) : null,
    confirm: VERB.amend,
    danger: pushed,
    kind: "commit",
  };
}
