/**
 * A rendered Markdown document, as the sources show it (ide/03 §Rendered
 * documents, ide/17): a file at a path renders its raw HTML as GitHub does —
 * through DOMPurify's prose profile, its front matter left out — while a
 * message keeps its HTML escaped; the HTML goes into the page through one
 * object per string, so react-dom never replaces the text nodes a find holds
 * ranges over; and the find over a rendering walks again when the DOM
 * changes, skips text nobody sees, reveals the current match inside the
 * rendering's own scrollports, leaves it selected when the bar closes, and
 * hands the keyboard back after Escape. Source assertions, as
 * `files.test.mjs` makes them — no DOM. Run with
 * `node --test desktop/src/scenarios/renderedMarkdown.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { PROSE_PROFILE } from "../ui/markdownHtmlModel.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

/** The text from one marker up to the next, both of which must be there. */
const between = (text, from, to) => {
  const start = text.indexOf(from);
  assert.ok(start >= 0, `the source says \`${from}\``);
  const end = text.indexOf(to, start + from.length);
  assert.ok(end > start, `and \`${to}\` after it`);
  return text.slice(start, end);
};

/** Every marker is there, each after the one before it. */
const inOrder = (text, markers, why) => {
  let at = -1;
  for (const marker of markers) {
    const next = text.indexOf(marker, at + 1);
    assert.ok(next > at, `${why}: \`${marker}\``);
    at = next;
  }
};

test("a document's raw HTML renders through the prose profile, after its front matter is left out, and a message's stays escaped", () => {
  const md = read("ui/Markdown.tsx");
  assert.ok(md.includes('import DOMPurify from "dompurify";') && md.includes("DOMPurify(window)"), "the kit's one sanitizer, an instance of this renderer's own");
  assert.ok(md.includes('purifier?.addHook("uponSanitizeElement"') && md.includes("isGfmTaskBox(el.getAttribute(\"type\"), el.getAttribute(\"disabled\"))"), "the one input kept is GFM's task box");
  assert.ok(md.includes("relativeLinks ? withoutFrontMatter(text) : text"), "front matter is a document's to lose, not a message's");
  const pipeline = between(md, "const html = useMemo(", "const inner = useMemo(");
  inOrder(
    pipeline,
    ["const prose = relativeLinks && purifier !== null;", "allowDangerousHtml: prose,", "allowDangerousProtocol: false,", "purifier.sanitize(raw, PROSE_PROFILE)", "sanitizeUrls(", "placeholderChips(linkifyHtml(relativeLinks ? withHeadingIds(sanitized) : sanitized)"],
    "micromark keeps raw HTML only for the sanitizer, which runs before every string pass",
  );
  assert.ok(!pipeline.includes("allowDangerousHtml: true"), "never unconditionally");
  assert.equal(PROSE_PROFILE.ALLOW_DATA_ATTR, false);
});

test("the HTML goes into the page as one object per string — in the document, the diagram and the Word view — so a re-render never replaces the text nodes a find holds", () => {
  const md = read("ui/Markdown.tsx");
  assert.ok(md.includes("const inner = useMemo(() => ({ __html: html }), [html]);"), "memoised on the string");
  assert.ok(md.includes("dangerouslySetInnerHTML={inner}") && md.includes("dangerouslySetInnerHTML={seg.inner}"), "both paths use it");
  assert.ok(!md.includes("dangerouslySetInnerHTML={{"), "never a fresh object at render");
  assert.ok(md.includes("fences.length !== blocks.length) return [{ inner }]"), "a mermaid fence is split only when the source's fences match the HTML's");
  const docx = read("ui/artifact/DocumentView.tsx");
  assert.ok(docx.includes("const inner = useMemo(() => (html === null ? undefined : { __html: html }), [html]);") && docx.includes("dangerouslySetInnerHTML={inner}"));
  const mermaid = read("ui/MermaidView.tsx");
  assert.ok(mermaid.includes("const drawing = useMemo(() => ({ __html: svg }), [svg]);") && mermaid.includes("dangerouslySetInnerHTML={drawing}"));
});

test("the find over a rendering walks again when its DOM changes, skips text nobody sees, reveals the match in its own scrollports, and can select it", () => {
  const hook = read("ui/find/useDomFind.ts");
  assert.ok(hook.includes("new MutationObserver(") && hook.includes("{ childList: true, characterData: true, subtree: true }"), "the DOM drawn again is walked again");
  assert.ok(hook.includes("cancelAnimationFrame(frame)") && hook.includes("observer.disconnect()"), "and let go on cleanup");
  assert.ok(hook.includes("UNSEEN.has(el.tagName.toUpperCase())") && hook.includes('"STYLE"') && hook.includes('"DESC"'), "an SVG's own stylesheet, lowercase, is not prose");
  assert.ok(hook.includes("el.getClientRects().length === 0) return NodeFilter.FILTER_REJECT"), "a closed details, a hidden block: not found");
  assert.ok(hook.includes("yieldKeptScroll(el);") && hook.includes("revealOffset(range.getBoundingClientRect(), el.getBoundingClientRect(), el.scrollTop)"), "revealed by its own rect, the kept place yielding first");
  assert.ok(!hook.includes("scrollIntoView("), "nothing outside the rendering moves");
  assert.ok(hook.includes("selection.setBaseAndExtent(first.startContainer, first.startOffset, last.endContainer, last.endOffset)"), "the whole match, across nodes");
  assert.ok(hook.includes('el.addEventListener("pointerdown", onPress, { once: true, capture: true })'), "the next press collapses it, so a link click is a click");
  assert.ok(hook.includes("return useMemo(() => ({ count: active ? ranges.length : null, select }), [active, ranges.length, select]);"));
});

test("Escape lets go of the keyboard before the close, and every surface leaves the found occurrence selected before handing the keyboard back", () => {
  const bar = read("ui/find/FindBar.tsx");
  assert.equal((bar.match(/e\.currentTarget\.blur\(\);\n\s*onClose\(\);/g) ?? []).length, 2, "both fields, the find and the replace");
  const editor = read("views/_workbench/EditorDoc.tsx");
  inOrder(between(editor, "const closeFind = () => {", "\n  };"), ["if (domFindable) domFound.select();", "takeKeyboard(renderedShown())"], "selected, then the keyboard back");
  const rendered = read("views/_workbench/RenderedFileDoc.tsx");
  inOrder(between(rendered, "onClose={() => {", "}}"), ["domFound.select();", "takeKeyboard(bodyBox.current)"], "the Word view the same");
  const note = read("notes/NoteEditor.tsx");
  assert.ok(between(note, "const closeFind = () => {", "\n  };").includes("if (reading) domFound.select();"), "a note's Read view the same; its editor keeps its own caret");
});

test("the highlights read on every surface: an amber wash, the current match darker and underlined, never the accent", () => {
  const css = read("styles.css");
  const all = between(css, "::highlight(bisa-find) {", "}");
  const current = between(css, "::highlight(bisa-find-current) {", "}");
  assert.ok(all.includes("color-mix(in oklch, var(--color-warn) 25%, transparent)") && all.includes("color: var(--color-text)"));
  assert.ok(current.includes("color-mix(in oklch, var(--color-warn) 50%, transparent)") && current.includes("text-decoration: underline"));
  assert.ok(!all.includes("accent") && !current.includes("accent"), "the accent means your attention, not a match");
});
