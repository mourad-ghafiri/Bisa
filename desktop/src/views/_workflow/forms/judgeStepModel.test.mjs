/**
 * A `judge` step as its form edits it (15 §The judge step): its options and
 * what each means, `otherwise`, and how sure a pick must be — a number from
 * 0 to 1, or none for the workspace's own bar. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/views/_workflow/forms/judgeStepModel.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { MIN_OPTIONS, addOption, confidenceText, judgeWords, readConfidence, removeOption, setConfidence, setMeaning } from "./judgeStepModel.mjs";
import { blankStep } from "../stepKinds.mjs";
import { relabelBranch } from "../workflowGraph.mjs";

const core = readFileSync(new URL("../../../../../crates/bisa-core/src/workflow.rs", import.meta.url), "utf8");

const triage = () => ({
  ...blankStep("judge", "urgent"),
  state: "{steps.summarize.output.report}",
  instructions: "Is this ticket urgent enough to page someone right now?",
  options: [
    { branch: "page", meaning: "an outage, data loss, or a payment failure affecting customers now" },
    { branch: "business_hours", meaning: "a real bug with no immediate harm" },
  ],
  otherwise: "business_hours",
  then: [{ to: "page-oncall", branch: "page" }, { to: "open-ticket", branch: "business_hours" }],
});

test("how sure a pick must be is a number from 0 to 1 — the core's rule — or none, and the workspace's bar applies", () => {
  assert.ok(core.replace(/\s+/g, " ").includes("min_confidence: Some(c), .. } if !(0.0..=1.0).contains(c)"), "the core refuses a confidence outside 0 to 1");
  assert.deepEqual(readConfidence(""), { ok: true, value: null });
  assert.deepEqual(readConfidence("   "), { ok: true, value: null });
  assert.deepEqual(readConfidence("0.8"), { ok: true, value: 0.8 });
  assert.deepEqual(readConfidence("0,8"), { ok: true, value: 0.8 }, "a comma is a decimal mark");
  assert.deepEqual(readConfidence("0"), { ok: true, value: 0 });
  assert.deepEqual(readConfidence("1"), { ok: true, value: 1 });
  for (const wrong of ["1.2", "-0.1", "80", "sure", "NaN", "Infinity"]) {
    assert.deepEqual(readConfidence(wrong), { ok: false, reason: "A confidence is a number from 0 to 1 — leave it blank for the workspace's own." }, wrong);
  }
});

test("a confidence the form refuses never reaches the step; a blank one takes the field off it", () => {
  const step = triage();
  const set = setConfidence(step, "0.8");
  assert.deepEqual([set.ok, set.step.min_confidence], [true, 0.8]);
  assert.equal(confidenceText(set.step), "0.8");
  const refused = setConfidence(set.step, "8");
  assert.equal(refused.ok, false);
  assert.equal("step" in refused, false, "nothing to write");
  const cleared = setConfidence(set.step, "");
  assert.equal(cleared.ok, true);
  assert.equal("min_confidence" in cleared.step, false, "absent, as the core writes it — never null on the wire");
  assert.equal(confidenceText(step), "");
  assert.equal(confidenceText({ ...step, min_confidence: null }), "");
  assert.equal(setConfidence(set.step, "0.8").step, set.step, "the same bar is no edit");
});

test("an option is a branch and what choosing it means; a new one takes a name the step has for nothing else", () => {
  let step = blankStep("judge", "urgent");
  assert.deepEqual(step.options, []);
  step = addOption(addOption(step));
  assert.deepEqual(step.options, [{ branch: "branch-1", meaning: "" }, { branch: "branch-2", meaning: "" }]);
  step = setMeaning(step, 0, "an outage affecting customers now");
  assert.deepEqual(step.options[0], { branch: "branch-1", meaning: "an outage affecting customers now" });
  assert.equal(setMeaning(step, 9, "x"), step, "a row that is not there writes nothing");
  // `otherwise` holds a name too: a new option never takes it.
  assert.equal(addOption({ ...step, otherwise: "branch-3" }).options[2].branch, "branch-4");
  // Renamed through the one rule, an option carries its flow.
  const renamed = relabelBranch(triage(), "page", "page_now");
  assert.equal(renamed.ok, true);
  assert.deepEqual(renamed.step.options[0].branch, "page_now");
  assert.deepEqual(renamed.step.then[0], { to: "page-oncall", branch: "page_now" });
  // Removed, its flow is the validator's to report — never silently rewritten.
  const removed = removeOption(triage(), 0);
  assert.deepEqual(removed.options.map((o) => o.branch), ["business_hours"]);
  assert.deepEqual(removed.then, triage().then);
  assert.equal(removeOption(triage(), 7).options.length, 2);
});

test("what the form says of the step: a judgement chooses between at least two, and what is taken when it is not sure", () => {
  assert.equal(MIN_OPTIONS, 2);
  assert.ok(core.includes("StepKind::Judge { options, .. } if options.len() < 2"), "the core's bound");
  assert.deepEqual(judgeWords(triage()), { options: null, otherwise: "When it is not sure enough, or gives no answer that holds, the run takes business_hours." });
  assert.equal(judgeWords({ ...triage(), options: [triage().options[0]] }).options, "A judgement chooses between at least two options: add 1 more.");
  assert.equal(judgeWords(blankStep("judge", "j")).options, "A judgement chooses between at least two options: add 2 more.");
  const form = readFileSync(new URL("./JudgeStepForm.tsx", import.meta.url), "utf8");
  assert.ok(form.includes("setConfidence(step, ") && form.includes("judgeWords(step)") && form.includes("addOption(step)"), "the form asks the model");
  assert.equal(/Number\(/.test(form), false, "and reads no number of its own");
});
