/**
 * The start dialog's closing word. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/startRunModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { startedWords } from "./startRunModel.mjs";

test("the word after a start names what happened: adopted, listening, queued, or started", () => {
  const words = new Set();
  for (const did of [
    { adopt: true, listen: true },
    { adopt: true, listen: false, queued: true },
    { adopt: false, listen: true },
    { queued: true },
    {},
  ]) {
    const said = startedWords(did);
    assert.ok(said.length > 0, JSON.stringify(did));
    words.add(said);
  }
  assert.equal(words.size, 5, "five situations, five different words");
  assert.equal(startedWords({ adopt: true, listen: false, queued: true }), startedWords({ adopt: true }), "an adoption never queues");
  assert.equal(startedWords(), startedWords({ queued: false }));
});
