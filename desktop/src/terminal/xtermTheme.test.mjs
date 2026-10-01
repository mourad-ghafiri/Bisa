/**
 * The terminal's half of the theme contract.
 *
 * `theme/roles.test.mjs` proves every theme answers every role. This proves
 * the terminal only ever asks for roles that are in that contract — the other
 * end of the same rope. Ask for `--color-terminal-bg` because it sounded
 * right, and no theme answers, and xterm is handed an empty string: the panel
 * either throws on construction or renders transparent over whatever is
 * behind it. Neither failure names the missing role.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { TERMINAL_ROLES, xtermTheme } from "./xtermTheme.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

/** The role names between the markers in `theme/tokens.css`. */
function contract() {
  const css = readFileSync(join(HERE, "..", "theme", "tokens.css"), "utf8");
  const region = css.match(/@roles:start\s*\*\/([\s\S]*?)\/\*\s*@roles:end/);
  assert.ok(region, "tokens.css lost its @roles:start / @roles:end markers");
  return new Set([...region[1].matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
}

const ANSI_NAMES = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
];

/** Every role answered with a colour that is obviously not a fallback. */
function fullyThemed() {
  const out = {};
  for (const [i, role] of TERMINAL_ROLES.entries()) {
    out[role] = `#0000${(0x11 + i).toString(16)}`;
  }
  return out;
}

test("every role the terminal reads is one the theme contract supplies", () => {
  const roles = contract();
  const missing = TERMINAL_ROLES.filter((r) => !roles.has(r));
  assert.deepEqual(missing, [], `tokens.css declares no ${missing.join(", ")}`);
});

test("the chrome comes from the tokens and nothing else", () => {
  const resolved = fullyThemed();
  const theme = xtermTheme(resolved, "dark");
  assert.equal(theme.background, resolved["--color-surface"]);
  assert.equal(theme.foreground, resolved["--color-text"]);
  assert.equal(theme.cursor, resolved["--color-accent"]);
  assert.equal(theme.selectionBackground, resolved["--color-accent-soft"]);
  assert.equal(theme.selectionInactiveBackground, resolved["--color-surface-2"]);
});

test("a block cursor never eats the character underneath it", () => {
  const theme = xtermTheme(fullyThemed(), "light");
  assert.equal(theme.cursorAccent, theme.background);
  assert.notEqual(theme.cursorAccent, theme.cursor);
});

test("a role the page could not resolve falls back to something visible", () => {
  for (const scheme of ["light", "dark"]) {
    const theme = xtermTheme({}, scheme);
    for (const [key, value] of Object.entries(theme)) {
      assert.match(value, /^#[0-9a-f]{6}$/i, `${scheme} ${key} is ${value ?? "absent"}`);
    }
    assert.notEqual(
      theme.background,
      theme.foreground,
      `${scheme} would render invisible text`,
    );
  }
});

test("a blank token value is a fallback, not a colour", () => {
  // getPropertyValue returns "" for a property no theme set, and " " for one
  // set to nothing at all. Passing either straight through makes xterm throw.
  const theme = xtermTheme({ "--color-surface": "  ", "--color-text": "" }, "dark");
  assert.equal(theme.background, "#16161a");
  assert.equal(theme.foreground, "#e4e4e9");
});

test("both sides of the ansi ramp are complete", () => {
  for (const scheme of ["light", "dark"]) {
    const theme = xtermTheme(fullyThemed(), scheme);
    for (const name of ANSI_NAMES) {
      assert.match(theme[name] ?? "", /^#[0-9a-f]{6}$/i, `${scheme} has no ${name}`);
    }
  }
});

test("the ansi ramp ignores the accent, so a diff does not change hue with it", () => {
  const accented = xtermTheme({ ...fullyThemed(), "--color-accent": "#ff00ff" }, "dark");
  const plain = xtermTheme(fullyThemed(), "dark");
  for (const name of ANSI_NAMES) {
    assert.equal(accented[name], plain[name], `${name} moved with the accent`);
  }
  assert.notEqual(accented.cursor, plain.cursor, "the cursor is meant to follow the accent");
});

test("light and dark get different ramps", () => {
  const light = xtermTheme(fullyThemed(), "light");
  const dark = xtermTheme(fullyThemed(), "dark");
  const shared = ANSI_NAMES.filter((n) => light[n] === dark[n]);
  assert.deepEqual(shared, [], `${shared.join(", ")} is the same on both sides`);
});

test("an unknown scheme is treated as light rather than left undefined", () => {
  const light = xtermTheme(fullyThemed(), "light");
  for (const odd of [null, undefined, "", "sepia", "system"]) {
    assert.deepEqual(xtermTheme(fullyThemed(), odd), light, `${odd} did not fall back to light`);
  }
});
