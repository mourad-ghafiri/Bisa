/**
 * Getting around: every destination the sidebar offers is a route the model
 * parses and prints, a detail screen lights its section, the palette's
 * chords reach the same doors, and a link built for a screen lands on it.
 * A source assertion for the sidebar's list, the models for the rest; no DOM.
 *
 * Run with `node --test desktop/src/scenarios/navigation.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { ROUTE_NAMES, href, parse, section } from "../routeModel.mjs";
import { COMMANDS, chordFor, resolveKeymap } from "../shell/keymapModel.mjs";

const nav = readFileSync(new URL("../shell/nav.ts", import.meta.url), "utf8");
/** The sidebar's destinations, read from the source: `key` and the route it opens. */
const destinations = [...nav.matchAll(/\{ key: "([a-z_]+)", glyph: "[a-z_]+", label: t\("([a-z0-9-]+)"\), icon: [^,]+, route: (\{[^}]+\}) \}/g)].map((m) => ({ key: m[1], label: m[2], route: JSON.parse(m[3].replace(/(\w+):/g, '"$1":').replace(/'/g, '"')) }));

test("every sidebar destination is a route the model prints and parses back, lit under its own key", () => {
  assert.equal(destinations.length, 7, `the sidebar lists ${destinations.length} destinations`);
  assert.deepEqual(destinations.map((d) => d.key), ["inbox", "agents", "teams", "projects", "workflows", "goals", "pulse"], "what concerns you first, what happened last");
  assert.ok(!destinations.some((d) => d.key === "triggers"), "no Triggers destination: a workflow's start events say what it listens for");
  for (const d of destinations) {
    assert.ok(ROUTE_NAMES.includes(d.route.name), `${d.label} opens a known route`);
    const link = href(d.route);
    assert.deepEqual(parse(link), d.route, `${link} lands on ${d.label}`);
    assert.equal(section(parse(link)), d.key, `${d.label} lights itself`);
  }
});

test("a detail screen keeps its section lit: a goal under Goals, a hosted channel under Channels, a workbench under Projects", () => {
  for (const [route, key] of [
    [{ name: "goal", id: "01G" }, "goals"],
    [{ name: "workflow", id: "01W" }, "workflows"],
    [{ name: "workbench", scope: "workstream", id: "01S" }, "projects"],
    [{ name: "hosted_channel", host: "ab".repeat(32), id: "general" }, "channels"],
    [{ name: "agent", id: "general-agent" }, "agents"],
  ]) {
    assert.equal(section(parse(href(route))), key);
  }
  assert.deepEqual(parse("#/goals/01G?tab=workflow"), { name: "goal", id: "01G" }, "the query rides along without changing the screen");
  assert.equal(href({ name: "goal", id: "01G" }, { tab: "workflow" }), "#/goals/01G?tab=workflow");
});

test("the palette's doors: the commands that open a screen exist under a chord in the default keymap, and every registered command has an id the document lists", () => {
  const keymap = resolveKeymap("default", null);
  for (const id of ["omnibox", "quick_open", "commands", "inbox", "settings", "board", "new_goal"]) {
    assert.ok(COMMANDS.some((c) => c.id === id), `${id} is a command`);
    const chord = chordFor(keymap, id);
    assert.ok(typeof chord === "string" && chord.length > 0, `${id} has a chord: ${chord}`);
  }
  const ids = COMMANDS.map((c) => c.id);
  assert.equal(new Set(ids).size, ids.length, "no command twice");
  assert.ok(ids.includes("next_conflict") && ids.includes("mark_resolved"), "the conflict document's commands are registered");
});
