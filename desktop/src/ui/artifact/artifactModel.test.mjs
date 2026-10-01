/**
 * Artifacts' words and addresses. Run with `node --test desktop/src/ui/artifact/artifactModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  ARTIFACT_KINDS,
  artifactFromFile,
  artifactKey,
  artifactVersions,
  bytesWords,
  defaultTitle,
  inlinePreview,
  isTextKind,
  kindOf,
  kindWords,
  languagePath,
  mimeOfName,
  parseArtifactKey,
  versionLabel,
  versionWords,
} from "./artifactModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("the kinds mirror the Rust enum, in its order", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-core/src/artifact.rs"), "utf8");
  const body = rust.match(/pub enum ArtifactKind \{([\s\S]*?)\n\}/);
  assert.ok(body, "artifact.rs declares ArtifactKind");
  const variants = [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*),/gm)].map((m) =>
    m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase(),
  );
  assert.deepEqual([...ARTIFACT_KINDS], variants);
  for (const k of ARTIFACT_KINDS) {
    assert.ok(kindWords(k).label, `${k} has a word`);
    assert.ok(kindWords(k).glyph, `${k} has a glyph`);
  }
  assert.equal(kindWords("deck").label, "File", "an unknown kind reads as a file");
});

test("what a card draws inline, and which kinds are text", () => {
  assert.equal(inlinePreview("image"), "image");
  assert.equal(inlinePreview("svg"), "image");
  assert.equal(inlinePreview("html"), "live");
  for (const k of ["pdf", "sheet", "document", "slides", "video", "audio", "file", "code"]) {
    assert.equal(inlinePreview(k), "poster", k);
  }
  assert.ok(isTextKind("markdown") && isTextKind("html") && isTextKind("code"));
  assert.ok(!isTextKind("pdf") && !isTextKind("image") && !isTextKind("file"));
});

test("bytes read as a person says them", () => {
  assert.equal(bytesWords(12), "12 B");
  assert.equal(bytesWords(2048), "2.0 kB");
  assert.equal(bytesWords(840 * 1024), "840 kB");
  assert.equal(bytesWords(1.25 * 1024 * 1024), "1.3 MB");
  assert.equal(bytesWords(-1), "");
});

test("an artifact's address round-trips and a bad one is null", () => {
  assert.equal(artifactKey("abc123", 2), "abc123:2");
  assert.deepEqual(parseArtifactKey("abc123:2"), { message: "abc123", ordinal: 2 });
  assert.deepEqual(parseArtifactKey("a:b:0"), { message: "a:b", ordinal: 0 });
  for (const bad of ["", "abc", ":1", "abc:", "abc:-1", "abc:x", null, undefined]) {
    assert.equal(parseArtifactKey(bad), null, String(bad));
  }
});

test("versions group by title, newest first, in order of first appearance", () => {
  const rows = [
    { title: "Report", sha256: "c", created_at: 3 },
    { title: "Chart", sha256: "b", created_at: 2 },
    { title: "Report", sha256: "a", created_at: 1 },
  ];
  const groups = artifactVersions(rows);
  assert.deepEqual(
    groups.map((g) => [g.title, g.latest.sha256, g.versions.map((v) => v.sha256)]),
    [
      ["Report", "c", ["c", "a"]],
      ["Chart", "b", ["b"]],
    ],
  );
  assert.equal(versionWords(1), "1 version");
  assert.equal(versionWords(2), "2 versions");
  assert.equal(versionLabel(0, 3), "v3");
  assert.equal(versionLabel(2, 3), "v1");
  assert.deepEqual(artifactVersions([]), []);
});

test("a shared file's kind mirrors the Rust table: the extension first, the mime for a bare name, a file otherwise", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-core/src/artifact.rs"), "utf8");
  // Every extension the Rust table names decides the same kind here.
  const table = rust.match(/fn kind_of_extension\(ext: String\)[\s\S]*?\n\}/)[0];
  for (const m of table.matchAll(/^\s+((?:"[a-z0-9]+"\s*\|\s*)*"[a-z0-9]+")\s*=>\s*ArtifactKind::([A-Z][a-z]+),/gm)) {
    const kind = m[2].toLowerCase();
    for (const ext of m[1].match(/"([a-z0-9]+)"/g).map((s) => s.slice(1, -1))) {
      assert.equal(kindOf(`x.${ext}`, ""), kind, ext);
    }
  }
  const code = rust.match(/const CODE_EXTENSIONS: &\[&str\] = &\[([\s\S]*?)\];/)[1].match(/"([a-z0-9]+)"/g).map((s) => s.slice(1, -1));
  for (const ext of code) assert.equal(kindOf(`x.${ext}`, ""), "code", ext);
  assert.equal(kindOf("README", "text/markdown"), "markdown");
  assert.equal(kindOf("blob", "image/webp; q=1"), "image");
  assert.equal(kindOf("thing.xyz", "application/octet-stream"), "file");
  assert.equal(kindOf(".env", ""), "file");
  assert.equal(defaultTitle("dashboard.html"), "dashboard");
  assert.equal(defaultTitle(".profile"), ".profile");
  const a = artifactFromFile({ sha256: "a".repeat(64), name: "q3.csv", mime: "text/csv", size: 9 }, "  ");
  assert.equal(a.title, "q3");
  assert.equal(a.kind, "sheet");
  assert.equal(artifactFromFile({ sha256: "a".repeat(64), name: "q3.csv", mime: "text/csv", size: 9 }, "Q3 numbers").title, "Q3 numbers");
});

test("a name's media type mirrors the Rust table, a code file is plain text, and the rest is bytes with a name", () => {
  const rust = readFileSync(join(here, "../../../../crates/bisa-core/src/artifact.rs"), "utf8");
  const table = rust.match(/pub fn mime_of_name\(name: &str\)[\s\S]*?\n\}/)[0];
  let seen = 0;
  for (const m of table.matchAll(/^\s+((?:"[a-z0-9]+"\s*\|\s*)*"[a-z0-9]+")\s*=>\s*"([^"]+)",/gm)) {
    for (const ext of m[1].match(/"([a-z0-9]+)"/g).map((s) => s.slice(1, -1))) {
      assert.equal(mimeOfName(`x.${ext}`), m[2], ext);
      seen += 1;
    }
  }
  assert.ok(seen > 30, `the table was read (${seen})`);
  assert.equal(mimeOfName("main.rs"), "text/plain");
  assert.equal(mimeOfName("thing.xyz"), "application/octet-stream");
  assert.equal(mimeOfName("README"), "application/octet-stream");
});

test("the language a text artifact is read in comes from its name, else its kind", () => {
  assert.equal(languagePath({ name: "main.rs", kind: "code" }), "main.rs");
  assert.equal(languagePath({ name: "page", kind: "html" }), "page.html");
  assert.equal(languagePath({ name: "", kind: "data" }), "data.json");
  assert.equal(languagePath({ name: "README", kind: "markdown" }), "README.md");
  assert.equal(languagePath({ name: "", kind: "text" }), "text.txt");
});

/** The core's table, read from `artifact.rs`: every extension it knows with its kind, and its code extensions. */
function rustKinds() {
  const rust = readFileSync(new URL("../../../../crates/bisa-core/src/artifact.rs", import.meta.url), "utf8");
  const body = rust.slice(rust.indexOf("fn kind_of_extension"), rust.indexOf("pub fn mime_of_name"));
  const snake = (variant) => variant.replace(/(?<!^)(?=[A-Z])/g, "_").toLowerCase();
  const table = new Map();
  for (const arm of body.matchAll(/((?:"[a-z0-9]+"\s*\|?\s*)+)=>\s*\{?\s*ArtifactKind::(\w+)/g)) {
    for (const ext of arm[1].matchAll(/"([a-z0-9]+)"/g)) table.set(ext[1], snake(arm[2]));
  }
  const codeBlock = rust.slice(rust.indexOf("const CODE_EXTENSIONS"), rust.indexOf("];", rust.indexOf("const CODE_EXTENSIONS")));
  const code = [...codeBlock.matchAll(/"([a-z0-9]+)"/g)].map((c) => c[1]);
  const mimeBlock = rust.slice(rust.indexOf("match mime.as_str()"), rust.indexOf("impl std::fmt::Display for ArtifactKind"));
  const mimes = new Map();
  for (const arm of mimeBlock.matchAll(/((?:"[a-z+/.-]+"\s*\|?\s*)+)=>\s*ArtifactKind::(\w+)/g)) {
    for (const mime of arm[1].matchAll(/"([a-z+/.-]+)"/g)) mimes.set(mime[1], snake(arm[2]));
  }
  assert.ok(table.size > 30 && code.length > 20 && mimes.size >= 8, `the core's tables are read: ${table.size}, ${code.length}, ${mimes.size}`);
  return { table, code, mimes };
}

test("the desktop names a file's kind exactly as the core does: every extension, every code extension, every media type", () => {
  const { table, code, mimes } = rustKinds();
  for (const [ext, kind] of table) assert.equal(kindOf(`made.${ext}`, "application/octet-stream"), kind, `.${ext}`);
  for (const ext of code) assert.equal(kindOf(`src/main.${ext}`, ""), "code", `.${ext}`);
  for (const [mime, kind] of mimes) assert.equal(kindOf("bare-name", mime), kind, mime);
  for (const [family, kind] of [["image/x-new", "image"], ["video/x-new", "video"], ["audio/x-new", "audio"], ["text/x-new", "text"], ["application/x-new", "file"], ["", "file"]]) {
    assert.equal(kindOf("bare-name", family), kind, family || "no type at all");
  }
  // The desktop knows no extension the core does not: a kind drawn here and named `file` on the wire would be two answers.
  const source = readFileSync(new URL("./artifactModel.mjs", import.meta.url), "utf8");
  const mine = source.slice(source.indexOf("const EXTENSION_KINDS"), source.indexOf("});", source.indexOf("const EXTENSION_KINDS")));
  for (const m of mine.matchAll(/^\s+([a-z0-9]+): "([a-z]+)",$/gm)) assert.equal(table.get(m[1]), m[2], `.${m[1]} is the desktop's alone, or another kind there`);
  const myCode = source.slice(source.indexOf("const CODE_EXTENSIONS"), source.indexOf(".split(", source.indexOf("const CODE_EXTENSIONS")));
  const mineCode = (myCode.match(/"([^"]+)"/) ?? ["", ""])[1].split(" ").filter(Boolean);
  assert.deepEqual([...mineCode].sort(), [...code].sort(), "the code extensions are one list on both sides");
});

test("a name with an extension is decided by it alone, a bare name by its type — a dotfile and a trailing dot are bare", () => {
  assert.equal(kindOf("report.pdf", "text/plain"), "pdf");
  assert.equal(kindOf("dir.v2/Chart.SVG", "image/png"), "svg");
  for (const mime of ["image/png", "text/html", "text/plain", ""]) assert.equal(kindOf("photo.unknownext", mime), "file", mime);
  for (const bare of ["README", ".gitignore", "notes.", "a/b.c/plain"]) {
    assert.equal(kindOf(bare, "text/markdown"), "markdown", bare);
    assert.equal(kindOf(bare, "Image/PNG; charset=binary"), "image", bare);
    assert.equal(kindOf(bare, ""), "file", bare);
  }
  for (const nothing of [null, undefined, "", 7]) assert.equal(kindOf(nothing, null), "file");
});

