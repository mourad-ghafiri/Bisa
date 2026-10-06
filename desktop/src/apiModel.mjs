/**
 * The client's facts (`api.ts` does the I/O): where the node is, how a
 * failure reads, and when a read is given a deadline. Plain `.mjs`, so
 * `node --test` reads it.
 *
 * - **The base** is the environment's word, else the shell's, else the
 *   default port — with no trailing slash, since every path starts with one.
 * - **A refusal** (4xx) is the node's own sentence when it sent one
 *   (`ErrorBody.error`), else the body's first line when it is not JSON, else
 *   the status line. The `code` and the typed `detail` ride beside it.
 * - **No answer** is one of three things, never confused: the caller gave up
 *   (its signal aborted — nothing to say), the read outstayed its deadline
 *   (the node is up but slow — the last value stands), or the node could not
 *   be reached (offline — every list waits for it).
 * - **A read has a deadline** — every `GET`, whether or not the caller hands
 *   a signal of its own: a caller's signal says *I left*, never *this took
 *   too long*, and nearly every read carries one — so a list that never
 *   answers becomes a named degraded read, never a pane that waits for the
 *   rest of the session. A write has none: a git operation or a run may
 *   legitimately take a while, and a caller that wants a bound passes its
 *   own signal.
 * - **A stream read once** (`api.ts` `sse`) that ends before its last frame
 *   did not finish, and says so in the catalog's words.
 */

import { t, tx } from "./i18n/l10n.mjs";

export const DEFAULT_BASE = "http://127.0.0.1:4477";

/** How long a plain read may take before it is said to have timed out. */
export const API_READ_TIMEOUT_MS = 30_000;

/** How much of a non-JSON error body is kept as the sentence. */
const MAX_BODY_DETAIL_CHARS = 200;

/** A base with no trailing slash, since every path starts with one. @param {string} base */
export function normalizeBase(base) {
  return String(base).replace(/\/+$/, "");
}

/**
 * The base to talk to: the environment's override, else what the shell
 * answered, else the default port.
 * @param {string | null | undefined} env `BISA_API_BASE` / `VITE_API_BASE`
 * @param {string | null | undefined} shell the shell's `api_base`, when it had a node
 */
export function baseOf(env, shell) {
  if (typeof env === "string" && env.trim()) return normalizeBase(env.trim());
  if (typeof shell === "string" && shell.trim()) return normalizeBase(shell.trim());
  return DEFAULT_BASE;
}

/** The node's refusal code, when the body named one. @param {unknown} body */
export function codeOf(body) {
  const b = /** @type {{code?: unknown} | null} */ (body && typeof body === "object" ? body : null);
  return typeof b?.code === "string" && b.code ? b.code : null;
}

/**
 * Which sentence a refusal is — the id of the catalog message it travels as
 * (`ErrorBody.text.id`). The same in every language, so a form that places a
 * refusal beside the input it is about switches on this, never on the words.
 * @param {unknown} body
 * @returns {string | null}
 */
export function refusalOf(body) {
  const text = /** @type {{text?: unknown} | null} */ (body && typeof body === "object" ? body : null)?.text;
  const id = /** @type {{id?: unknown} | null} */ (text && typeof text === "object" ? text : null)?.id;
  return typeof id === "string" && id ? id : null;
}

/** The typed facts behind a refusal, when the body carried any. @param {unknown} body */
export function detailOf(body) {
  const b = /** @type {{detail?: unknown} | null} */ (body && typeof body === "object" ? body : null);
  const d = b?.detail;
  return d !== undefined && d !== null && typeof d === "object" && !Array.isArray(d) ? /** @type {Record<string, unknown>} */ (d) : null;
}

/**
 * The sentence a failed answer reads under: the node's `error`, else the
 * body's first line when it was not JSON, else the status line.
 * @param {number} status @param {string} statusText @param {unknown} body the parsed JSON body, if any @param {string} [text] the raw body
 */
export function errorDetail(status, statusText, body, text = "") {
  const b = /** @type {{error?: unknown, text?: unknown} | null} */ (body && typeof body === "object" ? body : null);
  // The node's `text` is the sentence as data (17 — Internationalisation): said here, in this language; its `error` is the same sentence in the request's language, for anyone reading raw HTTP.
  const said = b?.text && typeof b.text === "object" && typeof (/** @type {{id?: unknown}} */ (b.text)).id === "string" ? tx(/** @type {{id: string, args?: Record<string, unknown> | null}} */ (b.text)) : "";
  if (said.trim()) return said;
  if (typeof b?.error === "string" && b.error.trim()) return b.error;
  const line = String(text ?? "")
    .split("\n")
    .map((l) => l.trim())
    .find(Boolean);
  if (line && body === undefined) return line.length > MAX_BODY_DETAIL_CHARS ? `${line.slice(0, MAX_BODY_DETAIL_CHARS)}…` : line;
  return `${status} ${statusText}`.trim();
}

/** The node is unreachable — every read waits for it. @param {number} status */
export function isOffline(status) {
  return status === 0;
}

/** A malfunction the log keeps, as against a refusal the caller shows. @param {number} status */
export function isServerError(status) {
  return status >= 500;
}

/**
 * The node answered, and said no: a refusal about what was asked — a plain
 * folder asked for its git identity, a thing that is not there — as against
 * a malfunction or no answer at all. A read that treats **only** this as
 * *nothing there* keeps the other two failures in view.
 * @param {unknown} error
 */
