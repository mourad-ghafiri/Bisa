/**
 * How a rail row is treated. Run with
 * `node --test desktop/src/views/_workbench/railStyleModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { ATTENTIONS, PILL_LEFT_PX, avatarSize, headingSpacing, pillClearance, pillInset, rowTreatment } from "./railStyleModel.mjs";

/** Every `--radius-control` a theme file declares, the token's default among them, in pixels. */
function controlRadii() {
  const themes = new URL("../../theme/themes/", import.meta.url);
  const files = [new URL("../../theme/tokens.css", import.meta.url), ...readdirSync(themes).filter((f) => f.endsWith(".css")).map((f) => new URL(f, themes))];
  const radii = new Set();
  for (const file of files) {
    for (const m of readFileSync(file, "utf8").matchAll(/--radius-control:\s*(\d+(?:\.\d+)?)px/g)) radii.add(Number(m[1]));
  }
  return [...radii].sort((a, b) => a - b);
}

test("the current root wears the current wash, the accent pill and the current ink, whatever else it wants", () => {
  assert.deepEqual(rowTreatment({ kind: "workstream", current: true }), { wash: "current", pill: "accent", ink: "current", muted: false });
  assert.deepEqual(rowTreatment({ kind: "workstream", current: true, attention: "danger" }), { wash: "current", pill: "accent", ink: "current", muted: false }, "where you are outranks what is asking");
  assert.deepEqual(rowTreatment({ kind: "project", current: true }), { wash: "current", pill: "accent", ink: "current", muted: false });
});

test("a row waiting on you wears the accent pill, a failed one the danger pill, a rest row none", () => {
  assert.equal(rowTreatment({ kind: "workstream", attention: "accent" }).pill, "accent");
  assert.equal(rowTreatment({ kind: "workstream", attention: "danger" }).pill, "danger");
  assert.equal(rowTreatment({ kind: "workstream" }).pill, "none");
  assert.equal(rowTreatment({ kind: "workstream", attention: "sideways" }).pill, "none", "a word off the list asks for nothing");
  assert.deepEqual([...ATTENTIONS], ["none", "accent", "danger"]);
});

test("the ink follows the kind: a project strong, a workstream plain, a shell or a session dim unless it wants attention", () => {
  assert.equal(rowTreatment({ kind: "project" }).ink, "strong");
  assert.equal(rowTreatment({ kind: "workstream" }).ink, "plain");
  assert.equal(rowTreatment({ kind: "terminal" }).ink, "dim");
  assert.equal(rowTreatment({ kind: "agent" }).ink, "dim");
  assert.equal(rowTreatment({ kind: "agent", attention: "accent" }).ink, "plain", "a session waiting on you is read in full ink");
  assert.equal(rowTreatment({ kind: "agent", attention: "danger" }).ink, "plain");
  assert.equal(rowTreatment({ kind: "group" }).ink, "dim");
  for (const kind of ["project", "workstream", "terminal", "agent", "group", "goal"]) assert.equal(rowTreatment({ kind }).wash, "rest", kind);
});

test("a row put away or without its folder is muted; the first heading wears no space; a card's avatar is the larger", () => {
  assert.equal(rowTreatment({ kind: "project", archived: true }).muted, true);
  assert.equal(rowTreatment({ kind: "project", exists: false }).muted, true);
  assert.equal(rowTreatment({ kind: "workstream", exists: false }).muted, true);
  assert.equal(rowTreatment({ kind: "project", exists: true }).muted, false);
  assert.equal(rowTreatment({ kind: "project" }).muted, false, "an unsaid folder is not a missing one");
  assert.equal(headingSpacing(true), "none");
  assert.equal(headingSpacing(false), "section");
  assert.equal(avatarSize("project"), 22);
  assert.equal(avatarSize("group"), 16);
  assert.equal(avatarSize("goal"), 16);
});

test("the pill's inset — half the control radius — clears the corner arc on every theme family and leaves a pill worth seeing", () => {
  // The geometry: where an arc of radius r crosses the pill's edge.
  assert.equal(PILL_LEFT_PX, 2);
  assert.equal(Math.round(pillClearance(8) * 10) / 10, 2.7);
  assert.equal(pillClearance(10), 4);
  assert.equal(Math.round(pillClearance(12) * 10) / 10, 5.4);
  assert.equal(pillClearance(2), 0, "a radius within the offset reaches nothing");
  assert.equal(pillClearance(0), 0);
  // Every family's radius, read from the theme files: the inset clears the
  // arc, and a 28 px tree row keeps at least 12 px of pill.
  const radii = controlRadii();
  assert.ok(radii.length >= 3, `the theme files declare their radii: ${radii}`);
  for (const r of radii) {
    assert.ok(pillInset(r) >= pillClearance(r), `a ${r}px corner: inset ${pillInset(r)} clears ${pillClearance(r)}`);
    assert.ok(28 - 2 * pillInset(r) >= 12, `a ${r}px corner leaves a pill in a tree row`);
  }
  // The rule's reach: past ~15 px the arc outruns half the radius, and a
  // family that asks for it fails here rather than on screen.
  assert.ok(pillInset(16) < pillClearance(16));
});
