/**
 * Your profile as Settings › Identity edits it. Run with
 * `node --test desktop/src/views/_settings/identityModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { ownerRow, profileDraft, profileWords } from "./identityModel.mjs";

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
