import test from "node:test";
import assert from "node:assert/strict";

import { caretLabel, cpuLabel, diskLabel, gpuLabel, languageLabel, memLabel } from "./statusBarModel.mjs";

test("cpu reads as a whole percent, a missing one as a dash", () => {
  assert.equal(cpuLabel(34.6), "35%");
  assert.equal(cpuLabel(0), "0%");
  assert.equal(cpuLabel(null), "—");
  assert.equal(cpuLabel(Number.NaN), "—");
});

test("memory reads used over total in one unit", () => {
  const GB = 1024 ** 3;
  assert.equal(memLabel(9.4 * GB, 16 * GB), "9.4 / 16 GB");
  assert.equal(memLabel(2 * GB, 16 * GB), "2.0 / 16 GB");
  assert.equal(memLabel(null, 16 * GB), "—");
  assert.equal(memLabel(1, 0), "—");
});

test("gpu reads as a whole percent or a dash without a reader; disk is a size or a dash", () => {
  assert.equal(gpuLabel({ util_percent: 29.6 }), "30%");
  assert.equal(gpuLabel(null), "—", "no reader on this machine is a dash, not a zero");
  assert.equal(diskLabel(1.2 * 1024 ** 3), "1.2 GB");
  assert.equal(diskLabel(0), "0 B");
  assert.equal(diskLabel(null), "—");
});

test("the caret reads as line and column, the language by name or as plain text, and no editor as nothing", () => {
  assert.equal(caretLabel({ line: 12, column: 4 }), "Ln 12, Col 4");
  assert.equal(caretLabel(null), "");
  assert.equal(languageLabel({ language: "rust" }), "rust");
  assert.equal(languageLabel({ language: null }), "plain text");
  assert.equal(languageLabel(null), "");
});
