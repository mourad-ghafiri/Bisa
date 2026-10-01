/**
 * The conversation's mode in words, tested against the Rust enum it mirrors
 * and the registry key it reads its default from. Run with `npm test` from
 * `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  BUILD_MESSAGE,
  CHECKOUT_ORIGIN_KINDS,
  CONVERSATION_MODES,
  DEFAULT_MODE,
  MODE_ICON,
  MODE_LABEL,
  MODE_MEANING,
  PLAN_NEEDS_GUARD_HINT,
  isCheckoutOrigin,
  modeOf,
  modeChoices,
  nextMode,
  planBannerWords,
} from "./conversationModeModel.mjs";

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
  assert.deepEqual([...CONVERSATION_MODES], variantsOf("conversation.rs", "ConversationMode"));
  for (const m of CONVERSATION_MODES) {
    assert.ok(MODE_LABEL[m], `${m} has a label`);
    assert.ok(MODE_MEANING[m].length > 30, `${m} says what an edit or a command does`);
    assert.ok(MODE_ICON[m], `${m} wears a glyph`);
  }
});

test("the default is the core's default and the registry's", () => {
  const conv = readFileSync(join(CORE, "conversation.rs"), "utf8");
  const body = conv.match(/pub enum ConversationMode \{([\s\S]*?)\n\}/)[1];
  const marked = body.match(/#\[default\]\s*\n\s*([A-Z][a-z]+)/);
  assert.ok(marked, "the core marks a default variant");
  assert.equal(DEFAULT_MODE, marked[1].toLowerCase());
  const settings = readFileSync(join(CORE, "settings.rs"), "utf8");
  assert.ok(settings.includes('"agents.conversation.mode"'), "the mode key is registered");
  assert.ok(settings.includes('"agents.review.checkpoints"'), "the checkpoints key is registered");
  assert.match(
    settings.match(/"agents\.conversation\.mode",[\s\S]*?json!\("([a-z]+)"\)/)[1],
    /^manual$/,
    "the registry's default is manual",
  );
});

test("only project and workstream origins are a checkout — the words 09 uses", () => {
  assert.deepEqual([...CHECKOUT_ORIGIN_KINDS].sort(), ["project", "workstream"]);
  assert.equal(isCheckoutOrigin("project"), true);
  assert.equal(isCheckoutOrigin("workstream"), true);
  assert.equal(isCheckoutOrigin("goal"), false);
  assert.equal(isCheckoutOrigin("node"), false);
  assert.equal(isCheckoutOrigin(undefined), false);
});

test("a conversation that names no mode, or an unknown one, reads as the default", () => {
  assert.equal(modeOf({ mode: "auto" }), "auto");
  assert.equal(modeOf({ mode: "plan" }), "plan");
  assert.equal(modeOf({}), DEFAULT_MODE);
  assert.equal(modeOf(null), DEFAULT_MODE);
  assert.equal(modeOf({ mode: "guided" }), DEFAULT_MODE);
});

test("the cycle is manual → auto → plan → manual, wrapping and repairing an unknown value", () => {
  assert.equal(nextMode("manual"), "auto");
  assert.equal(nextMode("auto"), "plan");
  assert.equal(nextMode("plan"), "manual");
  assert.equal(nextMode("bogus"), "auto", "an unknown value reads as manual, and the step from manual is auto");
  assert.equal(nextMode(undefined), "auto");
});

test("the chord never reaches what the control disables: without a tool guard the cycle is manual ⇄ auto", () => {
  assert.equal(nextMode("manual", false), "auto");
  assert.equal(nextMode("auto", false), "manual", "plan is skipped");
  assert.equal(nextMode("plan", false), "manual", "plan held on a harness that lost its guard steps out of it");
  const offered = modeChoices(false).filter((s) => !s.disabled).map((s) => s.id);
  for (const m of ["manual", "auto", "plan", "bogus"]) assert.ok(offered.includes(nextMode(m, false)), `${m} steps to an offered mode`);
});

test("plan is disabled with a hint when the harness carries no tool guard, and enabled when it does", () => {
  const guarded = modeChoices(true);
  assert.deepEqual(
    guarded.map((s) => s.id),
    [...CONVERSATION_MODES],
  );
  for (const s of guarded) assert.equal(s.disabled, false, `${s.id} is not disabled when the harness is guarded`);

  const unguarded = modeChoices(false);
  const plan = unguarded.find((s) => s.id === "plan");
  assert.equal(plan.disabled, true);
  assert.equal(plan.hint, PLAN_NEEDS_GUARD_HINT);
  for (const s of unguarded) if (s.id !== "plan") assert.equal(s.disabled, false);
  for (const c of guarded) assert.equal(c.description, MODE_MEANING[c.id], `${c.id}'s choice carries its meaning`);
});

test("the picker is a dropdown, not a segmented control", () => {
  const src = readFileSync(join(HERE, "ConversationModePicker.tsx"), "utf8");
  assert.ok(src.includes("ChoiceMenu"), "the picker draws through ChoiceMenu");
  assert.ok(!src.includes("SegmentedControl"), "no tab strip for the mode");
  assert.ok(!src.includes("Tooltip"), "a menu trigger wears a title, not a Tooltip");
});

test("the plan banner and the build message are worded present-tense", () => {
  const words = planBannerWords();
  assert.equal(words.build, "Build this plan");
  assert.ok(words.refine.length > 0);
  assert.equal(BUILD_MESSAGE, "Build this plan.");
});
