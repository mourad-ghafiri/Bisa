/**
 * What a rendered document's HTML may carry (ide/03 §Rendered documents,
 * ide/17): the facts behind `Markdown.tsx`'s passes, where `node --test`
 * can read them. A `.md` file out of somebody's repository — a README with
 * a `<details>`, a row of badges in `<p align="center">`, a comment, a
 * YAML front matter — is untrusted input, and GitHub's answer is the one
 * the person expects: the raw HTML drawn, after a sanitizer, with nothing
 * that runs, styles, frames, submits, plays or hides.
 *
 * - `PROSE_PROFILE` is DOMPurify's configuration for such a document, and
 *   `isGfmTaskBox` the one exception its hook makes among `<input>`s.
 * - `withoutFrontMatter` leaves a file's YAML front matter out of the
 *   rendering, as most previewers do; Source mode still shows it.
 * - `TAG_REST` is how every string pass after the sanitizer reads a tag:
 *   to its first `>` outside a double-quoted value, since DOMPurify's
 *   serializer always double-quotes an attribute and may leave a `>` inside
 *   one — micromark alone never did.
 *
 * Messages, notes and cards do not come this way: their raw HTML stays
 * escaped text, so `Vec<T>` in an agent's prose keeps its brackets.
 */

/**
 * The rest of a tag after its name: attributes and whitespace, a `>` inside a
 * double-quoted value included. Build a regex with it; never share one, since
 * a global regex keeps its place.
 */
export const TAG_REST = String.raw`(?:[^>"]|"[^"]*")*`;

/**
 * DOMPurify's prose profile for a rendered document. `USE_PROFILES: html`
 * is the library's own HTML allow-list, which already leaves out scripts,
 * frames, objects, SVG, MathML and every event handler; what it keeps and a
 * document must not have is forbidden here by name: anything that runs,
 * styles, submits, plays, draws, pops over or hides. `data-*` is off because
 * every mark the app reads — `data-link`, `data-path`, `data-doc-src`,
 * `data-scroll-keep`, `data-document`, `data-rendered-doc` — is the app's own,
 * added after the sanitizer or carried by the shell's elements, and a README
 * must not be able to mint one. Comments are never a tag, so the profile
 * removes every one.
 */
export const PROSE_PROFILE = Object.freeze({
  USE_PROFILES: Object.freeze({ html: true }),
  ALLOW_DATA_ATTR: false,
  FORBID_TAGS: Object.freeze([
    // runs, styles, frames, embeds — outside the profile already, named so a profile change cannot let them back
    "script", "style", "iframe", "object", "embed", "svg", "math", "link", "meta", "base", "template", "noscript", "title", "head", "body", "html",
    // submits, controls, dialogs
    "form", "button", "select", "option", "optgroup", "datalist", "textarea", "label", "fieldset", "legend", "output", "menu", "menuitem", "dialog",
    // plays, draws, scrolls
    "video", "audio", "source", "track", "canvas", "marquee", "meter", "progress",
  ]),
  FORBID_ATTR: Object.freeze([
    "style",
    // a second way to fetch, play or hide
    "srcset", "poster", "background", "autoplay", "hidden", "inert", "tabindex", "download", "draggable",
    // the popover and command invokers
    "popover", "popovertarget", "popovertargetaction", "command", "commandfor",
  ]),
});

/**
 * The one `<input>` a document keeps: GFM's task-list box, as micromark
 * writes it — `type="checkbox"` and `disabled`. Any other input is a control
 * a README must not draw.
 * @param {string | null | undefined} type the element's `type` attribute
 * @param {string | null | undefined} disabled the element's `disabled` attribute, `null` when absent
 */
export function isGfmTaskBox(type, disabled) {
  return String(type ?? "").toLowerCase() === "checkbox" && disabled !== null && disabled !== undefined;
}

/**
 * A file's YAML front matter left out: a block that opens on the very first
 * line with `---` and closes on a line that is exactly `---` or `...` (CRLF
 * tolerated). A `---` rule later in the file, or a block that never closes,
 * is content and stays.
 * @param {string} text
 * @returns {string}
 */
export function withoutFrontMatter(text) {
  const s = String(text ?? "");
  const lines = s.split("\n");
  if (lines.length < 2 || lines[0].replace(/\r$/, "") !== "---") return s;
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].replace(/\r$/, "");
    if (line === "---" || line === "...") return lines.slice(i + 1).join("\n");
  }
  return s;
}
