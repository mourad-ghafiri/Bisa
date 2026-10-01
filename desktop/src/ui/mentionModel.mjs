/**
 * The two halves of a mention, which are not the same list.
 *
 * A composer asks the directory two different questions, and they were being
 * answered by one array. **Suggesting** is the `@`-picker: what a reader is
 * offered while typing. **Resolving** is the outgoing wire: which of the known
 * names the finished body actually addresses. Anything dropped from the
 * directory to stop it being suggested also stopped being resolvable — typing
 * the name by hand then produced a message with an empty mentions array, which
 * posts, wakes nobody, and says nothing about it. That failure has no visible
 * symptom at all, which is why the two questions are now two functions.
 *
 * So a hidden entry — `suggest: false` — stays in the directory and out of the
 * picker. The core agent is the one that matters: it is an implicit member of
 * every room and answers anything addressed to nobody, so offering it in a
 * picker suggests a choice nobody needs to make, while still having to work
 * when someone types it.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `ui/fileTreeModel.mjs`:
 * there is no jsdom in this repo, so the way a rule gets a test is by not
 * living inside a component.
 */

/**
 * The one match predicate and the one ranking, shared with every other
 * picker in the app. This file used to hold a fourth private copy of
 * "does this row match?", which is how the `@`-dropdown ended up the only
 * agent list in the studio that could not be searched by tag or by role.
 */
import { rank } from "./agentPickerModel.mjs";

/** How many rows the `@`-picker shows at once. */
export const SUGGESTION_LIMIT = 6;

/**
 * Escape a name for use inside a regular expression.
 *
 * `name` is user data: an agent called "C++ helper" or "Ops (EU)" used to
 * throw here, during render, and blank the window.
 */
export function escapeRe(s) {
  return String(s ?? "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Whether an entry may be offered in the picker. Absent means yes. */
export function isSuggestable(m) {
  return m?.suggest !== false;
}

/**
 * The rows the `@`-picker offers for what has been typed after the `@`.
 *
 * Suggestability is filtered *before* matching, so a hidden entry cannot
 * surface by being the only thing a query matches.
 */
export function suggestions(mentionables, query, limit = SUGGESTION_LIMIT) {
  return rank((mentionables ?? []).filter(isSuggestable), query, limit);
}

/**
 * The ids a finished message body addresses — the outgoing mentions.
 *
 * Every known entry is considered, hidden ones included: this is resolution,
 * not suggestion, and a name somebody typed must reach whoever it names.
 *
 * `\b` never matches after the "…" of an unlabeled member, so those could
 * never be mentioned at all; a lookahead for "not a word character" handles
 * both that and a trailing punctuation mark.
 */
export function mentionsIn(mentionables, text) {
  const body = String(text ?? "");
  const found = new Set();
  for (const m of mentionables ?? []) {
    if (new RegExp(`@${escapeRe(m.name)}(?![\\w-])`, "i").test(body)) found.add(m.id);
  }
  return [...found];
}

/**
 * Where the `@`-picker is open and what it is querying, or `null`.
 *
 * The run after the `@` was previously cut at the first space, so a two-word
 * agent name closed the dropdown halfway through typing it — while
 * {@link mentionsIn} resolved that same full name perfectly happily. The
 * picker refused to offer the one name the message was about to address.
 *
 * A run containing a space stays a query only while some offerable name could
 * still complete it. That is what keeps "@Ada L" open for Ada Lovelace and
 * closes "@Ada is right" at "is" — without the guard, every `@` in a
 * paragraph would hold the dropdown open to the end of the line.
 */
export function mentionQuery(text, caret, mentionables) {
  const body = String(text ?? "");
  const at = body.lastIndexOf("@", Math.max(0, Number(caret) - 1));
  if (at < 0) return null;
  // Mid-word `@` is an email address or a handle already written, not the
  // start of a mention.
  if (at > 0 && !/\s/.test(body[at - 1] ?? "")) return null;

  const query = body.slice(at + 1, caret);
  if (query.includes("\n")) return null;
  if (!/\s/.test(query)) return { at, end: caret, query };

  const q = query.toLowerCase();
  const possible = (mentionables ?? [])
    .filter(isSuggestable)
    .some((m) => String(m?.name ?? "").toLowerCase().startsWith(q));
  return possible ? { at, end: caret, query } : null;
}

/**
 * The body with a picked name spliced in, and where the caret lands after it.
 *
 * Splices rather than truncates. The old insertion rebuilt the text as
 * "everything before the `@`" plus the name, which silently deleted whatever
 * stood after the caret — so completing a mention you had gone back to add
 * ate the rest of the sentence.
 */
export function insertMention(text, span, name) {
  const body = String(text ?? "");
  const token = `@${name} `;
  return {
    text: body.slice(0, span.at) + token + body.slice(span.end),
    caret: span.at + token.length,
  };
}
