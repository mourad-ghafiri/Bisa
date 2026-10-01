/**
 * What a read of a file's bytes that threw means (ide/03 §Rendered
 * documents): a `413` is a file over the size the node serves to a renderer
 * — with the size and **the limit the node said**, each a number or nothing,
 * never a guess — and anything else a failure in its own words.
 * `fileBytes.ts` reads through this; the words are the workbench's
 * (`fileDocModel.tooLargeWords`).
 *
 * Facts only. Plain `.mjs`, so `node --test` reads it.
 */

/** A whole number of bytes off the wire, or null for anything else. */
function bytesOf(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

/**
 * @param {unknown} error
 * @returns {{state: "too_large", size: number | null, limit: number | null} | {state: "failed", error: string}}
 */
export function bytesFailure(error) {
  const status = error && typeof error === "object" ? error.status : undefined;
  if (status === 413) {
    const body = error.body && typeof error.body === "object" ? error.body : {};
    return { state: "too_large", size: bytesOf(body.size), limit: bytesOf(body.limit) };
  }
  return { state: "failed", error: error instanceof Error ? error.message : String(error) };
}
