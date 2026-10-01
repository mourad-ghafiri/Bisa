/**
 * About Bisa, as words. Run with `node --test desktop/src/shell/aboutModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { PRODUCT, aboutFiles, aboutLinks, aboutRows, marksWords, versionCaution, versionLine } from "./aboutModel.mjs";

test("the version line names the platform, this desktop and the node — or that the node is unreachable", () => {
  assert.equal(PRODUCT, "Bisa");
  assert.equal(versionLine("0.1.0", "0.1.0"), "Bisa 0.1.0 · node 0.1.0");
  assert.equal(versionLine("0.1.0", null), "Bisa 0.1.0 · node unreachable");
  assert.equal(versionLine("0.1.0", ""), "Bisa 0.1.0 · node unreachable");
});

test("a mismatch is a caution; agreement or an unknown node is none", () => {
  assert.equal(versionCaution("0.1.0", "0.1.0"), null);
  assert.equal(versionCaution("0.1.0", null), null);
  assert.equal(versionCaution("0.1.1", "0.1.0"), "The node is 0.1.0 and this desktop 0.1.1 — restart the desktop so both are one version.");
});

test("the rows are the versions, then the data directory and the npub when known, values to copy marked", () => {
  assert.deepEqual(aboutRows({ app: "0.1.0", node: "0.1.0", dataDir: "/Users/me/.bisa", npub: "npub1abc" }), [
    { label: "Desktop", value: "0.1.0", copy: false },
    { label: "Node", value: "0.1.0", copy: false },
    { label: "Data directory", value: "/Users/me/.bisa", copy: true },
    { label: "You", value: "npub1abc", copy: true },
  ]);
  assert.deepEqual(
    aboutRows({ app: "0.1.0", node: null, dataDir: null, npub: null }).map((r) => [r.label, r.value]),
    [
      ["Desktop", "0.1.0"],
      ["Node", "unreachable"],
    ],
    "nothing known is nothing drawn",
  );
});

test("the doors out are the website then the source, https only, from what the build baked in", () => {
  assert.deepEqual(aboutLinks("https://bisa.dev", "https://github.com/mourad-ghafiri/Bisa"), [
    { id: "website", label: "Website", url: "https://bisa.dev" },
    { id: "source", label: "Source", url: "https://github.com/mourad-ghafiri/Bisa" },
  ]);
  assert.deepEqual(aboutLinks("", "https://github.com/mourad-ghafiri/Bisa").map((l) => l.id), ["source"], "a blank URL is no door");
  assert.deepEqual(aboutLinks("http://bisa.dev", undefined), [], "never a door that is not https");
  assert.match(marksWords(), /NOTICES\.md/);
  assert.match(marksWords(), /^The Bisa mark/);
});

test("the files revealed are the licence then the third-party notices, each only where the shell found it", () => {
  assert.deepEqual(aboutFiles({ licence: "/app/Resources/LICENSE", notices: "/app/Resources/THIRD-PARTY-NOTICES.md" }), [
    { id: "licence", label: "Licence", path: "/app/Resources/LICENSE" },
    { id: "notices", label: "Third-party notices", path: "/app/Resources/THIRD-PARTY-NOTICES.md" },
  ]);
  assert.deepEqual(aboutFiles({ licence: null, notices: "/x/THIRD-PARTY-NOTICES.md" }).map((f) => f.id), ["notices"]);
  assert.deepEqual(aboutFiles(null), [], "a browser session has no files to reveal");
});
