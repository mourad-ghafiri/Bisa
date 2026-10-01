/**
 * What a browser tab's bar shows. Run with `node --test desktop/src/shell/browserChromeModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { HELD, chromeOf, loadWords, wandWords } from "./browserChromeModel.mjs";

const facts = (over = {}) => ({ blank: false, loading: false, canBack: true, canForward: false, inIde: true, atWorkbenchHome: true, annotatable: true, ...over });

test("the whole bar is there on every tab: a blank tab holds every verb but the address, each with its reason", () => {
  const c = chromeOf(facts({ blank: true, canBack: false, inIde: false }));
  assert.equal(c.address, true);
  for (const verb of ["load", "wand", "camera", "openOutside"]) {
    assert.equal(c[verb].enabled, false, `${verb} is held`);
    assert.equal(c[verb].why, HELD.blank, `${verb} says why`);
  }
  assert.equal(c.load.verb, "reload", "held as Reload, never as Stop");
  assert.deepEqual(c.back, { enabled: false, why: HELD.noBack });
  assert.deepEqual(c.forward, { enabled: false, why: HELD.noForward });
  assert.ok(c.openInIde, "a blank tab at home in a root still opens there");
});

test("a page frees the verbs; back and forward follow the webview's own history; Stop stands in for Reload while it loads", () => {
  const idle = chromeOf(facts());
  assert.deepEqual([idle.load.verb, idle.load.enabled, idle.load.why], ["reload", true, null]);
  assert.ok(idle.wand.enabled && idle.camera.enabled && idle.openOutside.enabled);
  assert.deepEqual(idle.back, { enabled: true, why: null });
  assert.deepEqual(idle.forward, { enabled: false, why: HELD.noForward }, "nothing ahead until a back");
  assert.ok(chromeOf(facts({ canForward: true })).forward.enabled);
  const busy = chromeOf(facts({ loading: true }));
  assert.deepEqual([busy.load.verb, busy.load.enabled], ["stop", true], "a page on its way can be stopped, not reloaded");
  assert.equal(loadWords("reload"), "Reload");
  assert.equal(loadWords("stop"), "Stop loading");
});

test("the wand is held on a page nobody may edit, with the reason given or an artifact's by default; the IDE door is the pane's alone", () => {
  const artifact = chromeOf(facts({ annotatable: false }));
  assert.deepEqual(artifact.wand, { enabled: false, why: HELD.artifact });
  assert.equal(chromeOf(facts({ annotatable: false, whyNot: "Open a checkout first" })).wand.why, "Open a checkout first");
  assert.match(wandWords(artifact.wand, false), /nobody's to edit/);
  assert.match(wandWords({ enabled: true, why: null }, false), /hover an element/);
  assert.match(wandWords({ enabled: true, why: null }, true), /annotations stay/);
  assert.ok(chromeOf(facts({ inIde: false })).openInIde, "the pane offers the tab's home in the IDE");
  assert.ok(!chromeOf(facts({ inIde: false, atWorkbenchHome: false })).openInIde, "the workspace's tab has no home to open");
  assert.ok(!chromeOf(facts({ inIde: true })).openInIde, "already there");
});
