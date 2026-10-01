/**
 * What the governance panel writes (`PUT /governance`): who decides each of
 * the three gates. The panel's draft is the node's answer with the person's
 * edits over it; what leaves is built here, from the gates **by name** — the
 * node refuses a body carrying a key it does not know, and a draft passed
 * whole would carry whatever the answer it was seeded from did.
 *
 * Plain `.mjs`, so `node --test` reads it.
 */

/** The three gates, in the order the panel draws them — the keys of the body. */
export const GATES = Object.freeze(["approval", "escalation", "publish"]);

/** The policies a gate may have; the last one names its entries. */
export const POLICIES = Object.freeze(["owner", "admins", "members", "listed"]);

/**
 * One gate's policy as the wire takes it: the word, and for `listed` its
 * entries — pubkeys and `team:<id>` references — under `pubkeys`. A word the
 * platform does not have is the owner's, the default; entries ride with no
 * other word.
 * @param {{policy?: string, pubkeys?: readonly unknown[]} | null | undefined} policy
 * @returns {{policy: "owner" | "admins" | "members"} | {policy: "listed", pubkeys: string[]}}
 */
export function policyBody(policy) {
  const word = POLICIES.includes(policy?.policy) ? policy.policy : "owner";
  if (word !== "listed") return { policy: word };
  const entries = Array.isArray(policy.pubkeys) ? policy.pubkeys : [];
  return { policy: "listed", pubkeys: entries.filter((e) => typeof e === "string" && e.trim() !== "").map((e) => e.trim()) };
}

/**
 * The body of `PUT /governance`: each gate's policy, under the gate's name.
 * @param {Record<string, unknown> | null | undefined} draft the panel's draft
 */
export function governanceBody(draft) {
  return Object.fromEntries(GATES.map((gate) => [gate, policyBody(draft?.[gate])]));
}
