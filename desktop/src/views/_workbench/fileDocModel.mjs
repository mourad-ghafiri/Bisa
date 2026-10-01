/**
 * How a file opens in the workbench's centre (ide/03 §Rendered documents):
 * as the editor alone, as the editor with a rendered view beside it, or as
 * a rendered document with no editor at all. One question, one answer, from
 * the path — the same extension table the artifact kinds keep
 * (`artifactModel.kindOf`, the mirror of the Rust `ArtifactKind::of`), so a
 * PDF is a PDF whether an agent posted it or a person opened it.
 *
 * - **markdown · diagram · html**: the editor, with *Rendered · Split ·
 *   Source*. A page opens on *Source*: a repository's HTML rarely carries
 *   its assets; its rendered view is the same sandboxed page an agent's
 *   artifact runs in, and in a workstream it can be annotated for an agent.
 * - **sheet by text (csv, tsv) · svg**: the editor, with *Rendered ·
 *   Source* — the text stays the editor's; the rendered view is the grid or
 *   the figure.
 * - **pdf · sheet by bytes · document · slides · image · video · audio**: a
 *   rendered document — the bytes drawn as they are, never an editor.
 * - everything else: the editor.
 *
 * The mode control is glyphs alone — an eye for the rendering, brackets
 * for the source, two columns for both — with the word as the tooltip
 * (`modeGlyph`, `modeLabel`); the mode a document is in is remembered
 * under the document's own key (`docModeKey`), never in the layout.
 *
 * Pure: nothing here reads a file.
 */

import { kindOf } from "../../ui/artifact/artifactModel.mjs";
import { bytesWords } from "../../ui/artifact/artifactModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** Sheets whose bytes are text the editor holds: rendered from the buffer, not fetched. */
const TEXT_SHEETS = new Set(["csv", "tsv"]);

function extensionOf(path) {
  const base = String(path ?? "").split("/").pop() ?? "";
  const dot = base.lastIndexOf(".");
  return dot > 0 && dot < base.length - 1 ? base.slice(dot + 1).toLowerCase() : "";
}

/**
 * The kind a path opens as. A sheet is told apart by whether the editor can
 * hold it as text: `sheet_text` (csv, tsv) or `sheet` (xlsx, xls, ods).
 * @param {string} path
 * @returns {"markdown" | "diagram" | "sheet_text" | "svg" | "html" | "pdf" | "sheet" | "document" | "slides" | "image" | "video" | "audio" | "text"}
 */
export function docKindOf(path) {
  const kind = kindOf(String(path ?? ""), "");
  switch (kind) {
    case "markdown":
    case "diagram":
    case "svg":
    case "html":
    case "pdf":
    case "document":
    case "slides":
    case "image":
    case "video":
    case "audio":
      return kind;
    case "sheet":
      return TEXT_SHEETS.has(extensionOf(path)) ? "sheet_text" : "sheet";
    default:
      return "text";
  }
}

/**
 * The modes a kind offers, in the order the control draws them; an empty
 * list is the editor alone.
 * @param {ReturnType<typeof docKindOf>} kind
 */
export function docModes(kind) {
  switch (kind) {
    case "markdown":
    case "diagram":
    case "html":
      return ["rendered", "split", "source"];
    case "sheet_text":
    case "svg":
      return ["rendered", "source"];
    case "pdf":
    case "sheet":
    case "document":
    case "slides":
    case "image":
    case "video":
    case "audio":
      return ["rendered"];
    default:
      return [];
  }
}

/** The mode a kind opens on: a page on its source, everything rendered on its rendering, the rest on the editor. */
export function defaultMode(kind) {
  const modes = docModes(kind);
  if (modes.length === 0) return "source";
  return kind === "html" ? "source" : "rendered";
}

/** A rendered document: bytes drawn as they are, no editor behind them. */
export function isRenderedDoc(kind) {
  const modes = docModes(kind);
  return modes.length === 1 && modes[0] === "rendered";
}

/** What the mode control calls the rendered view: *Preview* for a diagram, *Rendered* for the rest. */
export function renderedLabel(kind) {
  return kind === "diagram" ? t("workbench-file-doc-preview") : t("workbench-file-doc-rendered");
}

/**
 * The word for a mode — the glyph's accessible name and tooltip: *Rendered*
 * (*Preview* for a diagram), *Split*, *Source*.
 * @param {ReturnType<typeof docKindOf>} kind
 * @param {"rendered" | "split" | "source"} mode
 */
export function modeLabel(kind, mode) {
  switch (mode) {
    case "rendered":
      return renderedLabel(kind);
    case "split":
      return t("workbench-file-doc-split");
    default:
      return t("workbench-file-doc-source");
  }
}

/**
 * The glyph a mode wears on the control, as a name in `ICON`: the eye for
 * the rendering, brackets for the source, two columns for both at once.
 * @param {"rendered" | "split" | "source"} mode
 * @returns {"rendered" | "splitRight" | "code"}
 */
export function modeGlyph(mode) {
  switch (mode) {
    case "rendered":
      return "rendered";
    case "split":
      return "splitRight";
    default:
      return "code";
  }
}

/**
 * Where a file's mode is remembered: under the document's own key — the
 * root and the file's tab id, as `editorKey` makes it — beside its place, so
 * a tab switch keeps it, a restart keeps it, and it goes when the tab closes.
 * @param {string} scope the root key (`rootKey(scope, id)`)
 * @param {string} path
 */
export function docModeKey(scope, path) {
  return `${scope}|file:${path}`;
}

/** The kind's word for the toolbar: the artifact kind's, `sheet_text` being a sheet. */
export function artifactKindOf(kind) {
  return kind === "sheet_text" ? "sheet" : kind === "text" ? "file" : kind;
}

/**
 * The refusal's words when the node will not serve a file's bytes: its size
 * against **the limit the node said** — the bound is the node's, and no
 * number of the desktop's stands in for it. An answer that carried neither
 * says what to do without a number.
 * @param {number | null | undefined} size @param {number | null | undefined} limit
 */
export function tooLargeWords(size, limit) {
  if (typeof size !== "number" || typeof limit !== "number") return t("workbench-file-doc-too-large-reveal");
  return t("workbench-file-doc-rendering-stops-reveal-open-another-application", { bytesWords: bytesWords(size), limit: bytesWords(limit) });
}

/** The words for a file the editor was given that turned out not to be text. */
export function binaryWords(size) {
  return t("workbench-file-doc-binary-bytes-no-text-show", { bytesWords: bytesWords(size) });
}
