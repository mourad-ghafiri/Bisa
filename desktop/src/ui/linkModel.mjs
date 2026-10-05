/**
 * Paths and links in text (ide/17): one scanner that finds them, one pass
 * that marks them in rendered HTML, one resolver that turns a path into a
 * door — a document under a root the workspace knows, a choice between
 * several, an absolute path outside every root (reveal only), a path under a
 * root that its index does not list (asked of the node before it is
 * offered), or nothing the desktop can vouch for.
 *
 * A relative path is read from **where the surface stands** when it says —
 * a shell's current directory — then from each root, then by its tail: the
 * `src/lib.rs` a compiler printed from a sub-crate, the `../README.md` a
 * shell listed, the `target/out.log` no index carries.
 *
 * Pure, so `node --test` holds the grammar: an agent's `src/main.rs:42`,
 * a `./docs/x.md`, a `.github/workflows/ci.yml`, a `~/Projects/app/README.md`,
 * a `https://…`. A redacted secret (`«secret:kind:tag»`) is never a link,
 * whatever it contains, and a URL is never mistaken for a path. The webview
 * never sends an absolute path anywhere: an absolute path is matched to a
 * root here and named to the node as `(scope, id, relative)` — the node still
 * checks containment.
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
 * A path token: an optional prefix — `~/`, `/`, or one or more `./` and
 * `../`, so `../../lib/a.ts` climbs as far as it says — then slash-separated
 * segments — the first may lead with one dot, `.github/workflows/ci.yml` — a trailing slash
 * for a directory, and the address a compiler or a reviewer appends — `:42`,
 * `:42:7`, `#L12`, `#L12-L20` — as part of the token, so the link and its
 * underline cover it and `parseAddress` reads it. A dotfile with no slash
 * (`.env`, `.gitignore`) is a word: `looksLikePath` reads no extension in it.
 */
