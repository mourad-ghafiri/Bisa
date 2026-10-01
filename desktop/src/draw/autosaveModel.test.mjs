import { strict as assert } from "node:assert";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { abandoned, adopted, changed, frameHeard, heardAfterSave, opened, reloadDecision, saveAsked, saveConflicted, saveFailed, saveLanded } from "./autosaveModel.mjs";

test("one save in the air at a time; a stroke during it re-arms one after it lands", () => {
  let s = opened("h0");
  assert.deepEqual(saveAsked(s), { run: false, next: s }, "nothing to save");
  s = changed(s);
  const first = saveAsked(s);
  assert.equal(first.run, true);
  s = first.next;
  assert.ok(s.inFlight && !s.dirty);
  s = changed(s);
  const second = saveAsked(s);
  assert.equal(second.run, false, "one PATCH at a time");
  s = second.next;
  assert.ok(s.again);
  s = saveLanded(s, "h1");
  assert.equal(s.savedHash, "h1");
  assert.ok(!s.inFlight && s.dirty && !s.again, "the stroke that arrived meanwhile is still to save");
  assert.equal(saveAsked(s).run, true);
});

test("a 409 freezes saving until the person adopts theirs; another failure keeps the strokes dirty", () => {
  let s = saveAsked(changed(opened("h0"))).next;
  s = saveConflicted(s);
  assert.ok(s.conflicted && !s.inFlight);
  assert.equal(saveAsked(changed(s)).run, false, "frozen");
  s = adopted(s, "h9");
  assert.deepEqual(s, opened("h9"));
  let f = saveAsked(changed(opened("h0"))).next;
  f = saveFailed(f);
  assert.ok(f.dirty && !f.inFlight && !f.conflicted, "tried again next time");
});

test("a canvas let go of is clean and refuses every later save", () => {
  const s = abandoned(changed(opened("h0")));
  assert.equal(s.dirty, false);
  assert.equal(saveAsked(s).run, false);
  assert.equal(saveAsked(changed(s)).run, false, "a late stroke asks nothing either");
});

test("a changed frame is ignored for our own hash, a reload when clean, a conflict when strokes are unsaved", () => {
  const clean = opened("h0");
  assert.equal(reloadDecision(clean, "h0"), "ignore");
  assert.equal(reloadDecision(clean, "h1"), "reload");
  assert.equal(reloadDecision(changed(clean), "h1"), "conflict");
  assert.equal(reloadDecision(saveAsked(changed(clean)).next, "h1"), "wait", "a save is in the air: the frame may be that save's own echo, and only its answer can tell");
});

test("a frame that outruns the save's own answer is that save's echo, never somebody else drawing", () => {
  // A stroke, the save goes out; the bus says the drawing moved to h1 before the PATCH answers.
  let s = saveAsked(changed(opened("h0"))).next;
  assert.equal(reloadDecision(s, "h1"), "wait");
  s = frameHeard(s, "h1");
  // The PATCH answers: it wrote h1. The frame was its echo — nothing froze, nothing reloads.
  s = saveLanded(s, "h1");
  const echo = heardAfterSave(s);
  assert.equal(echo.decision, "ignore", "the canvas once froze on its own save: somebody drew meanwhile, said of nobody");
  assert.equal(echo.next.heard, null, "and the frame is forgotten");
  assert.ok(!echo.next.conflicted && !echo.next.dirty);
});

test("a frame heard during a save that was not its echo is judged once the save answered", () => {
  // Our save landed h1; the frame heard meanwhile said h2: somebody drew right after it.
  let s = frameHeard(saveAsked(changed(opened("h0"))).next, "h2");
  s = saveLanded(s, "h1");
  assert.equal(heardAfterSave(s).decision, "reload", "clean since: their scene is read");
  // The same, with a stroke of ours since: a conflict, as any frame over unsaved strokes is.
  let d = frameHeard(saveAsked(changed(opened("h0"))).next, "h2");
  d = saveLanded(changed(d), "h1");
  assert.equal(heardAfterSave(d).decision, "conflict");
  // The save failed for another reason — no answer says what stands: the strokes are unsaved over a drawing that moved.
  let f = frameHeard(saveAsked(changed(opened("h0"))).next, "h2");
  f = saveFailed(f);
  assert.equal(heardAfterSave(f).decision, "conflict");
  // The latest frame heard is the one judged; a 409 needs no second word; nothing heard is nothing to judge.
  let two = frameHeard(frameHeard(saveAsked(changed(opened("h0"))).next, "h2"), "h3");
  two = saveLanded(two, "h3");
  assert.equal(heardAfterSave(two).decision, "ignore");
  assert.equal(heardAfterSave(saveConflicted(frameHeard(saveAsked(changed(opened("h0"))).next, "h2"))).decision, "ignore", "frozen already");
  assert.equal(heardAfterSave(saveLanded(saveAsked(changed(opened("h0"))).next, "h1")).decision, "ignore");
  assert.equal(opened("h0").heard, null);
  assert.equal(adopted(frameHeard(opened("h0"), "h5"), "h9").heard, null, "adopting what is there starts clean");
});

test("the editor waits on a frame during a save and judges it when the save answers", () => {
  const editor = readFileSync(new URL("./DrawEditor.tsx", import.meta.url), "utf8");
  assert.ok(editor.includes('else if (decision === "wait") live.current.auto = frameHeard(live.current.auto, e.payload.hash);'));
  const save = editor.slice(editor.indexOf("const save = useCallback(async (): Promise<boolean> => {"), editor.indexOf("/** Save now and say whether the canvas is clean afterwards"));
  assert.ok(save.includes("const late = heardAfterSave(live.current.auto);"), "once the save answered, whatever it was");
  assert.ok(save.indexOf("heardAfterSave(") > save.indexOf("} finally {"), "after the answer landed, on success and on failure alike");
});
