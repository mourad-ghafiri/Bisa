/**
 * The sidebar, as the sources show it: its one toggle — the top chrome
 * collapses the sidebar to its rail and expands it again, and neither the
 * sidebar nor the rail draws a second door for the same state — and its one
 * order — the destinations dragged in the rows or the rail, read by both and
 * by the palette from one store. A source assertion, as
 * `conversations.test.mjs` makes them — no DOM.
 *
 * Run with `node --test desktop/src/scenarios/sidebar.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const src = (name) => readFileSync(new URL(name, import.meta.url), "utf8");

test("the sidebar's toggle is the header's alone", () => {
  assert.ok(src("../shell/TopChrome.tsx").includes("toggleWords(sidebarMode)"), "the top chrome names the toggle by the mode it would leave");
  assert.ok(!src("../shell/Sidebar.tsx").includes("toggleWords"), "the sidebar draws no toggle of its own");
  const rail = src("../shell/SidebarRail.tsx");
  assert.ok(!rail.includes("toggleWords"), "nor does the rail");
  assert.ok(!rail.includes("panelClosed") && !rail.includes("panelOpen"), "the rail wears no panel glyph: one door for one state");
  assert.ok(!src("../App.tsx").includes("onExpand"), "nothing threads a second door down");
});

test("the destinations' order is the person's, dragged in either sidebar and read from one store", () => {
  const sidebar = src("../shell/Sidebar.tsx");
  const rail = src("../shell/SidebarRail.tsx");
  for (const [name, text] of [["the sidebar", sidebar], ["the rail", rail]]) {
    assert.ok(text.includes("<SortableList"), `${name} is a sortable list`);
    assert.ok(text.includes('direction="vertical"'), `${name} sorts top to bottom`);
    assert.ok(text.includes("navRowDrag("), `${name} carries the destination payload`);
    assert.ok(text.includes("onReorder={placeNav}"), `${name} places through the store`);
    assert.ok(text.includes("usePrimaryNav()"), `${name} reads the person's order`);
    assert.ok(!text.includes("PRIMARY_NAV"), `${name} never reads the file's order`);
  }
  const palette = src("../shell/Omnibox.tsx");
  assert.ok(palette.includes("usePrimaryNav()") && !palette.includes("PRIMARY_NAV"), "the palette's Go to follows the same order");
  assert.ok(src("../shell/SidebarSection.tsx").includes("handle?: SortableHandle"), "a row takes the sortable's handle");
  assert.ok(src("../shell/SidebarSection.tsx").includes("role={undefined}"), "and stays a link");
  const store = src("../shell/navOrderStore.ts");
  assert.ok(store.includes("NAV_ORDER_KEY") && store.includes("orderKeys(") && store.includes("useSyncExternalStore"), "one store, kept per viewer, made whole by the model");
  assert.ok(src("../views/_settings/AppearancePanel.tsx").includes("resetNavOrder"), "Settings › Appearance offers the way back");
});

test("a destination's door leads to where the person was in it: the rows, the rail and the palette read one rule", () => {
  assert.ok(src("../shell/Sidebar.tsx").includes("useSectionHref(entry.route)"), "the sidebar's rows");
  assert.ok(src("../shell/SidebarRail.tsx").includes("useSectionHref(door.route)"), "the rail's doors");
  const palette = src("../shell/Omnibox.tsx");
  assert.ok(palette.includes("goToSection(route)") && palette.includes("run: () => enter(entry.route)"), "the palette's Go to");
  const door = src("../shell/sectionDoor.ts");
  assert.ok(door.includes("sectionHash(places, key, at)") && door.includes("sectionHash(placesNow(), section(route), address())"), "one rule, the model's");
});

test("where nothing is remembered the app opens on the first destination of the person's order: the router's home is the store's, and nobody parses a hash without one", () => {
  const router = src("../router.ts");
  assert.ok(router.includes("parse(hash, homeRoute())") && router.includes("parse(address(), homeRoute())"), "one home for the hook and for the plain read");
  assert.ok(router.includes("launchPlace()") && router.includes("land(given, exact)"), "a launch opens where the app closed, and every address is resolved against the memory");
  assert.ok(!router.includes("getSnapshot = (): string => window.location.hash"), "nothing reads the window's hash unresolved");
  assert.ok(src("../shell/navOrderStore.ts").includes("export function homeRoute()"), "the store says where home is");
  assert.ok(!src("../routeModel.mjs").includes("DEFAULT_ROUTE") && src("../routeModel.mjs").includes("export function parse(hash, home)"), "the model names no landing of its own");
  for (const file of ["../shell/browserDoors.ts", "../views/_workbench/agentPaneStore.ts"]) {
    assert.ok(src(file).includes("currentRoute()") && !src(file).includes("parse(window.location.hash"), `${file} reads the route through the router`);
  }
});
