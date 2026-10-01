import { strict as assert } from "node:assert";
import { test } from "node:test";

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { DIAGRAM_ROLES, exportScale, isMermaidPath, mermaidBlocks, mermaidTheme, offsetError, themeVariablesFor } from "./mermaidModel.mjs";

test("fenced mermaid blocks are found with the file line their source starts on", () => {
  const md = ["# Title", "", "```mermaid", "graph LR", "  A --> B", "```", "text", "~~~mermaid", "pie", "~~~"].join("\n");
  const blocks = mermaidBlocks(md);
  assert.equal(blocks.length, 2);
  assert.equal(blocks[0].source, "graph LR\n  A --> B");
  assert.equal(blocks[0].startLine, 4);
  assert.equal(blocks[1].source, "pie");
  assert.equal(blocks[1].startLine, 9);
  assert.deepEqual(mermaidBlocks("```js\nx\n```"), []);
  assert.deepEqual(mermaidBlocks("```mermaid\nunterminated"), [], "an open fence is not a block");
});

test("an error's line is moved from the snippet to the file", () => {
  const e = offsetError("Parse error on line 2:\n...", 47);
  assert.equal(e.line, 48);
  assert.ok(e.message.startsWith("Parse error on line 48"));
  assert.equal(offsetError("Parse error on line 3", 1).line, 3, "a .mmd file has no offset");
  assert.equal(offsetError("something else", 10).line, null);
  assert.equal(offsetError(null, 10).message, "Mermaid could not parse this diagram.");
});

test("the theme is `base` — the one that takes the app's roles — unless pinned", () => {
  assert.equal(mermaidTheme(undefined), "base");
  assert.equal(mermaidTheme("follow_app"), "base");
  assert.equal(mermaidTheme(""), "base");
  assert.equal(mermaidTheme("forest"), "forest");
  assert.equal(isMermaidPath("docs/flow.mmd"), true);
  assert.equal(isMermaidPath("docs/flow.mermaid"), true);
  assert.equal(isMermaidPath("docs/flow.md"), false);
});

test("the export scale is an integer between 1 and 4, 2 by default", () => {
  assert.equal(exportScale(undefined), 2);
  assert.equal(exportScale(3), 3);
  assert.equal(exportScale("3"), 3);
  assert.equal(exportScale(9), 4);
  assert.equal(exportScale(0), 1);
  assert.equal(exportScale("x"), 2);
});

test("every role a diagram reads is one the theme contract supplies", () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const tokens = readFileSync(join(here, "../theme/tokens.css"), "utf8");
  const block = tokens.slice(tokens.indexOf("/* @roles:start */"), tokens.indexOf("/* @roles:end */"));
  const declared = new Set([...block.matchAll(/(--color-[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
  for (const role of DIAGRAM_ROLES) assert.ok(declared.has(role), `${role} is not a role the contract declares`);
});

test("the theme variables are the roles, with the accent kept for attention and gaps left to darkMode", () => {
  const resolved = { "--color-bg": "#f7f8fa", "--color-surface": "#ffffff", "--color-border": "#d0d4dc", "--color-text": "#1f2430", "--color-accent": "#b8741a" };
  const v = themeVariablesFor(resolved, "light", "Inter, sans-serif");
  assert.equal(v.darkMode, false);
  assert.equal(v.fontFamily, "Inter, sans-serif");
  assert.equal(v.fontSize, "14px");
  assert.equal(v.background, "#f7f8fa");
  assert.equal(v.primaryColor, "#ffffff", "a node is a surface, not the accent");
  assert.equal(v.primaryBorderColor, "#d0d4dc");
  assert.equal(v.textColor, "#1f2430");
  assert.equal(v.activeTaskBkgColor, "#b8741a", "the accent is spent on the active thing");
  assert.equal("secondaryColor" in v, false, "an unresolved role is left for `base` to fill");
  const dark = themeVariablesFor({ "--color-surface": " " }, "dark", "");
  assert.equal(dark.darkMode, true);
  assert.equal(dark.fontFamily, "inherit");
  assert.equal("primaryColor" in dark, false, "blank is missing, not a colour");
});

test("a fence is found however the document is written: CRLF, tildes, a longer fence, indented — and never one that does not close", () => {
  const crlf = "# Title\r\n\r\n```mermaid\r\ngraph TD\r\n  A-->B\r\n```\r\nafter\r\n";
  assert.deepEqual(mermaidBlocks(crlf), [{ source: "graph TD\n  A-->B", startLine: 4 }], "no carriage return rides into the diagram");
  assert.deepEqual(mermaidBlocks("~~~mermaid\nflowchart LR\n~~~"), [{ source: "flowchart LR", startLine: 2 }]);
  assert.deepEqual(mermaidBlocks("````mermaid\ngraph TD\n```\nstill inside\n````"), [{ source: "graph TD\n```\nstill inside", startLine: 2 }], "a longer fence closes only on its own length");
  assert.deepEqual(mermaidBlocks("  ```Mermaid\n  graph TD\n  ```"), [{ source: "  graph TD", startLine: 2 }]);
  assert.deepEqual(mermaidBlocks("```mermaid\n```"), [{ source: "", startLine: 2 }], "an empty diagram is a block, and the viewer's to say so");
  assert.deepEqual(mermaidBlocks("```mermaid\ngraph TD\n  A-->B"), [], "a fence nobody closed is prose still being typed");
  assert.deepEqual(mermaidBlocks("```js\nconst mermaid = 1;\n```\n```mermaidx\nno\n```"), [], "another language, and a name that merely begins the same");
  assert.deepEqual(mermaidBlocks("a\n```mermaid\none\n```\nb\n```mermaid\ntwo\n```").map((b) => b.startLine), [3, 7]);
  for (const nothing of ["", null, undefined]) assert.deepEqual(mermaidBlocks(nothing), []);
});

test("an error's line is the file's: a block deep in a document, a file that is all diagram, and a message that names none", () => {
  assert.deepEqual(offsetError("Parse error on line 3:\n...", 41), { line: 43, message: "Parse error on line 43:\n..." });
  assert.deepEqual(offsetError("Parse error on line 3:", 1), { line: 3, message: "Parse error on line 3:" }, "a `.mmd` file starts on its own first line");
  assert.deepEqual(offsetError("Lexical error on Line 1. Unrecognized text.", 10), { line: 10, message: "Lexical error on line 10. Unrecognized text." });
  assert.deepEqual(offsetError("No diagram type detected", 10), { line: null, message: "No diagram type detected" });
  assert.deepEqual(offsetError("", 10), { line: null, message: "Mermaid could not parse this diagram." });
  assert.deepEqual(offsetError(null, null), { line: null, message: "Mermaid could not parse this diagram." });
  assert.deepEqual(offsetError("error on line 2", undefined).line, 2, "no offset given is the first line");
  assert.deepEqual(offsetError("error on line 2", -5).line, 2, "and never a line before the file");
});

