/**
 * What kind of file a name says it is, for the glyph beside it (ide/03) —
 * code, data, text, an image, a PDF, a recording, a sheet, a document, a
 * deck, an archive, configuration, or just a file. The explorer and the tab
 * strip read the same answer, so a `.json` looks the same in both. Plain
 * `.mjs` so `node --test` pins the table.
 */

const BY_EXT = new Map([
  ...["ts", "tsx", "js", "jsx", "mjs", "cjs", "rs", "py", "go", "java", "kt", "swift", "c", "h", "cpp", "hpp", "cc", "cs", "rb", "php", "sh", "zsh", "bash", "fish", "sql", "lua", "dart", "scala", "ex", "exs", "erl", "hs", "ml", "zig", "vue", "svelte", "css", "scss", "html"].map((e) => [e, "code"]),
  ...["json", "jsonc", "json5", "yaml", "yml", "toml", "xml", "ndjson", "plist"].map((e) => [e, "data"]),
  ...["md", "mdx", "txt", "rst", "adoc", "org", "log"].map((e) => [e, "text"]),
  ...["png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "bmp", "avif", "heic"].map((e) => [e, "image"]),
  ...["pdf"].map((e) => [e, "pdf"]),
  ...["mp4", "webm", "mov", "m4v"].map((e) => [e, "video"]),
  ...["mp3", "wav", "ogg", "m4a", "flac", "aac"].map((e) => [e, "audio"]),
  ...["csv", "tsv", "xlsx", "xls", "ods"].map((e) => [e, "sheet"]),
  ...["docx"].map((e) => [e, "document"]),
  ...["pptx"].map((e) => [e, "slides"]),
  ...["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "jar"].map((e) => [e, "archive"]),
  ...["ini", "cfg", "conf", "env", "lock", "editorconfig", "gitignore", "gitattributes", "npmrc", "nvmrc"].map((e) => [e, "config"]),
]);

const BY_NAME = new Map([
  ["dockerfile", "config"],
  ["makefile", "config"],
  ["justfile", "config"],
  ["license", "text"],
  ["readme", "text"],
]);

/**
 * The kind a file name reads as. A dot-file with no other extension is
 * configuration (`.gitignore`, `.env`); a name with no extension is a file
 * unless it is one everybody knows (`Makefile`, `Dockerfile`).
 * @param {string} name the file's own name, not its path
 * @returns {"code" | "data" | "text" | "image" | "pdf" | "video" | "audio" | "sheet" | "document" | "slides" | "archive" | "config" | "file"}
 */
export function fileIconKind(name) {
  const n = String(name ?? "");
  const lower = n.toLowerCase();
  const dot = lower.lastIndexOf(".");
  const stem = dot === -1 ? lower : lower.slice(0, dot);
  const ext = dot === -1 ? "" : lower.slice(dot + 1);
  if (BY_NAME.has(stem || lower)) return BY_NAME.get(stem || lower);
  if (ext && BY_EXT.has(ext)) return BY_EXT.get(ext);
  if (lower.startsWith(".")) return "config";
  return "file";
}
