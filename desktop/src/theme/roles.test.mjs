/**
 * The theme contract, enforced.
 *
 * A theme that forgets a role does not look wrong — it looks *absent*. The
 * variable resolves to nothing, the utility that referenced it drops out, and
 * you get a button with no background on a surface of the same colour: still
 * focusable, still clickable, invisible. Nobody files that bug, because
 * nobody can see the thing they would be filing it about.
 *
 * So the roles are read out of `tokens.css` rather than restated here. Adding
 * a role to the contract fails every theme until every theme answers it,
 * which is the only ordering that cannot ship a hole.
 *
 * Run with `npm test` from `desktop/`. (`node --test src/theme` — the
 * directory form — does not work on Node 24; it needs the file or a glob.)
 */

import { strict as assert } from "node:assert";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const THEMES_DIR = join(HERE, "themes");

/** CSS comments would otherwise contribute selectors and properties. */
function strip(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/**
 * Every declaration block in a stylesheet, flattened out of any at-rules that
 * wrap them. A `@media (prefers-color-scheme: dark)` block is not an excuse
 * to define half a theme, so its contents are checked like any other.
 *
 * Returns `{ selector, props: Set<string> }`, where `props` includes plain
 * CSS properties such as `color-scheme` alongside the custom ones.
 */
function blocks(css) {
  const out = [];
  const walk = (text) => {
    let i = 0;
    while (i < text.length) {
      const open = text.indexOf("{", i);
      if (open === -1) return;
      let depth = 1;
      let close = open + 1;
      while (close < text.length && depth > 0) {
        if (text[close] === "{") depth++;
        else if (text[close] === "}") depth--;
        close++;
      }
      const head = text.slice(i, open).trim();
      const body = text.slice(open + 1, close - 1);
      if (head.startsWith("@")) walk(body);
      else if (head) {
        const props = new Set();
        for (const decl of body.split(";")) {
          const name = decl.split(":")[0]?.trim();
          if (name && !name.includes("{")) props.add(name);
        }
        out.push({ selector: head.replace(/\s+/g, " "), props });
      }
      i = close;
    }
  };
  walk(css);
  return out;
}

/** The role names between the markers in `tokens.css`. */
function contract() {
  // Read before stripping: the markers are themselves comments.
  const css = readFileSync(join(HERE, "tokens.css"), "utf8");
  const region = css.match(/@roles:start\s*\*\/([\s\S]*?)\/\*\s*@roles:end/);
  assert.ok(region, "tokens.css lost its @roles:start / @roles:end markers");
  const names = [...region[1].matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]);
  assert.ok(names.length > 0, "the role contract is empty");
  return names;
}

const ROLES = contract();
const THEME_FILES = readdirSync(THEMES_DIR).filter((f) => f.endsWith(".css"));

test("there is at least one theme, and glass is one of them", () => {
  assert.ok(THEME_FILES.includes("glass.css"), "glass is the default family and must exist");
});

for (const file of THEME_FILES) {
  const css = strip(readFileSync(join(THEMES_DIR, file), "utf8"));
  const declared = blocks(css);

  test(`${file} declares at least one palette`, () => {
    assert.ok(declared.length > 0, `${file} defines no selectors`);
  });

  for (const { selector, props } of declared) {
    test(`${file} — ${selector} supplies every role`, () => {
      const missing = ROLES.filter((r) => !props.has(r));
      assert.deepEqual(missing, [], `${selector} is missing ${missing.join(", ")}`);
    });

    test(`${file} — ${selector} states its color-scheme`, () => {
      // Without it the webview paints native scrollbars, form controls and
      // autofill from the wrong side, on top of a correctly themed page.
      assert.ok(props.has("color-scheme"), `${selector} does not set color-scheme`);
    });
  }
}

test("accent overlays touch only the accent roles", () => {
  const css = strip(readFileSync(join(HERE, "accents.css"), "utf8"));
  const allowed = new Set([
    "--color-accent",
    "--color-accent-ink",
    "--color-accent-soft",
    "--color-accent-contrast",
  ]);
  for (const { selector, props } of blocks(css)) {
    for (const p of props) {
      assert.ok(
        allowed.has(p),
        `${selector} sets ${p}; an accent choice must not repaint anything but the accent`,
      );
    }
    const missing = [...allowed].filter((r) => !props.has(r));
    assert.deepEqual(missing, [], `${selector} is missing ${missing.join(", ")}`);
  }
});
