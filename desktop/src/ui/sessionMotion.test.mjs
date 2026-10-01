/**
 * How a state mark moves, and that every motion has a class the stylesheet
 * defines with a reduced-motion stop. Run with `npm test` from `desktop/`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { MOTIONS, motionClass, motionOf, repeats } from "./sessionMotion.mjs";
import { STATES } from "./sessionState.mjs";

const here = dirname(fileURLToPath(import.meta.url));

test("live states move, settled states are still, and the two arrivals move once", () => {
  assert.equal(motionOf("running"), "spin");
  assert.equal(motionOf("starting"), "spin");
  assert.equal(motionOf("thinking"), "breathe", "thinking is not the same mark as running a tool");
  assert.equal(motionOf("waiting"), "nudge", "attention, not alarm — a nudge every few seconds, never a spin");
  assert.equal(motionOf("done"), "pop");
  assert.equal(motionOf("failed"), "shake");
  for (const still of ["aborted", "idle", "parked"]) assert.equal(motionOf(still), "none", `${still} holds still`);
  assert.equal(motionOf({ state: "running", tool: "Edit" }), "spin", "a state object reads like its word");
  assert.equal(motionOf(undefined), "none", "no state is idle, and idle is still");
  for (const word of STATES) assert.ok(MOTIONS.includes(motionOf(word)), `${word} has a motion`);
});

test("repeating motions repeat, arrivals do not, and every motion but none has a class the stylesheet stops under reduced motion", () => {
  assert.ok(repeats("spin") && repeats("breathe") && repeats("nudge"));
  assert.ok(!repeats("pop") && !repeats("shake") && !repeats("none"));
  const css = readFileSync(join(here, "../theme/motion.css"), "utf8");
  const reduced = css.slice(css.lastIndexOf("@media (prefers-reduced-motion: reduce)"));
  for (const m of MOTIONS.filter((m) => m !== "none")) {
    const cls = motionClass(m);
    assert.equal(cls, `motion-${m}`);
    assert.ok(css.includes(`.${cls} {`), `${cls} is defined`);
    assert.ok(reduced.includes(`.${cls}`), `${cls} stops under reduced motion`);
  }
  assert.equal(motionClass("none"), "");
});
