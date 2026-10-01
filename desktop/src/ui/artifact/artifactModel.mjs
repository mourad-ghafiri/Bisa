/**
 * Artifacts (ide/12): the kinds and their words, what a card draws inline,
 * the versions a title has, and the key an artifact is addressed by in a URL
 * or a tab. Pure; the renderers read it, `node --test` holds it.
 *
 * `ARTIFACT_KINDS` mirrors `ArtifactKind` in `crates/bisa-core/src/artifact.rs`;
 * the test reads the Rust source and fails when the two drift.
 */

import { t as tr } from "../../i18n/l10n.mjs";

export const ARTIFACT_KINDS = Object.freeze([
  "html",
  "svg",
  "image",
  "video",
  "audio",
  "pdf",
  "sheet",
  "document",
  "slides",
  "markdown",
  "diagram",
  "code",
  "data",
  "text",
  "file",
]);

const WORDS = Object.freeze({
  html: { label: tr("ui-artifact-page"), glyph: "page" },
  svg: { label: tr("ui-artifact-vector-figure"), glyph: "image" },
  image: { label: tr("ui-artifact-image"), glyph: "image" },
  video: { label: tr("ui-artifact-video"), glyph: "video" },
  audio: { label: tr("ui-artifact-audio"), glyph: "audio" },
  pdf: { label: "PDF", glyph: "document" },
  sheet: { label: tr("ui-artifact-spreadsheet"), glyph: "sheet" },
  document: { label: tr("ui-artifact-document"), glyph: "document" },
  slides: { label: tr("ui-artifact-slides"), glyph: "slides" },
  markdown: { label: tr("ui-artifact-markdown"), glyph: "note" },
  diagram: { label: tr("ui-artifact-diagram"), glyph: "workflow" },
  code: { label: tr("ui-artifact-code"), glyph: "code" },
  data: { label: tr("ui-artifact-data"), glyph: "code" },
  text: { label: tr("ui-artifact-text"), glyph: "document" },
  file: { label: tr("ui-artifact-file"), glyph: "file" },
});

/** The kind's word and the `ICON` key it wears. An unknown kind reads as a file. */
export function kindWords(kind) {
  return WORDS[kind] ?? WORDS.file;
}

/** Whether the bytes are text a person can read and copy. */
export function isTextKind(kind) {
  return ["html", "svg", "markdown", "diagram", "code", "data", "text"].includes(kind);
}

/**
 * What the card draws under the message: the picture itself, the page live
 * in its sandbox, or a poster with the kind's word and *Open*.
 */
export function inlinePreview(kind) {
  if (kind === "image" || kind === "svg") return "image";
  if (kind === "html") return "live";
  return "poster";
}

/** At most this many pages run live inline at once; the rest are posters until opened. */
export const MAX_LIVE_FRAMES = 3;

/** `1.2 MB`, `840 kB`, `12 B`. */
export function bytesWords(n) {
  if (!Number.isFinite(n) || n < 0) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} kB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * The address of one artifact: its message and its place on it. What the
 * aux pane's `?auxId=` and a workbench tab carry.
 */
export function artifactKey(messageId, ordinal) {
  return `${messageId}:${ordinal}`;
}

/** The inverse, or null. A message id is hex, so the last colon is the split. */
export function parseArtifactKey(key) {
  if (typeof key !== "string") return null;
  const at = key.lastIndexOf(":");
  if (at <= 0) return null;
  const digits = key.slice(at + 1);
  // `abc:` is not an address: an ordinal is digits, never the number an
  // empty string coerces to.
  if (!/^\d+$/.test(digits)) return null;
  const ordinal = Number(digits);
  const message = key.slice(0, at);
  if (!message || !Number.isInteger(ordinal) || ordinal < 0) return null;
  return { message, ordinal };
}

/**
 * A conversation's artifacts grouped by title, newest first — each group's
 * `latest` is the newest, `versions` every one newest first. Rows arrive
 * newest first from the node; the order of first appearance is kept.
 */
export function artifactVersions(rows) {
  const groups = new Map();
  for (const row of rows) {
    const title = row.title;
    if (!groups.has(title)) groups.set(title, { title, latest: row, versions: [] });
    groups.get(title).versions.push(row);
  }
  return [...groups.values()];
}

/** *3 versions*, *1 version*. */
export function versionWords(count) {
  return count === 1 ? "1 version" : tr("ui-artifact-versions", { count });
}

/** The words for a version in a list: *v3 · 2 h ago by Scout* is the caller's; this is *v3*. */
export function versionLabel(index, count) {
  return `v${count - index}`;
}

/**
 * The name `CodeEditor` infers a language from: the artifact's own name, or
 * one that says what the kind is when the name has no extension.
 */
export function languagePath(artifact) {
  const name = artifact.name || "";
  if (/\.[A-Za-z0-9]+$/.test(name)) return name;
  switch (artifact.kind) {
    case "html":
      return `${name || "page"}.html`;
    case "svg":
      return `${name || "figure"}.svg`;
    case "data":
      return `${name || "data"}.json`;
    case "markdown":
      return `${name || "notes"}.md`;
    default:
      return name || "text.txt";
  }
}

/** Above this a text artifact is shown plainly, without tokens. */
export const PLAIN_TEXT_BYTES = 1024 * 1024;

