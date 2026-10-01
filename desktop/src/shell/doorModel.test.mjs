/**
 * A door's request waits for its listener. Run with
 * `node --test desktop/src/shell/doorModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { DOOR_TTL_MS, emptyDoors, pendDoor, takeDoor } from "./doorModel.mjs";

test("a request fired before its listener mounts is delivered on mount, once", () => {
  const doors = pendDoor(emptyDoors(), "bisa:new-goal", undefined, 1_000);
  const first = takeDoor(doors, "bisa:new-goal", 1_500);
  assert.deepEqual(first.request, { detail: undefined });
  const second = takeDoor(first.doors, "bisa:new-goal", 1_600);
  assert.equal(second.request, null, "taken once");
  assert.deepEqual(second.doors, emptyDoors());
});

test("a request carries its detail, and a newer request for the same door replaces the older", () => {
  let doors = pendDoor(emptyDoors(), "bisa:new-workstream", { pid: "A" }, 1_000);
  doors = pendDoor(doors, "bisa:new-workstream", { pid: "B" }, 1_200);
  assert.deepEqual(takeDoor(doors, "bisa:new-workstream", 1_300).request, { detail: { pid: "B" } });
});

test("a stale request is dropped, not delivered", () => {
  const doors = pendDoor(emptyDoors(), "bisa:new-goal", undefined, 1_000);
  const late = takeDoor(doors, "bisa:new-goal", 1_000 + DOOR_TTL_MS + 1);
  assert.equal(late.request, null);
  assert.deepEqual(late.doors, emptyDoors(), "and forgotten");
  assert.ok(DOOR_TTL_MS >= 5_000 && DOOR_TTL_MS <= 30_000, String(DOOR_TTL_MS));
});

test("a door with nothing waiting hands over nothing and changes nothing", () => {
  const doors = pendDoor(emptyDoors(), "bisa:new-goal", undefined, 1_000);
  const other = takeDoor(doors, "bisa:new-channel", 1_100);
  assert.equal(other.request, null);
  assert.equal(other.doors, doors);
});

/** Every `.ts` / `.tsx` source under `desktop/src`, tests left out. */
function sources(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) sources(path, out);
    else if (/\.tsx?$/.test(name) && !name.includes(".test.")) out.push(path);
  }
  return out;
}

test("every door `fire` opens is listened at through `onDoor`, never a raw listener", () => {
  // `shortcuts.fire` dispatches only to a listener `onDoor` counted; a
  // listener the window was given by hand leaves its door shut with no
  // error — the way ⌘N once went quiet. So the rule is read off the source.
  const root = new URL("..", import.meta.url).pathname;
  const files = sources(root).map((path) => ({ path: path.slice(root.length), text: readFileSync(path, "utf8") }));
  const shortcuts = files.find((f) => f.path === "shell/shortcuts.ts");
  assert.ok(shortcuts, "shell/shortcuts.ts");
  const exported = new Set([...shortcuts.text.matchAll(/^export const ([A-Z_]+) = "bisa:/gm)].map((m) => m[1]));
  const fired = new Set(files.flatMap((f) => [...f.text.matchAll(/\bfire\(([A-Z_]+)/g)].map((m) => m[1])).filter((id) => exported.has(id)));
  assert.ok(fired.has("NEW_DOCUMENT") && fired.has("NEW_GOAL") && fired.has("SELECT_TAB_AT"), [...fired].join(", "));
  for (const f of files) {
    for (const door of fired) {
      assert.doesNotMatch(f.text, new RegExp(`addEventListener\\(\\s*${door}\\b`), `${f.path} listens at ${door} by hand; stand at it through onDoor`);
    }
  }
  for (const door of fired) {
    assert.doesNotMatch(shortcuts.text, new RegExp(`new CustomEvent\\(\\s*${door}\\b`), `${door} is dispatched by hand; open it through fire`);
  }
});
