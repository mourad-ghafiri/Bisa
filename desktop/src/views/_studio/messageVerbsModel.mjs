/**
 * A message's verbs, as facts (05, ide/09): which ones a message offers and
 * in what order — the same in every conversation, on the right-click menu
 * and the ⋮ menu alike, since `Chat.tsx` builds both from this one list.
 * *Reply* and *Copy* also stand on the hover bar beside *React*; *React* is
 * a picker, not a verb here. `node --test` checks.
 */

import { t } from "../../i18n/l10n.mjs";

/** Every verb, in the order a menu lists them. */
export const VERBS = Object.freeze(["reply", "copy", "copy-link", "retract"]);

/**
 * The verbs one message offers, in order: reply, copy its text, copy its
 * link; retract, last and apart, only your own; nothing on a retracted one.
 * @param {{mine: boolean, retracted: boolean}} message
 * @returns {string[]}
 */
export function messageVerbs({ mine, retracted }) {
  if (retracted) return [];
  return mine ? [...VERBS] : VERBS.filter((v) => v !== "retract");
}

/** A verb's words and glyph key; *retract* is the one danger, set apart. */
export function verbWords(id) {
  switch (id) {
    case "reply":
      return { label: t("studio-chat-reply"), icon: "reply", danger: false, apart: false };
    case "copy":
      return { label: t("studio-chat-copy"), icon: "copy", danger: false, apart: false };
    case "copy-link":
      return { label: t("studio-message-verbs-copy-link"), icon: "link", danger: false, apart: false };
    case "retract":
      return { label: t("studio-message-verbs-retract"), icon: "delete", danger: true, apart: true };
    default:
      throw new Error(`no such verb: ${id}`);
  }
}

/** How much of a message a question about it quotes. */
const QUOTED = 80;

/**
 * The words a question about one message quotes, so the person sees which
 * one it is about: its first line, cut at a word — `null` for a message of
 * files alone, which the question names in general.
 * @param {{content?: string | null} | null | undefined} message
 * @returns {string | null}
 */
export function quotedWords(message) {
  const line = String(message?.content ?? "").split("\n").map((l) => l.trim()).find((l) => l !== "");
  if (!line) return null;
  if (line.length <= QUOTED) return line;
  const cut = line.slice(0, QUOTED);
  const at = cut.lastIndexOf(" ");
  return `${(at > QUOTED / 2 ? cut.slice(0, at) : cut).trimEnd()}…`;
}
