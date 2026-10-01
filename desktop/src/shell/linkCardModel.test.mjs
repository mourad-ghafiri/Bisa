/**
 * What the card at the pointer offers (ide/17 §The handler).
 * Run with `node --test --import ./src/i18n/preload.mjs src/shell/linkCardModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { resolveLink } from "../ui/linkModel.mjs";
import { MAX_DEFAULT_ROOTS, copiedWords, defaultRoots, pathCard, rootLabel, urlCard } from "./linkCardModel.mjs";

const REVEAL = { reveal: "Reveal in Finder" };
const root = (id, path, paths, label = id) => ({ scope: "workstream", id, root: path, label, paths });
const APP = root("w1", "/Users/me/app", ["README.md", "src/a.rs", "src/lib/b.rs"], "App › fix");
const SITE = root("w2", "/Users/me/site", ["README.md", "index.html"], "Site");
const hit = (path, line = null, col = null) => ({ kind: "path", path, line, col });
const verbsOf = (card) => card.verbs.map((v) => [v.id, v.label, v.primary === true]);

test("a surface that names no root is read against the root on screen, then every checkout on disk, each once, at most eight", () => {
  const ws = (id, exists = true, path = `/ws/${id}`) => ({ exists, path, workstream: { id } });
  const list = [ws("w1"), ws("w2", false), ws("w3", true, null), ws("w4"), ws("w1")];
  assert.deepEqual(defaultRoots({ name: "workbench", scope: "workstream", id: "w4" }, list), [{ scope: "workstream", id: "w4" }, { scope: "workstream", id: "w1" }], "the root on screen first; one not on disk and one with no path left out");
  assert.deepEqual(defaultRoots({ name: "inbox" }, list), [{ scope: "workstream", id: "w1" }, { scope: "workstream", id: "w4" }]);
  assert.deepEqual(defaultRoots({ name: "workbench", scope: "goal", id: "g1" }, [ws("w1")]), [{ scope: "goal", id: "g1" }, { scope: "workstream", id: "w1" }]);
  const many = Array.from({ length: 30 }, (_, i) => ws(`w${i}`));
  assert.equal(defaultRoots({ name: "pulse" }, many).length, MAX_DEFAULT_ROOTS);
  assert.equal(defaultRoots({ name: "workbench", scope: "workstream", id: "w29" }, many).length, MAX_DEFAULT_ROOTS, "the root on screen counts among them");
  assert.deepEqual(defaultRoots({ name: "pulse" }, null), []);
  assert.equal(MAX_DEFAULT_ROOTS, 8);
});

test("a root is called by its project and its workstream's name, else by its id; a goal's by the word", () => {
  assert.equal(rootLabel({ scope: "workstream", id: "w1" }, { project_name: "App", workstream: { name: "fix" } }), "App › fix");
  assert.equal(rootLabel({ scope: "workstream", id: "w1" }, { project_name: "App", workstream: { name: null } }), "App");
  assert.equal(rootLabel({ scope: "workstream", id: "w1" }, { project_name: null, workstream: { name: null } }), "w1");
  assert.equal(rootLabel({ scope: "goal", id: "g1" }, undefined), "the goal");
  assert.equal(rootLabel({ scope: "work_item", id: "i1" }, undefined), "i1");
});

test("one document: Open it at its line is the verb Enter takes, then reveal where the checkout is on disk, then copy", () => {
  const card = pathCard(hit("src/a.rs", 42, 7), resolveLink(hit("src/a.rs", 42, 7), [APP]), REVEAL);
  assert.equal(card.title, "src/a.rs");
  assert.equal(card.subtitle, "App › fix · /Users/me/app");
  assert.deepEqual(verbsOf(card), [["open", "Open src/a.rs:42:7 in the IDE", true], ["reveal", "Reveal in Finder", false], ["copy", "Copy the path", false]]);
  assert.equal(card.verbs[0].doc.path, "src/a.rs");
  assert.equal(card.verbs[1].absolute, "/Users/me/app/src/a.rs", "the absolute path is made here, on the desktop, and goes to the shell alone");
  assert.deepEqual([card.verbs[2].text, card.verbs[2].what], ["src/a.rs", "path"]);
  // A checkout with no place on this machine reveals nothing.
  const nowhere = pathCard(hit("src/a.rs"), resolveLink(hit("src/a.rs"), [root("w9", null, ["src/a.rs"])]), REVEAL);
  assert.deepEqual(nowhere.verbs.map((v) => v.id), ["open", "copy"]);
  assert.equal(nowhere.subtitle, "w9");
});

test("a directory of a root opens the root, and reveals the root itself", () => {
  const card = pathCard(hit("/Users/me/app"), resolveLink(hit("/Users/me/app"), [APP]), REVEAL);
  assert.deepEqual(verbsOf(card)[0], ["open", "Open App › fix in the IDE", true]);
  assert.equal(card.title, "App › fix");
  assert.equal(card.verbs[1].absolute, "/Users/me/app");
});

test("a file several checkouts hold is a door per checkout, none of them Enter's, then copy", () => {
  const res = resolveLink(hit("README.md"), [APP, SITE]);
  assert.equal(res.kind, "choice");
  const card = pathCard(hit("README.md"), res, REVEAL);
  assert.equal(card.subtitle, "In more than one checkout");
  assert.deepEqual(verbsOf(card), [["open", "Open in App › fix", false], ["open", "Open in Site", false], ["copy", "Copy the path", false]]);
  assert.deepEqual(card.verbs.slice(0, 2).map((v) => v.doc.id), ["w1", "w2"]);
});

test("a path outside every checkout opens as a loose file, is revealed or copied; one nobody vouches for is copied and nothing else", () => {
  const outside = pathCard(hit("/etc/hosts"), resolveLink(hit("/etc/hosts"), [APP]), REVEAL);
  assert.equal(outside.subtitle, "Outside every checkout the desktop knows");
  assert.deepEqual(verbsOf(outside), [["open_loose", "Open in the IDE", true], ["reveal", "Reveal in Finder", false], ["copy", "Copy the path", false]]);
  assert.deepEqual(outside.verbs.slice(0, 2).map((v) => v.absolute), ["/etc/hosts", "/etc/hosts"]);
  const unknown = pathCard(hit("../up/and/out.rs"), resolveLink(hit("../up/and/out.rs"), [APP]), REVEAL);
  assert.equal(unknown.subtitle, "Not found in any checkout");
  assert.deepEqual(verbsOf(unknown), [["copy", "Copy the path", false]]);
  assert.equal(unknown.title, "../up/and/out.rs");
});

test("a URL: the host in bold and the URL beneath; the embedded browser first where there is one; only http and https open", () => {
  const both = urlCard("https://example.com/docs?a=1", { embedded: true });
  assert.deepEqual([both.title, both.subtitle], ["example.com", "https://example.com/docs?a=1"]);
  assert.deepEqual(verbsOf(both), [["open_here", "Open in Bisa's browser", true], ["open_machine", "Open in the machine's browser", false], ["copy", "Copy the URL", false]]);
  const machine = urlCard("http://localhost:5173/", { embedded: false });
  assert.deepEqual(verbsOf(machine), [["open_machine", "Open in the machine's browser", true], ["copy", "Copy the URL", false]], "with no embedded browser the machine's is Enter's");
  for (const url of ["mailto:ada@example.com", "file:///etc/hosts", "javascript:alert(1)", "ftp://example.com/x", "not a url"]) {
    const card = urlCard(url, { embedded: true });
    assert.deepEqual(card.verbs.map((v) => v.id), ["copy"], `${url} is copied and nothing else`);
    assert.equal(card.subtitle, "Only http and https open in the browser");
    assert.equal(card.title, url);
  }
  assert.deepEqual([both.verbs[2].text, both.verbs[2].what], ["https://example.com/docs?a=1", "url"]);
});

test("a copy says what was copied, or that the clipboard refused — each a sentence of its own", () => {
  assert.equal(copiedWords("path", true), "Path copied.");
  assert.equal(copiedWords("path", false), "The path could not be copied.");
  assert.equal(copiedWords("url", true), "URL copied.");
  assert.equal(copiedWords("url", false), "The URL could not be copied.");
});

test("the provider draws what the model says, in the order the clicks came", () => {
  const provider = readFileSync(new URL("./linkHandler.tsx", import.meta.url), "utf8");
  assert.ok(provider.includes("show(pathCard(hit, res, { reveal }), at)") && provider.includes("show(urlCard(hit.url, { embedded: canOpenBrowser() }), at)"));
  assert.ok(provider.includes("const ticket = clicks.begin();"), "a click takes its ticket");
  assert.ok(provider.indexOf("if (!clicks.lands(ticket)) return;") < provider.indexOf("const res = resolveLink(hit, known);"), "an earlier click's card never lands over a later one's");
  const code = provider.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
  assert.ok(!/label: t\(/.test(code), "no verb is worded in the provider");
});
