import { strict as assert } from "node:assert";
import { test } from "node:test";
import {
  DRAW_TABS,
  SAVE_DEFAULT_MS,
  SAVE_MAX_MS,
  SAVE_MIN_MS,
  canvasAppState,
  clampSaveDelay,
  drawStatusWords,
  drawTab,
  drawTabAdmits,
  drawTabQuery,
  drawnElements,
  filterDrawings,
  goneQuietly,
  movesDrawingCount,
  persistedAppState,
  sceneMoved,
} from "./drawModel.mjs";
import { OWNER_GONE_FRAMES } from "../notes/notesModel.mjs";

test("the tabs are the notes' seven, All first, and a stored word outside them lands on All", () => {
  assert.deepEqual(DRAW_TABS, ["all", "workspace", "projects", "goals", "workflows", "channels", "node"]);
  assert.equal(drawTab("goals"), "goals");
  assert.equal(drawTab("pictures"), "all");
  assert.equal(drawTab(null), "all");
  assert.equal(drawTabQuery("all"), "");
  assert.equal(drawTabQuery("projects"), "?scope=project");
  assert.ok(drawTabAdmits("all", "goal") && drawTabAdmits("goals", "goal") && !drawTabAdmits("goals", "project"));
});

test("the dock's count is read again on a drawing changed and on the places the notes' rule names gone — on nothing else", () => {
  assert.ok(movesDrawingCount("drawing_changed"));
  for (const gone of OWNER_GONE_FRAMES) assert.ok(movesDrawingCount(gone), `${gone}: the notes' list, shared`);
  for (const still of ["note_changed", "drawing_request", "project_archived", "channel_deleted", "file_changed", undefined, 3]) assert.ok(!movesDrawingCount(still), String(still));
});

test("the search keeps a drawing whose title says every word, in order", () => {
  const rows = [{ title: "Order flow" }, { title: "Where the data goes" }, { title: "Retro board" }];
  assert.deepEqual(filterDrawings(rows, "").map((r) => r.title), ["Order flow", "Where the data goes", "Retro board"]);
  assert.deepEqual(filterDrawings(rows, "DATA goes").map((r) => r.title), ["Where the data goes"]);
  assert.deepEqual(filterDrawings(rows, "flow order").map((r) => r.title), ["Order flow"]);
  assert.deepEqual(filterDrawings(rows, "nothing"), []);
});

test("only the ground and the grid are kept of the canvas's state, and read back in the canvas's words", () => {
  const kept = persistedAppState({ viewBackgroundColor: "#f8f9fa", gridModeEnabled: true, zoom: { value: 2 }, selectedElementIds: { a: true }, openMenu: "canvas" });
  assert.deepEqual(kept, { view_background_color: "#f8f9fa", grid: true });
  assert.deepEqual(persistedAppState(null), { view_background_color: "#ffffff", grid: false });
  assert.deepEqual(persistedAppState({ viewBackgroundColor: "   " }), { view_background_color: "#ffffff", grid: false });
  assert.deepEqual(canvasAppState(kept), { viewBackgroundColor: "#f8f9fa", gridModeEnabled: true, gridSize: 20 });
  assert.deepEqual(canvasAppState(undefined), { viewBackgroundColor: "#ffffff", gridModeEnabled: false, gridSize: 20 });
});

test("a deleted element is not drawn, and a scene moved when an element's version or count did", () => {
  const a = { id: "a", version: 1 };
  const b = { id: "b", version: 1 };
  assert.deepEqual(drawnElements([a, { ...b, isDeleted: true }]), [a]);
  assert.equal(sceneMoved([a, b], [a, b]), false);
  assert.equal(sceneMoved([{ ...a, version: 2 }, b], [a, b]), true, "a stroke bumps the version");
  assert.equal(sceneMoved([a], [a, b]), true, "an element gone");
  assert.equal(sceneMoved([a, { ...b, isDeleted: true }], [a]), false, "a deleted one does not count");
  assert.equal(sceneMoved([a, b], [b, a]), false, "order is not a change");
});

test("the save delay is bounded, stepped and defaulted", () => {
  assert.equal(clampSaveDelay(undefined), SAVE_DEFAULT_MS);
  assert.equal(clampSaveDelay("abc"), SAVE_DEFAULT_MS);
  assert.equal(clampSaveDelay(0), SAVE_MIN_MS);
  assert.equal(clampSaveDelay(999999), SAVE_MAX_MS);
  assert.equal(clampSaveDelay("1250"), 1300, "stepped to the nearest hundred");
});

test("the status says the loudest thing first", () => {
  assert.equal(drawStatusWords({ conflict: "theirs", error: "e", saving: true, dirty: true }), "theirs");
  assert.equal(drawStatusWords({ error: "e", saving: true }), "e");
  assert.equal(drawStatusWords({ saving: true, dirty: true }), "Saving…");
  assert.equal(drawStatusWords({ dirty: true }), "Unsaved");
  assert.equal(drawStatusWords({}), "");
});

test("a drawing kept from the last window that no longer exists opens nothing and says nothing", () => {
  assert.equal(goneQuietly("d1", "d1", 404), true);
});

test("any other drawing that cannot be read is said", () => {
  assert.equal(goneQuietly("d2", "d1", 404), false, "one opened in this window");
  assert.equal(goneQuietly("d1", null, 404), false, "nothing came back from the last window");
  assert.equal(goneQuietly("d1", "d1", 500), false, "the node failed: the drawing may well be there");
  assert.equal(goneQuietly("d1", "d1", 0), false, "no answer at all");
  assert.equal(goneQuietly(null, null, 404), false);
});
