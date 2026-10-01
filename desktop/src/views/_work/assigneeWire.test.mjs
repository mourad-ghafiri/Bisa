/**
 * The wire grammar for the three kinds of principal.
 *
 * This is a round trip that has to hold exactly, because both directions are
 * used in the same breath: a response is decoded to tags, the reader edits the
 * selection, and the same strings go back up as a request body. A conversion
 * that lost the prefix would read fine on screen and assign the work to an
 * agent that does not exist.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  ASSIGNEE_KINDS,
  assigneeToWire,
  idOf,
  isWire,
  kindOf,
  requireWire,
  wireToAssignee,
} from "./assigneeWire.mjs";

/** A person's key is 64 hex characters, which is what the routes check for. */
const PUBKEY = "a".repeat(64);

test("every kind survives the trip to a wire key and back", () => {
  const cases = [{ agent: "reviewer" }, { human: PUBKEY }, { team: "engineering" }];
  for (const assignee of cases) {
    assert.deepEqual(wireToAssignee(assigneeToWire(assignee)), assignee);
  }
  assert.deepEqual([...ASSIGNEE_KINDS], ["agent", "human", "team"]);
});

test("a wire key names its kind by its prefix, and its id by the rest", () => {
  assert.equal(kindOf("agent:reviewer"), "agent");
  assert.equal(kindOf(`human:${PUBKEY}`), "human");
  assert.equal(kindOf("team:engineering"), "team");
  assert.equal(idOf("team:engineering"), "engineering");
  assert.equal(idOf(`human:${PUBKEY}`), PUBKEY);
});

test("an id containing a colon keeps every character after the first one", () => {
  // Only the first colon separates; an id is never re-split. Splitting on the
  // last one instead would quietly truncate anything namespaced.
  assert.equal(idOf("agent:acme:reviewer"), "acme:reviewer");
  assert.deepEqual(wireToAssignee("agent:acme:reviewer"), { agent: "acme:reviewer" });
});

test("a bare id is not a wire key, for reading or for sending", () => {
  // The server refuses a bare id deliberately, because `7ZK…` could name an
  // agent or a team and guessing is how two assignment surfaces end up
  // disagreeing about who was named. The reader refuses the same way rather
  // than inventing a kind the routes would not accept.
  assert.equal(kindOf("reviewer"), null);
  assert.equal(wireToAssignee("reviewer"), null);
  assert.throws(() => requireWire("reviewer"), /Not an agent, team or person/);
  assert.deepEqual(requireWire("team:engineering"), { team: "engineering" });
  assert.equal(isWire("reviewer"), false);
  assert.equal(isWire("agent:reviewer"), true);
});

test("a prefix with nothing after it is not a wire key", () => {
  for (const key of ["agent:", "team:", "human:", "", ":x"]) {
    assert.equal(isWire(key), false, `${JSON.stringify(key)} should be refused`);
  }
});

test("a person's key is checked for shape, not just for a prefix", () => {
  // `human:bob` would otherwise be stored as a pubkey nobody holds, and an
  // assignment to a ghost fails silently at routing time — the work simply
  // never finds anyone, and nothing says why.
  assert.equal(isWire(`human:${PUBKEY}`), true);
  assert.equal(isWire("human:bob"), false);
  assert.equal(isWire(`human:${"a".repeat(63)}`), false);
  assert.equal(isWire(`human:${"A".repeat(64)}`), false);
});

// ---------------------------------------------------------------------------
// The mirror guard (audit A3)
// ---------------------------------------------------------------------------
//
// `ASSIGNEE_KINDS` and the `<kind>:<id>` grammar are a hand copy of
// `Assignee`'s `Display` / `FromStr` in `bisa-core`. Nothing generates
// them — the wire form is a string, so the schema cannot say it — which
// makes this the arrangement where one side gains a kind and the other does
// not. This test reads the Rust source, so the drift is a failing test here
// rather than a 400 from the node.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ASSIGNEE_RS = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../../../crates/bisa-core/src/assignee.rs",
);

test("the kinds and their prefixes are exactly what bisa-core parses and prints", () => {
  const src = readFileSync(ASSIGNEE_RS, "utf8");
  const parsed = [...src.matchAll(/^\s*"([a-z]+)" => Ok\(Assignee::/gm)].map((m) => m[1]).sort();
  const printed = [...src.matchAll(/write!\(f, "([a-z]+):\{/g)].map((m) => m[1]).sort();
  assert.ok(parsed.length > 0, "FromStr is not where this test expects it in assignee.rs");
  assert.ok(printed.length > 0, "Display is not where this test expects it in assignee.rs");
  const ours = [...ASSIGNEE_KINDS].sort();
  assert.deepEqual(parsed, ours, "FromStr accepts a different set of prefixes");
  assert.deepEqual(printed, ours, "Display prints a different set of prefixes");
});

test("a person's key is the same shape on both sides: 64 lowercase hex characters", () => {
  const src = readFileSync(
    resolve(dirname(fileURLToPath(import.meta.url)), "../../../../crates/bisa-core/src/id.rs"),
    "utf8",
  );
  // `PrincipalId::new` is the one place the shape is checked.
  assert.match(src, /hex\.len\(\) == 64/, "PrincipalId no longer checks for 64 characters");
  assert.match(src, /is_ascii_hexdigit\(\) && !b\.is_ascii_uppercase\(\)/, "PrincipalId no longer insists on lowercase hex");
  assert.equal(isWire(`human:${PUBKEY}`), true);
  assert.equal(isWire(`human:${PUBKEY.toUpperCase()}`), false);
});
