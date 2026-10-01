/**
 * The System panel's words agree with the shell's vocabulary and say the two
 * things they owe. Run with `node --test desktop/src/views/_settings/systemPermissionsModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { KINDS, STATUSES, recommendation, requestVerb, settingKeyOf, statusWords, titleOf, unavailableWords } from "./systemPermissionsModel.mjs";

const rust = readFileSync(new URL("../../../src-tauri/src/permissions.rs", import.meta.url), "utf8");
const variants = (name) => {
  const start = rust.indexOf(`pub enum ${name} {`);
  const block = rust.slice(start, rust.indexOf("\n}", start));
  return [...block.matchAll(/^\s+([A-Z][A-Za-z]+),$/gm)].map((m) => m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase());
};

test("the kinds and statuses are the shell's, and each kind has a machine setting and a title", () => {
  assert.deepEqual([...KINDS], variants("PermissionKind"));
  assert.deepEqual([...STATUSES], variants("PermissionStatus"));
  const settings = readFileSync(new URL("../../../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");
  for (const kind of KINDS) {
    assert.ok(settings.includes(`"${settingKeyOf(kind)}",`), `${settingKeyOf(kind)} is a registry key`);
    assert.notEqual(titleOf(kind), kind, `${kind} has a title`);
  }
});

test("the grant is macOS's and the switch is ours: granted-but-off reads as both, and every status has words", () => {
  assert.equal(statusWords("full_disk_access", "granted", false).label, "granted · off here");
  assert.equal(statusWords("full_disk_access", "granted", true).label, "granted");
  assert.equal(statusWords("microphone", "denied", true).tone, "warn");
  assert.match(statusWords("microphone", "denied", true).sentence, /System Settings/);
  assert.match(statusWords("microphone", "not_determined", false).sentence, /asks, once/);
  assert.match(statusWords("full_disk_access", "not_determined", false).sentence, /Check again/);
  assert.equal(statusWords("microphone", "unsupported", false).tone, "quiet");
  assert.equal(statusWords("microphone", null, false).label, "checking…");
  for (const kind of KINDS) for (const status of STATUSES) assert.ok(statusWords(kind, status, true).sentence.length > 10);
});

test("the verb: the pane for Full Disk Access, macOS's one prompt for an unasked microphone, the pane after a refusal, nothing when granted", () => {
  assert.equal(requestVerb("full_disk_access", "denied"), "Open System Settings…");
  assert.equal(requestVerb("full_disk_access", "not_determined"), "Open System Settings…");
  assert.equal(requestVerb("microphone", "not_determined"), "Ask macOS…");
  assert.equal(requestVerb("microphone", "denied"), "Open System Settings…");
  assert.equal(requestVerb("microphone", "granted"), null);
  assert.equal(requestVerb("full_disk_access", "unsupported"), null);
  assert.equal(requestVerb("full_disk_access", null), null);
});

test("the recommendation says productivity and best effort for Full Disk Access, and voice mode later for the microphone", () => {
  const fda = recommendation("full_disk_access");
  assert.match(fda.why, /Recommended for productivity/);
  assert.match(fda.safety, /best-effort basis/);
  assert.match(fda.safety, /not a sandbox/);
  assert.match(fda.how, /Full Disk Access; add Bisa/);
  const mic = recommendation("microphone");
  assert.match(mic.why, /voice mode, which is coming/);
  assert.match(mic.safety, /revokes nothing/);
  assert.equal(mic.how, null);
  assert.match(unavailableWords("not_desktop"), /browser session/);
  assert.match(unavailableWords("not_macos"), /not macOS|this system/);
  assert.equal(unavailableWords("AVFoundation is missing"), "AVFoundation is missing", "the shell's own reason passes through");
});
