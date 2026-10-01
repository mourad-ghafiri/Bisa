/**
 * Where a link inside a rendered markdown file points.
 *
 * A `.md` in a repository is full of `./CONTRIBUTING.md` and `../docs/API.md`,
 * and those are the links most worth following — they are how a repository's
 * own documentation is navigated. So the workbench resolves them against the
 * file's own directory and opens the result as another document.
 *
 * # This is a containment check, not a convenience
 *
 * The resolved path is handed straight back to `GET /file/{scope}/{id}?path=`.
 * The node checks containment again — every path is resolved against the
 * scope's root there, and `..` out of the tree is refused with a message naming
 * the boundary — so this is the second of two checks rather than the only one.
 * It is here anyway, because a client that sends a request it knows will be
 * refused is a client that shows people errors instead of not offering the
 * link. Same reasoning as the slug allowlist: refuse early, and refuse for a
 * reason you can name.
 *
 * Anything that is not a relative path within the tree answers `null`, and the
 * caller leaves the link alone for the browser to handle (or neuters it). There
 * is deliberately no third outcome.
 */

/** A protocol, a protocol-relative URL, or a root-absolute path. */
const EXTERNAL = /^([a-z][a-z0-9+.-]*:|\/\/)/i;

/**
 * The document path a link points at, or `null` when it is not a document.
 *
 * `from` is the path of the file the link is *in*, relative to the scope root.
 */
import { docKindOf } from "./fileDocModel.mjs";

export function resolveDocLink(from, href) {
  if (typeof href !== "string") return null;
  const raw = href.trim();
  if (!raw) return null;

  // An anchor is a position in the file you are already reading.
  if (raw.startsWith("#")) return null;
  // `http:`, `mailto:`, `//example.com` — somebody else's business.
  if (EXTERNAL.test(raw)) return null;
  // A root-absolute path is ambiguous in a way that is not worth guessing at:
  // the root of the repository, or of the disk? Refused rather than assumed.
  if (raw.startsWith("/")) return null;

  // A query or fragment on a relative link addresses the same file either way.
  const path = raw.split(/[?#]/, 1)[0];
  if (!path) return null;

  const base = from.includes("/") ? from.slice(0, from.lastIndexOf("/")) : "";
  const segments = base ? base.split("/") : [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      // Out of the top is out of the scope's root. The node would refuse it;
      // this refuses to ask.
      if (segments.length === 0) return null;
      segments.pop();
      continue;
    }
    segments.push(part);
  }
  if (segments.length === 0) return null;
  return segments.join("/");
}

/** Whether a file should render as markdown rather than as source — the one kind table's answer (`fileDocModel`). */
export function isMarkdown(path) {
  return typeof path === "string" && docKindOf(path) === "markdown";
}
