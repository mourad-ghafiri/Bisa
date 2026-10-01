/**
 * What a read of bytes that threw means.
 * Run with `node --test --import ./src/i18n/preload.mjs src/ui/artifact/fileBytesModel.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { bytesFailure } from "./fileBytesModel.mjs";

const refused = (status, message, body) => Object.assign(new Error(message), { status, body });

test("a file over the size the node serves carries the size and the limit the node said, and nothing the node did not say", () => {
  assert.deepEqual(bytesFailure(refused(413, "too large", { error: "…", size: 300, limit: 256 })), { state: "too_large", size: 300, limit: 256 });
  assert.deepEqual(bytesFailure(refused(413, "too large", undefined)), { state: "too_large", size: null, limit: null }, "no body: no number is made up");
  assert.deepEqual(bytesFailure(refused(413, "too large", { size: "300", limit: Number.NaN })), { state: "too_large", size: null, limit: null });
  assert.deepEqual(bytesFailure(refused(413, "too large", { size: -1, limit: 0 })), { state: "too_large", size: null, limit: 0 });
});

test("anything else is a failure in its own words", () => {
  assert.deepEqual(bytesFailure(refused(404, "no such file: a.pdf")), { state: "failed", error: "no such file: a.pdf" });
  assert.deepEqual(bytesFailure(new Error("the node did not answer")), { state: "failed", error: "the node did not answer" });
  assert.deepEqual(bytesFailure("offline"), { state: "failed", error: "offline" });
  assert.deepEqual(bytesFailure(null), { state: "failed", error: "null" });
});

test("the hook that reads the bytes asks this model and keeps no number of its own", () => {
  const hook = readFileSync(new URL("./fileBytes.ts", import.meta.url), "utf8");
  assert.ok(hook.includes("setState(bytesFailure(e))"));
  assert.ok(!/\?\? 0\b/.test(hook), "a size the node did not say is never drawn as zero");
});
