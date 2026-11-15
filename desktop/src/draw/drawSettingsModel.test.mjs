import { strict as assert } from "node:assert";
import { test } from "node:test";
import { DEFAULT_SNAPSHOT_WIDTH, MAX_SNAPSHOT_WIDTH, MIN_SNAPSHOT_WIDTH, POLICIES, policyWords, snapshotWidth } from "./drawSettingsModel.mjs";

test("the snapshot width is the setting within bounds, and the default for anything that is not a number", () => {
  assert.equal(snapshotWidth(undefined), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(""), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(true), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth("abc"), DEFAULT_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(10), MIN_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth(99999), MAX_SNAPSHOT_WIDTH);
  assert.equal(snapshotWidth("1920.4"), 1920);
});

test("the three policies each have a sentence", () => {
  assert.deepEqual([...POLICIES], ["everyone", "assigned", "nobody"]);
  const words = POLICIES.map(policyWords);
  assert.equal(new Set(words).size, 3);
  assert.ok(words.every((w) => w.length > 10));
});


test("a delay is said in the reader's language, and a chord as ARIA spells one", async () => {
  const { ariaChord, secondsWords } = await import("./drawSettingsModel.mjs");
  assert.equal(secondsWords(1000, "en"), "1.0s");
  assert.equal(secondsWords(700, "en"), "0.7s");
  assert.match(secondsWords(1500, "fr"), /^1,5\s?s$/, "the decimal mark and the unit are the language's");
  assert.equal(typeof secondsWords(1000), "string", "the runtime's language when none is given");
  assert.equal(ariaChord("Mod+Alt+D", true), "Meta+Alt+D");
  assert.equal(ariaChord("Mod+Alt+D", false), "Control+Alt+D");
  assert.equal(ariaChord("Alt+N", true), "Alt+N");
  assert.equal(ariaChord(null, true), undefined);
});
