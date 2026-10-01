/**
 * Paths and links in text. Run with `node --test desktop/src/ui/linkModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { addressWords, findLinks, linkifyHtml, parseAddress, relativeUnder, resolveLink, urlWords } from "./linkModel.mjs";

const paths = (text) => findLinks(text).filter((h) => h.kind === "path").map((h) => h.raw);
const urls = (text) => findLinks(text).filter((h) => h.kind === "url").map((h) => h.url);

test("a path is found with every prefix, its line and column read, the sentence's punctuation left behind", () => {
  assert.deepEqual(paths("see src/main.rs:42, then ./docs/x.md and ../lib/a.ts."), ["src/main.rs:42", "./docs/x.md", "../lib/a.ts"]);
  assert.deepEqual(paths("open ~/Projects/app/README.md and /etc/hosts (both)"), ["~/Projects/app/README.md", "/etc/hosts"]);
  assert.deepEqual(paths("`src/a.rs:12:7` failed; docs/x.md#L12 too"), ["src/a.rs:12:7", "docs/x.md#L12"]);
  const [hit] = findLinks("at src/a.rs:12:7!");
  assert.deepEqual([hit.path, hit.line, hit.col, hit.start, hit.end], ["src/a.rs", 12, 7, 3, 16]);
  assert.deepEqual(parseAddress("docs/x.md#L12-L15"), { path: "docs/x.md", line: 12, col: null });
  assert.deepEqual(parseAddress("Cargo.toml"), { path: "Cargo.toml", line: null, col: null });
});

test("a bare word is a path only with an extension a source tree carries, and a number is never one", () => {
  assert.deepEqual(paths("edit Cargo.toml and main.rs, e.g. v1.2.3 or 10/12"), ["Cargo.toml", "main.rs"]);
  assert.deepEqual(paths("README says nothing; src alone is a word"), []);
  assert.deepEqual(paths("the folder src/ and the root /"), ["src/"]);
  assert.deepEqual(paths("a URL http://x.y/a/b.rs is not a path"), []);
  assert.deepEqual(paths("see hyphen-ated/path and crates/bisa-core"), ["hyphen-ated/path", "crates/bisa-core"], "a hyphen is a path character, as every crate folder shows");
  assert.deepEqual(paths("id$src/main.rs and x%docs/y.md"), [], "a boundary is needed before a path — a letter or a symbol glued to it is not one");
  assert.deepEqual(paths("(src/main.rs) [docs/y.md] <a.rs> {b.rs}"), ["src/main.rs", "docs/y.md", "a.rs", "b.rs"], "a bracket of any kind is a boundary");
});

test("a URL is a URL, with what the sentence appended dropped and a bare www given its scheme", () => {
  assert.deepEqual(urls("see https://example.com/docs, and www.x.org."), ["https://example.com/docs", "https://www.x.org"]);
  assert.deepEqual(urls("(https://en.wikipedia.org/wiki/Rust_(programming_language))"), ["https://en.wikipedia.org/wiki/Rust_(programming_language)"]);
  assert.deepEqual(urls("in http://localhost:4477/goals)"), ["http://localhost:4477/goals"]);
  const [u] = findLinks("go to https://a.b/c.");
  assert.equal(u.raw, "https://a.b/c");
});

test("a redacted secret is never a link, whatever it holds", () => {
  const text = "token «secret:github_token:7f3a2c» beside src/a.rs and «secret:env:MY_KEY:abcdef» https://x.y";
  assert.deepEqual(findLinks(text).map((h) => h.raw), ["src/a.rs", "https://x.y"]);
  assert.deepEqual(findLinks("«secret:k:abcdef»/etc/passwd"), []);
});

test("rendered HTML gains anchors in text and inline code, never in a block, an anchor or a secret", () => {
  const html = linkifyHtml('<p>see <code>src/a.rs:12</code> and docs/x.md</p><pre><code>src/b.rs:1</code></pre><p><a href="https://x.y">x</a> and <a href="./rel.md">rel</a></p>');
  assert.match(html, /<code><a data-link="path" data-path="src\/a.rs" data-line="12" class="link-path">src\/a.rs:12<\/a><\/code>/);
  assert.match(html, /and <a data-link="path" data-path="docs\/x.md" class="link-path">docs\/x.md<\/a>/);
  assert.match(html, /<pre><code>src\/b.rs:1<\/code><\/pre>/, "a fenced block is left alone");
  assert.match(html, /<a data-link="url" href="https:\/\/x.y">x<\/a>/, "micromark's anchor is tagged, not nested");
  assert.match(html, /<a data-link="doc" href=".\/rel.md">rel<\/a>/);
  assert.equal(linkifyHtml("<p>«secret:k:abcdef» and nothing</p>"), "<p>«secret:k:abcdef» and nothing</p>");
  assert.equal(linkifyHtml('<a href="#">x</a>'), '<a href="#">x</a>', "a neutered anchor stays neutered");
  assert.match(linkifyHtml("<p>a &quot;src/q.rs&quot; here</p>"), /a &quot;<a data-link="path" data-path="src\/q.rs" class="link-path">src\/q.rs<\/a>&quot; here/, "entities survive around a link");
});

const roots = [
  { scope: "workstream", id: "app", root: "/Users/me/Projects/app", label: "app", paths: ["src/main.rs", "src/lib.rs", "docs/x.md", "README.md"] },
  { scope: "workstream", id: "blog", root: "/Users/me/Projects/blog", label: "blog", paths: ["README.md", "posts/a.md"] },
];

test("an absolute or a home path is named relative to the longest root it is under", () => {
  const r = resolveLink({ path: "/Users/me/Projects/app/src/main.rs", line: 4, col: null }, roots);
  assert.deepEqual([r.kind, r.id, r.path, r.line, r.indexed], ["doc", "app", "src/main.rs", 4, true]);
  const home = resolveLink({ path: "~/Projects/blog/posts/a.md", line: null, col: null }, roots);
  assert.deepEqual([home.kind, home.id, home.path], ["doc", "blog", "posts/a.md"]);
  const ignored = resolveLink({ path: "/Users/me/Projects/app/target/out.log", line: null, col: null }, roots);
  assert.deepEqual([ignored.kind, ignored.indexed], ["doc", false], "under a root but not indexed still opens");
  assert.deepEqual(resolveLink({ path: "/Users/me/Projects/app/src", line: null, col: null }, roots).kind, "dir");
  assert.deepEqual(resolveLink({ path: "/etc/hosts", line: null, col: null }, roots), { kind: "outside", absolute: "/etc/hosts", line: null, col: null });
  assert.equal(resolveLink({ path: "~/elsewhere/x.md", line: null, col: null }, roots).kind, "outside");
  assert.equal(relativeUnder("/a/b", "/a/b/"), "");
  assert.equal(relativeUnder("/a/bc", "/a/b"), null, "a prefix is a whole segment");
});

test("a relative path is looked up in each root, exact first, then by its name; two roots are a choice", () => {
  assert.equal(resolveLink({ path: "./src/main.rs", line: null, col: null }, roots).id, "app");
  assert.equal(resolveLink({ path: "posts/a.md", line: null, col: null }, roots).id, "blog");
  const both = resolveLink({ path: "README.md", line: null, col: null }, roots);
  assert.equal(both.kind, "choice");
  assert.deepEqual(both.candidates.map((c) => c.id), ["app", "blog"]);
  assert.equal(resolveLink({ path: "main.rs", line: 3, col: null }, roots).path, "src/main.rs", "a bare name finds its file");
  assert.equal(resolveLink({ path: "src", line: null, col: null }, roots).kind, "dir");
  assert.deepEqual(resolveLink({ path: "nope/none.rs", line: null, col: null }, roots), { kind: "unknown", raw: "nope/none.rs" });
  assert.equal(resolveLink({ path: "../up.rs", line: null, col: null }, roots).kind, "unknown", "no base to climb from");
});

test("the words: an address with its line, a URL with its host", () => {
  assert.equal(addressWords("src/a.rs", 12, 7), "src/a.rs:12:7");
  assert.equal(addressWords("src/a.rs", null, null), "src/a.rs");
  assert.deepEqual(urlWords("https://example.com/x?y=1"), { host: "example.com", url: "https://example.com/x?y=1", scheme: "https" });
  assert.equal(urlWords("not a url").scheme, null);
});

test("a long message is scanned well inside a frame", () => {
  const text = Array.from({ length: 2000 }, (_, i) => `line ${i}: changed src/mod_${i}/file.rs:${i} see https://example.com/${i} and «secret:k:abcdef»`).join("\n");
  const start = performance.now();
  const hits = findLinks(text);
  const html = linkifyHtml(`<p>${text.replace(/</g, "&lt;")}</p>`);
  const ms = performance.now() - start;
  assert.equal(hits.length, 4000);
  assert.ok(html.length > text.length);
  assert.ok(ms < 250, `${ms.toFixed(1)} ms`);
});

test("an address is part of its path's token: the range covers it, the line and column are read, a directory keeps its slash", () => {
  const [hit] = findLinks("see src/a.rs:12:3 now");
  assert.deepEqual([hit.raw, hit.path, hit.line, hit.col], ["src/a.rs:12:3", "src/a.rs", 12, 3]);
  assert.equal("see src/a.rs:12:3 now".slice(hit.start, hit.end), "src/a.rs:12:3", "the underline covers the address");
  const [anchor] = findLinks("in docs/x.md#L12-L20.");
  assert.deepEqual([anchor.raw, anchor.line], ["docs/x.md#L12-L20", 12]);
  const [dir] = findLinks("look in src/ first");
  assert.deepEqual([dir.raw, dir.path], ["src/", "src/"]);
});
