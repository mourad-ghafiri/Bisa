import test from "node:test";
import assert from "node:assert/strict";

import { groupPhoto, renameGroupMeta, withGroupPhoto } from "./railGroupsModel.mjs";

const ref = (sha) => ({ sha256: sha, name: `${sha}.png`, mime: "image/png", size: 10 });

test("groupPhoto reads a group's photo, else null", () => {
  const meta = { Shop: { photo: ref("aa") } };
  assert.deepEqual(groupPhoto(meta, "Shop"), ref("aa"));
  assert.equal(groupPhoto(meta, "Admin"), null, "a group with no entry");
  assert.equal(groupPhoto(null, "Shop"), null, "no map at all");
});

test("withGroupPhoto sets, replaces and removes, dropping an empty entry", () => {
  const set = withGroupPhoto({}, "Shop", ref("aa"));
  assert.deepEqual(set, { Shop: { photo: ref("aa") } });

  const replaced = withGroupPhoto(set, "Shop", ref("bb"));
  assert.deepEqual(replaced, { Shop: { photo: ref("bb") } });

  const removed = withGroupPhoto(replaced, "Shop", null);
  assert.deepEqual(removed, {}, "removing the only field drops the group's entry");

  // Other groups are untouched, and the input is not mutated.
  const two = { Shop: { photo: ref("aa") }, Admin: { photo: ref("cc") } };
  const one = withGroupPhoto(two, "Shop", null);
  assert.deepEqual(one, { Admin: { photo: ref("cc") } });
  assert.deepEqual(two, { Shop: { photo: ref("aa") }, Admin: { photo: ref("cc") } }, "input unchanged");
});

test("renameGroupMeta moves an entry and drops the old key", () => {
  const meta = { Shop: { photo: ref("aa") }, Admin: { photo: ref("cc") } };
  assert.deepEqual(renameGroupMeta(meta, "Shop", "Store"), { Store: { photo: ref("aa") }, Admin: { photo: ref("cc") } });
  // Merges onto an existing target, old wins on conflict.
  assert.deepEqual(renameGroupMeta({ A: { photo: ref("aa") }, B: {} }, "A", "B"), { B: { photo: ref("aa") } });
  // No-ops: same name, or no entry to move — a plain copy.
  assert.deepEqual(renameGroupMeta(meta, "Shop", "Shop"), meta);
  assert.deepEqual(renameGroupMeta(meta, "Gone", "New"), meta);
  assert.deepEqual(renameGroupMeta(null, "a", "b"), {});
});
