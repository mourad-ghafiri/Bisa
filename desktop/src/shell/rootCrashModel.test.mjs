/**
 * The root crash as a fact, and the card's doors.
 * Run with `node --test desktop/src/shell/rootCrashModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { test } from "node:test";

import { ROOT_CRASH_DOORS, createRootCrash, doorRank, rootCrash, rootCrashDoors, rootCrashWords, rootDoorFailedWords, rootDoorWords } from "./rootCrashModel.mjs";

test("the crash is a fact set while the card is up and clear otherwise", () => {
  const crash = createRootCrash();
  assert.equal(crash.crashed(), false);
  const error = new Error("boom");
  crash.markCrashed(error);
  assert.equal(crash.crashed(), true);
  assert.equal(crash.error(), error);
  crash.clearCrash();
  assert.equal(crash.crashed(), false);
  assert.equal(crash.error(), null);
  crash.markCrashed(undefined);
  assert.equal(crash.crashed(), true, "a crash with no error is still a crash");
  assert.equal(rootCrash.crashed(), false, "the window's own fact starts clear");
});

test("the card's doors: six in the desktop shell, none of them needing the node; two in a browser dev session", () => {
  assert.deepEqual([...ROOT_CRASH_DOORS], ["try_again", "reload", "restart_node", "reveal_log", "open_data_folder", "quit"]);
  assert.deepEqual(rootCrashDoors(true), [...ROOT_CRASH_DOORS]);
  assert.deepEqual(rootCrashDoors(false), ["try_again", "reload"], "the page itself can do these two");
  assert.equal(doorRank("reload"), "primary", "the one most likely to bring the window back");
  assert.equal(doorRank("try_again"), "default");
  assert.equal(doorRank("restart_node"), "default");
  for (const door of ["reveal_log", "open_data_folder", "quit"]) assert.equal(doorRank(door), "ghost", `${door} is quiet`);
  assert.equal(ROOT_CRASH_DOORS.filter((d) => doorRank(d) === "primary").length, 1, "one primary per region");
});

test("the words are the catalog's", () => {
  assert.deepEqual(rootCrashWords(), {
    title: "Bisa hit an error it could not draw around.",
    body: "Nothing durable is lost — goals, journals and settings are on disk. Try again, reload, or restart the node; the details are in the diagnostic log.",
  });
  assert.deepEqual(ROOT_CRASH_DOORS.map(rootDoorWords), ["Try again", "Reload", "Restart the node", "Reveal the log", "Open the data folder", "Quit Bisa"]);
  assert.equal(rootDoorWords("elsewhere"), "elsewhere");
  assert.equal(rootDoorFailedWords("restart_node", new Error("node is external")), "Restart the node did not work: node is external");
});
