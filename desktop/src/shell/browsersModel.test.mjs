/**
 * The browser tabs as facts. Run with `node --test desktop/src/shell/browsersModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { BLANK_URL, BROWSER_SCOPES, BROWSER_SESSIONS_KEY, EMPTY_BROWSERS, OPENERS, PERSON, asked, atHome, browserLabel, browserTitle, browsersRootedAt, closeAll, closeBrowser, draftScopeOf, focusBrowser, byAgent, byPage, homeOf, navigated, normalizeUrl, openBrowser, openerOf, openerWords, parseBrowsers, reveal, seenRootedAt, seenSessions, serializeBrowsers, sessionOf, titled, urlWords, visibilityWords,
  workbenchHomeOf,
} from "./browsersModel.mjs";

test("the bar takes a bare host as http, keeps a scheme it was given, and refuses anything that is not a page", () => {
  assert.deepEqual(normalizeUrl("localhost:5173"), { url: "http://localhost:5173/" });
  assert.deepEqual(normalizeUrl(" 127.0.0.1:3000/app?x=1 "), { url: "http://127.0.0.1:3000/app?x=1" });
  assert.deepEqual(normalizeUrl("https://example.com/docs"), { url: "https://example.com/docs" });
  assert.match(normalizeUrl("").error, /Type a URL/);
  assert.match(normalizeUrl("file:///etc/passwd").error, /not file/);
  assert.match(normalizeUrl("javascript:alert(1)").error, /not javascript/);
  assert.match(normalizeUrl("http://").error, /not a URL|names no host/);
  assert.equal(urlWords("http://localhost:5173/"), "localhost:5173");
  assert.equal(urlWords("https://example.com/docs/a?b=1"), "example.com/docs/a?b=1");
});

test("a tab opens at home in a place — or the workspace — with a minted key, moves as the webview says, and closes to its neighbour", () => {
  let s = openBrowser(EMPTY_BROWSERS, { by: PERSON, home: { scope: "workstream", id: "w1" }, url: "http://localhost:5173/" });
  assert.equal(s.sessions.length, 1);
  assert.equal(s.active, "b1");
  assert.equal(sessionOf(s, "b1").loading, true, "a URL given loads at once");
  assert.deepEqual(sessionOf(s, "b1").home, { scope: "workstream", id: "w1" });
  assert.equal(openBrowser(s, { by: PERSON, home: { scope: "nowhere", id: "x" } }).sessions[1].home, null, "a place the tabs do not know is the workspace");
  assert.equal(openBrowser(s, { by: PERSON, home: { scope: "goal", id: "" } }).sessions[1].home, null);
  assert.equal(openBrowser(s, { by: PERSON }).sessions[1].home, null, "no place named: the workspace's tab");
  assert.equal(homeOf(null), null);
  assert.deepEqual(homeOf({ scope: "conversation", id: "c1" }), { scope: "conversation", id: "c1" }, "a conversation is a home an agent asks from");
  s = openBrowser(s, { by: PERSON, home: { scope: "workstream", id: "w1" } });
  assert.equal(sessionOf(s, "b2").url, BLANK_URL, "no URL: a blank tab, nothing loading");
  assert.equal(sessionOf(s, "b2").loading, false);
  s = openBrowser(s, { by: PERSON, home: { scope: "goal", id: "g1" }, url: "https://example.com/" });
  assert.deepEqual(browsersRootedAt(s.sessions, "workstream", "w1").map((x) => x.key), ["b1", "b2"]);
  assert.ok(atHome(sessionOf(s, "b3"), { scope: "goal", id: "g1" }));
  assert.ok(!atHome(sessionOf(s, "b3"), { scope: "goal", id: "g2" }));
  assert.ok(!atHome(openBrowser(s, { by: PERSON }).sessions[3], { scope: "goal", id: "g1" }), "the workspace's tab is at home nowhere in particular");
  assert.deepEqual([...BROWSER_SCOPES], ["workstream", "goal", "work_item", "workflow", "channel", "dm", "conversation"], "every screen a tab can be opened beside");
  for (const scope of ["workflow", "channel", "dm"]) assert.deepEqual(homeOf({ scope, id: "x1" }), { scope, id: "x1" }, `${scope} is a home`);
  assert.equal(draftScopeOf(sessionOf(s, "b1")), "workstream:w1", "a tab's drafts live under its root's key, as the IDE's documents do");
  assert.equal(draftScopeOf(openBrowser(s, { by: PERSON, home: { scope: "conversation", id: "c1" } }).sessions[3]), "conversation:c1");
  assert.equal(draftScopeOf(openBrowser(s, { by: PERSON }).sessions[3]), "workspace");
  assert.deepEqual([sessionOf(s, "b1").canBack, sessionOf(s, "b1").canForward], [false, false], "a fresh tab has no history yet");
  s = navigated(s, "b1", "http://localhost:5173/pricing", "started");
  assert.equal(sessionOf(s, "b1").url, "http://localhost:5173/pricing");
  assert.ok(sessionOf(s, "b1").loading);
  s = navigated(s, "b1", "http://localhost:5173/pricing", "finished", { canBack: true, canForward: false });
  assert.ok(!sessionOf(s, "b1").loading);
  assert.deepEqual([sessionOf(s, "b1").canBack, sessionOf(s, "b1").canForward], [true, false], "the webview's own word on its history");
  s = navigated(s, "b1", "http://localhost:5173/pricing#faq", "finished");
  assert.ok(sessionOf(s, "b1").canBack, "a move that says nothing of history keeps it");
  s = navigated(s, "b1", BLANK_URL, "finished", { canBack: false, canForward: true });
  assert.equal(sessionOf(s, "b1").url, BLANK_URL, "the blank page is a page the tab can be on");
  assert.equal(navigated(s, "nope", "x", "finished"), s, "an unknown tab changes nothing");
  s = titled(s, "b1", "  Pricing — Storefront  ");
  assert.equal(sessionOf(s, "b1").title, "Pricing — Storefront");
  assert.equal(titled(s, "b1", "Pricing — Storefront"), s, "the same title is no change");
  s = asked(s, "b2", "http://localhost:4173/");
  assert.ok(sessionOf(s, "b2").loading);
  s = focusBrowser(s, "b1");
  assert.equal(s.active, "b1");
  assert.equal(focusBrowser(s, "b1"), s);
  s = closeBrowser(s, "b1");
  assert.equal(s.active, "b2", "the right neighbour takes over");
  s = closeBrowser(s, "b3");
  assert.equal(s.active, "b2");
  s = closeBrowser(s, "b2");
  assert.equal(s.active, null, "no tab, no active");
  assert.equal(closeBrowser(s, "b2"), s);
  const many = openBrowser(openBrowser(EMPTY_BROWSERS, { by: PERSON }), { by: PERSON });
  const closed = closeAll(many);
  assert.deepEqual([closed.sessions.length, closed.active, closed.seq], [0, null, 2], "the browser switched off: every tab gone, the sequence kept");
  assert.equal(openBrowser(closed, { by: PERSON }).sessions[0].key, "b3", "a key is never minted twice in a run");
  assert.equal(closeAll(closed), closed, "nothing to close is no change");
});

test("a tab is named by its title, else its host and path, else Browser; its tooltip is the URL", () => {
  const blank = openBrowser(EMPTY_BROWSERS, { by: PERSON, home: { scope: "workstream", id: "w1" } }).sessions[0];
  assert.equal(browserLabel(blank), "Browser");
  assert.equal(browserTitle(blank), "Browser — type a URL");
  const loaded = { ...blank, url: "http://localhost:5173/pricing" };
  assert.equal(browserLabel(loaded), "localhost:5173/pricing");
  assert.equal(browserTitle(loaded), "http://localhost:5173/pricing");
  assert.equal(browserLabel({ ...loaded, title: "Pricing" }), "Pricing");
  assert.equal(browserTitle({ ...loaded, title: "Pricing" }), "Pricing — http://localhost:5173/pricing");
  assert.equal(browserLabel(null), "Browser");
});

test("a headless tab opens out of sight — not active, not seen — until it is revealed, and the lists say so", () => {
  let s = openBrowser(EMPTY_BROWSERS, { by: PERSON, home: { scope: "goal", id: "g1" }, url: "http://localhost:5173/" });
  s = openBrowser(s, { by: PERSON, home: { scope: "goal", id: "g1" }, url: "http://localhost:5173/admin", headless: true });
  assert.equal(s.active, "b1", "a tab kept out of sight takes no host from the one the person looks at");
  assert.ok(sessionOf(s, "b2").headless);
  assert.deepEqual(seenSessions(s.sessions).map((x) => x.key), ["b1"]);
  assert.deepEqual(seenRootedAt(s.sessions, "goal", "g1").map((x) => x.key), ["b1"]);
  assert.match(visibilityWords(sessionOf(s, "b2")), /^unseen/);
  assert.equal(visibilityWords(sessionOf(s, "b1")), null);
  assert.equal(focusBrowser(s, "b2").active, "b2", "focus alone does not reveal — the store's word is reveal");
  s = reveal(s, "b2");
  assert.ok(!sessionOf(s, "b2").headless);
  assert.equal(s.active, "b2", "revealed and to the front");
  assert.equal(reveal(s, "b2"), focusBrowser(s, "b2"), "revealing a seen tab is a focus");
  assert.equal(reveal(s, "nope"), s);
  // Out of sight is an agent's working page for that run: it ends with the window, and is never loaded again at a launch nobody asked it of.
  const withUnseen = openBrowser(s, { by: byAgent("reviewer"), headless: true });
  assert.equal(withUnseen.sessions.length, 3);
  const raw = JSON.parse(JSON.stringify(serializeBrowsers(withUnseen)));
  assert.deepEqual(raw.sessions.map((x) => x.key), ["b1", "b2"], "only the tabs a person can look at are remembered");
  assert.ok(parseBrowsers(raw).sessions.every((x) => x.headless === false), "and what is remembered is in sight");
  assert.equal(parseBrowsers(raw).seq, 3, "the unseen tab's key is never minted again");
});

test("what is remembered survives a round trip and a wrong shape reads as nothing", () => {
  let s = openBrowser(EMPTY_BROWSERS, { by: PERSON, home: { scope: "workstream", id: "w1" }, url: "http://localhost:5173/" });
  s = titled(s, "b1", "Home");
  s = openBrowser(s, { by: PERSON, home: { scope: "goal", id: "g1" } });
  s = openBrowser(s, { by: PERSON, url: "https://example.com/" });
  const raw = JSON.parse(JSON.stringify(serializeBrowsers(s)));
  const back = parseBrowsers(raw);
  assert.deepEqual(back.sessions.map((x) => [x.key, x.home, x.url, x.title, x.loading]), [
    ["b1", { scope: "workstream", id: "w1" }, "http://localhost:5173/", "Home", false],
    ["b2", { scope: "goal", id: "g1" }, BLANK_URL, "", false],
    ["b3", null, "https://example.com/", "", false],
  ]);
  assert.equal(back.active, "b3");
  assert.equal(back.seq, 3, "the next key is never one already issued");
  assert.equal(parseBrowsers(null), EMPTY_BROWSERS);
  assert.deepEqual(back.sessions.map((x) => x.by), [PERSON, PERSON, PERSON], "and who opened each");
  assert.equal(BROWSER_SESSIONS_KEY, "bisa.browser.sessions.v3");
  assert.equal(parseBrowsers({ version: 1, sessions: [] }), EMPTY_BROWSERS, "an older shape is not read");
  assert.equal(parseBrowsers({ version: 2, sessions: [{ key: "b1", home: null, url: "http://x/", title: "", headless: false, pane: null }], active: "b1", seq: 1 }), EMPTY_BROWSERS, "the shape before an opener was kept is not read either");
  const junk = parseBrowsers({ version: 3, sessions: [{ key: "nope", home: null, by: PERSON }, { key: "b6", home: null, url: "http://y/" }, { key: "b7", home: { scope: "nowhere", id: "w" }, url: "http://x/", by: { kind: "person" } }], active: "b7", seq: 1 });
  assert.deepEqual(junk.sessions.map((x) => [x.key, x.home]), [["b7", null]], "a key that was never minted is dropped, a tab that names no opener is dropped; an unknown home is the workspace");
  assert.equal(junk.seq, 7, "the seq never falls under a remembered key");
});

test("a tab is born only by somebody: a target that names no opener opens nothing, and every tab wears who opened it", () => {
  assert.deepEqual([...OPENERS], ["person", "agent", "page"]);
  assert.equal(openBrowser(EMPTY_BROWSERS, {}), EMPTY_BROWSERS, "no opener named: no tab — a door cannot forget to say");
  assert.equal(openBrowser(EMPTY_BROWSERS, { home: { scope: "goal", id: "g1" }, url: "http://localhost:5173/" }), EMPTY_BROWSERS);
  assert.equal(openBrowser(EMPTY_BROWSERS, { by: { kind: "pane" } }), EMPTY_BROWSERS, "a pane is nobody: an opener the tabs do not know opens nothing");
  assert.equal(openBrowser(EMPTY_BROWSERS, null), EMPTY_BROWSERS);
  let s = openBrowser(EMPTY_BROWSERS, { by: PERSON });
  s = openBrowser(s, { by: byAgent("reviewer"), headless: true });
  s = openBrowser(s, { by: byPage("b1") });
  assert.deepEqual(s.sessions.map((x) => x.by), [{ kind: "person" }, { kind: "agent", agent: "reviewer" }, { kind: "page", from: "b1" }]);
  // The three openers, and what is none.
  assert.equal(openerOf({ kind: "person", extra: 1 }), PERSON);
  assert.deepEqual(openerOf({ kind: "agent" }), { kind: "agent", agent: null }, "a request that named nobody is still an agent's");
  assert.deepEqual(byAgent(""), { kind: "agent", agent: null });
  assert.deepEqual(openerOf({ kind: "page", from: "b4" }), { kind: "page", from: "b4" });
  for (const junk of [null, undefined, "person", 3, {}, { kind: "page" }, { kind: "page", from: "" }, { kind: "launch" }]) assert.equal(openerOf(junk), null, JSON.stringify(junk));
  // In a row's words: nothing for a person's own, the agent by name, a page.
  const names = (id) => (id === "reviewer" ? "Reviewer" : null);
  assert.equal(openerWords(PERSON, names), null, "the common case stays quiet");
  assert.equal(openerWords(byAgent("reviewer"), names), "opened by Reviewer");
  assert.equal(openerWords(byAgent("scout"), names), "opened by scout", "an agent the roster no longer names is said by its id");
  assert.equal(openerWords(byAgent(null)), "opened by an agent");
  assert.equal(openerWords(byPage("b1")), "opened by a page");
  assert.equal(openerWords(undefined), null);
});

test("the door to the Project IDE is offered only from a home the IDE can be rooted at", () => {
  for (const scope of ["workstream", "work_item", "goal"]) {
    assert.deepEqual(workbenchHomeOf({ home: { scope, id: "01J" } }), { scope, id: "01J" });
  }
  for (const scope of ["workflow", "channel", "dm", "conversation", "nowhere"]) {
    assert.equal(workbenchHomeOf({ home: { scope, id: "01J" } }), null, scope);
  }
  assert.equal(workbenchHomeOf({ home: null }), null);
  assert.equal(workbenchHomeOf({ home: { scope: "goal", id: "" } }), null);
  assert.equal(workbenchHomeOf(null), null);
});
