/**
 * A sentence that sends a person to a Settings panel says where it is as the
 * rail shows it: *Settings › group › panel*. The rail is one model
 * (`views/_settings/settingsLink.mjs` — `SETTINGS_GROUPS`, `settingsPath`):
 * `Settings.tsx` draws from it, a link's label is `settingsPath(tab)`, and
 * this guard holds every path a catalog sentence spells to a real group and a
 * real panel of it — so *Settings › Editor* (no such group) or *Settings ›
 * Decision Making* (a panel with its group left off) cannot come back.
 * Source assertions over the catalog, no DOM. Run with
 * `node --test --import ./src/i18n/preload.mjs desktop/src/scenarios/settingsPaths.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { t } from "../i18n/l10n.mjs";
import { SETTINGS_GROUPS, SETTINGS_TABS, settingsPath, settingsTab } from "../views/_settings/settingsLink.mjs";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(src, "..", "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");

/**
 * The catalogs the guard reads, under `locales/en/`: the desktop's own, and
 * the crates' files beside the folder (`settings.ftl`, `errors.ftl`,
 * `engine.ftl`, `problems.ftl`, `cli.ftl`, …) — every sentence a person can
 * read, whichever side says it, spells a path as the rail does.
 */
const CATALOG_FOLDERS = ["desktop", ""];

/** Every message line of the English catalogs in reach, as `{ file, id, text }` — comments left out, a value's continuation lines joined to its id. */
function messages(folders = CATALOG_FOLDERS) {
  const out = [];
  for (const folder of folders) {
    const dir = join(root, "locales", "en", folder);
    for (const name of readdirSync(dir).sort()) {
      if (!name.endsWith(".ftl")) continue;
      const file = relative(root, join(dir, name));
      let id = null;
      for (const line of readFileSync(join(dir, name), "utf8").split("\n")) {
        if (line.startsWith("#")) continue;
        const head = /^(-?[a-z0-9-]+) = /.exec(line);
        if (head) id = head[1];
        if (id && line.trim()) out.push({ file, id, text: line });
      }
    }
  }
  return out;
}

/** The rail's paths, longest first, so *Relays & sync* is tried before a shorter label that begins it. */
const RAIL = SETTINGS_GROUPS.flatMap((g) => g.panels.map((p) => ({ group: g.label, panel: p.label, path: `${g.label} › ${p.label}` }))).sort((a, b) => b.path.length - a.path.length);

/**
 * The faults of one line: each `Settings › …` that is not a group and a panel
 * of the rail, with what it says and what it should say. *System Settings ›*
 * (the Mac's), *Xcode › Settings ›* and *About › Settings ›* (the IDE's About
 * tab) are other places and are left alone, and so is the rail's own path
 * message, whose group and panel are arguments. A panel said by a placeable
 * (`Settings › Git & code hosts › { $label }`) is held to its group.
 */
export function pathFaults(text) {
  const faults = [];
  for (const m of text.matchAll(/Settings › /g)) {
    const before = text.slice(0, m.index);
    // Another product's Settings, the IDE's About tab, or the tail of the rail's own group *Decision Settings*.
    if (/(System |Decision |› )$/.test(before)) continue;
    const rest = text.slice(m.index + m[0].length);
    if (rest.startsWith("{ $group } › { $panel }")) continue; // the model's own message
    const hit = RAIL.find((r) => rest.startsWith(r.path) && !/^[\p{L}\p{N}]/u.test(rest.slice(r.path.length)));
    if (hit) continue;
    if (SETTINGS_GROUPS.some((g) => rest.startsWith(`${g.label} › { $`))) continue;
    const said = `Settings › ${rest.split(/[.,;:)*<\n]| — | is | has | sets | lists | turns | says | on the | \{ /)[0].trim()}`;
    // What it meant: the panels whose label the words open with — one when the label is one panel's, several when two groups share it.
    const meant = RAIL.filter((r) => rest.startsWith(r.panel) && !/^[\p{L}\p{N}]/u.test(rest.slice(r.panel.length))).sort((a, b) => b.panel.length - a.panel.length);
    const longest = meant.filter((r) => r.panel.length === meant[0].panel.length);
    faults.push({ said, should: longest.length ? longest.map((r) => `Settings › ${r.path}`).join(" | ") : "a group and a panel of the rail" });
  }
  return faults;
}

test("the guard's own reader: a full path passes, a short one is named with what it should say, and another product's Settings is left alone", () => {
  assert.deepEqual(pathFaults("Off in Settings › Capabilities › Browser."), []);
  assert.deepEqual(pathFaults("(Settings › Project IDE › Editor)"), []);
  assert.deepEqual(pathFaults("Settings › Workspace › Relays & sync."), []);
  assert.deepEqual(pathFaults("Settings › Desktop › Where you were"), [{ said: "Settings › Desktop › Where you were", should: "Settings › Capabilities › Desktop" }], "a card is said after its panel, never in place of the group");
  assert.deepEqual(pathFaults("Settings › Capabilities › Desktop › Where you were"), []);
  assert.deepEqual(pathFaults("sign in under Settings › Git & code hosts › { $label }"), [], "a panel said by a placeable is held to its group");
  assert.deepEqual(pathFaults("turned off in Settings › Browser."), [{ said: "Settings › Browser", should: "Settings › Capabilities › Browser" }]);
  assert.deepEqual(pathFaults("Open Settings › Decision Making"), [{ said: "Settings › Decision Making", should: "Settings › Decision Settings › Decision Making" }]);
  assert.deepEqual(pathFaults("see Settings › Identity"), [{ said: "Settings › Identity", should: "Settings › Git & code hosts › Identity | Settings › You › Identity" }], "a label two groups share names both");
  assert.deepEqual(pathFaults("Settings › Security has the rest"), [{ said: "Settings › Security", should: "a group and a panel of the rail" }], "a group alone is not a place");
  assert.deepEqual(pathFaults("Settings › Capabilities › Browsers"), [{ said: "Settings › Capabilities › Browsers", should: "a group and a panel of the rail" }], "a label is matched whole");
  assert.deepEqual(pathFaults("System Settings › Notifications › Bisa; Xcode › Settings › Components; About › Settings › Workstream scripts"), []);
});