export function isRefusal(error) {
  const status = error && typeof error === "object" ? error.status : undefined;
  return typeof status === "number" && status >= 400 && status < 500;
}

/**
 * The shell's tagged refusal of a loose file (ide/03 §Loose files) as the
 * status and body the node's editor route would answer for the same fact,
 * so the editor reads a loose file's refusal exactly as a root file's:
 * `conflict` a 409 carrying the current hash and text, `missing` a 404,
 * `too_large` a 413 carrying the size and the limit the shell set — what
 * `editorModel.loadFailure` and `saveFailure` read for their sentence — and
 * any other tag a 400. What carries no tag is a 400 in its own words.
 * @param {unknown} e what the shell threw
 * @param {string} untold the sentence for a tagged refusal that says nothing
 * @returns {{status: number, message: string, body: Record<string, unknown>}}
 */
export function looseRefusal(e, untold) {
  const err = /** @type {{kind?: unknown, message?: unknown, current_hash?: unknown, current_text?: unknown, size?: unknown, limit?: unknown} | null} */ (e && typeof e === "object" ? e : null);
  if (err && typeof err.kind === "string") {
    const message = typeof err.message === "string" && err.message ? err.message : untold;
    switch (err.kind) {
      case "conflict":
        return { status: 409, message, body: { error: message, current_hash: err.current_hash, current_text: err.current_text } };
      case "missing":
        return { status: 404, message, body: { error: message } };
      case "too_large":
        return { status: 413, message, body: { error: message, size: err.size, limit: err.limit } };
      default:
        return { status: 400, message, body: { error: message } };
    }
  }
  const message = e instanceof Error ? e.message : String(e);
  return { status: 400, message, body: { error: message } };
}

/**
 * Whether a request takes the read deadline: every `GET`. The caller's own
 * signal does not excuse it — that signal ends the read when the caller
 * leaves, and the deadline ends it when the node never answers.
 * @param {string} method
 */
export function takesDeadline(method) {
  return String(method).toUpperCase() === "GET";
}

/**
 * A deadline as the sentence says it: whole seconds. The clock counts in
 * milliseconds and the message (`app-api-no-answer-within-s`) says seconds,
 * so the change of unit happens here and nowhere else — rounded **up**, so
 * the sentence never names a shorter wait than the one that ran out (1.5 s
 * is *within 2 s*, never *within 1 s*), and never *0 s*. A deadline that is
 * no number reads as the reads' own.
 * @param {number} timeoutMs
 * @returns {number}
 */
export function deadlineSeconds(timeoutMs) {
  const ms = Number.isFinite(timeoutMs) && timeoutMs > 0 ? timeoutMs : API_READ_TIMEOUT_MS;
  return Math.max(1, Math.ceil(ms / 1000));
}

/**
 * What a request that produced no answer means. The caller's word comes
 * first: a caller that left has nothing to be told, whatever else ran out.
 * @param {unknown} error what `fetch` threw
 * @param {boolean} callerAborted whether the caller's own signal is aborted
 * @param {boolean} [timedOut] whether the read's own deadline ran out
 * @param {number} [timeoutMs] the deadline that may have run out, in milliseconds — said in seconds (`deadlineSeconds`)
 * @returns {{kind: "aborted" | "timeout" | "unreachable", reason: string}}
 */
export function failureOf(error, callerAborted, timedOut = false, timeoutMs = API_READ_TIMEOUT_MS) {
  if (callerAborted) return { kind: "aborted", reason: t("app-api-caller-gave-up") };
  const name = /** @type {{name?: unknown} | null} */ (error && typeof error === "object" ? error : null)?.name;
  if (timedOut || name === "TimeoutError") return { kind: "timeout", reason: t("app-api-no-answer-within-s", { seconds: deadlineSeconds(timeoutMs) }) };
  // The reason a person reads is the catalog's word for an absent node; the
  // engine's own text ("Failed to fetch", "Load failed") is `detail`, for the
  // log — it named the transport, never the node, and read as the node's own
  // sentence in every error note.
  const message = /** @type {{message?: unknown} | null} */ (error && typeof error === "object" ? error : null)?.message;
  return { kind: "unreachable", reason: t("app-api-node-unreachable"), detail: typeof message === "string" && message ? message : null };
}

/**
 * The sentence of an answer that came and could not be read — a `200` whose
 * body is no JSON. The node is there: this is never *unreachable*.
 */
export function answerUnreadableWords() {
  return t("app-api-answer-could-not-be-read");
}

/**
 * The sentence of a stream read once that ended before its last frame — the
 * node went away, or was asked to stop, while the answer was still coming.
 */
export function streamEndedWords() {
  return t("app-api-stream-ended-before-finished");
}

/**
 * The `Authorization` value for a token: the protocol's word and the token,
 * for the node to read. **Words for a machine, written here** — never a
 * message of the catalog, where a translation would change what the node is
 * sent and every call would read `401`.
 * @param {string} token
 * @returns {string}
 */
export function bearer(token) {
  return `Bearer ${token}`; // for the machine
}

/**
 * The token to keep from what a source answered — the environment, or the
 * shell. An empty answer is kept by nobody: the shell mints its token with
 * the node's first start, and a start that failed before that answers `""`;
 * a page that remembered it would send no token, and read `401`, for the
 * rest of its life. `null` — ask again on the next call.
 * @param {unknown} answer
 * @returns {string | null}
 */
export function tokenToKeep(answer) {
  const token = typeof answer === "string" ? answer.trim() : "";
  return token ? token : null;
}
