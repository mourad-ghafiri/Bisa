/**
 * Paths and links in text (ide/17): one scanner that finds them, one pass
 * that marks them in rendered HTML, one resolver that turns a path into a
 * door — a document under a root the workspace knows, a choice between
 * several, an absolute path outside every root (reveal only), or nothing
 * the desktop can vouch for.
 *
 * Pure, so `node --test` holds the grammar: an agent's `src/main.rs:42`,
 * a `./docs/x.md`, a `~/Projects/app/README.md`, a `https://…`. A redacted
 * secret (`«secret:kind:tag»`) is never a link, whatever it contains, and a
 * URL is never mistaken for a path. The webview never sends an absolute path
 * anywhere: an absolute path is matched to a root here and named to the node
 * as `(scope, id, relative)` — the node still checks containment.
 */

const URL_RE = /\b(?:https?:\/\/|www\.)[^\s<>"'`\]]+/gi;
const PLACEHOLDER_RE = /«secret:[^»]*»/g;
/** Trailing punctuation a sentence leaves on a link. */
const TRAILING = /[.,;:!?'"’)\]]+$/;
/** What a sentence appends to a URL — a closing paren is judged by balance, below. */
const URL_TRAILING = /[.,;:!?'"’\]]+$/;

/**
 * A bare name — no slash — is a path only with an extension a source tree
 * carries; `e.g.` and `v1.2.3` are not files. A path with a slash needs no
 * extension at all.
 */
const BARE_EXTENSIONS = new Set([
  "rs", "ts", "tsx", "js", "jsx", "mjs", "mts", "cjs", "json", "toml", "yaml", "yml", "md", "mdx", "txt", "css", "scss",
  "html", "svg", "png", "jpg", "jpeg", "gif", "webp", "py", "rb", "go", "java", "kt", "swift", "c", "h", "cpp", "hpp", "cs",
  "sh", "zsh", "bash", "lock", "sql", "sqlite", "xml", "csv", "pdf", "mmd", "env", "log", "ini", "cfg", "conf",
]);

/**
 * A path token: an optional prefix, then slash-separated segments, a
 * trailing slash for a directory, and the address a compiler or a reviewer
 * appends — `:42`, `:42:7`, `#L12`, `#L12-L20` — as part of the token, so
 * the link and its underline cover it and `parseAddress` reads it.
 */
const PATH_RE = /(?:~\/|\.\.?\/|\/)?[A-Za-z0-9_@+][A-Za-z0-9_.@+-]*(?:\/[A-Za-z0-9_.@+-]+)*\/?(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)?/g;
const ADDRESS_RE = /^(.*?)(?::(\d+)(?::(\d+))?|#L(\d+)(?:-L?\d+)?)$/;

/**
 * `src/a.rs:42:7` is the path and where in it; `docs/x.md#L12` too.
 * @param {string} raw
 * @returns {{path: string, line: number | null, col: number | null}}
 */
export function parseAddress(raw) {
  const m = ADDRESS_RE.exec(raw);
  if (!m) return { path: raw, line: null, col: null };
  const line = m[2] ?? m[4];
  return { path: m[1], line: line ? Number(line) : null, col: m[3] ? Number(m[3]) : null };
}

function boundaryBefore(text, i) {
  if (i === 0) return true;
  return /[\s(`"'<[{,;:=]/.test(text[i - 1]);
}

function boundaryAfter(text, i) {
  if (i >= text.length) return true;
  return /[\s)`"'>\]},;:.!?]/.test(text[i]);
}

/** A bare `src` or `README` is a word; a path needs a slash or a known extension, and a letter somewhere. */
function looksLikePath(path) {
  if (!/[A-Za-z]/.test(path)) return false;
  if (path.includes("/")) return path !== "/" && !/^\.\.?\/?$/.test(path);
  const dot = path.lastIndexOf(".");
  if (dot <= 0 || dot === path.length - 1) return false;
  return BARE_EXTENSIONS.has(path.slice(dot + 1).toLowerCase());
}

/**
 * Every link in a text, in order: URLs, then paths outside them and outside
 * any redacted secret.
 * @param {string} text
 * @returns {import("./linkModel.d.mts").LinkSpan[]}
 */
export function findLinks(text) {
  const out = [];
  // What is spoken for — a secret, a URL — as one mark per character, so a
  // long message is scanned once and every overlap check is a glance, not a
  // walk over everything found so far.
  const taken = new Uint8Array(text.length + 1);
  const covered = (s, e) => {
    for (let i = s; i < e; i++) if (taken[i]) return true;
    return false;
  };
  const take = (s, e) => taken.fill(1, s, e);
  for (const m of text.matchAll(PLACEHOLDER_RE)) take(m.index, m.index + m[0].length);
  for (const m of text.matchAll(URL_RE)) {
    let raw = m[0].replace(URL_TRAILING, "");
    // A closing paren belongs to the sentence unless the URL opened one.
    while (raw.endsWith(")") && (raw.match(/\(/g) ?? []).length < (raw.match(/\)/g) ?? []).length) raw = raw.slice(0, -1).replace(URL_TRAILING, "");
    const start = m.index;
    const end = start + raw.length;
    if (end <= start || covered(start, end) || !boundaryBefore(text, start)) continue;
    take(start, end);
    const url = raw.toLowerCase().startsWith("www.") ? `https://${raw}` : raw;
    out.push({ kind: "url", start, end, raw, url });
  }
  for (const m of text.matchAll(PATH_RE)) {
    const raw = m[0].replace(TRAILING, "");
    const start = m.index;
    const end = start + raw.length;
    if (end <= start || covered(start, end) || !boundaryBefore(text, start) || !boundaryAfter(text, end)) continue;
    const { path, line, col } = parseAddress(raw);
    if (!looksLikePath(path)) continue;
    take(start, end);
    out.push({ kind: "path", start, end, raw, path, line, col });
  }
  return out.sort((a, b) => a.start - b.start);
}

function escapeHtml(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function decodeHtml(s) {
  return s.replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
}

/** The anchor a text hit becomes. */
function anchorFor(hit) {
  if (hit.kind === "url") return `<a data-link="url" href="${escapeHtml(hit.url)}" class="link-url">${escapeHtml(hit.raw)}</a>`;
  const line = hit.line === null ? "" : ` data-line="${hit.line}"`;
  const col = hit.col === null ? "" : ` data-col="${hit.col}"`;
  return `<a data-link="path" data-path="${escapeHtml(hit.path)}"${line}${col} class="link-path">${escapeHtml(hit.raw)}</a>`;
}

/**
 * Mark the links in rendered HTML. Text and inline `<code>` gain anchors
 * where `findLinks` says; an anchor micromark made for a URL is tagged and
 * left; fenced `<pre>` blocks, existing anchors and a redacted secret are
 * left alone. Runs after the sanitizer (no raw HTML, no handlers) and before
 * the placeholder chips.
 * @param {string} html
 * @returns {string}
 */
export function linkifyHtml(html) {
  if (!html) return html;
  const out = [];
  let inAnchor = 0;
  let inPre = 0;
  let last = 0;
  const TAG = /<\/?([a-zA-Z][a-zA-Z0-9]*)\b[^>]*>/g;
  const emitText = (chunk) => {
    if (!chunk) return;
    if (inAnchor > 0 || inPre > 0 || !/[A-Za-z]/.test(chunk)) {
      out.push(chunk);
      return;
    }
    const text = decodeHtml(chunk);
    const hits = findLinks(text);
    if (hits.length === 0) {
      out.push(chunk);
      return;
    }
    let at = 0;
    for (const h of hits) {
      out.push(escapeHtml(text.slice(at, h.start)));
      out.push(anchorFor(h));
      at = h.end;
    }
    out.push(escapeHtml(text.slice(at)));
  };
  for (const m of html.matchAll(TAG)) {
    emitText(html.slice(last, m.index));
    let tag = m[0];
    const name = m[1].toLowerCase();
    const closing = tag.startsWith("</");
    if (name === "a") {
      if (closing) inAnchor = Math.max(0, inAnchor - 1);
      else {
        inAnchor += 1;
        const href = /href="([^"]*)"/i.exec(tag)?.[1] ?? "";
        if (!/\sdata-link=/.test(tag)) {
          if (/^https?:/i.test(href)) tag = tag.replace(/^<a\b/i, '<a data-link="url"');
          else if (/^mailto:/i.test(href)) tag = tag.replace(/^<a\b/i, '<a data-link="mail"');
          else if (href && href !== "#" && !/^[a-z][a-z0-9+.-]*:|^\/\//i.test(href)) tag = tag.replace(/^<a\b/i, '<a data-link="doc"');
        }
      }
    } else if (name === "pre") {
      inPre = closing ? Math.max(0, inPre - 1) : inPre + 1;
    }
    out.push(tag);
    last = m.index + m[0].length;
  }
  emitText(html.slice(last));
  return out.join("");
}

function basename(p) {
  const i = p.lastIndexOf("/");
  return i === -1 ? p : p.slice(i + 1);
}

/** A root holds a relative path when the index lists it, or a directory of it. */
function holds(root, rel) {
  if (root.paths.includes(rel)) return "file";
  const dir = rel.endsWith("/") ? rel : `${rel}/`;
  return root.paths.some((p) => p.startsWith(dir)) ? "dir" : null;
}

/**
 * Where a path points, over the roots the surface knows — the current one
 * first. An absolute path (or `~/…`) is matched to the longest root prefix
 * and named relative to it; a relative path is looked up in each root's
 * index, exact first, then by its basename; the answer is one document, a
 * choice, an absolute path outside every root, or nothing vouched for.
 * @param {{path: string, line: number | null, col: number | null}} hit
 * @param {readonly import("./linkModel.d.mts").LinkRoot[]} roots
 * @returns {import("./linkModel.d.mts").LinkResolution}
 */
export function resolveLink(hit, roots) {
  const { line, col } = hit;
  let path = hit.path.replace(/\/+$/, "");
  const doc = (root, rel, kind = "file") => ({ kind: kind === "dir" ? "dir" : "doc", scope: root.scope, id: root.id, path: rel, line, col, root: root.root, label: root.label, indexed: root.paths.includes(rel) });
  if (path.startsWith("/") || path.startsWith("~/")) {
    let best = null;
    for (const root of roots) {
      if (!root.root) continue;
      const rel = relativeUnder(path, root.root);
      if (rel !== null && (best === null || root.root.length > best.root.root.length)) best = { root, rel };
    }
    if (!best) return { kind: "outside", absolute: path, line, col };
    if (best.rel === "") return { kind: "dir", scope: best.root.scope, id: best.root.id, path: "", line: null, col: null, root: best.root.root, label: best.root.label, indexed: true };
    const kind = holds(best.root, best.rel);
    if (path.startsWith("~/") && kind === null) return { kind: "outside", absolute: path, line, col };
    return doc(best.root, best.rel, kind ?? "file");
  }
  if (path.startsWith("./")) path = path.slice(2);
  if (path.split("/").includes("..")) return { kind: "unknown", raw: hit.path };
  const candidates = [];
  for (const root of roots) {
    const kind = holds(root, path);
    if (kind) candidates.push(doc(root, path, kind));
  }
  if (candidates.length === 0 && !path.includes("/")) {
    const name = basename(path);
    for (const root of roots) {
      for (const p of root.paths) {
        if (basename(p) === name) candidates.push(doc(root, p));
        if (candidates.length >= 8) break;
      }
    }
  }
  if (candidates.length === 1) return candidates[0];
  if (candidates.length > 1) return { kind: "choice", candidates };
  return { kind: "unknown", raw: hit.path };
}

/**
 * `absolute` under `root`, as a relative path — `""` for the root itself,
 * null when it is not under it. `~/…` is matched against the root's own
 * tail: the webview has no home directory, so `~/Projects/app` is under
 * `/Users/me/Projects/app` because that root ends that way.
 * @param {string} absolute
 * @param {string} root
 */
export function relativeUnder(absolute, root) {
  const r = root.replace(/\/+$/, "");
  if (absolute.startsWith("~/")) {
    const tail = absolute.slice(2);
    const parts = r.split("/");
    for (let i = 1; i < parts.length; i += 1) {
      const suffix = parts.slice(i).join("/");
      if (tail === suffix) return "";
      if (tail.startsWith(`${suffix}/`)) return tail.slice(suffix.length + 1);
    }
    return null;
  }
  if (absolute === r) return "";
  if (absolute.startsWith(`${r}/`)) return absolute.slice(r.length + 1);
  return null;
}

/** `src/a.rs:42` — the words a menu's first verb carries. */
export function addressWords(path, line, col) {
  if (line === null) return path;
  return col === null ? `${path}:${line}` : `${path}:${line}:${col}`;
}

/** The host a URL card leads with, and the URL under it. */
export function urlWords(url) {
  try {
    const u = new URL(url);
    return { host: u.host, url: u.toString(), scheme: u.protocol.replace(/:$/, "") };
  } catch {
    return { host: url, url, scheme: null };
  }
}
