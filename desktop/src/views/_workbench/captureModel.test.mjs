/**
 * Marking a device's screen for an agent (ide/19). Run with
 * `node --test desktop/src/views/_workbench/captureModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { EMPTY_CAPTURE_DRAFT, addCapture, captureChips, captureLabel, capturesContent, capturesKey, capturesTarget, markWords, removeCapture, setCaptureMessage } from "./captureModel.mjs";
import { chipLabel, fitsBudget, sameChip } from "./contextChips.mjs";

const shot = (n) => ({ sha256: String(n).repeat(64).slice(0, 64), name: `mobile-A-${n}.png`, mime: "image/png", size: 1000 });

test("a capture is added with the next number and its picture, removed with the numbers kept, and a note with no words adds nothing", () => {
  let draft = addCapture(EMPTY_CAPTURE_DRAFT, { shot: shot(1), mark: { x: 120, y: 340, width: 200, height: 48 }, label: "iPhone 16", width: 1170, height: 2532 }, " make the button blue ");
  assert.equal(draft.captures.length, 1);
  assert.equal(draft.captures[0].id, 1);
  assert.equal(draft.captures[0].note, "make the button blue", "trimmed");
  assert.equal(draft.seq, 2);
  assert.equal(addCapture(draft, { shot: shot(2), mark: null, label: "iPhone 16" }, "   "), draft, "no words, nothing added");
  draft = addCapture(draft, { shot: shot(2), mark: null, label: "iPhone 16" }, "too dark");
  assert.equal(draft.captures[1].mark, null, "the whole screen");
  assert.equal(draft.captures[1].width, null);
  const fewer = removeCapture(draft, 1);
  assert.deepEqual(fewer.captures.map((c) => c.id), [2]);
  assert.equal(fewer.seq, 3, "numbers are never reused");
  assert.equal(removeCapture(draft, 99), draft);
  assert.equal(setCaptureMessage(draft, "please").message, "please");
});

test("the captures become one chip each carrying the picture and the mark, the same spot twice is one chip, and the request is an edit about the marked screen", () => {
  let draft = addCapture(EMPTY_CAPTURE_DRAFT, { shot: shot(1), mark: { x: 120, y: 340, width: 200, height: 48 }, label: "iPhone 16" }, "blue");
  draft = addCapture(draft, { shot: shot(2), mark: null, label: "iPhone 16" }, "too dark");
  const chips = captureChips("AAAA-1", "iPhone 16", draft.captures);
  assert.equal(chips.length, 2);
  assert.equal(chips[0].kind, "capture");
  assert.equal(chips[0].device, "AAAA-1");
  assert.deepEqual(chips[0].mark, { x: 120, y: 340, width: 200, height: 48 });
  assert.equal(chips[0].shot.sha256, shot(1).sha256);
  assert.equal(chips[1].mark, null);
  assert.ok(fitsBudget(chips), "a capture is a reference, never bytes");
  assert.equal(chipLabel(chips[0]), "iPhone 16 · 200×48");
  assert.equal(chipLabel(chips[1]), "iPhone 16 · screen");
  assert.ok(sameChip(chips[0], { ...chips[0], note: "green" }), "the same spot of the same picture is one chip, its note the newer");
  assert.ok(!sameChip(chips[0], chips[1]));
  assert.equal(capturesTarget(1, "iPhone 16"), "the marked spot on the iPhone 16 screen");
  assert.equal(capturesTarget(2, "iPhone 16"), "the 2 marked spots on the iPhone 16 screen");
  assert.ok(capturesContent("", 1).startsWith("The captured screen is attached as a chip"));
  assert.ok(capturesContent("please", 2).startsWith("please\n\nEach of the 2 captured screens"));
  assert.ok(capturesContent("", 1).includes("Hot reload"));
  assert.equal(capturesKey("workstream:w1", "AAAA-1"), "workstream:w1|captures|AAAA-1");
});

test("a mark's words name the whole screen or the rectangle", () => {
  assert.equal(markWords(null), "the whole screen");
  assert.equal(markWords({ x: 120, y: 340, width: 200, height: 48 }), "x 120, y 340 · 200×48");
  assert.equal(captureLabel(2, { id: 2, mark: null, note: "too dark", shot: shot(1), label: "iPhone 16", width: null, height: null }), "2 · the whole screen — too dark");
});
