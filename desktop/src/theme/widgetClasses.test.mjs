/**
 * The app's utilities never restyle a bundled widget.
 *
 * Tailwind reads every word in the sources as a class it might make, and a
 * bare utility is a global selector — `.visible`, `.contents`. The widgets
 * the app bundles — xterm, Monaco, Excalidraw, the designer's canvas — wear
 * class names of their own, and where one is also a utility, the utility
 * restyles the widget. That is how a hidden terminal's scrollbar, wearing
 * `visible` while it scrolled, showed through its hidden layer and brought the
 * layer's frost back over the page whenever a shell printed (`styles.css`,
 * `theme/material.css`).
 *
 * So this file compiles the real stylesheet with Tailwind's own compiler and
 * holds three things:
 *
 * - every class a widget's stylesheet styles is no utility of ours — or one
 *   that means the same in the widget, or one whose property the widget's own
 *   rule sets (unlayered, it outranks `@layer utilities`), each allowed with
 *   its reason; any other is a collision a person decides on;
 * - every name `styles.css` leaves out (`@source not inline`) is worn by a
 *   widget, is never made, and its spelled-out form is;
 * - no class string in the app uses a left-out name, which would style nothing.
 *
 * A source guard, as `ui/platformMark.test.mjs` is; the parser is the one
 * `i18n/ratchetModel.mjs` reads sources with.
 *
 * Run with `npm test` from `desktop/`.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { compile } from "tailwindcss";
import ts from "typescript";
import { sourceFiles } from "../testWalk.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC = join(HERE, "..");
const MODULES = join(SRC, "..", "node_modules");
const require = createRequire(import.meta.url);

/** The stylesheets each bundled widget ships — the ones the app imports, or the widget's code does. */
const WIDGETS = Object.freeze({
  xterm: [join(MODULES, "@xterm/xterm/css/xterm.css")],
  monaco: sourceFiles(join(MODULES, "monaco-editor/esm"), (p) => p.endsWith(".css")),
  excalidraw: [join(MODULES, "@excalidraw/excalidraw/dist/prod/index.css")],
  xyflow: [join(MODULES, "@xyflow/react/dist/style.css")],
});

/** A widget class that is also a utility, allowed: it means the same thing in the widget. */
const SAME_MEANING = Object.freeze({
  fixed: "Monaco's context view is `position: fixed`, as the utility is",
  hidden: "Monaco's hover and the rest hide with `display: none`, as the utility does",
  italic: "Monaco's icon label is `font-style: italic`, as the utility is",
});

/** A widget class that is also a utility, allowed: the widget's own unlayered rule sets the utility's property, and wins. */
const WIDGET_WINS = Object.freeze({
  inline: "Monaco's references widget sets `display: inline-block` on its `.inline`",
  shadow: "xterm's and Monaco's scrollbar shadows set `display` and their own `box-shadow` on `.shadow`",
  static: "Excalidraw's `.excalidraw__canvas` is `position: absolute`, so its `.static` canvas stays put",
});

/** The helpers whose arguments are class lists — the same four `i18n/ratchetModel.mjs` reads as classes. */
const CLASS_HELPERS = Object.freeze(["cn", "clsx", "cva", "twMerge"]);

/** `@import`s resolved as Vite's Tailwind plugin resolves them: from the importing file's folder, or from `node_modules`. */
async function loadStylesheet(id, base) {
  const path = id.startsWith(".") || id.startsWith("/") ? resolve(base, id) : require.resolve(id === "tailwindcss" ? "tailwindcss/index.css" : id, { paths: [base] });
  return { path, base: dirname(path), content: readFileSync(path, "utf8") };
}

const STYLES = readFileSync(join(SRC, "styles.css"), "utf8");

/** A fresh compiler over the real stylesheet; a compiler remembers every candidate it was given. */
const compiler = () => compile(STYLES, { base: SRC, loadStylesheet });

/**
 * The bare class names — `.name`, no variant, no escape — that `css`'s
 * utilities layer makes. The layer alone, to its closing brace: the app's own
 * unlayered rules after it (`theme/flow.css` styles xyflow's classes on
 * purpose) are not utilities.
 */
function bareUtilities(css) {
  const open = css.indexOf("{", css.indexOf("@layer utilities"));
  if (css.indexOf("@layer utilities") < 0 || open < 0) return new Set();
  let depth = 0;
  let end = open;
  for (; end < css.length; end++) {
    if (css[end] === "{") depth++;
    else if (css[end] === "}" && --depth === 0) break;
  }
  return new Set([...css.slice(open, end).matchAll(/(?:^|[\s{};,])\.([_a-zA-Z][\w-]*)\s*\{/g)].map((m) => m[1]));
}

/** Every class a stylesheet styles, its comments and `url(…)`s (`.svg`, `.ttf`) left out. */
function classesOf(file) {
  const css = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/url\([^)]*\)/g, "");
  return new Set([...css.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1]));
}

/** Each widget's class names. */
const WIDGET_CLASSES = new Map(Object.entries(WIDGETS).map(([widget, files]) => [widget, new Set(files.flatMap((f) => [...classesOf(f)]))]));

/** The names `styles.css` tells Tailwind never to make. */
function leftOut() {
  const css = STYLES.replace(/\/\*[\s\S]*?\*\//g, "");
  const m = /@source\s+not\s+inline\(\s*"([^"]*)"\s*\)/.exec(css);
  assert.ok(m, "styles.css leaves out the utilities a widget's class would collide with (`@source not inline`)");
  const list = /^\{(.*)\}$/.exec(m[1].trim());
  return (list ? list[1].split(",") : m[1].split(/\s+/)).map((s) => s.trim()).filter(Boolean);
}

