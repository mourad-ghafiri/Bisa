/**
 * Folding a conversation's `ChangesView` into cards for the timeline
 * (ide/09), and the words the cards and the toasts use — the bar above the
 * composer reads the same view through `changedFilesModel.mjs`. No DOM, no
 * fetch: the server computes every hunk and every state, this only arranges
 * what came back.
 *
 * A card is keyed to the turn's **reply** message, so it draws directly
 * under the answer that made the changes; a turn whose reply has not landed
 * yet (or fallen off a paged timeline) keys to its **prompt**; a turn naming
 * neither draws at the end of the timeline, after the last message —
 * `turnCardsByAnchor`'s `atEnd`.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The sentinel `Chat` calls `afterMessage` with once, after the last landed
 * message and before any live turn's row — where a card whose reply and
 * prompt are both absent or off this page draws (`turnCardsByAnchor`'s
 * `atEnd`), and where the plan banner draws.
 */
export const TIMELINE_END = "$timeline-end";

/** *3 files · +40 −12* — a turn's or a file list's tally. */
export function turnWords(files) {
  const list = files ?? [];
  const n = list.length;
  const added = list.reduce((sum, f) => sum + (f?.added ?? 0), 0);
  const removed = list.reduce((sum, f) => sum + (f?.removed ?? 0), 0);
  return t("studio-turn-changes-files", { n, added, removed });
}

/** The chip a file wears: its state, or *also edited by someone else* when it overlaps an outside write. */
export function fileChipWords(file) {
  if (!file) return "";
  if (file.overlapped) return t("studio-turn-changes-also-edited-someone-else");
  switch (file.state) {
    case "pending":
      return t("studio-turn-changes-review");
    case "kept":
      return t("studio-turn-changes-kept");
    case "undone":
      return t("studio-turn-changes-undone");
    case "gone":
      return t("studio-turn-changes-gone");
    default:
      return String(file.state ?? "");
  }
}

/** The chip's tone, for the kit's `Chip`. */
export function fileChipTone(file) {
  if (!file) return "quiet";
  if (file.overlapped) return "warn";
  switch (file.state) {
    case "pending":
      return "accent";
    case "gone":
      return "warn";
    default:
      return "quiet";
  }
}

/**
 * The verbs offered on a pending file: Keep, Undo, and Undo with a note
 * (undo, then post the note as a message — the desktop's own act, not a
 * server verb). A settled file (kept, undone, gone) offers none — its chip
 * already says what happened.
 */
export function fileVerbs(file) {
  if (!file || file.state !== "pending") return [];
  return ["keep", "undo", "undo_with_note"];
}

/** Whether a turn has anything left to settle. */
export function turnHasPending(turn) {
  return (turn?.files ?? []).some((f) => f.state === "pending");
}

/** The bulk verbs on a turn's card — the same three, only when something in it is still pending. */
export function turnVerbs(turn) {
  return turnHasPending(turn) ? ["keep", "undo", "undo_with_note"] : [];
}

/** One card's shape, built from a `TurnChangesView`. */
function buildCard(turn) {
  return {
    turn: turn.turn,
    agent: turn.agent,
    mode: turn.mode,
    files: turn.files ?? [],
    words: turnWords(turn.files),
    pending: turnHasPending(turn),
    verbs: turnVerbs(turn),
  };
}

/**
 * Every turn's card, keyed to where it draws: `byMessage` maps a message id
 * to the cards that anchor there (usually one), and `atEnd` holds the cards
 * whose reply and prompt are both absent or not on this timeline.
 *
 * @param {{turns?: readonly {turn: string, prompt?: string|null, reply?: string|null, agent: string, mode: string, files?: readonly object[]}[]} | null | undefined} view
 * @param {Iterable<string> | null} [timelineIds] the message ids actually on screen; omit to trust every id named
 */
export function turnCardsByAnchor(view, timelineIds) {
  const known = timelineIds ? new Set(timelineIds) : null;
  const has = (id) => id != null && (known === null || known.has(id));
  const byMessage = new Map();
  const atEnd = [];
  for (const turn of view?.turns ?? []) {
    const card = buildCard(turn);
    const anchor = has(turn.reply) ? turn.reply : has(turn.prompt) ? turn.prompt : null;
    if (anchor) {
      const list = byMessage.get(anchor) ?? [];
      list.push(card);
      byMessage.set(anchor, list);
    } else {
      atEnd.push(card);
    }
  }
  return { byMessage, atEnd };
}

/** Every path still pending across every turn — the tab's and the explorer row's dot. */
export function pendingPathsOf(view) {
  const set = new Set();
  for (const turn of view?.turns ?? []) {
    for (const file of turn.files ?? []) {
      if (file.state === "pending") set.add(file.path);
    }
  }
  return set;
}

/** What the bar says in `auto`, where a pending change is kept without a click. */
export function autoKeptHint() {
  return t("studio-turn-changes-kept-when-send-next-message");
}

/** A settle's `skipped` list, as one line for a toast — `null` when nothing was skipped. */
export function skippedWords(skipped) {
  const list = skipped ?? [];
  if (list.length === 0) return null;
  return list.map((s) => `${s.path} — ${s.why}`).join("; ");
}
