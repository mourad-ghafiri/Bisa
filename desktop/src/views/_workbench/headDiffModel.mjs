/**
 * The editor's diff against `HEAD` (ide/04 §Blame and history), as facts:
 * which documents can ask for one, what the node's two sides mean for it,
 * and what the bar over it says. The buffer itself is the modified side —
 * unsaved changes included — so the diff is of what you are typing, not of
 * the file as saved; the node is asked for `HEAD`'s side alone
 * (`GET /workstreams/{wid}/git/sides?staged=true`, whose `original` is
 * `HEAD`). Plain JavaScript so `node --test` reads it; `EditorDoc.tsx` asks
 * and draws.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * Whether a document can be diffed against `HEAD`: blame's rule — a
 * checkout question, so a goal's or an agent's folder has no repository to
 * ask — and the document must be a file of that root on disk; an untitled
 * or a loose document has no path git knows.
 * @param {{scope: string, isFile: boolean, onDisk: boolean}} doc
 */
export function canDiffHead({ scope, isFile, onDisk }) {
  return scope === "workstream" && isFile === true && onDisk === true;
}

/**
 * What the node's sides mean for the diff. `HEAD`'s text is the `original`
 * of a staged read; a file `HEAD` does not hold diffs against nothing —
 * every line yours; a side that is not text cannot be diffed at all, which
 * outranks every other verdict; a side cut at the cap is drawn and said so.
 * @param {{original?: string | null, binary: boolean, truncated: boolean}} sides
 * @returns {{kind: "binary"} | {kind: "new" | "truncated" | "ready", original: string}}
 */
export function headSides(sides) {
  if (sides.binary) return { kind: "binary" };
  if (sides.original === null || sides.original === undefined) return { kind: "new", original: "" };
  return { kind: sides.truncated ? "truncated" : "ready", original: sides.original };
}

/**
 * The words the bar over the diff — or the strip, for a side that cannot
 * be drawn — says for a verdict.
 * @param {ReturnType<typeof headSides>["kind"]} kind
 */
export function headDiffWords(kind) {
  switch (kind) {
    case "binary":
      return t("workbench-editor-doc-head-binary");
    case "new":
      return t("workbench-editor-doc-head-new");
    case "truncated":
      return t("workbench-editor-doc-head-truncated");
    default:
      return t("workbench-editor-doc-head-yours");
  }
}