test("every Settings path a catalog sentence spells is a group and a panel of the rail", () => {
  const lines = messages();
  assert.ok(lines.length > 5000, `the walk found ${lines.length} message lines; it is broken`);
  const faults = [];
  let paths = 0;
  for (const { file, id, text } of lines) {
    paths += (text.match(/Settings › /g) ?? []).length;
    for (const f of pathFaults(text)) faults.push(`${file}: ${id} says "${f.said}" — should say "${f.should}"`);
  }
  assert.ok(paths >= 20, `${paths} paths read: the sentences that send a person to a panel are in reach`);
  for (const crates of ["errors.ftl", "settings.ftl", "problems.ftl", "cli.ftl"]) {
    const theirs = lines.filter((l) => l.file === join("locales", "en", crates));
    assert.ok(theirs.some((l) => l.text.includes("Settings › ")), `${crates} is read: the crates' sentences are held to the rail as the desktop's are`);
  }
  assert.deepEqual(faults, []);
});

test("the rail is one model: every panel has a group, a label and a line, the path is the rail's words, and Settings draws from it", () => {
  assert.equal(new Set(SETTINGS_TABS).size, SETTINGS_TABS.length, "a tab id is one panel");
  assert.equal(new Set(SETTINGS_GROUPS.map((g) => g.id)).size, SETTINGS_GROUPS.length);
  assert.equal(new Set(SETTINGS_GROUPS.map((g) => g.label)).size, SETTINGS_GROUPS.length, "a group's label is its own — it is what tells two panels of one label apart");
  assert.equal(new Set(RAIL.map((r) => r.path)).size, SETTINGS_TABS.length, "no two panels are at one path");
  for (const g of SETTINGS_GROUPS)
    for (const p of g.panels) {
      assert.ok(p.label && !p.label.includes("screens-") && p.blurb && !p.blurb.includes("screens-"), `${p.id} has its words in the catalog`);
      assert.equal(settingsPath(p.id), `Settings › ${g.label} › ${p.label}`);
      assert.equal(settingsPath(p.id, { within: true }), `${g.label} › ${p.label}`);
      assert.deepEqual(pathFaults(`in ${settingsPath(p.id)}.`), [], "what the model says passes the guard that holds the catalog");
    }
  assert.equal(settingsPath("browser"), "Settings › Capabilities › Browser");
  assert.equal(settingsPath("decision-making"), "Settings › Decision Settings › Decision Making");
  assert.equal(settingsPath("editor"), "Settings › Project IDE › Editor");
  assert.equal(settingsPath("git"), "Settings › Git & code hosts › Identity");
  assert.equal(settingsPath("identity"), "Settings › You › Identity");
  assert.equal(settingsPath("nonesuch"), settingsPath(settingsTab("nonesuch")), "an id that is no panel is said as the link to it lands");
  assert.equal(t("screens-settings-path", { group: "G", panel: "P" }), "Settings › G › P");

  // The type other screens link with names exactly the rail's panels.
  const declared = [...read("views/_settings/settingsLink.d.mts").matchAll(/^ {2}\| "([a-z-]+)"/gm)].map((m) => m[1]);
  assert.deepEqual(declared, [...SETTINGS_TABS], "SettingsTab is the rail's ids, in rail order — one declared and not drawn would type-check and land on Identity");

  // The screen holds no structure of its own: groups, panels, labels and lines are the model's; it adds a glyph per panel.
  const screen = read("views/Settings.tsx");
  assert.ok(screen.includes("{SETTINGS_GROUPS.map((g) => (") && screen.includes("SETTINGS_GROUPS.flatMap((g) => g.panels)"), "the rail and the panel list are read off the model");
  assert.ok(screen.includes("const GLYPH: Record<SettingsTab, LucideIcon> = {"), "a glyph for every panel id, or the compiler refuses");
  assert.ok(!/\blabel: t\(/.test(screen) && !/\bblurb:/.test(screen), "no label or line is spelt in the screen");
  for (const id of SETTINGS_TABS) assert.ok(screen.includes(`panel.id === "${id}" &&`), `${id} draws a panel`);

  // A link's label is the path itself — never a message that repeats it.
  for (const [file, tab] of [
    ["shell/BrowserOverlay.tsx", "browser"],
    ["shell/NetworkOverlay.tsx", "network"],
    ["shell/NodeOverlay.tsx", "node"],
    ["shell/PetOverlay.tsx", "pet"],
    ["shell/AddonsOverlay.tsx", "addons"],
    ["views/_settings/SecurityPanels.tsx", "decision-making"],
  ])
    assert.ok(read(file).includes(`{settingsPath("${tab}")}`), `${file} labels its door with the rail's path`);
  const bare = messages().filter(({ id, text }) => RAIL.some((r) => text === `${id} = Settings › ${r.path}` || text === `${id} = ${r.path}`));
  assert.deepEqual(bare.map((m) => m.id), [], "a message that is only a path repeats the model: say settingsPath(tab)");
});