const EXTENSION_KINDS = Object.freeze({
  html: "html",
  htm: "html",
  svg: "svg",
  png: "image",
  jpg: "image",
  jpeg: "image",
  gif: "image",
  webp: "image",
  avif: "image",
  bmp: "image",
  heic: "image",
  ico: "image",
  mp4: "video",
  webm: "video",
  mov: "video",
  m4v: "video",
  mp3: "audio",
  wav: "audio",
  ogg: "audio",
  m4a: "audio",
  flac: "audio",
  aac: "audio",
  pdf: "pdf",
  csv: "sheet",
  tsv: "sheet",
  xlsx: "sheet",
  xls: "sheet",
  ods: "sheet",
  docx: "document",
  pptx: "slides",
  md: "markdown",
  mdx: "markdown",
  markdown: "markdown",
  mmd: "diagram",
  mermaid: "diagram",
  json: "data",
  jsonl: "data",
  yaml: "data",
  yml: "data",
  toml: "data",
  xml: "data",
  txt: "text",
  log: "text",
});

const CODE_EXTENSIONS = new Set(
  "rs ts tsx js jsx mjs cjs mts py rb go java kt swift c h cpp hpp cc cs php sh bash zsh fish sql css scss less lua r scala ex exs erl hs ml clj dart vue svelte astro graphql proto dockerfile makefile cmake nix tf ini cfg conf env diff patch".split(
    " ",
  ),
);

/**
 * The kind a name and a declared type make — the desktop's mirror of
 * `ArtifactKind::of`, for a file a person shares from the composer. The
 * extension decides first; the mime family answers for a bare name; `file`
 * is the honest fallback.
 */
export function kindOf(name, mime) {
  const base = String(name ?? "").split("/").pop() ?? "";
  const dot = base.lastIndexOf(".");
  const ext = dot > 0 && dot < base.length - 1 ? base.slice(dot + 1).toLowerCase() : "";
  if (ext) {
    if (EXTENSION_KINDS[ext]) return EXTENSION_KINDS[ext];
    if (CODE_EXTENSIONS.has(ext)) return "code";
    return "file";
  }
  const m = String(mime ?? "").split(";")[0].trim().toLowerCase();
  if (m === "text/html") return "html";
  if (m === "image/svg+xml") return "svg";
  if (m === "application/pdf") return "pdf";
  if (m === "text/csv" || m === "text/tab-separated-values") return "sheet";
  if (m === "text/markdown") return "markdown";
  if (m === "application/json" || m === "application/xml" || m === "text/xml") return "data";
  if (m.startsWith("image/")) return "image";
  if (m.startsWith("video/")) return "video";
  if (m.startsWith("audio/")) return "audio";
  if (m.startsWith("text/")) return "text";
  return "file";
}

const EXTENSION_MIMES = Object.freeze({
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  avif: "image/avif",
  bmp: "image/bmp",
  heic: "image/heic",
  ico: "image/x-icon",
  svg: "image/svg+xml",
  mp4: "video/mp4",
  m4v: "video/mp4",
  webm: "video/webm",
  mov: "video/quicktime",
  mp3: "audio/mpeg",
  wav: "audio/wav",
  ogg: "audio/ogg",
  m4a: "audio/mp4",
  flac: "audio/flac",
  aac: "audio/aac",
  pdf: "application/pdf",
  csv: "text/csv",
  tsv: "text/tab-separated-values",
  xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  xls: "application/vnd.ms-excel",
  ods: "application/vnd.oasis.opendocument.spreadsheet",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
  json: "application/json",
  jsonl: "application/json",
  yaml: "application/yaml",
  yml: "application/yaml",
  toml: "application/toml",
  xml: "application/xml",
  md: "text/markdown",
  mdx: "text/markdown",
  markdown: "text/markdown",
  mmd: "text/vnd.mermaid",
  mermaid: "text/vnd.mermaid",
  html: "text/html",
  htm: "text/html",
  css: "text/css",
  js: "text/javascript",
  mjs: "text/javascript",
  cjs: "text/javascript",
  txt: "text/plain",
  log: "text/plain",
});

/**
 * A media type from a file name — the desktop's mirror of the Rust
 * `mime_of_name`, the one table for a file a person opens or shares. A code
 * file is plain text; anything else is bytes with a name.
 */
export function mimeOfName(name) {
  const base = String(name ?? "").split("/").pop() ?? "";
  const dot = base.lastIndexOf(".");
  const ext = dot > 0 && dot < base.length - 1 ? base.slice(dot + 1).toLowerCase() : "";
  if (!ext) return "application/octet-stream";
  if (EXTENSION_MIMES[ext]) return EXTENSION_MIMES[ext];
  if (CODE_EXTENSIONS.has(ext)) return "text/plain";
  return "application/octet-stream";
}

/** `dashboard.html` is titled *dashboard*; a dotfile keeps its whole name. */
export function defaultTitle(name) {
  const base = String(name ?? "");
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(0, dot) : base;
}

/** A person's uploaded file as an artifact: the kind from its name, the title its stem. */
export function artifactFromFile(file, title) {
  const t = (title ?? "").trim() || defaultTitle(file.name);
  return { ...file, title: t, kind: kindOf(file.name, file.mime) };
}
