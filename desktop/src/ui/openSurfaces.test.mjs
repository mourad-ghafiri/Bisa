/**
 * Every kit surface says when it opens (ide/18 §The browser tab): the browser
 * tab's page is a native view that paints above every DOM element, so a
 * surface that does not count itself is drawn under the page wherever the two
 * overlap — a right-click menu from the explorer hidden behind the page it
 * should sit over. This guard reads the kit's sources: a component that
 * portals at the `z-50` tier registers with `useSurface` or `useOpenSurface`,
 * or is named here with the reason it must not. Run with
 * `node --test desktop/src/ui/openSurfaces.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { basename, dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { sourceFiles } from "../testWalk.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const read = (rel) => readFileSync(join(here, rel), "utf8");

/** A surface the page must never yield to, each with its reason — and the fact that keeps the reason true. */
const ALLOWED = Object.freeze({
  "Tooltip.tsx": { why: "hover-driven and pointer-events-none: counting it would blank the page on every hover", holds: "pointer-events-none" },
});

/** Whether a source portals a surface at the tier every dialog and menu is drawn at. */
// The tier is spelled `z-50`, or worn through a kit class that carries it: `POPOVER_SURFACE` (`ui/surfaces.ts`),
// or a dialog's `OVERLAY` and `PANEL` (`ui/dialogLayout.mjs`).
const wearsTheTier = (text) => text.includes("z-50") || text.includes("POPOVER_SURFACE") || (text.includes('from "./dialogLayout.mjs"') && text.includes("OVERLAY"));
const portalsAtTheTier = (text) => (text.includes("createPortal(") || text.includes(".Portal>")) && wearsTheTier(text);

test("every kit surface that portals at the z-50 tier counts itself, or is allowed here with its reason", () => {
  const files = sourceFiles(here, (p) => p.endsWith(".tsx"));
  assert.ok(files.length > 10, "the walk reaches the kit");
  const surfaces = files.filter((p) => portalsAtTheTier(readFileSync(p, "utf8")));
  for (const name of ["ContextMenu.tsx", "Menu.tsx", "Popover.tsx", "Dialog.tsx", "LinkCard.tsx", join("artifact", "ArtifactStage.tsx"), "Tooltip.tsx"]) {
    assert.ok(surfaces.some((p) => relative(here, p) === name), `${name} is a portalled surface this guard reaches`);
  }
  for (const p of surfaces) {
    const text = readFileSync(p, "utf8");
    const name = basename(p);
    const allowance = ALLOWED[name];
    if (allowance) {
      assert.ok(text.includes(allowance.holds), `${name} is allowed because it is ${allowance.holds}; the reason must still hold`);
      assert.ok(!/use(Open)?Surface\(/.test(text), `${name} must not count: ${allowance.why}`);
      continue;
    }
    assert.ok(/use(Open)?Surface\(/.test(text), `${relative(here, p)} portals at z-50 and never says it opened — the browser's page would hide it`);
  }
});

test("the classes a surface wears the tier through still carry it", () => {
  assert.ok(read("surfaces.ts").includes("z-50"), "POPOVER_SURFACE is at the tier");
  for (const name of ["OVERLAY", "PANEL"]) {
    const line = read("dialogLayout.mjs").split("export const ").find((c) => c.startsWith(`${name} =`));
    assert.ok(line && line.includes("z-50"), `dialogLayout's ${name} is at the tier`);
  }
});

test("a toast never hides the page, and the store hands out what a wait for the layer needs", () => {
  assert.ok(!read("Toast.tsx").includes("useSurface("), "a toast is read over the page, never the reason the page hides");
  const store = read("openSurfaces.ts");
  for (const fact of ["export function subscribeSurfaces(", "export function useOpenSurface(", "export function useSurface(", "export function layerMayShowNow("]) {
    assert.ok(store.includes(fact), `the store ${fact}`);
  }
  assert.ok(store.includes("useSyncExternalStore(subscribeSurfaces"), "the React reader and the wait subscribe to the same listeners");
});

test("the two menus are one surface and one close: both own their open state and run the chosen item after the menu has left", () => {
  for (const name of ["Menu.tsx", "ContextMenu.tsx"]) {
    const text = read(name);
    assert.ok(text.includes("useOpenSurface()"), `${name} owns its open state as a surface`);
    assert.ok(text.includes("onOpenChange={setOpen}"), `${name} hands the setter to Radix`);
    assert.ok(text.includes("useChosenOnClose()"), `${name} runs the chosen item the shared way`);
    assert.ok(text.includes("onCloseAutoFocus={chosen.onCloseAutoFocus}"), `${name} waits for the menu's last word`);
    assert.ok(!text.includes("useRef<(() => void) | null>"), `${name} keeps no chosen ref of its own`);
  }
  const hook = read("useChosenOnClose.ts");
  assert.ok(hook.includes("if (item.immediate) item.onSelect();"), "an immediate item runs in the gesture");
  assert.ok(hook.includes("e.preventDefault();"), "the action takes the focus the trigger would have");
  for (const name of ["LinkCard.tsx", join("artifact", "ArtifactStage.tsx")]) {
    assert.ok(read(name).includes("useSurface(true)"), `${name} is a surface for as long as it is mounted`);
  }
});
