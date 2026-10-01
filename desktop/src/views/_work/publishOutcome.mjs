/**
 * What a refused push or pull request *was* — read from the node's `code`,
 * never guessed from the status.
 *
 * Every refusal below answers 409, so the status alone once made the panel
 * tell people their project publishes manually when their branch had simply
 * never been pushed. The node now names the refusal (`ErrorBody.code`); this
 * turns the name into the banner's kind. A 409 with no code, or a code this
 * app does not know, is an error with the node's own sentence — it is never
 * read as a policy.
 */

/** Banner kinds, each with its own words in `WorkstreamPanel`. */
const BY_CODE = Object.freeze({
  publish_manual: "manual",
  publish_no_goal: "no_goal",
  publish_declined: "declined",
  nothing_to_publish: "not_ready",
  workstream_state: "state",
  pull_request_state: "state",
});

/**
 * @param {{status: number, code?: string | null, message: string}} err
 * @returns {{kind: "manual" | "no_goal" | "declined" | "not_ready" | "state" | "error", detail: string}}
 */
export function refusalOf(err) {
  const kind = err.code != null && Object.hasOwn(BY_CODE, err.code) ? BY_CODE[err.code] : "error";
  return { kind, detail: err.message };
}

/**
 * What a person approved at the Publish gate and did not go out (ide/08): a
 * `202` was the last word the caller heard, so the act says its own end on
 * the bus — `workstream_publish_failed { workstream, what, reason }`, `what`
 * the act as the gate asked it, `reason` the refusal in words, the node's
 * sentence drawn as it came. This checkout's frame is the banner's `failed`
 * kind; another checkout's, or any other frame, is nothing.
 * @param {unknown} payload an engine frame's payload, whatever its type
 * @param {string} wid the checkout the panel stands on
 * @returns {{kind: "failed", what: string, reason: string} | null}
 */
export function publishFailure(payload, wid) {
  const p = /** @type {{type?: unknown, workstream?: unknown, what?: unknown, reason?: unknown} | null} */ (payload && typeof payload === "object" ? payload : null);
  if (!p || p.type !== "workstream_publish_failed" || p.workstream !== wid) return null;
  return { kind: "failed", what: String(p.what ?? ""), reason: String(p.reason ?? "") };
}
