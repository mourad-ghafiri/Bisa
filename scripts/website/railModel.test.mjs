/**
 * The tour's rail (scripts/website/railModel.mjs, bundled into the served
 * script): the stop being read, each chip's state — the gate marked as
 * waiting on you until it is reached — the counter and the reading line.
 * Run with `node --test scripts/website/railModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { STATES, chipStates, counterWords, currentStep, nextSlide, previousSlide, progress, slideAt } from "./railModel.mjs";

test("the slide in view is the nearest whole slide, and the next one comes round after the last", () => {
  assert.equal(slideAt(0, 600, 7), 0);
  assert.equal(slideAt(1200, 600, 7), 2);
  assert.equal(slideAt(1490, 600, 7), 2, "mid-scroll, the nearer one");
  assert.equal(slideAt(1520, 600, 7), 3);
  assert.equal(slideAt(99999, 600, 7), 6, "past the end is the last");
  assert.equal(slideAt(-50, 600, 7), 0);
  assert.equal(slideAt(300, 0, 7), 0, "a track not laid out yet is on the first");
  assert.equal(slideAt(300, 600, 0), 0);
  assert.equal(nextSlide(0, 7), 1);
  assert.equal(nextSlide(6, 7), 0, "after the last, the first");
  assert.equal(nextSlide(0, 1), 0, "one slide stays");
  assert.equal(nextSlide(-1, 3), 0);
  assert.equal(previousSlide(3, 7), 2);
  assert.equal(previousSlide(0, 7), 6, "before the first, the last");
  assert.equal(previousSlide(0, 1), 0, "one slide stays");
});

test("the stop being read is the last whose top has passed the reading line — none before the first", () => {
  assert.equal(currentStep([100, 900, 1800], 360), 0, "before the second, the first");
  assert.equal(currentStep([-800, 200, 1100], 360), 1);
  assert.equal(currentStep([-3000, -2000, -900], 360), 2, "past every top, the last");
  assert.equal(currentStep([500, 900], 360), -1, "the page's opening, above the tour");
  assert.equal(currentStep([], 360), -1);
  assert.equal(currentStep([Number.NaN, 100], 360), 1, "a position that is not a number is skipped");
});

test("each chip wears its state: done behind, running where you read, pending ahead — and the gate marked as waiting until you reach it", () => {
  assert.deepEqual(STATES, ["pending", "running", "done", "waiting"]);
  assert.deepEqual(chipStates(8, 0), ["running", "pending", "pending", "pending", "pending", "pending", "pending", "waiting"], "the tour opens on its first stop, the gate only marked ahead");
  assert.deepEqual(chipStates(8, 3), ["done", "done", "done", "running", "pending", "pending", "pending", "waiting"]);
  assert.deepEqual(chipStates(8, 7), ["done", "done", "done", "done", "done", "done", "done", "running"], "reached, the gate is the one being read");
  assert.deepEqual(chipStates(3, 99), ["done", "done", "running"], "past the end is the end");
  assert.deepEqual(chipStates(3, -1), ["pending", "pending", "waiting"], "above the tour nothing runs, and the gate already waits");
  assert.deepEqual(chipStates(0, 0), []);
});

test("the counter names the stop, says nothing above the tour, and never a stop that is not there", () => {
  assert.equal(counterWords(0, 12), "1 of 12");
  assert.equal(counterWords(11, 12), "12 of 12");
  assert.equal(counterWords(30, 12), "12 of 12");
  assert.equal(counterWords(-1, 12), "");
  assert.equal(counterWords(0, 0), "1 of 1");
});

test("the reading line runs from 0 to 1 over the room the page has to scroll", () => {
  assert.equal(progress(0, 5000, 1000), 0);
  assert.equal(progress(2000, 5000, 1000), 0.5);
  assert.equal(progress(4000, 5000, 1000), 1);
  assert.equal(progress(9000, 5000, 1000), 1);
  assert.equal(progress(-50, 5000, 1000), 0);
  assert.equal(progress(0, 800, 1000), 1, "a page shorter than the window is read whole");
});
