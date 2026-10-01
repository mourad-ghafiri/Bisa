/**
 * The Browser button's words. Run with `node --test desktop/src/views/_workbench/serversModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { annotatable, annotationHome, browserMenu, openLabel, servedWords, serverAt, serverLabel, servingWords, stopLabel, stoppedWords } from "./serversModel.mjs";
import { runDoor } from "./runCommandModel.mjs";

const server = (id, folder, port) => ({ id, owner: { kind: "workstream", workstream: "w1", folder }, port, url: `http://127.0.0.1:${port}/`, page: `http://127.0.0.1:${port}/` });
const artifact = (port) => ({ id: `a${port}`, owner: { kind: "artifact", sha256: "abc", name: "report.html" }, port, url: `http://127.0.0.1:${port}/`, page: `http://127.0.0.1:${port}/report.html` });
const run = { command: "npm run dev", trusted: true };
/** A checkout's facts, the door computed the way the launcher computes it (`runCommandModel.runDoor`) unless the test says otherwise. */
const checkout = (over = {}) => {
  const facts = { checkout: true, servers: [], ports: [], tabs: [], annotate: null, busy: false, ...over };
  return { ...facts, door: "door" in over ? over.door : runDoor({ run: over.run ?? null, servers: facts.servers }).id };
};
const ids = (items) => items.map((i) => i.id);

test("the main click opens the newest server, else the newest port, else a blank tab", () => {
  assert.deepEqual(browserMenu(checkout()).main, { url: null, hint: "Open a browser tab — type a URL" });
  assert.equal(browserMenu(checkout({ ports: [{ port: 3000, process: "node" }] })).main.url, "http://localhost:3000/");
  const m = browserMenu(checkout({ servers: [server("s1", "", 4173)], ports: [{ port: 3000, process: "node" }] }));
  assert.equal(m.main.url, "http://127.0.0.1:4173/", "a server the IDE hosts comes before a port it only observed");
});

test("the caret lists New tab and one From folder…, each server with Open and Stop, the ports, the tabs, then the annotation — plain words, nothing about running", () => {
  const items = browserMenu(
    checkout({
      servers: [server("s1", "", 4173)],
      ports: [{ port: 5173, process: "vite" }],
      tabs: [{ key: "b1", label: "Home" }, { key: "b2", label: "Admin", headless: true }],
      annotate: { key: "b1" },
    }),
  ).items;
  assert.deepEqual(
    items.map((i) => [i.id, i.label, i.separatorBefore ?? false, i.danger ?? false]),
    [
      ["blank", "New tab", false, false],
      ["serve-folder", "From folder…", true, false],
      ["open:s1", "Open :4173", true, false],
      ["stop:s1", "Stop", false, true],
      ["port:5173", "Open :5173 · vite", true, false],
      ["tab:b1", "Home", true, false],
      ["tab:b2", "Admin · unseen", false, false],
      ["annotate:b1", "Annotate the page for an agent…", true, false],
    ],
  );
  assert.ok(items.every((i) => !/serve|checkout|run command/i.test(i.label)), "no label says Serve, checkout or run command");
  assert.match(items.find((i) => i.id === "open:s1").hint, /the checkout · :4173/, "where a server serves is the hint");
  assert.match(items.find((i) => i.id === "stop:s1").hint, /Stop serving the checkout on :4173/);
  assert.equal(items.find((i) => i.id === "tab:b2").icon, "hidden", "a tab kept out of sight wears the hidden glyph");
  assert.match(items.find((i) => i.id === "tab:b2").hint, /show it/);
  assert.equal(items.find((i) => i.id === "open:s1").command, "run_project", "the chord shows beside the door it opens");
  assert.ok(items.every((i) => i.id === "open:s1" || i.command === undefined), "one door, one chord");
  assert.deepEqual(ids(browserMenu(checkout()).items), ["blank", "serve-folder"], "no server, no port, no tab, no page: the two fixed items");
  assert.equal(browserMenu(checkout()).items.find((i) => i.id === "serve-folder").command, "run_project", "nothing up and no command: ⌘⇧R opens the folder picker");
  assert.ok(browserMenu(checkout({ servers: [server("s1", "", 4173)] })).items.every((i) => !/branch/i.test(`${i.id} ${i.label} ${i.hint}`)), "nothing here is named after a branch: what is served is a folder");
  assert.match(browserMenu(checkout()).items.find((i) => i.id === "serve-folder").hint, /the root or one folder/);
  assert.ok(!ids(browserMenu(checkout({ run })).items).some((id) => /run/.test(id)), "the run command is not a Browser item");
  assert.ok(
    browserMenu(checkout({ run })).items.every((i) => i.command === undefined),
    "when the door is the run command, the chord shows on the Terminal caret's item — nowhere here",
  );
});

