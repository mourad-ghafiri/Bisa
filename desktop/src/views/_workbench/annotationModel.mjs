/**
 * Annotating a rendered page for an agent, as facts with no React in them
 * (ide/03 §Annotate). A person points at elements of the page and says what
 * should change; each becomes an annotation with a number, drawn as a badge
 * on the element and a line in the tray under the page. The annotations are
 * a **draft of the session** on that one document — nothing on disk, nothing
 * on the node; contrast the diff's review notes (ide/04), which are records
 * with a resolve. Sending posts them as `annotation` chips under the edit
 * contract (`editContent`); attaching moves them to the Agent pane's tray;
 * either empties the draft.
 *
 * The numbering is the order of the list: removing one renumbers, and
 * pointing at an element that already has an annotation replaces its note
 * in place, number kept — the badge the frame draws follows `marksOf`.
 */

import { annotationChip, pageWords } from "./contextChips.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * @typedef {{id: number, selector: string, tag: string, excerpt: string, text: string, note: string}} Annotation
 * @typedef {{seq: number, annotations: Annotation[], message: string}} AnnotationDraft
 */

/** No annotation, no words: what a document starts with. */
export const EMPTY_DRAFT = Object.freeze({ seq: 1, annotations: Object.freeze([]), message: "" });

/**
 * Add an annotation for a picked element — or, when that element already has
 * one, replace its note in place. A note with no words adds nothing.
 * @param {AnnotationDraft} draft
 * @param {{selector: string, tag: string, excerpt: string, text: string}} pick
 * @param {string} note
 * @returns {AnnotationDraft}
 */
export function addAnnotation(draft, pick, note) {
  const words = String(note ?? "").trim();
  if (!words) return draft;
  const at = draft.annotations.findIndex((a) => a.selector === pick.selector);
  if (at >= 0) {
    const annotations = draft.annotations.map((a, i) => (i === at ? { ...a, tag: pick.tag, excerpt: pick.excerpt, text: pick.text, note: words } : a));
    return { ...draft, annotations };
  }
  const next = { id: draft.seq, selector: pick.selector, tag: pick.tag, excerpt: pick.excerpt, text: pick.text, note: words };
  return { ...draft, seq: draft.seq + 1, annotations: [...draft.annotations, next] };
}

/** Remove one; the rest keep their order and take the numbers that follow. @param {AnnotationDraft} draft @param {number} id */
export function removeAnnotation(draft, id) {
  return { ...draft, annotations: draft.annotations.filter((a) => a.id !== id) };
}

/** The optional message that leads the request. @param {AnnotationDraft} draft @param {string} message */
export function setMessage(draft, message) {
  return { ...draft, message: String(message ?? "") };
}

/** The badges the frame draws: each element's locator with its number and its note — what a box on it opens with. @param {AnnotationDraft} draft */
export function marksOf(draft) {
  return draft.annotations.map((a, i) => ({ selector: a.selector, n: i + 1, note: a.note }));
}

/**
 * A tray row's words: the number, the element as an inspector names it, the
 * change wanted — `1 · <button.cta> make it blue`.
 * @param {number} n
 * @param {Pick<Annotation, "tag" | "note">} a
 */
export function annotationLabel(n, a) {
  return `${n} · <${a.tag}> ${a.note}`;
}

/** The words a tray row wears when the page no longer has its element. @param {readonly number[]} lost @param {number} n */
export function staleWords(lost, n) {
  return lost.includes(n) ? t("workbench-annotation-not-page-any-more-chip-keeps") : null;
}

/** The chips the annotations become, in number order. @param {import("./contextChips.mjs").PageRef} page @param {readonly Annotation[]} annotations */
export function annotationChips(page, annotations) {
  return annotations.map((a) => annotationChip(page, a.selector, a.excerpt, a.note));
}

/**
 * What the request is about, for the edit contract's sentence: *the
 * annotated element in index.html*, *the 3 annotated elements in index.html*
 * — or, for a page the browser showed from a server, *the annotated element
 * of the page at http://localhost:5173/pricing*.
 * @param {number} count
 * @param {import("./contextChips.mjs").PageRef} page
 */
export function annotationsTarget(count, page) {
  const what = count === 1 ? t("workbench-annotation-annotated-element") : t("workbench-annotation-annotated-elements", { count });
  if (page.kind === "url") return t("workbench-annotation-page", { what, url: page.url });
  const name = String(page.path ?? "").split("/").pop() || String(page.path ?? "");
  return t("workbench-annotation-words", { what, name });
}

/**
 * The words that lead the request: the person's own, then where the
 * changes are — each annotation a chip with the change wanted for it. Fed
 * to `messageBody({mode: "edit"})`, which appends the edit contract.
 * @param {string} words
 * @param {number} count
 */
export function annotationsContent(words, count) {
  const own = String(words ?? "").trim();
  const where =
    count === 1
      ? t("workbench-annotation-annotated-element-attached-chip-change-wanted")
      : t("workbench-annotation-each-annotated-elements-attached-chip-change", { count });
  return own ? `${own}\n\n${where}` : where;
}

/**
 * Where a page's annotations are kept in the checkout's session: a file's
 * under its path, a served page's under its URL — two pages, two drafts.
 * @param {string} scope the root key
 * @param {import("./contextChips.mjs").PageRef} page
 */
export function annotationsKey(scope, page) {
  return `${scope}|annotations|${page.kind}:${pageWords(page)}`;
}
