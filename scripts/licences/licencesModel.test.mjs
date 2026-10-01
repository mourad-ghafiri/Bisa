/**
 * The licence rules: an expression's branch under the one allow list, what
 * counts as copyleft, and the notices' rendering. Run with
 * `node --test scripts/licences/licencesModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { chooseBranch, copyrightLines, groupByLicence, idsOf, isCopyleftOnly, needsSourcePointer, parseAllow, renderPackage } from "./licencesModel.mjs";

const ALLOW = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "CC0-1.0", "MPL-2.0", "BSL-1.0", "0BSD", "Unlicense", "MIT-0", "CDLA-Permissive-2.0"];

test("an OR takes its first allowed alternative; an AND needs every term; a bare id must be allowed", () => {
  assert.equal(chooseBranch("MIT OR GPL-3.0-or-later", ALLOW), "MIT");
  assert.equal(chooseBranch("(MIT OR GPL-3.0-or-later)", ALLOW), "MIT");
  assert.equal(chooseBranch("(MPL-2.0 OR Apache-2.0)", ALLOW), "MPL-2.0", "the first allowed alternative, in the author's order");
  assert.equal(chooseBranch("Apache-2.0 OR GPL-2.0-only", ALLOW), "Apache-2.0");
  assert.equal(chooseBranch("MIT OR Apache-2.0 OR LGPL-2.1-or-later", ALLOW), "MIT");
  assert.equal(chooseBranch("GPL-3.0-only", ALLOW), null);
  assert.equal(chooseBranch("ISC AND (Apache-2.0 OR ISC)", ALLOW), "ISC AND Apache-2.0");
  assert.equal(chooseBranch("(MIT OR Apache-2.0) AND Unicode-3.0", ALLOW), "MIT AND Unicode-3.0");
  assert.equal(chooseBranch("MIT AND GPL-3.0-only", ALLOW), null, "an AND with a term off the list has no branch");
  assert.equal(chooseBranch("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT", ALLOW), "Apache-2.0 WITH LLVM-exception");
  assert.equal(chooseBranch("MIT/Apache-2.0", ALLOW), "MIT", "the legacy slash is an OR");
  assert.equal(chooseBranch("Apache-2.0 / MIT", ALLOW), "Apache-2.0");
  assert.equal(chooseBranch("Zlib OR Apache-2.0 OR MIT", ALLOW), "Zlib");
  assert.equal(chooseBranch("CDLA-Permissive-2.0", ALLOW), "CDLA-Permissive-2.0");
  assert.equal(chooseBranch("", ALLOW), null);
  assert.equal(chooseBranch("(MIT", ALLOW), null, "a broken expression is no branch, never a throw");
  assert.deepEqual(idsOf("ISC AND (Apache-2.0 OR ISC OR MIT-0)"), ["ISC", "Apache-2.0", "ISC", "MIT-0"]);
});

test("copyleft with no permissive alternative is named; copyleft as an alternative is not", () => {
  assert.equal(isCopyleftOnly("GPL-3.0-only", ALLOW), true);
  assert.equal(isCopyleftOnly("LGPL-2.1-or-later", ALLOW), true);
  assert.equal(isCopyleftOnly("AGPL-3.0-or-later", ALLOW), true);
  assert.equal(isCopyleftOnly("CC-BY-SA-4.0", ALLOW), true, "share-alike is copyleft for a work that ships it");
  assert.equal(isCopyleftOnly("MIT OR GPL-3.0-or-later", ALLOW), false);
  assert.equal(isCopyleftOnly("CC-BY-4.0", ALLOW), false, "attribution alone is not copyleft — it is off the list, which the gate reports on its own");
  assert.equal(isCopyleftOnly("WTFPL", ALLOW), false);
  assert.equal(needsSourcePointer("MPL-2.0"), true);
  assert.equal(needsSourcePointer("MIT AND MPL-2.0"), true);
  assert.equal(needsSourcePointer("MIT"), false);
  assert.equal(needsSourcePointer(null), false);
});

test("the one allow list is deny.toml's, read as written, and the two workspaces carry the same", () => {
  const root = parseAllow(readFileSync(new URL("../../deny.toml", import.meta.url), "utf8"));
  const shell = parseAllow(readFileSync(new URL("../../desktop/src-tauri/deny.toml", import.meta.url), "utf8"));
  assert.deepEqual(shell, root, "the desktop shell is gated by the same list");
  for (const id of ["MIT", "Apache-2.0", "MPL-2.0", "Unlicense", "MIT-0", "CDLA-Permissive-2.0", "Unicode-3.0", "BSL-1.0"]) assert.ok(root.includes(id), `${id} is on the list`);
  for (const id of ["OpenSSL", "Artistic-2.0", "Unicode-DFS-2016"]) assert.ok(!root.includes(id), `${id} is on no graph and off the list`);
  assert.ok(!root.some((id) => /GPL|CC-BY/.test(id)), "no copyleft and no attribution licence is allowed outright");
  assert.deepEqual(parseAllow("nothing here"), []);
});

test("the copyright lines are the notices' own words, deduplicated and bounded; the rendering is one line per fact", () => {
  const text = "MIT License\n\nCopyright (c) 2016 Someone\n// Copyright (c) 2016 Someone\n * (c) 2020 Another\nPermission is hereby granted";
  assert.deepEqual(copyrightLines(text), ["Copyright (c) 2016 Someone", "(c) 2020 Another"]);
  assert.deepEqual(copyrightLines(null), []);
  assert.equal(copyrightLines(Array.from({ length: 20 }, (_, i) => `Copyright ${i}`).join("\n")).length, 6);
  assert.equal(
    renderPackage({ name: "jszip", version: "3.10.1", declared: "(MIT OR GPL-3.0-or-later)", branch: "MIT", copyrights: ["Copyright (c) 2009-2016 Stuart Knightley"] }),
    "- **jszip** 3.10.1 — (MIT OR GPL-3.0-or-later) — taken as MIT\n  Copyright (c) 2009-2016 Stuart Knightley",
  );
  assert.equal(
    renderPackage({ name: "option-ext", version: "0.2.0", declared: "MPL-2.0", branch: "MPL-2.0", copyrights: [], source: "https://crates.io/crates/option-ext/0.2.0" }),
    "- **option-ext** 0.2.0 — MPL-2.0\n  source: https://crates.io/crates/option-ext/0.2.0",
  );
  const groups = groupByLicence([
    { name: "b", version: "1", branch: "MIT" },
    { name: "a", version: "2", branch: "MIT" },
    { name: "c", version: "1", branch: null },
    { name: "d", version: "1", branch: "Apache-2.0" },
  ]);
  assert.deepEqual([...groups.keys()], ["Apache-2.0", "MIT", null], "by licence, the unlicensed last");
  assert.deepEqual(groups.get("MIT").map((p) => p.name), ["a", "b"]);
});
