import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { ADOPT_SUBJECT, adoptAction, bandActions, isAdoptProposal, namesAdoption } from "./proposalRouting.mjs";

const adopt = { gate_id: "g1", subject: "adopt:wf1@2", proposal: { name: "Plan", steps: [{ id: "s1" }] } };
const adoptNoProposal = { gate_id: "g2", subject: "adopt:wf1@2", proposal: null };
const question = { gate_id: "g3", subject: "ask_human:clarify", question: "which one?" };
const permission = { gate_id: "g4", subject: "permission:Bash" };

test("an adopt action with a proposal is the plan card; one without is not", () => {
  assert.equal(isAdoptProposal(adopt), true);
  assert.equal(isAdoptProposal(adoptNoProposal), false, "no proposal, no plan to show");
  assert.equal(isAdoptProposal(question), false);
  assert.equal(isAdoptProposal(null), false);
});

test("the one proposal is found, and the band shows everything else", () => {
  const actions = [question, adopt, permission];
  assert.equal(adoptAction(actions), adopt);
  assert.deepEqual(bandActions(actions), [question, permission], "the proposal is hosted centrally, not in the band");
  assert.equal(adoptAction([question, permission]), null, "no proposal pending");
  assert.deepEqual(bandActions([]), []);
  assert.equal(adoptAction(null), null);
});

test("the adoption's prefix is read in one place: a subject names an adoption or it does not, and no screen spells the prefix", () => {
  assert.equal(ADOPT_SUBJECT, "adopt:");
  assert.equal(namesAdoption("adopt:01WF@3"), true);
  for (const other of ["amend:01RUN@2", "approval:01RUN/review", "adoption", "", null, undefined, 7]) assert.equal(namesAdoption(other), false);
  const read = (name) => readFileSync(new URL(name, import.meta.url), "utf8");
  for (const file of ["./YourMoveBand.tsx", "../_studio/NeedsAction.tsx", "../_studio/inboxModel.mjs", "./goalPageModel.mjs", "../GoalDetail.tsx"]) {
    assert.ok(!read(file).includes('"adopt:"'), `${file} spells no prefix`);
  }
  assert.ok(read("./YourMoveBand.tsx").includes("namesAdoption(a.subject)"), "the band asks for the adoption's inputs by the one rule");
});