const PATH_RE = /(?:~\/|\/|(?:\.\.?\/)+)?\.?[A-Za-z0-9_@+][A-Za-z0-9_.@+-]*(?:\/[A-Za-z0-9_.@+-]+)*\/?(?::\d+(?::\d+)?|#L\d+(?:-L?\d+)?)?/g;
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

/** How many documents a path found by its tail may offer: enough for a choice, bounded for a click. */
export const MAX_CANDIDATES = 8;

/** A root holds a relative path when the index lists it, or a directory of it. */
function holds(root, rel) {
  if (root.paths.includes(rel)) return "file";
  const dir = rel.endsWith("/") ? rel : `${rel}/`;
  return root.paths.some((p) => p.startsWith(dir)) ? "dir" : null;
}

/** The root an absolute path is under — the longest — with the path relative to it; null outside every root. */
function rootOf(absolute, roots) {
  let best = null;
  for (const root of roots) {
    if (!root.root) continue;
    const rel = relativeUnder(absolute, root.root);
    if (rel !== null && (best === null || root.root.length > best.root.root.length)) best = { root, rel };
  }
  return best;
}

/** A path that says where it starts — `./a`, `../a` — as an absolute one does; a bare `a/b` or `a.rs` does not. */
function saysWhereItStarts(path) {
  return path.startsWith("./") || path.startsWith("../");
}

/**
 * `path` read from `base`, an absolute directory: `.` and `..` resolved, a
 * doubled slash dropped, a climb past `/` stopped there. An absolute `path`
 * is its own base.
 * @param {string} base
 * @param {string} path
 * @returns {string}
 */
export function absolutePath(base, path) {
  const segments = [];
  const walk = (p) => {
    for (const part of p.split("/")) {
      if (part === "" || part === ".") continue;
      if (part === "..") segments.pop();
      else segments.push(part);
    }
  };
  if (!path.startsWith("/")) walk(base);
  walk(path);
  return `/${segments.join("/")}`;
}

/**
 * Where a path points, over the roots the surface knows — the current one
 * first — and from where the surface stands, when it says (`from`: a
 * shell's current directory, absolute).
 *
 * An absolute path (or `~/…`) is matched to the longest root prefix and
 * named relative to it. A relative path is read from `from` first — under a
 * root and listed, it is that document; under a root and unlisted, it is the
 * guess the node is asked about (`unlisted`); under no root, one that says
 * where it starts (`./`, `../`) is `outside`, a bare one is looked further.
 * Then it is looked up in each root's index exactly, then by its tail (the
 * `src/lib.rs` of every crate, the `main.rs` of every `src/`), at most
 * `MAX_CANDIDATES` — one is a document, several a choice. Nothing listed is
 * `unlisted` with the best guess — from `from`, else the surface's first
 * root — for the node to confirm (`confirmListing`); a climb with no base to
 * climb from, or no root at all, is `unknown`.
 * @param {{path: string, line: number | null, col: number | null}} hit
 * @param {readonly import("./linkModel.d.mts").LinkRoot[]} roots
 * @param {{from?: string | null}} [opts]
 * @returns {import("./linkModel.d.mts").LinkResolution}
 */
export function resolveLink(hit, roots, { from = null } = {}) {
  const { line, col } = hit;
  let path = hit.path.replace(/\/+$/, "");
  const doc = (root, rel, kind = "file") => ({ kind: kind === "dir" ? "dir" : "doc", scope: root.scope, id: root.id, path: rel, line, col, root: root.root, label: root.label, indexed: root.paths.includes(rel) });
  const rootItself = (root) => ({ kind: "dir", scope: root.scope, id: root.id, path: "", line: null, col: null, root: root.root, label: root.label, indexed: true });
  if (path.startsWith("/") || path.startsWith("~/")) {
    const best = rootOf(path, roots);
    if (!best) return { kind: "outside", absolute: path, line, col };
    if (best.rel === "") return rootItself(best.root);
    const kind = holds(best.root, best.rel);
    if (path.startsWith("~/") && kind === null) return { kind: "outside", absolute: path, line, col };
    return doc(best.root, best.rel, kind ?? "file");
  }
  // Where the surface stands is where the tool that printed the path ran.
  let unlisted = null;
  if (from) {
    const absolute = absolutePath(from, path);
    const best = rootOf(absolute, roots);
    if (best) {
      if (best.rel === "") return rootItself(best.root);
      const kind = holds(best.root, best.rel);
      if (kind) return doc(best.root, best.rel, kind);
      unlisted = doc(best.root, best.rel);
    } else if (saysWhereItStarts(path)) {
      return { kind: "outside", absolute, line, col };
    }
  }
  if (path.startsWith("./")) path = path.slice(2);
  if (!path.split("/").includes("..")) {
    const candidates = [];
    for (const root of roots) {
      const kind = holds(root, path);
      if (kind) candidates.push(doc(root, path, kind));
    }
    if (candidates.length === 0) {
      const tail = `/${path}`;
      outer: for (const root of roots) {
        for (const p of root.paths) {
          if (!p.endsWith(tail)) continue;
          candidates.push(doc(root, p));
          if (candidates.length >= MAX_CANDIDATES) break outer;
        }
      }
    }
    if (candidates.length === 1) return candidates[0];
    if (candidates.length > 1) return { kind: "choice", candidates };
    if (!unlisted && roots.length > 0) unlisted = doc(roots[0], path);
  }
  if (unlisted) return { kind: "unlisted", doc: unlisted, raw: hit.path };
  return { kind: "unknown", raw: hit.path };
}

/**
 * The node's answer to an `unlisted` guess: the listing of the guess's own
 * folder names it — a document, or a directory by the entry's word, either
 * way one the index skipped — or it is nothing the desktop can vouch for.
 * @param {{doc: import("./linkModel.d.mts").DocResolution, raw: string}} unlisted
 * @param {readonly {path: string, dir: boolean}[]} entries
 * @returns {import("./linkModel.d.mts").DocResolution | {kind: "unknown", raw: string}}
 */
export function confirmListing(unlisted, entries) {
  const entry = entries.find((e) => e.path === unlisted.doc.path);
  if (!entry) return { kind: "unknown", raw: unlisted.raw };
  return { ...unlisted.doc, kind: entry.dir ? "dir" : "doc", indexed: false };
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