test("a bundled widget's class is no utility of ours, unless it means the same or the widget's own rule wins", async () => {
  const made = bareUtilities((await compiler()).build([...new Set([...WIDGET_CLASSES.values()].flatMap((s) => [...s]))]));
  const allowed = { ...SAME_MEANING, ...WIDGET_WINS };
  const met = new Set();
  for (const [widget, classes] of WIDGET_CLASSES) {
    assert.ok(classes.size > 0, `${widget}'s stylesheet was read`);
    for (const name of classes) {
      if (!made.has(name)) continue;
      met.add(name);
      assert.ok(
        name in allowed,
        `${widget} styles its own .${name}, and Tailwind makes a global .${name} that restyles it — leave it out in styles.css (\`@source not inline\`) and spell the app's uses out, or allow it here with the reason it is harmless`,
      );
    }
  }
  for (const name of Object.keys(allowed)) assert.ok(met.has(name), `.${name} is allowed but no widget collides on it any more — take the allowance out`);
});

test("every name styles.css leaves out is worn by a widget, is never made, and its spelled-out form is", async () => {
  const names = leftOut();
  assert.ok(names.length > 0, "the list is not empty");
  for (const name of names) {
    const wearers = [...WIDGET_CLASSES].filter(([, classes]) => classes.has(name)).map(([widget]) => widget);
    assert.ok(wearers.length > 0, `.${name} is left out, but no bundled widget wears it — nothing to protect, so it stays a utility`);
  }
  // The leak behind the blur: the terminal's and the editor's scrollbars toggle these two.
  for (const widget of ["xterm", "monaco"]) {
    for (const name of ["visible", "invisible"]) {
      assert.ok(WIDGET_CLASSES.get(widget).has(name), `${widget} still wears .${name}`);
      assert.ok(names.includes(name), `.${name} is left out — ${widget}'s scrollbar wears it`);
    }
  }
  const spelled = ["[visibility:hidden]", "[display:contents]", "@container"];
  const css = (await compiler()).build([...names, ...spelled]);
  const made = bareUtilities(css);
  for (const name of names) assert.ok(!made.has(name), `.${name} is still made`);
  const flat = css.replace(/\s+/g, "");
  assert.ok(flat.includes(".\\[visibility\\:hidden\\]{visibility:hidden"), "`[visibility:hidden]` is made — it is how the app hides in place");
  assert.ok(flat.includes(".\\[display\\:contents\\]{display:contents"), "`[display:contents]` is made — it is how the app drops a box");
  assert.ok(flat.includes(".\\@container{container-type:inline-size"), "leaving out `container` leaves the container queries alone");
});

/** Every string the parser sees in one source, with whether it sits in a class position — a `className`, a class helper's argument. */
function literalsOf(path) {
  const text = readFileSync(path, "utf8");
  const kind = path.endsWith(".tsx") ? ts.ScriptKind.TSX : path.endsWith(".ts") ? ts.ScriptKind.TS : ts.ScriptKind.JS;
  const file = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true, kind);
  const found = [];
  const visit = (node, inClass) => {
    const here =
      inClass ||
      (ts.isJsxAttribute(node) && /^(className|class)$|ClassName$/.test(node.name.getText(file))) ||
      (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && CLASS_HELPERS.includes(node.expression.text));
    if (ts.isStringLiteralLike(node) || ts.isTemplateExpression(node)) {
      const line = file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1;
      const pieces = ts.isTemplateExpression(node) ? [node.head.text, ...node.templateSpans.map((s) => s.literal.text)] : [node.text];
      for (const piece of pieces) found.push({ line, text: piece, inClass: here });
    }
    ts.forEachChild(node, (child) => visit(child, here));
  };
  visit(file, false);
  return found;
}

test("no class string in the app uses a name styles.css leaves out — it would style nothing", async () => {
  const names = new Set(leftOut());
  const isSource = (p) => /\.(ts|tsx|mjs)$/.test(p) && !p.endsWith(".d.mts") && !/\.test\.mjs$/.test(p) && !p.endsWith("types.gen.ts");
  const suspects = [];
  for (const path of sourceFiles(SRC, isSource)) {
    for (const lit of literalsOf(path)) {
      const tokens = lit.text.split(/\s+/).filter(Boolean);
      if (tokens.some((t) => names.has(t))) suspects.push({ where: `${relative(SRC, path)}:${lit.line}`, tokens, inClass: lit.inClass });
    }
  }
  // Outside a class position, a string is a class list when every word in it is one:
  // a utility's shape (a variant, a dash, an arbitrary value) or a bare utility Tailwind makes.
  const words = [...new Set(suspects.flatMap((s) => s.tokens).filter((t) => /^[a-z][a-z0-9]*$/.test(t)))];
  const made = bareUtilities((await compiler()).build(words));
  const classWord = (t) => names.has(t) || /[-:[\]/]/.test(t) || made.has(t);
  const offences = suspects.filter(
    (s) => s.inClass || (s.tokens.length === 1 && s.tokens[0] === "invisible") || (s.tokens.length > 1 && s.tokens.every(classWord)),
  );
  assert.deepEqual(
    offences.map((s) => `${s.where}: "${s.tokens.join(" ")}"`),
    [],
    `write [visibility:hidden] for invisible and [display:contents] for contents: ${[...names].join(", ")} are never made (styles.css)`,
  );
});
