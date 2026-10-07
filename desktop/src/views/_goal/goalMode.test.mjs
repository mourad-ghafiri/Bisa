/**
 * The goal's mode in words, tested against the Rust enum it mirrors. Run with
 * `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  AUTO_CEILING,
  AUTO_CEILING_KEY,
  AUTO_CEILING_LABEL,
  AUTO_CEILING_MEANING,
  AUTO_PERMISSIONS,
  AUTO_PERMISSIONS_KEY,
  AUTO_PERMISSIONS_LABEL,
  AUTO_PERMISSIONS_MEANING,
  DEFAULT_AUTO_CEILING,
  DEFAULT_AUTO_PERMISSIONS,
  DEFAULT_MODE,
  DEFAULT_MODE_KEY,
  GOAL_MODES,
  MODE_ICON,
  MODE_LABEL,
  MODE_MEANING,
  afterCapture,
  captureHint,
  captureToast,
  designs,
  modeOf,
  modeSegments,
} from "./goalMode.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const CORE = join(HERE, "../../../../crates/bisa-core/src");

/** snake_case variants of a Rust enum, read from the source (recipe 7). */
function variantsOf(file, name) {
  const src = readFileSync(join(CORE, file), "utf8");
  const body = src.match(new RegExp(`pub enum ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(body, `${name} in ${file}`);
  return [...body[1].matchAll(/^\s{4}([A-Z][A-Za-z]*)[ ,{(]/gm)].map((m) =>
    m[1].replace(/([a-z])([A-Z])/g, "$1_$2").toLowerCase(),
  );
}

test("the three modes are the core's, in the core's order, each with a label, a meaning and a glyph", () => {
  assert.deepEqual([...GOAL_MODES], variantsOf("goal.rs", "GoalMode"));
  for (const m of GOAL_MODES) {
    assert.ok(MODE_LABEL[m], `${m} has a label`);
    assert.ok(MODE_MEANING[m].length > 40, `${m} says what happens after the capture`);
    assert.ok(MODE_ICON[m], `${m} wears a glyph`);
    assert.ok(captureHint(m).length > 20, `${m} has a capture hint`);
    assert.ok(captureToast(m).startsWith("Captured"), `${m}'s toast says it landed`);
  }
  assert.deepEqual(
    modeSegments().map((s) => s.id),
    [...GOAL_MODES],
  );
});

test("the default is the core's default, and the keys are the registry's", () => {
  const goal = readFileSync(join(CORE, "goal.rs"), "utf8");
  const body = goal.match(/pub enum GoalMode \{([\s\S]*?)\n\}/)[1];
  const marked = body.match(/#\[default\]\s*\n\s*([A-Z][a-z]+)/);
  assert.ok(marked, "the core marks a default variant");
  assert.equal(DEFAULT_MODE, marked[1].toLowerCase());
  const settings = readFileSync(join(CORE, "settings.rs"), "utf8");
  for (const key of [DEFAULT_MODE_KEY, AUTO_PERMISSIONS_KEY, AUTO_CEILING_KEY]) assert.ok(settings.includes(`"${key}"`), `${key} is registered`);
  assert.match(settings.match(new RegExp(`"${DEFAULT_MODE_KEY}",[\\s\\S]*?json!\\("([a-z]+)"\\)`))[1], /^auto$/, "the registry's default is auto");
});

test("an auto goal's ceiling is the classifier's to read by default, and the words say so", () => {
  const settings = readFileSync(join(CORE, "settings.rs"), "utf8");
  const def = settings.match(new RegExp(`"${AUTO_PERMISSIONS_KEY}",\\s*Choice\\(&\\[([^\\]]*)\\]\\),\\s*json!\\("([a-z]+)"\\)`));
  assert.ok(def, "the key is a choice with a default");
  assert.deepEqual(
    def[1].split(",").map((s) => s.trim().replace(/"/g, "")),
    [...AUTO_PERMISSIONS],
    "the two answers are the registry's, in its order",
  );
  assert.equal(DEFAULT_AUTO_PERMISSIONS, def[2]);
  assert.equal(DEFAULT_AUTO_PERMISSIONS, "classify", "unattended by default");
  for (const v of AUTO_PERMISSIONS) {
    assert.ok(AUTO_PERMISSIONS_LABEL[v], `${v} has a word`);
    assert.ok(AUTO_PERMISSIONS_MEANING[v].length > 40, `${v} says what happens`);
  }
  assert.match(MODE_MEANING.auto, /classifier/, "the auto sentence names who reads the ceiling");
  assert.match(AUTO_PERMISSIONS_MEANING.classify, /harmful/, "harmful still reaches a person");
});

test("an auto goal's write step runs commands by default, and the words say so", () => {
  const settings = readFileSync(join(CORE, "settings.rs"), "utf8");
  const def = settings.match(new RegExp(`"${AUTO_CEILING_KEY}",\\s*Choice\\(&\\[([^\\]]*)\\]\\),\\s*json!\\("([a-z]+)"\\)`));
  assert.ok(def, "the key is a choice with a default");
  assert.deepEqual(
    def[1].split(",").map((s) => s.trim().replace(/"/g, "")),
    [...AUTO_CEILING],
    "the two answers are the registry's, in its order",
  );
  assert.equal(DEFAULT_AUTO_CEILING, def[2]);
  assert.equal(DEFAULT_AUTO_CEILING, "exec", "ordinary commands run on their own by default");
  for (const v of AUTO_CEILING) {
    assert.ok(AUTO_CEILING_LABEL[v], `${v} has a word`);
    assert.ok(AUTO_CEILING_MEANING[v].length > 40, `${v} says what happens`);
  }
  assert.match(AUTO_CEILING_MEANING.exec, /read-only/, "a read-only step stays read-only");
  assert.match(MODE_MEANING.auto, /run on their own/, "the auto sentence says ordinary commands run");
});

test("the agent designs for auto and guided goals; a manual goal's person does", () => {
  assert.equal(designs("auto"), true);
  assert.equal(designs("guided"), true);
  assert.equal(designs("manual"), false);
  assert.equal(designs(null), false);
});

test("a goal that names no mode, or an unknown one, reads as the default", () => {
  assert.equal(modeOf({ mode: "manual" }), "manual");
  assert.equal(modeOf({ mode: "guided" }), "guided");
  assert.equal(modeOf({}), DEFAULT_MODE);
  assert.equal(modeOf(null), DEFAULT_MODE);
  assert.equal(modeOf({ mode: "interactive" }), DEFAULT_MODE);
});

test("only a manual capture lands in the designer; the others land on the goal", () => {
  assert.deepEqual(afterCapture("manual"), { tab: "workflow", edit: "1" });
  assert.equal(afterCapture("auto"), null);
  assert.equal(afterCapture("guided"), null);
  assert.match(captureToast("manual"), /Workflow tab/);
  assert.match(captureToast("auto"), /start it/);
  assert.match(captureToast("guided"), /adopt/);
  assert.match(captureHint("manual"), /designer/);
});
