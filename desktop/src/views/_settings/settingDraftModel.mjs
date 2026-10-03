/**
 * What a typed setting commits, decided where `node --test` can reach it.
 *
 * A registry row writes to the node, and a write per keystroke is a write of
 * every half-typed value: "h", "ht", "htt"… for a URL, "1" before "12" for a
 * number, and a field that greys out under its own write loses the caret
 * between letters. So a text, number or secret setting keeps a draft — the
 * string in the box — and commits it on blur or Enter; this is the one rule
 * that turns the draft into the value written, or into nothing at all.
 *
 * Integers are the kit's `NumberInput` (`ui/numberInputModel.mjs`); this
 * covers the decimals and the text a `NumberInput` cannot.
 */

/** How long the quiet *Saved* beside a row stays after a write lands. */
export const SAVED_NOTE_MS = 2000;

/** The box's text for a value: blank for none, the value's own spelling otherwise. */
export function draftOf(value) {
  return value === null || value === undefined ? "" : String(value);
}

/**
 * The value a draft commits, or `undefined` when it commits nothing — the
 * draft says what is already there, or a number box holds no number (a blank
 * or a word put the value back rather than writing 0). A number is held to
 * the kind's `[min, max]`; an integer kind drops a decimal, never rounds it.
 *
 * @param {{ type: string, min?: number, max?: number }} kind
 * @param {string} draft
 * @param {unknown} current
 */
export function committedValue(kind, draft, current) {
  const text = String(draft ?? "");
  if (kind.type === "integer" || kind.type === "number") {
    const trimmed = text.trim();
    if (trimmed === "") return undefined;
    const n = Number(trimmed);
    if (!Number.isFinite(n)) return undefined;
    const whole = kind.type === "integer" ? Math.trunc(n) : n;
    const held = Math.max(kind.min ?? -Infinity, Math.min(kind.max ?? Infinity, whole));
    return held === current ? undefined : held;
  }
  return text === draftOf(current) ? undefined : text;
}
