/**
 * A remembered choice never throws: no storage, no value, a value that does
 * not parse, a storage that refuses — each is the default or a held choice.
 * Run with `node --test desktop/src/shell/storedPrefModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { forgetPref, forgetPrefsUnder, jsonPref, readPref, switchPref, switchWord, webStorage, writePref } from "./storedPrefModel.mjs";

/** A storage in memory, with a switch to refuse every call. */
function memory(refuse = false) {
  const m = new Map();
  const bar = () => {
    if (refuse) throw new Error("denied");
  };
  return {
    map: m,
    getItem: (k) => (bar(), m.has(k) ? m.get(k) : null),
    setItem: (k, v) => (bar(), void m.set(k, String(v))),
    removeItem: (k) => (bar(), void m.delete(k)),
    get length() {
      bar();
      return m.size;
    },
    key: (i) => (bar(), [...m.keys()][i] ?? null),
  };
}

test("a read is the parsed value, else the fallback — for no storage, no value, a parse that throws, a parse that answers nothing, or a storage that refuses", () => {
  const s = memory();
  s.setItem("k", '{"a":1}');
  assert.deepEqual(readPref(s, "k", jsonPref, {}), { a: 1 });
  assert.deepEqual(readPref(null, "k", jsonPref, { d: true }), { d: true }, "no storage");
  assert.deepEqual(readPref(undefined, "k", jsonPref, 0), 0);
  assert.equal(readPref(s, "missing", jsonPref, "fallback"), "fallback", "no value");
  s.setItem("bad", "{not json");
  assert.equal(readPref(s, "bad", jsonPref, "fallback"), "fallback", "a parse that throws");
  s.setItem("word", "maybe");
  assert.equal(readPref(s, "word", switchPref, true), true, "a parse that answers nothing is the fallback");
  assert.equal(readPref(memory(true), "k", jsonPref, "fallback"), "fallback", "a storage that refuses");
  s.setItem("empty", "");
  assert.equal(readPref(s, "empty", (raw) => raw, "fallback"), "", "an empty string is a value, not an absence");
});

test("a write keeps text as it is and anything else as JSON, forgets on null or undefined, and says whether the storage took it", () => {
  const s = memory();
  assert.equal(writePref(s, "t", "words"), true);
  assert.equal(s.map.get("t"), "words");
  assert.equal(writePref(s, "j", { a: [1] }), true);
  assert.equal(s.map.get("j"), '{"a":[1]}');
  assert.equal(writePref(s, "n", 3), true);
  assert.equal(s.map.get("n"), "3");
  assert.equal(writePref(s, "t", null), true);
  assert.equal(s.map.has("t"), false, "null forgets");
  assert.equal(writePref(s, "j", undefined), true);
  assert.equal(s.map.has("j"), false, "undefined forgets");
  assert.equal(forgetPref(s, "n"), true);
  assert.equal(s.map.has("n"), false);
  assert.equal(writePref(null, "k", "v"), false, "no storage: not taken, no throw");
  assert.equal(writePref(memory(true), "k", "v"), false, "a storage that refuses: not taken, no throw");
  assert.equal(forgetPref(memory(true), "k"), false);
});

test("every key under a prefix is forgotten and no other, and a storage that refuses leaves what it holds", () => {
  const s = memory();
  for (const key of ["bisa:draft:channel:a", "bisa:draft:dm:b", "bisa:draft:", "bisa:drafts", "bisa.theme", "draft:bisa:draft:c"]) writePref(s, key, "words");
  assert.equal(forgetPrefsUnder(s, "bisa:draft:"), 3);
  assert.deepEqual([...s.map.keys()], ["bisa:drafts", "bisa.theme", "draft:bisa:draft:c"], "a key that only looks like it stays, and so does one that holds the prefix further in");
  assert.equal(forgetPrefsUnder(s, "bisa:draft:"), 0, "nothing left under it");
  assert.equal(forgetPrefsUnder(s, ""), 0, "no prefix is no sweep: the whole storage is never forgotten by this door");
  assert.equal(s.map.size, 3);
  // Another store's keys under the same prefix are spared by name.
  for (const key of ["bisa:draft:channel:a", "bisa:draft:note:n1", "bisa:draft:note:n2", "bisa:draft:notes"]) writePref(s, key, "words");
  assert.equal(forgetPrefsUnder(s, "bisa:draft:", ["bisa:draft:note:", ""]), 2, "the channel's and the one that only looks like a note's");
  assert.deepEqual([...s.map.keys()].filter((k) => k.startsWith("bisa:draft:")), ["bisa:draft:note:n1", "bisa:draft:note:n2"]);
  for (const key of ["bisa:draft:note:n1", "bisa:draft:note:n2"]) forgetPref(s, key);
  assert.equal(forgetPrefsUnder(null, "bisa:draft:"), 0, "no storage, no throw");
  const refusing = memory(true);
  refusing.map.set("bisa:draft:x", "words");
  assert.equal(forgetPrefsUnder(refusing, "bisa:draft:"), 0);
  assert.equal(refusing.map.size, 1);
});

test("a switch is the two words and nothing else; under node there is no web storage", () => {
  assert.equal(switchPref("1"), true);
  assert.equal(switchPref("0"), false);
  assert.equal(switchPref("true"), undefined, "an unknown word falls to the fallback");
  assert.equal(switchWord(true), "1");
  assert.equal(switchWord(false), "0");
  assert.equal(webStorage(), null, "no window here — and nothing throws");
});
