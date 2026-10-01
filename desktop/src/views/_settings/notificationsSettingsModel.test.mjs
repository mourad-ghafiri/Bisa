/**
 * The Notifications card's words: the master first, then the categories in
 * the platform's order, every registry key a real one; the permission is
 * macOS's and reads as such beside our switches. Run with
 * `node --test desktop/src/views/_settings/notificationsSettingsModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { CATEGORY_KEYS, MASTER_KEY, PERMISSIONS, askVerb, iconWords, permissionOf, permissionWords } from "./notificationsSettingsModel.mjs";
import { NOTIFY_KEYS } from "../../shell/notificationsModel.mjs";

test("the master first, then one switch per category, each a registry key of the notifications group", () => {
  assert.equal(MASTER_KEY, "notifications.enabled");
  assert.ok(!CATEGORY_KEYS.includes(MASTER_KEY), "the master is no category");
  assert.deepEqual([...CATEGORY_KEYS], ["notifications.asks", "notifications.failures", "notifications.done", "notifications.workflows", "notifications.addons"]);
  assert.deepEqual(new Set([MASTER_KEY, ...CATEGORY_KEYS]), new Set(Object.values(NOTIFY_KEYS)), "the card draws every switch the model reads, and no other");
  const settings = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  for (const key of [MASTER_KEY, ...CATEGORY_KEYS]) assert.ok(settings.includes(`"${key}",`), `${key} is registered`);
});

test("the permission is macOS's and the master is ours: granted-but-off reads as both, a refusal points at System Settings, and only an unasked permission can be asked", () => {
  assert.equal(permissionWords("granted", true).tone, "ok");
  assert.equal(permissionWords("granted", false).label, "granted · off here");
  assert.match(permissionWords("granted", false).sentence, /nothing is sent/);
  assert.equal(permissionWords("denied", true).tone, "warn");
  assert.match(permissionWords("denied", true).sentence, /System Settings/);
  assert.equal(permissionWords("default", true).tone, "neutral");
  assert.match(permissionWords("default", true).sentence, /Allow/);
  assert.equal(permissionWords("unavailable", true).tone, "quiet");
  assert.match(permissionWords("unavailable", true).sentence, /desktop app/);
  assert.equal(permissionWords(null, true).label, "checking…");
  for (const p of PERMISSIONS) assert.ok(permissionWords(p, true).sentence.length > 10);
  assert.equal(askVerb("default"), "Allow");
  assert.equal(askVerb("granted"), null);
  assert.equal(askVerb("denied"), null, "after a refusal only System Settings can change the answer");
  assert.equal(askVerb(null), null);
});

test("the icon fact names the development build and Terminal", () => {
  assert.match(iconWords(), /development build/);
  assert.match(iconWords(), /Terminal/);
});

test("a refusal is read as a refusal when the panel opens again — never as not asked yet, with a button that cannot change it", () => {
  assert.equal(permissionOf(true, "default"), "granted", "the plugin's word");
  assert.equal(permissionOf(true, "denied"), "granted");
  assert.equal(permissionOf(false, "granted"), "granted", "the answer to the question just asked");
  assert.equal(permissionOf(false, "denied"), "denied");
  assert.equal(permissionOf(false, "default"), "default");
  for (const odd of [null, undefined, "", "prompt", 7]) assert.equal(permissionOf(false, odd), "default", `${JSON.stringify(odd)}: asking is what finds out`);
  for (const p of PERMISSIONS) assert.equal(permissionOf(p === "granted", p), p);
  // What the card then offers: *Allow* only where asking can change the answer.
  assert.equal(askVerb(permissionOf(false, "denied")), null);
  assert.match(permissionWords(permissionOf(false, "denied"), true).sentence, /System Settings/);
  assert.equal(askVerb(permissionOf(false, "default")), "Allow");
  const panel = readFileSync(new URL("./SystemPanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes("permissionOf(await n.isPermissionGranted(), typeof Notification === \"undefined\" ? null : Notification.permission)"), "the read asks the webview's own word too");
  assert.ok(panel.includes("setPermission(permissionOf(false, answer))") && !panel.includes('answer === "denied" ? "denied"'), "and the answer to *Allow* is read by the same rule");
});
