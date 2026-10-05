/**
 * The anchors a document's headings carry (ide/03 §Rendered documents): the
 * id GitHub gives a heading, so a table of contents written for GitHub —
 * `[Build and upload](#10-build-and-upload)` — lands here too.
 *
 * GitHub's rule (docs.github.com › Basic writing and formatting syntax ›
 * Section links): letters lower-cased; spaces to hyphens; every other
 * whitespace and punctuation character removed; leading and trailing
 * whitespace trimmed; markup removed and its contents kept, so `_italics_`
 * is `italics` and `Θ` stays; a heading whose anchor repeats an earlier
 * one's gets `-1`, `-2`, … appended.
 *
 * Pure, over the HTML micromark gave: a heading's text is its inner HTML
 * with the tags stripped and the entities decoded. Nothing here reads a DOM.
 */

/** A heading, open tag to close tag; headings never nest. */
const HEADING = /<h([1-6])(\s[^>]*)?>([\s\S]*?)<\/h\1>/gi;
/**
 * What GitHub drops from a heading: whitespace other than the spaces already
 * made hyphens, and punctuation of every script — the hyphen and the
 * underscore excepted; then the ASCII symbols it counts as punctuation —
 * `C++` is `c`. A symbol of another kind, an emoji say, stays.
 */
const DROPPED = /[^\p{L}\p{N}\p{M}\p{S}_-]/gu;
const ASCII_SYMBOLS = /[$+<=>^`|~]/g;
const NAMED_ENTITIES = Object.freeze({ amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " });

/** The entities micromark writes, read back; a numeric one by its code. */
function decodeEntities(s) {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, body) => {
    const lower = body.toLowerCase();
    if (lower.startsWith("#x")) return String.fromCodePoint(Number.parseInt(lower.slice(2), 16));
    if (lower.startsWith("#")) return String.fromCodePoint(Number(lower.slice(1)));
    return NAMED_ENTITIES[lower] ?? whole;
  });
}

/** A heading's words: its inner HTML with the markup removed and the entities read back. */
function plainText(inner) {
  return decodeEntities(inner.replace(/<[^>]*>/g, ""));
}

/**
 * GitHub's anchor for one heading's text, unique among the headings before
 * it: `taken` counts how many times each anchor was given, and the caller
 * threads it through a document.
 * @param {string} text the heading's words, markup already removed
 * @param {Map<string, number>} taken
 * @returns {string} the anchor, or `""` for a heading with no letter or number in it
 */
export function headingSlug(text, taken) {
  const base = text.trim().toLowerCase().replace(/ /g, "-").replace(DROPPED, "").replace(ASCII_SYMBOLS, "");
  if (!base) return "";
  const n = taken.get(base) ?? 0;
  taken.set(base, n + 1);
  return n === 0 ? base : `${base}-${n}`;
}

/**
 * The HTML with each heading given its anchor as an `id` — a heading that
 * already carries one is left as it is, and one with no words to anchor
 * (punctuation alone) gets none.
 * @param {string} html
 * @returns {string}
 */
export function withHeadingIds(html) {
  if (!html) return html;
  const taken = new Map();
  return html.replace(HEADING, (whole, level, attrs, inner) => {
    const attributes = attrs ?? "";
    if (/\sid=/i.test(attributes)) return whole;
    const slug = headingSlug(plainText(inner), taken);
    if (!slug) return whole;
    return `<h${level}${attributes} id="${slug}">${inner}</h${level}>`;
  });
}
