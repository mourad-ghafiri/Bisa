/**
 * The document a page artifact runs as (ide/12).
 *
 * A page an agent wrote is untrusted. It runs in an `<iframe sandbox>` with
 * scripts and nothing else — an opaque origin, no access to the app's origin,
 * its token or its storage — and, as a second wall the sandbox does not
 * depend on, the document carries its own Content-Security-Policy in a
 * `<meta>` at the top of its head: no network at all, except scripts, styles
 * and fonts from three public CDNs when the `artifacts.html.libraries`
 * setting allows them. `connect-src 'none'` means a page that tries the
 * node's own port gets nothing back; `frame-src 'none'` and
 * `form-action 'none'` close the two other ways out.
 *
 * When the IDE annotates a page (ide/03 §Annotate) it asks for the
 * inspector: one inline script (`pageInspector.mjs`) placed right after the
 * metas, before anything of the page's own. The policy already allows an
 * inline script, so the inspector adds no capability, and it is never
 * injected for an agent's artifact card or stage.
 */

import { inspectorScript } from "./pageInspector.mjs";

export const PAGE_SANDBOX = "allow-scripts";

/** The libraries a page may load, when the setting allows it. */
export const PAGE_CDNS = Object.freeze([
  "https://cdnjs.cloudflare.com",
  "https://cdn.jsdelivr.net",
  "https://unpkg.com",
]);

const FONT_CDNS = Object.freeze(["https://fonts.googleapis.com", "https://fonts.gstatic.com"]);

/** The policy, as the `content` of the meta. */
export function pageCsp({ libraries }) {
  const cdns = libraries ? PAGE_CDNS.join(" ") : "";
  const styleHosts = libraries ? `${cdns} ${FONT_CDNS[0]}` : "";
  const fontHosts = libraries ? `${cdns} ${FONT_CDNS[1]}` : "";
  return [
    "default-src 'none'",
    `script-src 'unsafe-inline' 'unsafe-eval' ${cdns}`.trim(),
    `style-src 'unsafe-inline' ${styleHosts}`.trim(),
    `font-src data: ${fontHosts}`.trim(),
    "img-src data: blob: https:",
    "media-src data: blob:",
    "connect-src 'none'",
    "frame-src 'none'",
    "form-action 'none'",
    "base-uri 'none'",
  ].join("; ");
}

function escapeAttr(s) {
  return String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

/**
 * The page with its walls: the CSP meta and a colour-scheme meta placed at
 * the top of `<head>` — first, so nothing in the page runs before the
 * policy applies — and, given an `inspector` theme, the inspector script
 * dressed in it right after them, before anything of the page's. A fragment
 * with no head gets one.
 * @param {string} html
 * @param {{ libraries?: boolean, scheme?: "light" | "dark", inspector?: import("./inspectorTheme.mjs").InspectorTheme | null }} [options] `inspector` is the overlay's theme — the request and the dress are one word; `null` carries no inspector
 */
export function pageDocument(html, { libraries = true, scheme = "light", inspector = null } = {}) {
  const metas =
    `<meta http-equiv="Content-Security-Policy" content="${escapeAttr(pageCsp({ libraries }))}">` +
    `<meta name="color-scheme" content="${scheme === "dark" ? "dark light" : "light dark"}">` + // for the machine
    (inspector ? `<script>${inspectorScript(inspector)}</script>` : "");
  const source = String(html ?? "");
  const head = /<head(\s[^>]*)?>/i.exec(source);
  if (head) {
    const at = head.index + head[0].length;
    return source.slice(0, at) + metas + source.slice(at);
  }
  const htmlTag = /<html(\s[^>]*)?>/i.exec(source);
  if (htmlTag) {
    const at = htmlTag.index + htmlTag[0].length;
    return `${source.slice(0, at)}<head>${metas}</head>${source.slice(at)}`;
  }
  return `<!doctype html><html><head>${metas}</head><body>${source}</body></html>`;
}
