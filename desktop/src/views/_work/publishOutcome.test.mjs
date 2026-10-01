import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { publishFailure, refusalOf } from "./publishOutcome.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const DTO = readFileSync(join(HERE, "../../../../crates/bisa-node/src/dto.rs"), "utf8");

/** The node's `ErrorCode` variants, snake_cased, read from the source (recipe 7). */
function nodeErrorCodes() {
  const body = DTO.match(/pub enum ErrorCode \{([\s\S]*?)\n\}/);
  assert.ok(body, "ErrorCode in dto.rs");
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*),/gm)].map((m) =>
    m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase(),
  );
}

test("each publishing code the node sends has a banner of its own; the words are the node's", () => {
  const msg = "project storefront publishes manually; a person must push work/x";
  assert.deepEqual(refusalOf({ status: 409, code: "publish_manual", message: msg }), { kind: "manual", detail: msg });
  assert.equal(refusalOf({ status: 409, code: "publish_no_goal", message: "m" }).kind, "no_goal");
  assert.equal(refusalOf({ status: 409, code: "publish_declined", message: "m" }).kind, "declined");
  assert.equal(refusalOf({ status: 409, code: "nothing_to_publish", message: "m" }).kind, "not_ready");
  assert.equal(refusalOf({ status: 409, code: "workstream_state", message: "cannot pr_opened a workstream that is pr_open" }).kind, "state");
});

test("a 409 is never read as the manual policy on its own: no code, or an unknown one, is an error", () => {
  const stale = "cannot pr_opened a workstream that is open";
  assert.deepEqual(refusalOf({ status: 409, message: stale }), { kind: "error", detail: stale });
  assert.deepEqual(refusalOf({ status: 409, code: null, message: stale }), { kind: "error", detail: stale });
  assert.equal(refusalOf({ status: 409, code: "something_new", message: "m" }).kind, "error");
  assert.equal(refusalOf({ status: 502, code: null, message: "bad gateway" }).kind, "error");
  assert.equal(refusalOf({ status: 409, code: "constructor", message: "m" }).kind, "error", "own properties only");
});

test("what was approved and did not go out is this checkout's notice — the act as the gate asked it and the node's reason, as they came; another checkout's frame is nothing", () => {
  const frame = { type: "workstream_publish_failed", workstream: "w1", project: "p1", what: "push work/x and open a pull request for it", reason: "the remote refused: permission denied to acme/web" };
  assert.deepEqual(publishFailure(frame, "w1"), { kind: "failed", what: "push work/x and open a pull request for it", reason: "the remote refused: permission denied to acme/web" });
  assert.equal(publishFailure(frame, "w2"), null, "another checkout's");
  assert.equal(publishFailure({ type: "workstream_pushed", workstream: "w1" }, "w1"), null, "another fact");
  assert.equal(publishFailure(null, "w1"), null);
  assert.deepEqual(publishFailure({ type: "workstream_publish_failed", workstream: "w1" }, "w1"), { kind: "failed", what: "", reason: "" }, "a frame with no words is still the notice, with none");
});

test("every code the node can send is one this app renders — except a commit's, which is a toast", () => {
  const codes = nodeErrorCodes();
  assert.ok(codes.length >= 6, `read the enum: ${codes}`);
  // The publishing codes: a git operation's own (`conflict`, `in_progress`, …) are the operation model's words.
  for (const code of codes.filter((c) => c.includes("publish") || c.endsWith("_state"))) {
    if (code === "nothing_to_commit") continue;
    assert.notEqual(refusalOf({ status: 409, code, message: "m" }).kind, "error", `${code} has a banner`);
  }
});
