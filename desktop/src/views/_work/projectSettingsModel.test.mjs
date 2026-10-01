/**
 * Every project-scope key has one home in the Project IDE, and the lists here
 * are the registry's. Run with
 * `node --test desktop/src/views/_work/projectSettingsModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PROJECT_AGENT_KEYS, PROJECT_BROWSER_KEYS, PROJECT_EDITOR_KEYS, PROJECT_GIT_KEYS, PROJECT_KEYS_ELSEWHERE, PROJECT_WORKSTREAM_KEYS, originWords, projectRow } from "./projectSettingsModel.mjs";

/** The keys whose `ScopeSet` admits `Project`, read from the registry's source. */
function projectKeysInRust() {
  const src = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  const keys = [];
  for (const m of src.matchAll(/def!\(\s*"([a-z0-9_.]+)",[\s\S]*?S::(M|W|MW|WP|MWP)\s*\)/g)) {
    if (m[2].includes("P")) keys.push(m[1]);
  }
  return keys.sort();
}

test("the five cards and the two hand-written editors together cover every project-scope key, once", () => {
  const listed = [...PROJECT_GIT_KEYS, ...PROJECT_WORKSTREAM_KEYS, ...PROJECT_BROWSER_KEYS, ...PROJECT_AGENT_KEYS, ...PROJECT_EDITOR_KEYS, ...PROJECT_KEYS_ELSEWHERE].sort();
  assert.deepEqual(listed, projectKeysInRust(), "a project key with no home, or a home for a key that is not a project's");
  assert.equal(new Set(listed).size, listed.length, "no key is edited in two cards");
  assert.ok(PROJECT_GIT_KEYS.every((k) => k.startsWith("git.")));
  assert.ok(PROJECT_WORKSTREAM_KEYS.every((k) => k.startsWith("workstreams.") && !k.startsWith("workstreams.script.")), "the scripts have their own card");
  assert.ok(PROJECT_EDITOR_KEYS.every((k) => k.startsWith("editor.") || k === "terminal.default_harness"));
  assert.ok(PROJECT_BROWSER_KEYS.every((k) => k.startsWith("browser.") || k === "mobile_development.agents" || k === "draw.agents"), "the browser card takes the browser keys, who may drive a device and who may draw");
  assert.ok(PROJECT_KEYS_ELSEWHERE.includes("workstreams.script.run"), "the run command is the scripts card's fourth text");
  assert.ok(PROJECT_AGENT_KEYS.every((k) => k.startsWith("agents.") || k.startsWith("decisions.")), "the agents card takes how agents start and what the Decision-Making Agent decides");
  assert.deepEqual(PROJECT_AGENT_KEYS.slice(0, 2), ["agents.conversation.mode", "agents.effort"], "how hard a model works sits beside the other agent key");
  const view = readFileSync(new URL("./ProjectSettingsView.tsx", import.meta.url), "utf8");
  for (const list of ["PROJECT_GIT_KEYS", "PROJECT_WORKSTREAM_KEYS", "PROJECT_BROWSER_KEYS", "PROJECT_AGENT_KEYS", "PROJECT_EDITOR_KEYS"]) {
    assert.ok(view.includes(`keys={${list}}`), `${list} is a card the screen draws — a list nobody mounts is a key nobody can set`);
  }
});

test("a row says where its value comes from and whether the project holds it", () => {
  assert.equal(originWords("project"), "set for this project");
  assert.equal(originWords("workspace"), "the workspace's value");
  assert.equal(originWords("machine"), "this machine's value");
  assert.equal(originWords("default"), "the default");
  const own = projectRow({ key: "git.pull", value: "rebase", origin: "project" });
  assert.deepEqual(own, { key: "git.pull", value: "rebase", origin: "set for this project", own: true });
  assert.equal(projectRow({ key: "git.pull", value: "ff_only", origin: "workspace" }).own, false, "nothing to inherit from when the project holds nothing");
});
