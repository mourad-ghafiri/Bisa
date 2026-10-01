/**
 * The webview's log facts: the words are the Rust crate's, a line passes
 * its level and nothing louder, the settings read into a configuration, an
 * event is bounded, and an error yields its facts and never a body.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  DEFAULT_CONFIG,
  KEYS,
  LEVELS,
  MAX_FIELD,
  MAX_FIELDS,
  MAX_MESSAGE,
  ROTATIONS,
  STACK_LINES,
  configFrom,
  errorFields,
  passes,
  shapeEvent,
} from "./logModel.mjs";

const rust = readFileSync(new URL("../../crates/bisa-log/src/config.rs", import.meta.url), "utf8");
const settings = readFileSync(new URL("../../crates/bisa-core/src/settings.rs", import.meta.url), "utf8");

/** The words a `pub const WORDS: [&'static str; N] = [...]` spells, for one type. */
function words(type) {
  const block = rust.slice(rust.indexOf(`pub enum ${type}`));
  const m = block.match(/pub const WORDS: \[&'static str; \d+\] = \[([^\]]+)\]/);
  assert.ok(m, `${type}::WORDS is spelled in config.rs`);
  return m[1].match(/"([a-z]+)"/g).map((w) => w.replaceAll('"', ""));
}

test("the levels and rotations are the Rust crate's words, and the keys are registered", () => {
  assert.deepEqual([...LEVELS], words("LogLevel"));
  assert.deepEqual([...ROTATIONS], words("LogRotation"));
  for (const key of Object.values(KEYS)) {
    assert.ok(settings.includes(`"${key}",`), `${key} is a registry key`);
  }
  // The registry's default is the crate's default: errors only, daily, fourteen.
  assert.match(rust, /level: LogLevel::Error,\s*rotation: LogRotation::Daily,\s*keep_files: Self::DEFAULT_KEEP_FILES/);
  assert.match(rust, /DEFAULT_KEEP_FILES: usize = 14/);
  assert.deepEqual(DEFAULT_CONFIG, { enabled: true, level: "error", rotation: "daily", keep_files: 14 });
});

test("a line passes its level and everything quieter, never an unknown word", () => {
  assert.ok(passes("error", "error"));
  assert.ok(passes("error", "trace"));
  assert.ok(passes("warn", "info"));
  assert.ok(!passes("info", "warn"));
  assert.ok(!passes("debug", "error"));
  assert.ok(!passes("loud", "trace"));
  assert.ok(!passes("error", "loud"));
});

test("the settings read into a configuration and anything odd is the default", () => {
  assert.deepEqual(configFrom(null), DEFAULT_CONFIG);
  assert.deepEqual(
    configFrom([
      { key: "logging.enabled", value: false },
      { key: "logging.level", value: "debug" },
      { key: "logging.rotation", value: "hourly" },
      { key: "logging.keep_files", value: 3 },
    ]),
    { enabled: false, level: "debug", rotation: "hourly", keep_files: 3 },
  );
  assert.deepEqual(
    configFrom([
      { key: "logging.level", value: "loud" },
      { key: "logging.keep_files", value: 0 },
    ]),
    { ...DEFAULT_CONFIG, keep_files: 1 },
  );
});

test("an event is bounded: the message cut, the fields stringified and counted", () => {
  const long = "x".repeat(MAX_MESSAGE + 50);
  const ev = shapeEvent("warn", "api", long, {
    status: 500,
    path: "/goals",
    ok: false,
    nothing: null,
    nested: { a: [1, 2, 3] },
    big: "y".repeat(MAX_FIELD + 10),
  });
  assert.equal(ev.level, "warn");
  assert.equal(ev.target, "api");
  assert.equal(ev.message.length, MAX_MESSAGE);
  assert.ok(ev.message.endsWith("…"));
  assert.equal(ev.fields.status, 500);
  assert.equal(ev.fields.path, "/goals");
  assert.equal(ev.fields.ok, false);
  assert.equal(ev.fields.nothing, null);
  assert.equal(ev.fields.nested, '{"a":[1,2,3]}');
  assert.equal(ev.fields.big.length, MAX_FIELD);

  const many = Object.fromEntries(Array.from({ length: MAX_FIELDS + 5 }, (_, i) => [`f${i}`, i]));
  const wide = shapeEvent("error", "", "m", many);
  assert.equal(Object.keys(wide.fields).length, MAX_FIELDS + 1);
  assert.equal(wide.fields["…"], "more fields dropped");
  assert.equal(wide.target, "webview");
  assert.equal(shapeEvent("loud", "t", "m").level, "error");

  const cyclic = {};
  cyclic.self = cyclic;
  assert.equal(shapeEvent("error", "t", "m", { cyclic }).fields.cyclic, "<object>");
});

test("an error yields its name, message, status, path, code and a few stack lines — never a body", () => {
  class ApiError extends Error {
    constructor() {
      super("the node refused");
      this.name = "ApiError";
      this.status = 500;
      this.path = "/goals";
      this.code = "conflict";
      this.body = { secret: "never" };
    }
  }
  const f = errorFields(new ApiError());
  assert.equal(f.error, "ApiError");
  assert.equal(f.message, "the node refused");
  assert.equal(f.status, 500);
  assert.equal(f.path, "/goals");
  assert.equal(f.code, "conflict");
  assert.ok(!("body" in f));
  assert.ok(typeof f.stack === "string" && f.stack.split("\n").length <= STACK_LINES);
  assert.deepEqual(errorFields("plain"), { error: "plain" });
  assert.deepEqual(errorFields(undefined), { error: null });
});
