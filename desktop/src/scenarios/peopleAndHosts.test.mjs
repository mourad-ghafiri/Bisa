/**
 * Settings › Identity, People and Relays & sync, and a workspace this node
 * is a guest of, as a person lives in them — stepped through the models the
 * panels read: a name typed through a reload, a host's person renamed in an
 * open thread, a message not sent yet, a membership left and then
 * forgotten, a relay typed twice — and source guards that the components
 * ask those models and the node's own doors. No DOM.
 *
 * Run with `node --test desktop/src/scenarios/peopleAndHosts.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { ownerRow, profileDraft, profileKey, profileSeed, sameProfile } from "../views/_settings/identityModel.mjs";
import { inviteRole } from "../views/_settings/peopleModel.mjs";
import { canonicalRelayUrl, checkStanding, healthFor, relayEntry } from "../views/_settings/relayHealthModel.mjs";
import { hostedNameOf, hostedPage, isMember, isUnsent, membershipAction, stateWords } from "../shell/hostedModel.mjs";
import { t } from "../i18n/l10n.mjs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("a name typed in Settings › Identity survives the workspace reading itself again", () => {
  const me = "ab".repeat(32);
  const load = (label) => ({ pubkey: me, members: [{ pubkey: me, label }] });
  let ws = load("Ada");
  let form = { seeded: profileDraft(ownerRow(ws)), draft: profileDraft(ownerRow(ws)) };
  // The person types; a run starts and the shell reads the workspace again — a new object, the same row.
  form = { ...form, draft: { ...form.draft, label: "Ada L" } };
  const before = profileKey(ownerRow(ws));
  ws = load("Ada");
  assert.equal(profileKey(ownerRow(ws)), before, "the effect that seeds the form does not run: nothing moved");
  form = profileSeed(form, ownerRow(ws));
  assert.equal(form.draft.label, "Ada L", "what was typed is still there");
  // They save: the row says what they typed, and the form is clean.
  ws = load("Ada L");
  form = profileSeed(form, ownerRow(ws));
  assert.ok(sameProfile(form.draft, form.seeded));
});

test("a person renamed on a host is renamed in the open thread and the sidebar, and a word not sent yet says so", () => {
  const bob = "cd".repeat(32);
  const section = (label) => ({ host: { state: { state: "member" } }, channels: [], dms: [], directory: [{ pubkey: bob, role: "member", label }] });
  // The thread and the sidebar's direct row read the section's directory — the shell reads it again when the host's side moves.
  assert.equal(hostedNameOf(section("Bob").directory, bob), "Bob");
  assert.equal(hostedNameOf(section("Robert").directory, bob), "Robert");
  assert.equal(hostedNameOf(null, bob), `${bob.slice(0, 8)}…`, "people that could not be read are keys, never a wrong name");

  const page = hostedPage([{ id: "m1", scope: "general", author: bob, at: 1, text: "kept", retracted: false, unsent: true }]);
  assert.ok(isUnsent(page.messages[0]));
});

test("a membership is left while it stands and forgotten once it is over", () => {
  const hosted = (state) => ({ host: { pubkey: "ef".repeat(32), name: "Acme", relays: [] }, role: "guest", state: { state }, requested_at: 1 });
  let h = hosted("member");
  assert.ok(isMember(h));
  assert.equal(membershipAction(h), "leave");
  h = hosted("left");
  assert.equal(membershipAction(h), "forget");
  assert.ok(stateWords(h), "and the row still says where it stands until it is forgotten");
  assert.equal(membershipAction(hosted("requested")), "leave", "a request is withdrawn, not forgotten");
});

test("a relay typed twice is one relay, and a check's word is about the relay as it stood", () => {
  const relays = ["wss://relay.example"];
  assert.equal(relayEntry("WSS://Relay.Example:443/", relays).ok, false);
  assert.equal(relayEntry("wss://other.example", relays).ok, true);
  assert.equal(canonicalRelayUrl("wss://[not-an-address"), null, "what the parser refuses is no relay");
  const report = { relays: [{ url: "wss://relay.example", connected: false }] };
  const kept = { check: { url: relays[0], ok: false }, connected: false, at: 5 };
  assert.equal(checkStanding(kept, healthFor(report, relays[0])), kept);
  report.relays[0].connected = true;
  assert.equal(checkStanding(kept, healthFor(report, relays[0])), null, "it connected since: *not reachable* no longer stands over it");
});

test("the invite dialog opens on the workspace's default role", () => {
  assert.equal(inviteRole("member"), "member");
  assert.equal(inviteRole("admin"), "admin");
  assert.equal(inviteRole(undefined), "guest");
  assert.equal(inviteRole("owner"), "guest", "nobody is invited as an owner");
});

test("a role is said in a whole sentence, in the catalogue's own words", () => {
  for (const [role, words] of [["admin", "an admin"], ["member", "a member"], ["guest", "a guest"], ["owner", "the owner"]]) {
    assert.equal(t("settings-people-panel-words", { role }), `you are ${words}`);
    assert.equal(t("screens-hosted-words", { role, host: "Acme" }), `you are ${words} at Acme`);
    assert.equal(t("settings-people-panel-make", { p: "Bob", to: role }), `Make Bob ${words}?`);
    assert.equal(t("settings-people-panel-now", { p: "Bob", to: role }), `Bob is now ${words}.`);
  }
  assert.equal(t("settings-people-panel-welcomed-channels-sidebar-under-hosted", { answer: "Acme", role: "admin" }), "Welcomed by Acme as an admin. Its channels are in the sidebar under “Hosted by Acme”.");
  assert.equal(t("settings-people-panel-leave-named", { host: "Acme" }), "Leave Acme");
  assert.equal(t("settings-people-panel-forget-named", { host: "Acme" }), "Forget Acme…");
  // No sentence is assembled by lowercasing a label any more.
  for (const file of ["../views/_settings/PeoplePanel.tsx", "../views/Hosted.tsx"]) assert.doesNotMatch(src(file), /roleLabel\([^)]*\)\.toLowerCase\(\)|inviteOffer\([^)]*\)\.toLowerCase\(\)/, file);
});

test("the panels ask the models and the node's own doors", () => {
  const identity = src("../views/_settings/IdentityPanel.tsx");
  assert.match(identity, /profileSeed\(f, me\)/, "the form is re-seeded by the model's rule");
  assert.match(identity, /\}, \[rowKey\]\);/, "…when the row's value moves, not its object");

  const people = src("../views/_settings/PeoplePanel.tsx");
  assert.match(people, /onClose=\{\(\) => setLeaving\(true\)\}/, "Escape, a click outside and the ✕ ask before the invitation is closed");
  assert.match(people, /confirmLabel=\{tr\("settings-people-panel-close-invitation-confirm"\)\}/);
  assert.match(people, /membershipAction\(h\) === "leave"/, "a row offers Leave or Forget by the model's word");
  assert.match(people, /api\.forgetHost\(target\)/);
  for (const door of ["admitInvite", "refuseInvite", "revokeInvite", "releaseMessage", "refuseMessage"]) {
    assert.match(people, new RegExp(`once\\([^)]*\\(\\) =>\\s*attempt\\(\\(\\) => api\\.${door}\\(`), `${door} is asked once a row`);
  }
  assert.match(people, /setRole\(defaultRole\)/);
  assert.match(people, /aria-label=\{tr\("settings-people-panel-leave-named", \{ host: hostName\(h\) \}\)\}/, "a row of Leave buttons names its workspace to a screen reader");
  assert.match(people, /aria-label=\{tr\("settings-people-panel-forget-named", \{ host: hostName\(h\) \}\)\}/);
  assert.match(src("../views/_work/GovernancePanel.tsx"), /matrixNote\(table\)/, "the matrix says what is not offered yet");
  assert.match(people, /if \(t === "people_changed"\) held\.reload\(\)/, "the held list follows the people");

  const relays = src("../views/_settings/RelaysPanel.tsx");
  assert.match(relays, /api\.addRelay\(addUrl\)/);
  assert.match(relays, /api\.removeRelay\(target\)/);
  assert.match(relays, /api\.resetRelays\(\)/);
  assert.doesNotMatch(relays, /setSettings\("machine", \{ \[RELAYS_KEY\]/, "the list is never written back whole from this panel's copy");

  const hosted = src("../views/Hosted.tsx");
  assert.match(hosted, /section\?\.directory/, "the thread reads the people the shell holds");
  assert.doesNotMatch(hosted, /api\.hostedMembers/, "…and never reads them once for itself");
  assert.match(src("../shell/Sidebar.tsx"), /hostedNameOf\(section\.directory, p\)/, "a hosted direct row is named from the host's people");
  assert.match(src("../shell/useWorkspaceData.ts"), /return \{ host, channels, dms, directory \};/);
  assert.match(src("../views/_studio/Chat.tsx"), /isUnsent\(message\) &&/);
});
