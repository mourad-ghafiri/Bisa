import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  expiryWords,
  inviteOffer,
  inviteState,
  matrixRows,
  orderInvites,
  permissionLabel,
  personName,
  personWords,
  roleChangeWords,
  roleLabel,
  admitKey,
} from "./peopleModel.mjs";

test("roles and permissions have words, and unknown ones read as themselves", () => {
  assert.equal(roleLabel("guest"), "Guest");
  assert.equal(roleLabel("nope"), "nope");
  assert.equal(permissionLabel("open_dms"), "Open direct messages");
  assert.equal(permissionLabel("zzz"), "zzz");
});

test("the matrix becomes one row per permission with a cell per role", () => {
  const { roles, rows } = matrixRows({
    roles: [
      { role: "owner", words: "", permissions: ["read_workspace", "open_dms"] },
      { role: "guest", words: "", permissions: ["open_dms"] },
    ],
    permissions: [
      { permission: "read_workspace", words: "read" },
      { permission: "open_dms", words: "dms" },
    ],
  });
  assert.deepEqual(roles, ["owner", "guest"]);
  assert.deepEqual(rows.map((r) => r.holds), [[true, false], [true, true]]);
  assert.deepEqual(matrixRows(null), { roles: [], rows: [] });
});

test("a person reads by label, else by the key's first letters, with a second line", () => {
  assert.equal(personName({ pubkey: "ab".repeat(32), label: " Bob " }), "Bob");
  assert.equal(personName({ pubkey: "ab".repeat(32), label: null }), "abababab…");
  assert.equal(personName(null), "someone");
  assert.equal(personWords({ role: "guest", channels: ["design", "ops"] }), "on 2 channels");
  assert.equal(personWords({ role: "guest", channels: ["design"], client: "mobile" }), "on #design · from the mobile");
  assert.equal(personWords({ role: "guest", channels: [] }), "on no channel yet");
  assert.equal(personWords({ role: "member" }), "");
});

test("a role change and a removal say what they mean", () => {
  assert.equal(roleChangeWords("guest", "guest"), null);
  assert.match(roleChangeWords("guest", "member"), /Every standing channel opens/);
  assert.match(roleChangeWords("admin", "member"), /lose the channels and the people/);
  assert.match(roleChangeWords("member", "guest"), /only the channels/);
  assert.match(roleChangeWords("member", "admin"), /invite, promote and remove/);
});

test("invitations have a state in words, an order, an offer and a life", () => {
  assert.deepEqual(inviteState({ state: { state: "requested" } }), { word: "waiting on you", tone: "warn", open: true });
  assert.equal(inviteState({ state: { state: "revoked" } }).word, "withdrawn");
  assert.equal(inviteState(null).word, "pending");
  assert.deepEqual(["pending", "accepted", "refused", "revoked", "expired"].map((state) => inviteState({ state: { state } }).word), ["pending", "accepted", "refused", "withdrawn", "expired"]);
  // Every state's word is the catalog's: the model spells none of its own.
  const model = readFileSync(new URL("./peopleModel.mjs", import.meta.url), "utf8");
  assert.ok(!/word: "[a-z]+"/.test(model) && !model.includes('"someone"') && !model.includes("`on #"), "no English word outside the catalog");
  const ordered = orderInvites([
    { id: "a", created_at: 1, state: { state: "accepted" } },
    { id: "b", created_at: 2, state: { state: "pending" } },
    { id: "c", created_at: 3, state: { state: "pending" } },
  ]);
  assert.deepEqual(ordered.map((i) => i.id), ["c", "b", "a"]);
  assert.equal(inviteOffer({ role: "guest", channels: ["design", "ops"] }), "Guest on #design, #ops");
  assert.equal(inviteOffer({ role: "guest", channels: [] }), "Guest on no channel yet");
  assert.equal(inviteOffer({ role: "member", channels: [] }), "Member");
  assert.equal(expiryWords(24), "1 day");
  assert.equal(expiryWords(6), "6 hours");
  assert.equal(expiryWords(0), "");
});

test("a key admitted by hand is 64 hex characters and nobody already here", () => {
  const me = "a".repeat(64);
  const people = [{ pubkey: "B".repeat(64) }];
  assert.deepEqual(admitKey("", people, me), { key: "", problem: "" });
  assert.equal(admitKey("not-a-key", people, me).problem, "A key is 64 hexadecimal characters.");
  assert.equal(admitKey("c".repeat(63), people, me).problem, "A key is 64 hexadecimal characters.");
  assert.equal(admitKey(` ${"A".repeat(64)} `, people, me).problem, "That is your own key.");
  assert.equal(admitKey("b".repeat(64), people, me).problem, "Already a member.", "case does not make a second person");
  assert.deepEqual(admitKey("C".repeat(64), people, me), { key: "c".repeat(64), problem: null });
  // The panel says the model's own sentence under the field — the one that is true of the key typed — and decides nothing from its words.
  const panel = readFileSync(new URL("./PeoplePanel.tsx", import.meta.url), "utf8");
  assert.ok(panel.includes('hint={keyProblem || tr("settings-people-panel-somebody-who-already-knows-relays-invitation")}'), "a key that is yours, or already a member's, is told so — it once read as no hex key at all");
  assert.ok(panel.includes("disabled={keyProblem !== null || adding}"), "nothing typed, or a key with a problem, admits nobody");
  assert.ok(!/keyProblem\.includes\(/.test(panel), "a sentence of the catalog is never matched for a word");
  // A read that refused is said where its answer goes: *None made yet* and *None yet* are claims only an answer makes.
  assert.ok(panel.includes('{phase(invites) === "failed" && invites.error ? (') && panel.includes("<ErrorNote error={invites.error} retry={invites.reload} />"));
  assert.ok(panel.includes('{phase(hosts) === "failed" && hosts.error ? (') && panel.includes("<ErrorNote error={hosts.error} retry={hosts.reload} />"));
});
