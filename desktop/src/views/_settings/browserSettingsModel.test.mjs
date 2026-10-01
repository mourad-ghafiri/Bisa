/**
 * Settings › Browser's words. Run with `node --test desktop/src/views/_settings/browserSettingsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { AGENTS_KEY, DEFAULT_HEADLESS, DEFAULT_POLICY, DEFAULT_SCRIPTS, ENABLED_KEY, HEADLESS_KEY, HEADLESS_MODES, POLICIES, REACHES, REACH_KEY, SCRIPTS_KEY, SCRIPTS_MODES, headlessSegments, headlessWords, policySegments, policyWords, reachWords, scriptsSegments, scriptsWords, statusWords } from "./browserSettingsModel.mjs";

test("the policy is three words, everyone first and by default, each with a sentence that says who and what to attach", () => {
  assert.deepEqual([...POLICIES], ["everyone", "assigned", "nobody"]);
  assert.equal(DEFAULT_POLICY, "everyone");
  assert.deepEqual(policySegments().map((s) => s.id), [...POLICIES]);
  assert.match(policyWords("assigned"), /Embedded Browser skill/);
  assert.match(policyWords("assigned"), /General Agent and the Workflow Agent — always may/);
  assert.match(policyWords("everyone"), /Any agent/);
  assert.match(policyWords("everyone"), /refused by the guard/, "why everyone is the default");
  assert.match(policyWords("nobody"), /platform's own included/);
  assert.equal(policyWords("???"), policyWords("everyone"), "an unknown word reads as the default");
  assert.deepEqual([...REACHES], ["anywhere", "local_only"]);
  assert.match(reachWords("local_only"), /served on this machine/);
  assert.match(reachWords("anywhere"), /any http or https page/);
  assert.equal(AGENTS_KEY, "browser.agents");
  assert.equal(REACH_KEY, "browser.agents.reach");
  assert.equal(ENABLED_KEY, "browser.enabled");
});

test("out of sight is three words, unattended by default, each saying when an agent's tab is kept from view", () => {
  assert.deepEqual([...HEADLESS_MODES], ["unattended", "always", "never"]);
  assert.equal(DEFAULT_HEADLESS, "unattended");
  assert.equal(HEADLESS_KEY, "browser.agents.headless");
  assert.deepEqual(headlessSegments().map((s) => s.id), [...HEADLESS_MODES]);
  assert.match(headlessWords("unattended"), /goal in auto mode/);
  assert.match(headlessWords("unattended"), /ask either way/);
  assert.match(headlessWords("always"), /Every tab an agent opens is kept out of sight/);
  assert.match(headlessWords("never"), /shown beside you/);
  assert.equal(headlessWords("???"), headlessWords("unattended"));
});

test("the status card says whether there is a browser here, whether it is on, and how many tabs are open", () => {
  assert.deepEqual(statusWords({ available: false, enabled: true, tabs: 0 }).label, "not here");
  assert.equal(statusWords({ available: false, enabled: true, tabs: 0 }).tone, "quiet");
  const off = statusWords({ available: true, enabled: false, tabs: 3 });
  assert.equal(off.label, "off");
  assert.equal(off.tone, "warn");
  assert.match(off.sentence, /agents are told so/);
  assert.match(statusWords({ available: true, enabled: true, tabs: 0 }).sentence, /^No tab is open\./);
  assert.match(statusWords({ available: true, enabled: true, tabs: 1 }).sentence, /^One tab is open\./);
  const three = statusWords({ available: true, enabled: true, tabs: 3 });
  assert.equal(three.label, "on");
  assert.match(three.sentence, /^3 tabs are open\. ⌘⇧L opens the Browser pane/);
  assert.match(statusWords({ available: true, enabled: true, tabs: 3, headless: 1 }).sentence, /^3 tabs are open\. One is kept out of sight\./);
  assert.match(statusWords({ available: true, enabled: true, tabs: 3, headless: 2 }).sentence, /2 are kept out of sight/);
});

test("scripts in a page are two words, allow by default, each saying what browser_eval does", () => {
  assert.equal(SCRIPTS_KEY, "browser.agents.scripts");
  assert.deepEqual([...SCRIPTS_MODES], ["allow", "refuse"]);
  assert.equal(DEFAULT_SCRIPTS, "allow");
  assert.deepEqual(
    scriptsSegments().map((s) => s.id),
    [...SCRIPTS_MODES],
  );
  assert.match(scriptsWords("allow"), /browser_eval/);
  assert.match(scriptsWords("allow"), /bounded/);
  assert.match(scriptsWords("refuse"), /refuses and names this setting/);
  assert.match(scriptsWords("refuse"), /browser_snapshot/);
  assert.equal(scriptsWords("sideways"), scriptsWords("allow"), "a word off the list reads as the default");
});
