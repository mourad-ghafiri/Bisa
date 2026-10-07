/**
 * The pet's decisions, tested where they live.
 *
 * Nothing here renders — there is no jsdom in this repo. What can actually be
 * wrong in a way a person notices: a pet that says nothing is happening while
 * three agents are working, an animation that plays blank frames because it
 * trusted the grid instead of the art, and a click that lands on a screen
 * nobody asked for.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  FRAME_MS,
  IDLE_SLOWDOWN,
  SHEET,
  STATES,
  alphaProbe,
  PET_HEIGHT,
  PET_SIZE_DEFAULT,
  PET_SIZE_MAX,
  PET_SIZE_MIN,
  cellOffset,
  clampPetSize,
  petHeight,
  petStateOfSession,
  transientForTransition,
  dragState,
  frameCounts,
  frameDuration,
  rowOf,
  spriteBox,
  routeFor,
  standingState,
  transientFor,
  DEFAULT_PET_ID,
  defaultPet,
  framePlan,
  originWords,
  stateWords,
  tileWords,
} from "./petModel.mjs";

test("the nine rows are in the order a pet was drawn against", () => {
  // Not cosmetic: an artist put waving in row 3, so row 3 is waving. Reorder
  // this and every existing pet silently animates the wrong thing.
  assert.deepEqual(STATES, [
    "idle",
    "running-right",
    "running-left",
    "waving",
    "jumping",
    "failed",
    "waiting",
    "running",
    "review",
  ]);
  assert.equal(STATES.length, SHEET.rows);
  assert.equal(rowOf("waving"), 3);
  assert.equal(rowOf("review"), 8);
});

test("the geometry multiplies out to the sheet the format specifies", () => {
  assert.equal(SHEET.cols * SHEET.cellWidth, SHEET.width);
  assert.equal(SHEET.rows * SHEET.cellHeight, SHEET.height);
  assert.equal(SHEET.width, 1536);
  assert.equal(SHEET.height, 1872);
});

test("a name the sheet does not have falls to row 0 rather than off it", () => {
  // A negative row would scroll the background to somewhere with no art, and
  // the pet would vanish instead of standing still.
  assert.equal(rowOf("nonesuch"), 0);
});

test("the standing state is ordered by what it costs you to miss", () => {
  // Waiting outranks everything: it is the only one that has stopped.
  assert.equal(standingState({ waiting: 1, review: 3, working: 5 }), "waiting");
  assert.equal(standingState({ review: 2, working: 5 }), "review");
  assert.equal(standingState({ working: 1 }), "running");
  assert.equal(standingState({}), "idle");
  assert.equal(standingState(), "idle", "no argument is idle, not a crash");
});

test("only events about the work play something once", () => {
  assert.equal(transientFor({ type: "execution_ended", outcome: { outcome: "failed", reason: "boom" } }), "failed");
  assert.equal(transientFor({ type: "execution_ended", outcome: { outcome: "completed" } }), null, "a session ending well is not news");
  assert.equal(transientFor({ type: "step_changed", state: "failed", kind: "check" }), "failed");
  assert.equal(transientFor({ type: "step_changed", state: "done", kind: "check" }), null, "progress is not news");
  assert.equal(transientFor({ type: "run_finished", outcome: "failed" }), "failed");
  assert.equal(transientFor({ type: "run_finished", outcome: "done" }), "jumping");
  assert.equal(transientFor("listener_failed"), "failed", "a start event that could not start a run");
  assert.equal(transientFor("trigger_failed"), null, "a retired frame plays nothing");
  assert.equal(transientFor("gate_decided"), "jumping");
  assert.equal(transientFor("result_accepted"), "jumping");
  assert.equal(transientFor({ type: "guided", status: "proposed" }), "jumping");
  assert.equal(transientFor({ type: "guided", status: "stalled" }), "failed");
  assert.equal(transientFor({ type: "guided", status: "failed" }), "failed");
  assert.equal(transientFor({ type: "guided", status: "working" }), null, "work in progress is the standing state's");

  // The streaming payloads are absent on purpose: a pet that reacted to every
  // token would be one nobody could look at.
  for (const quiet of ["session", "agent_thinking", "agent_replied", "note_changed", "drawing_changed", "drawing_request"]) {
    assert.equal(transientFor(quiet), null, quiet);
  }
});

test("the pet faces the way it is dragged, and does not flicker when it is not", () => {
  assert.equal(dragState(20), "running-right");
  assert.equal(dragState(-20), "running-left");
  // A hand resting on a mouse moves a pixel or two; that is not a direction.
  assert.equal(dragState(0), null);
  assert.equal(dragState(1), null);
  assert.equal(dragState(-1), null);
});

test("frame counts come from the art, not from the grid", () => {
  // Rows are 8 cells wide but a state may use three. Playing all eight would
  // show five blank frames — the pet disappearing five times a second.
  const used = { 0: 3, 1: 8, 4: 1 };
  const counts = frameCounts((row, col) => col < (used[row] ?? 0));
  assert.equal(counts[0], 3);
  assert.equal(counts[1], 8, "a full row uses every cell");
  assert.equal(counts[4], 1);
});

test("a row with no art in it is one frame, never zero", () => {
  // Zero frames divides the clock by nothing and stops the animation dead
  // mid-render. A pet whose artist skipped a state should stand still.
  const counts = frameCounts(() => false);
  assert.equal(counts.length, SHEET.rows);
  for (const n of counts) assert.equal(n, 1);
});

test("a gap in a row counts through to the last cell with art", () => {
  // Frames are consecutive by construction, but a sheet is somebody else's
  // file. Counting to the last non-empty cell keeps a stray gap from
  // truncating an animation halfway.
  const counts = frameCounts((row, col) => row === 0 && (col === 0 || col === 4));
  assert.equal(counts[0], 5);
});

test("the alpha probe finds art anywhere in a cell, and ignores near-transparent noise", () => {
  // A tiny sheet so the arithmetic is checkable by hand: 2×1 cells of 4×4.
  const sheet = { width: 8, height: 4, cols: 2, rows: 1, cellWidth: 4, cellHeight: 4 };
  const data = new Uint8ClampedArray(sheet.width * sheet.height * 4);
  // One opaque pixel in the second cell, at (5, 2).
  data[(2 * sheet.width + 5) * 4 + 3] = 255;
  // And a nearly-invisible one in the first cell, which must not count: an
  // exported sheet can carry stray alpha where the artist saw nothing.
  data[(1 * sheet.width + 1) * 4 + 3] = 3;

  const at = alphaProbe(data, sheet, 1);
  assert.equal(at(0, 0), false, "3/255 alpha is not art");
  assert.equal(at(0, 1), true);
});

test("cell offsets move the sheet under the window, in rendered cells", () => {
  const full = { width: SHEET.cellWidth, height: SHEET.cellHeight };
  assert.deepEqual(cellOffset(0, 0, full), { x: 0, y: 0 });
  assert.deepEqual(cellOffset(1, 2, full), { x: -384, y: -208 });
  // The last cell of the last row still lands inside the sheet.
  const last = cellOffset(SHEET.rows - 1, SHEET.cols - 1, full);
  assert.equal(last.x, -(SHEET.width - SHEET.cellWidth));
  assert.equal(last.y, -(SHEET.height - SHEET.cellHeight));
  // A smaller box steps by that box, so the offsets follow the rendering.
  assert.deepEqual(cellOffset(1, 2, { width: 96, height: 104 }), { x: -192, y: -104 });
});

test("the reference sheet is measured at half size, as it was drawn to be", () => {
  const box = spriteBox(SHEET.width, SHEET.height);
  assert.deepEqual(box, { width: 96, height: 104, sheetWidth: 768, sheetHeight: 936 });
  assert.equal(box.height, PET_HEIGHT);
  // An unmeasured sheet falls back to the reference rather than collapsing to
  // zero: the first paint happens before the image has loaded.
  assert.deepEqual(spriteBox(0, 0), box);
});

test("a sheet at any resolution renders whole, because the grid is the contract", () => {
  // The bug this replaces: `backgroundSize` was hard-coded to the reference
  // 1536×1872, so a sheet exported at a different size showed a slice of two
  // cells at once — the pet arrived cut in half.
  for (const [w, h] of [
    [1536, 1872], // the reference
    [768, 936], // half
    [3072, 3744], // double
    [800, 900], // square-ish cells, drawn by someone who read only the grid
  ]) {
    const box = spriteBox(w, h);
    assert.equal(box.height, PET_HEIGHT, `${w}x${h} stands the same height`);
    // The whole sheet is exactly `cols × rows` of the rendered cell, which is
    // what makes every `cellOffset` land on a frame boundary.
    assert.equal(box.sheetWidth, box.width * SHEET.cols, `${w}x${h} width`);
    assert.equal(box.sheetHeight, box.height * SHEET.rows, `${w}x${h} height`);
    const last = cellOffset(SHEET.rows - 1, SHEET.cols - 1, box);
    assert.equal(last.x, -(box.sheetWidth - box.width));
    assert.equal(last.y, -(box.sheetHeight - box.height));
  }
});

test("idle runs slower, at the pace the format's own defaults propose", () => {
  assert.equal(frameDuration("running"), FRAME_MS);
  assert.equal(frameDuration("waiting"), FRAME_MS);
  assert.equal(frameDuration("idle"), FRAME_MS * IDLE_SLOWDOWN);
  assert.ok(FRAME_MS === 150 && IDLE_SLOWDOWN === 6, "taken from openai/codex#20863");
});

test("a click goes to whatever the pet is reacting to", () => {
  assert.deepEqual(routeFor("waiting"), { name: "inbox" });
  assert.deepEqual(routeFor("review"), { name: "inbox" });
  assert.deepEqual(routeFor("running", "01GOAL"), { name: "goal", id: "01GOAL" });
  assert.deepEqual(routeFor("failed", "01GOAL"), { name: "goal", id: "01GOAL" });
});

test("a click goes nowhere when the pet is reacting to nothing", () => {
  // Picking a screen for an idle pet would be inventing an intention. Nothing
  // happening is a real answer and it reads as one.
  assert.equal(routeFor("idle"), null);
  assert.equal(routeFor("waving"), null);
  assert.equal(routeFor("running-left"), null);
});

test("a following pet is a door to the workstream's Agent panel, unless it is saying nothing", () => {
  const follow = { workstream: "01WS" };
  for (const state of ["waiting", "review", "failed", "running"]) {
    assert.deepEqual(routeFor(state, "01GOAL", follow), { name: "workbench", scope: "workstream", id: "01WS" }, state);
  }
  assert.equal(routeFor("idle", "01GOAL", follow), null);
  assert.equal(routeFor("jumping", null, follow), null);
  assert.deepEqual(routeFor("running", "01GOAL", null), { name: "goal", id: "01GOAL" }, "not following: today's door");
});

test("a followed session's state is the pet's standing state, in the pet's rows", () => {
  assert.equal(petStateOfSession({ state: "starting" }), "running");
  assert.equal(petStateOfSession({ state: "thinking" }), "running");
  assert.equal(petStateOfSession({ state: "running", tool: "Edit", args: "", tier: "write" }), "running");
  assert.equal(petStateOfSession({ state: "waiting", on: { on: "permission", tool: "Bash" } }), "waiting");
  assert.equal(petStateOfSession({ state: "failed", reason: "x" }), "failed");
  assert.equal(petStateOfSession({ state: "aborted" }), "failed");
  assert.equal(petStateOfSession({ state: "done" }), "idle");
  assert.equal(petStateOfSession({ state: "idle" }), "idle");
  assert.equal(petStateOfSession({ state: "parked" }), "idle");
  assert.equal(petStateOfSession("thinking"), "running", "the word alone works too");
});

test("a followed session's edges play once: a finish jumps, a failure fails, the rest is standing", () => {
  assert.equal(transientForTransition({ state: "running" }, { state: "done" }), "jumping");
  assert.equal(transientForTransition(null, { state: "done" }), "jumping", "first sight of a finished session");
  assert.equal(transientForTransition({ state: "thinking" }, { state: "failed", reason: "x" }), "failed");
  assert.equal(transientForTransition({ state: "running" }, { state: "aborted" }), "failed");
  assert.equal(transientForTransition({ state: "idle" }, { state: "thinking" }), null);
  assert.equal(transientForTransition({ state: "thinking" }, { state: "waiting" }), null, "waiting stands, it does not play");
  assert.equal(transientForTransition({ state: "done" }, { state: "done" }), null, "a state repeated is not news");
});

test("a working pet with no scope falls back to the list rather than nowhere", () => {
  // `working` is keyed by conversation scope, and a scope is not always an
  // goal — an agent mid-turn in a channel has no goal to open.
  assert.deepEqual(routeFor("running"), { name: "goals" });
  assert.deepEqual(routeFor("running", null), { name: "goals" });
});

test("the size dial cannot make a pet that swallows the window", () => {
  // The ceiling is the point of the control having one. The whole sprite is
  // its own drag handle, so an unbounded pet is an unbounded click-blocker
  // sitting over whatever you were reading.
  assert.equal(clampPetSize(10_000), PET_SIZE_MAX);
  assert.equal(clampPetSize(PET_SIZE_MAX + 1), PET_SIZE_MAX);
  assert.equal(clampPetSize(0), PET_SIZE_MIN);
  assert.equal(clampPetSize(-50), PET_SIZE_MIN);
  assert.ok(PET_SIZE_MAX <= 150, "half again as big is the most that stays glanceable");

  // Anything unusable is the default, never a throw: this parses a string out
  // of localStorage that a person can edit.
  for (const junk of [undefined, null, "", "big", NaN, {}, []]) {
    assert.equal(clampPetSize(junk), PET_SIZE_DEFAULT, `${JSON.stringify(junk) ?? junk}`);
  }
  // A stored string is the ordinary case, not junk.
  assert.equal(clampPetSize("120"), 120);
});

test("size is a percentage of the standing height, and stays a whole pixel", () => {
  assert.equal(petHeight(PET_SIZE_DEFAULT), PET_HEIGHT);
  assert.equal(petHeight(PET_SIZE_MAX), Math.round((PET_HEIGHT * PET_SIZE_MAX) / 100));
  // Every reachable size is a whole number of pixels: a fractional height
  // would put `cellOffset` between frames and show a seam.
  for (let pct = PET_SIZE_MIN; pct <= PET_SIZE_MAX; pct += 10) {
    const h = petHeight(pct);
    assert.ok(Number.isInteger(h) && h > 0, `${pct}% -> ${h}`);
  }
  // Out-of-range input still yields a drawable pet rather than nothing.
  assert.ok(petHeight(99_999) > 0 && petHeight(-1) > 0);
});

test("a resized pet still lands on whole frames", () => {
  // `spriteBox` is what the size dial actually drives, so the property that
  // made the pet render whole has to survive every step of it.
  for (let pct = PET_SIZE_MIN; pct <= PET_SIZE_MAX; pct += 10) {
    const box = spriteBox(SHEET.width, SHEET.height, petHeight(pct));
    assert.equal(box.height, petHeight(pct), `${pct}% height`);
    assert.equal(box.sheetWidth, box.width * SHEET.cols, `${pct}% sheet width`);
    assert.equal(box.sheetHeight, box.height * SHEET.rows, `${pct}% sheet height`);
  }
});

// ---------------------------------------------------------------------------
// The mirror guard (audit A3)
// ---------------------------------------------------------------------------
//
// The sheet's geometry and the nine animation rows are declared twice: here,
// where the sprite is drawn, and in `bisa-core::pet`, where a pet package
// is validated. Neither side generates the other — the constants are numbers
// in a `.mjs`, not a schema — so this reads the Rust source and fails the
// moment an artist's row order or a cell size moves on one side only.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const PET_RS = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../../crates/bisa-core/src/pet.rs",
);

function rustConst(src, name) {
  const m = src.match(new RegExp(`pub const ${name}: u32 = (\\d+);`));
  assert.ok(m, `${name} is not where this test expects it in pet.rs`);
  return Number(m[1]);
}

test("the sheet geometry is the one bisa-core validates a pet package against", () => {
  const src = readFileSync(PET_RS, "utf8");
  assert.equal(SHEET.cols, rustConst(src, "SHEET_COLS"));
  assert.equal(SHEET.rows, rustConst(src, "SHEET_ROWS"));
  assert.equal(SHEET.cellWidth, rustConst(src, "CELL_WIDTH"));
  assert.equal(SHEET.cellHeight, rustConst(src, "CELL_HEIGHT"));
  // The sheet is exactly the grid, no margin: the drawing code relies on it.
  assert.equal(SHEET.width, SHEET.cols * SHEET.cellWidth);
  assert.equal(SHEET.height, SHEET.rows * SHEET.cellHeight);
});

test("the animation rows are in the order bisa-core lays them out", () => {
  const src = readFileSync(PET_RS, "utf8");
  const block = src.match(/pub const STATES: \[&str; (\d+)\] = \[([\s\S]*?)\];/);
  assert.ok(block, "STATES is not where this test expects it in pet.rs");
  const rust = [...block[2].matchAll(/"([a-z-]+)"/g)].map((m) => m[1]);
  assert.equal(rust.length, Number(block[1]));
  assert.deepEqual([...STATES], rust);
});

// ---------------------------------------------------------------------------
// The nine that ship: the default, the manifest's pace, the tile's words
// ---------------------------------------------------------------------------

const SHIPPED = [
  { id: "bisa-pets.midnight-shipping.bracket", displayName: "Bracket", origin: "catalog" },
  { id: DEFAULT_PET_ID, displayName: "Moonrice", origin: "catalog" },
  { id: "bisa-pets.mine.byte", displayName: "Byte" },
];

test("Moonrice is the pet when none was chosen; the last one shown comes back first", () => {
  assert.equal(DEFAULT_PET_ID, "bisa-pets.midnight-shipping.moonrice");
  assert.equal(defaultPet(SHIPPED, null), DEFAULT_PET_ID);
  assert.equal(defaultPet(SHIPPED, "bisa-pets.mine.byte"), "bisa-pets.mine.byte", "the last shown, while it exists");
  assert.equal(defaultPet(SHIPPED, "gone"), DEFAULT_PET_ID, "a last pet the list lost falls back");
  assert.equal(defaultPet(SHIPPED.filter((p) => p.id !== DEFAULT_PET_ID), null), "bisa-pets.midnight-shipping.bracket", "no Moonrice: the first built-in");
  assert.equal(defaultPet([{ id: "bisa-pets.mine.byte" }], null), "bisa-pets.mine.byte", "no built-in: the first pet");
  assert.equal(defaultPet([], null), null);
});

test("a manifest's animations decide the frames and their durations; a pet without them keeps the probed count at the fixed pace", () => {
  const def = {
    fps: 8,
    animations: {
      idle: { row: 0, frames: 3, frameDurationsMs: [2070, 856, 2346] },
      waving: { row: 3, frames: 2 },
      failed: { row: 5, frames: 12, frameDurationsMs: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] },
    },
  };
  assert.deepEqual(framePlan(def, "idle", 7), { frames: 3, durations: [2070, 856, 2346], fromManifest: true }, "the manifest wins over the probe");
  assert.deepEqual(framePlan(def, "waving", 1), { frames: 2, durations: [125, 125], fromManifest: true }, "no durations: the pack's fps");
  assert.deepEqual(framePlan(def, "failed", 1).frames, SHEET.cols, "held to the sheet's columns");
  assert.deepEqual(framePlan(def, "jumping", 5), { frames: 5, durations: [150, 150, 150, 150, 150], fromManifest: false }, "a state the manifest lacks: the probe, the fixed pace");
  assert.deepEqual(framePlan(null, "idle", 4), { frames: 4, durations: [900, 900, 900, 900], fromManifest: false }, "no manifest: idle runs slow");
  assert.deepEqual(framePlan(undefined, "running", 0), { frames: 1, durations: [150], fromManifest: false }, "never no frame");
  assert.deepEqual(framePlan({ animations: { idle: { row: 0, frames: 2, frameDurationsMs: [0, -5] } } }, "idle", 1).durations, [150, 150], "a duration of nothing is the fixed pace");
});

test("a tile says who the pet is in the pack's own words, and where it came from", () => {
  const moonrice = {
    id: DEFAULT_PET_ID,
    displayName: "Moonrice",
    description: "Brought you something warm. A small rice-dumpling ghost.",
    origin: "catalog",
    "x-bisa-pets": { tagline: "Brought you something warm.", archetype: "ghost", mood: "serene", personality: "quiet-comfort" },
  };
  assert.deepEqual(tileWords(moonrice), { tagline: "Brought you something warm.", chips: ["ghost", "serene"] });
  assert.deepEqual(tileWords({ description: "A small friend. Likes tea." }), { tagline: "A small friend.", chips: [] }, "no flavour: the description's first sentence");
  assert.deepEqual(tileWords({ description: "no full stop" }), { tagline: "no full stop", chips: [] });
  assert.deepEqual(tileWords(null), { tagline: "", chips: [] });
  assert.equal(originWords("catalog"), "built in");
  assert.equal(originWords(undefined), "yours");
});

test("the manifest's animation fields are the ones bisa-core reads", () => {
  const src = readFileSync(PET_RS, "utf8");
  const block = src.match(/pub struct PetAnimation \{([\s\S]*?)\n\}/);
  assert.ok(block, "PetAnimation is not where this test expects it in pet.rs");
  const fields = [...block[1].matchAll(/pub (\w+):/g)].map((m) => m[1]);
  assert.deepEqual(fields, ["row", "frames", "frame_durations_ms"], "row, frames, frameDurationsMs — as framePlan reads them");
  assert.ok(src.includes('rename = "x-bisa-pets"'), "the pack's own key for who the pet is");
});

test("what the pet is doing is said in the catalog's words for every row of the sheet — never the row's own name", async () => {
  assert.equal(stateWords("waiting"), "waiting on you");
  assert.equal(stateWords("review"), "something to review");
  assert.equal(stateWords("running"), "working");
  assert.equal(stateWords("idle"), "idle");
  assert.equal(stateWords("failed"), "something failed");
  for (const state of STATES) assert.ok(stateWords(state).length > 3 && !stateWords(state).includes("pet-state"), `${state} has words`);
  assert.equal(new Set(["waiting", "review", "running", "idle", "failed", "waving", "jumping"].map(stateWords)).size, 7, "each standing and each once-through state reads differently");
  const { readFileSync } = await import("node:fs");
  const companion = readFileSync(new URL("./PetCompanion.tsx", import.meta.url), "utf8");
  assert.ok(companion.includes('const saying = followed ? t("shell-followed-session-who", { state: stateWords(state), who: followWords(followed) }) : stateWords(state);'), "the label a person hears is worded, the join a catalog message");
  assert.ok(companion.includes("useReloadOnReconnect(() => void refreshPets());"), "a node that was away at boot is asked for the pets once it is back");
  const model = readFileSync(new URL("./petModel.mjs", import.meta.url), "utf8");
  assert.ok(!model.includes('"yours"'), "no word of its own");
});
