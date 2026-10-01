/**
 * What the governance panel writes. Run with
 * `node --test desktop/src/views/_work/governanceModel.test.mjs`.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";

import { GATES, POLICIES, governanceBody, policyBody } from "./governanceModel.mjs";

/** The definitions the node's types are generated into — what the desktop's types are made from. */
const definitions = JSON.parse(readFileSync(new URL("../../../api-schema.json", import.meta.url), "utf8")).definitions;

test("the body names the three gates, each with its policy, and nothing else", () => {
  const draft = { approval: { policy: "owner" }, escalation: { policy: "members" }, publish: { policy: "listed", pubkeys: ["ab".repeat(32), "team:01TEAM"] } };
  assert.deepEqual(governanceBody(draft), {
    approval: { policy: "owner" },
    escalation: { policy: "members" },
    publish: { policy: "listed", pubkeys: ["ab".repeat(32), "team:01TEAM"] },
  });
});

test("a draft that carries more than the gates leaves it behind: the answer's other keys, the panel's own, a policy's stray entries", () => {
  const draft = {
    approval: { policy: "admins", pubkeys: ["ab".repeat(32)], label: "Me and the admins" },
    escalation: { policy: "listed", pubkeys: [" team:01TEAM ", "", 7, "cd".repeat(32)], touched: true },
    publish: { policy: "owner" },
    updated_at: 1700000000,
    dirty: true,
  };
  assert.deepEqual(governanceBody(draft), {
    approval: { policy: "admins" },
    escalation: { policy: "listed", pubkeys: ["team:01TEAM", "cd".repeat(32)] },
    publish: { policy: "owner" },
  });
});

test("a gate the draft does not hold, or holds under a word nobody knows, is the owner's — the platform's default", () => {
  assert.deepEqual(governanceBody({ approval: { policy: "everyone" } }), { approval: { policy: "owner" }, escalation: { policy: "owner" }, publish: { policy: "owner" } });
  assert.deepEqual(governanceBody(null), { approval: { policy: "owner" }, escalation: { policy: "owner" }, publish: { policy: "owner" } });
  assert.deepEqual(policyBody({ policy: "listed" }), { policy: "listed", pubkeys: [] }, "a list nobody filled is an empty list");
});

test("the gates and the policies are the node's own: the body's fields and the policy's words, as the schema says them", () => {
  const body = definitions.GovernanceBody;
  assert.deepEqual(Object.keys(body.properties), [...GATES], "`GovernanceBody` takes exactly the gates");
  assert.equal(body.additionalProperties, false, "and refuses a key that is none of them");
  const policies = definitions.GatePolicy.oneOf;
  assert.deepEqual(policies.map((policy) => policy.properties.policy.const), [...POLICIES], "`GatePolicy`'s words, in its order");
  assert.deepEqual(policies.map((policy) => Object.keys(policy.properties)), [["policy"], ["policy"], ["policy"], ["policy", "pubkeys"]], "the entries ride under `pubkeys`, on a list alone");
});