test("Stop is one word while one server is up, and names its port when several are", () => {
  const one = [server("s1", "", 4173)];
  const two = [server("s1", "", 4173), server("s2", "docs", 8000)];
  assert.equal(stopLabel(one[0], one), "Stop");
  assert.deepEqual(two.map((s) => stopLabel(s, two)), ["Stop :4173", "Stop :8000"]);
  assert.deepEqual(two.map(openLabel), ["Open :4173", "Open :8000"]);
  const items = browserMenu(checkout({ servers: two })).items;
  assert.deepEqual(ids(items), ["blank", "serve-folder", "open:s1", "stop:s1", "open:s2", "stop:s2"]);
  assert.deepEqual(
    items.filter((i) => i.id.startsWith("stop:")).map((i) => i.label),
    ["Stop :4173", "Stop :8000"],
  );
  assert.equal(items.find((i) => i.id === "open:s2").command, "run_project", "the newest server is the door");
});

test("a goal or a work item lists a new tab and the tabs here — nothing serves without a checkout, and there is no door", () => {
  const goal = browserMenu({ checkout: false, servers: [], ports: [{ port: 3000, process: "node" }], tabs: [{ key: "b2", label: "Docs" }], annotate: null, busy: false, door: null });
  assert.deepEqual(ids(goal.items), ["blank", "port:3000", "tab:b2"]);
  assert.ok(goal.items.every((i) => i.command === undefined), "no door off a checkout");
});

test("a start in flight holds the serves and the stops — never an open, a tab or the annotation", () => {
  const busy = browserMenu(checkout({ servers: [server("s1", "", 1)], tabs: [{ key: "b1", label: "t" }], annotate: { key: "b1" }, busy: true })).items;
  const held = (id) => busy.find((i) => i.id === id).disabled;
  assert.ok(held("serve-folder") && held("stop:s1"));
  assert.ok(!held("blank") && !held("open:s1") && !held("tab:b1") && !held("annotate:b1"), "opening a tab is never held by a start in flight");
});

test("a page is the server's whose origin it is on, and any page can be annotated but an artifact's own and a blank tab; the chips go to the checkout or the screen", () => {
  const all = [server("s1", "", 4173), artifact(4174)];
  assert.equal(serverAt(all, "http://127.0.0.1:4173/about.html")?.id, "s1");
  assert.equal(serverAt(all, "http://127.0.0.1:4174/report.html")?.id, "a4174");
  assert.equal(serverAt(all, "http://localhost:5173/"), null, "a dev server is nobody's");
  assert.equal(serverAt(all, "not a url"), null);
  const home = { scope: "workstream", id: "w1" };
  assert.ok(annotatable({ home, url: "http://127.0.0.1:4173/" }, all), "the checkout the node serves");
  assert.ok(annotatable({ home, url: "http://localhost:5173/" }, all), "a dev server on this machine");
  assert.ok(annotatable({ home, url: "https://example.com/" }, all), "the web too: the agent reads the element and the URL");
  assert.ok(annotatable({ home: { scope: "goal", id: "g1" }, url: "https://example.com/pricing" }, all), "a goal's tab, annotated for the goal's conversation");
  assert.ok(annotatable({ home: null, url: "http://localhost:5173/" }, all), "the workspace's tab too");
  assert.ok(!annotatable({ home, url: "http://127.0.0.1:4174/report.html" }, all), "an artifact's page is an agent's own");
  assert.ok(!annotatable({ home, url: "about:blank" }, all), "nothing loaded");
  assert.ok(!annotatable(null, all));
  assert.deepEqual(annotationHome({ home }), { kind: "checkout", wid: "w1" }, "a checkout's tab: its conversation, file chips when the file is known");
  assert.deepEqual(annotationHome({ home: { scope: "goal", id: "g1" } }), { kind: "screen" });
  assert.deepEqual(annotationHome({ home: null }), { kind: "screen" });
  assert.deepEqual(annotationHome(null), { kind: "screen" });
});

test("the toasts and the folder field say what happened and what is wrong", () => {
  assert.equal(servedWords({ owner: { kind: "workstream", workstream: "w1", folder: "" } }), "the checkout");
  assert.equal(serverLabel({ owner: { kind: "workstream", workstream: "w1", folder: "docs" }, port: 8000 }), "docs/ · :8000");
  assert.equal(serverLabel({ owner: { kind: "artifact", sha256: "a", name: "report.html" }, port: 8001 }), "report.html · :8001", "an artifact's server is named by its file");
  assert.equal(servingWords(server("s", "site", 4173)), "Serving site/ at http://127.0.0.1:4173/");
  assert.equal(stoppedWords(server("s", "", 4173)), "Stopped serving the checkout on :4173.");
});
