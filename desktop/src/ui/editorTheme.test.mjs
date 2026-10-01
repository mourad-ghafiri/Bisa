/**
 * The editor theme is a function of the token roles, and it covers every
 * role the theme contract requires — read from the markers in `tokens.css`
 * rather than restated here, the way `theme/roles.test.mjs` does.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { EDITOR_ROLE_MAP, editorThemeFor, isHex } from "./editorTheme.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));

function contractRoles() {
  const css = readFileSync(join(HERE, "../theme/tokens.css"), "utf8");
  const block = css.match(/@roles:start \*\/([\s\S]*?)\/\* @roles:end/);
  assert.ok(block, "the roles markers are not where this test expects them");
  return [...block[1].matchAll(/--color-([a-z0-9-]+):/g)].map((m) => m[1]);
}

test("every colour role in the contract maps to at least one Monaco colour", () => {
  for (const role of contractRoles()) {
    assert.ok(EDITOR_ROLE_MAP[role]?.length > 0, `role ${role} has no Monaco colour`);
  }
});

test("the map names only roles that exist, so a renamed role fails here", () => {
  const roles = new Set(contractRoles());
  for (const role of Object.keys(EDITOR_ROLE_MAP)) {
    assert.ok(roles.has(role), `${role} is not a role in tokens.css`);
  }
});

test("a full set of roles paints every mapped key and the syntax rules", () => {
  const roles = Object.fromEntries(contractRoles().map((r, i) => [r, `#${String(i + 10).padStart(2, "0")}a0b0`]));
  const { theme, missing } = editorThemeFor(roles, "light");
  assert.deepEqual(missing, []);
  assert.equal(theme.base, "vs");
  assert.equal(theme.colors["editor.background"], roles.bg);
  assert.equal(theme.colors["editor.foreground"], roles.text);
  assert.ok(theme.rules.some((r) => r.token === "keyword"));
  assert.equal(editorThemeFor(roles, "dark").theme.base, "vs-dark");
});

test("a missing or malformed role is reported, never painted black", () => {
  const { theme, missing } = editorThemeFor({ bg: "#ffffff", text: "not a colour" }, "light");
  assert.ok(missing.includes("text"));
  assert.ok(missing.includes("surface"));
  assert.equal(theme.colors["editor.background"], "#ffffff");
  assert.equal(theme.colors["editor.foreground"], undefined);
  assert.equal(isHex("#abcdef"), true);
  assert.equal(isHex("#abcdef80"), true);
  assert.equal(isHex("rgb(1, 2, 3)"), false);
});
