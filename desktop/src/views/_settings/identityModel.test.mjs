/**
 * Your profile as Settings › Identity edits it. Run with
 * `node --test desktop/src/views/_settings/identityModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { ownerRow, profileDraft, profileKey, profileSeed, profileWords, sameProfile } from "./identityModel.mjs";

test("the owner's row is found by the workspace's own key, and the draft is its label and face", () => {
  const me = "ab".repeat(32);
  const face = { sha256: "f".repeat(64), name: "me.jpg", mime: "image/jpeg", size: 900 };
  const ws = { pubkey: me, members: [{ pubkey: "cd".repeat(32), label: "Bob" }, { pubkey: me, label: " Ada ", photo: face }] };
  assert.equal(ownerRow(ws)?.label, " Ada ");
  assert.equal(ownerRow({ pubkey: me, members: [] }), null);
  assert.equal(ownerRow(null), null);
  assert.deepEqual(profileDraft(ownerRow(ws)), { label: "Ada", photo: face });
  assert.deepEqual(profileDraft({ pubkey: me }), { label: "", photo: null }, "a row without a profile opens empty");
  assert.deepEqual(profileDraft(null), { label: "", photo: null });
  assert.match(profileWords(), /travel/);
});

test("a name being typed survives the row read again, and an untouched form follows it", () => {
  const face = { sha256: "f".repeat(64), name: "me.jpg", mime: "image/jpeg", size: 900 };
  const row = { label: "Ada", photo: null };
  const opened = { seeded: profileDraft(row), draft: profileDraft(row) };
  // The same profile in a new object — what every workspace reload hands over.
  assert.equal(profileKey(row), profileKey({ label: " Ada ", photo: null }), "the key is the value, not the object");
  const typing = { ...opened, draft: { label: "Ada Lovel", photo: null } };
  assert.equal(profileSeed(typing, { label: "Ada", photo: null }), typing, "nothing moved: the form is the same form");
  // The row moved elsewhere while the person types: the draft is theirs, the seed is the row's.
  const moved = profileSeed(typing, { label: "Countess", photo: face });
  assert.deepEqual(moved.draft, { label: "Ada Lovel", photo: null });
  assert.deepEqual(moved.seeded, { label: "Countess", photo: face });
  // Untouched, the form follows the row.
  assert.deepEqual(profileSeed(opened, { label: "Countess", photo: face }), { seeded: { label: "Countess", photo: face }, draft: { label: "Countess", photo: face } });
  // A save that landed: the row says what the draft says, and nothing is dirty.
  const saved = profileSeed(typing, { label: "Ada Lovel", photo: null });
  assert.ok(sameProfile(saved.draft, saved.seeded));
  assert.ok(sameProfile({ label: " Ada ", photo: face }, { label: "Ada", photo: { ...face } }), "a face is one by its content");
  assert.ok(!sameProfile({ label: "Ada", photo: face }, { label: "Ada", photo: null }));
  assert.notEqual(profileKey({ label: "Ada", photo: face }), profileKey(row));
});
