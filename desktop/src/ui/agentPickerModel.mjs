/**
 * Picking a principal: one match predicate, one ranking, one cursor.
 *
 * Seven surfaces asked "which agent?" and four of them answered with a
 * different predicate — `name || id.startsWith`, `name` alone, `name ||
 * id.includes`, `name || sub`. The visible cost was not the duplication: it
 * was that the same typing found different agents depending on which control
 * you were in, and only two of the seven ever looked at a description or a
 * tag. Somebody searching for "the one that reviews diffs" found it on the
 * Agents screen and nowhere they could actually address it.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `ui/fileTreeModel.mjs`:
 * there is no jsdom in this repo, so a rule gets a test by not living inside
 * a component. Everything a picker can get *wrong* — what a query matches,
 * what order the rows come back in, what a key press means at the cursor,
 * what a toggle does at the cap — is decided here. The `.tsx` is paint.
 */

function lower(s) {
  return String(s ?? "").toLowerCase();
}

/**
 * Everything a query is matched against, as one lowercased string.
 *
 * Description, tags and harness are in here on purpose. A picker that matches
 * only names asks the reader to already know which agent they want, which is
 * the opposite of what a picker is for — and it is why "review" used to find
 * the Reviewer on the Agents screen and nothing at all in the address tray.
 */
export function haystack(c) {
  return [
    c?.name,
    c?.description,
    c?.harness,
    ...(Array.isArray(c?.tags) ? c.tags : []),
  ]
    .filter(Boolean)
    .map(lower)
    .join(" ");
}

/**
 * Does this candidate match what was typed?
 *
 * The id is matched by *prefix* rather than by substring. An agent pubkey is
 * 64 hex characters, so a substring match makes any three-letter query hit
 * whichever agents happen to contain those letters somewhere in their key —
 * rows a reader cannot explain and cannot reproduce.
 */
export function matches(c, query) {
  const q = lower(query).trim();
  if (!q) return true;
  return haystack(c).includes(q) || lower(c?.id).startsWith(q);
}

/**
 * How good a match is, lowest first. Ties keep the order they arrived in.
 *
 * Arrival order is load-bearing: a channel's roster orders the `@`-picker in
 * that channel, and that ordering is most of what a roster buys. So this ranks
 * *within* what the caller already ordered rather than replacing it.
 */
function tier(c, q) {
  const name = lower(c?.name);
  if (name === q) return 0;
  if (name.startsWith(q)) return 1;
  // A word inside the name: "lovelace" should find "Ada Lovelace" above an
  // agent that merely mentions Ada in its description.
  if (name.split(/\s+/).some((w) => w.startsWith(q))) return 2;
  if (lower(c?.id).startsWith(q)) return 3;
  if (name.includes(q)) return 4;
  return 5;
}

/**
 * The rows a query offers, best first, capped.
 *
 * `limit` of `0` means no cap — a panel that scrolls wants every match, while
 * a dropdown floating over a textarea wants a screenful.
 */
export function rank(candidates, query, limit = 0) {
  const q = lower(query).trim();
  const hits = (candidates ?? []).filter((c) => matches(c, q));
  const ordered = q
    ? hits
        .map((c, i) => ({ c, i, t: tier(c, q) }))
        .sort((a, b) => a.t - b.t || a.i - b.i)
        .map((x) => x.c)
    : hits;
  return limit > 0 ? ordered.slice(0, limit) : ordered;
}

/**
 * Rows bucketed by `group`, in the order the groups first appear.
 *
 * A Map because insertion order is display order, the same reason the omnibox
 * builds its groups this way. Candidates with no group land in one unlabelled
 * bucket, so a caller that does not group gets a plain list.
 */
export function groupRows(candidates) {
  const m = new Map();
  for (const c of candidates ?? []) {
    const label = c?.group ?? "";
    const bucket = m.get(label);
    if (bucket) bucket.push(c);
    else m.set(label, [c]);
  }
  return [...m.entries()].map(([label, rows]) => ({ label, rows }));
}

/**
 * Keep the cursor on a row that exists.
 *
 * A filtered list gets shorter under a cursor that was pointing at row nine,
 * and an `aria-activedescendant` naming an element that is no longer in the
 * DOM is announced as nothing at all.
 */
export function clampCursor(cursor, count) {
  if (count <= 0) return 0;
  return Math.min(Math.max(cursor ?? 0, 0), count - 1);
}

/**
 * What a key press means at the cursor. `null` means "not ours" — the caller
 * must let the event through, or Enter would stop sending the message.
 *
 * The cursor clamps at both ends rather than wrapping. The omnibox is the one
 * fully accessible list in the app and it clamps; a wrapping list read aloud
 * jumps from "last of eleven" to "first of eleven" with no boundary spoken,
 * which is how a keyboard reader loses their place.
 */
export function cursorAction(count, cursor, key) {
  if (key === "Escape") return { type: "dismiss" };
  if (count <= 0) return null;
  const at = clampCursor(cursor, count);
  switch (key) {
    case "ArrowDown":
      return { type: "move", index: Math.min(at + 1, count - 1) };
    case "ArrowUp":
      return { type: "move", index: Math.max(at - 1, 0) };
    case "Home":
      return { type: "move", index: 0 };
    case "End":
      return { type: "move", index: count - 1 };
    case "Enter":
    case "Tab":
      return { type: "pick", index: at };
    default:
      return null;
  }
}

/**
 * Add or remove one id, honouring the cap.
 *
 * `max === 1` swaps rather than refusing: a single-choice control that makes
 * you deselect before you can select reads as broken. Above one, the cap is a
 * cap — and a no-op returns the *same array*, so a caller can tell that
 * nothing happened without diffing it.
 */
export function toggleValue(value, id, max) {
  const current = value ?? [];
  if (current.includes(id)) return current.filter((k) => k !== id);
  if (max === 1) return [id];
  if (max !== undefined && current.length >= max) return current;
  return [...current, id];
}
