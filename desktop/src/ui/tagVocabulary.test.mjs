/**
 * `TAG_VOCABULARY` in `Tags.tsx` mirrors `VOCABULARY` in
 * `crates/bisa-core/src/tags.rs`, and until M12 nothing held the two
 * together. When `legal` was added to the Rust the mirror kept offering the
 * old twenty, and nothing failed: tagging still worked, because free-form
 * tags are accepted. It simply stopped offering the word the catalog files a
 * whole agent under.
 *
 * That is why this test exists rather than a comment asking the next person
 * to remember. The failure mode is an **absence** — a suggestion nobody sees
 * is a suggestion nobody files a bug about, exactly like the theme role that
 * `roles.test.mjs` guards.
 *
 * It reads the Rust rather than a second copy of the list. A test holding its
 * own third copy would be one more thing to keep in step.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const TAGS_RS = resolve(here, "../../../crates/bisa-core/src/tags.rs");
const TAGS_TSX = resolve(here, "Tags.tsx");

/** The `&[&str]` literal, read as the list it is. */
function vocabularyFromRust() {
  const src = readFileSync(TAGS_RS, "utf8");
  const block = src.match(/pub const VOCABULARY: &\[&str\] = &\[([\s\S]*?)\];/);
  assert.ok(block, "VOCABULARY is not where this test expects it in tags.rs");
  return [...block[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]);
}

function vocabularyFromDesktop() {
  const src = readFileSync(TAGS_TSX, "utf8");
  const block = src.match(/export const TAG_VOCABULARY = \[([\s\S]*?)\] as const;/);
  assert.ok(block, "TAG_VOCABULARY is not where this test expects it in Tags.tsx");
  return [...block[1].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]);
}

test("the desktop offers exactly the vocabulary the library draws from", () => {
  const rust = vocabularyFromRust();
  assert.ok(rust.length > 0, "read no words out of tags.rs");
  assert.deepEqual(
    vocabularyFromDesktop(),
    rust,
    "TAG_VOCABULARY has drifted from VOCABULARY in crates/bisa-core/src/tags.rs",
  );
});

test("the vocabulary is sorted, so the picker's order is not an accident", () => {
  const words = vocabularyFromDesktop();
  assert.deepEqual(words, [...words].sort(), "the vocabulary is not in sorted order");
});
