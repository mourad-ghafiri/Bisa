#!/usr/bin/env node
/**
 * Every ```mermaid block under docs/ parses with the same Mermaid the desktop
 * renders with (ide/11, ADR-0025). Mermaid needs a DOM even to parse, so
 * this runs under happy-dom — a dev dependency of the desktop imported by this
 * script and nothing else; `desktop/src/noHappyDom.test.mjs` keeps it that way.
 *
 * Usage: node scripts/check-mermaid.mjs [dir...]   (default: docs)
 * Exits non-zero naming every block that does not parse, with the file and
 * the line the block starts on.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DESKTOP_MODULES = join(ROOT, "desktop", "node_modules");

async function importFromDesktop(name, file) {
  return import(pathToFileURL(join(DESKTOP_MODULES, name, file)).href);
}

const { Window } = await importFromDesktop("happy-dom", "lib/index.js");
const window = new Window({ url: "http://localhost/" });
for (const key of ["window", "document", "navigator", "DOMParser", "HTMLElement", "SVGElement", "Element", "Node", "getComputedStyle", "requestAnimationFrame"]) {
  if (!(key in globalThis)) globalThis[key] = window[key] ?? window;
}
globalThis.window = window;
globalThis.document = window.document;

const { mermaidBlocks } = await import(pathToFileURL(join(ROOT, "desktop", "src", "ui", "mermaidModel.mjs")).href);
const mermaid = (await importFromDesktop("mermaid", "dist/mermaid.core.mjs")).default;
mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });

function* markdownFiles(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* markdownFiles(p);
    else if (p.endsWith(".md")) yield p;
  }
}

const dirs = process.argv.slice(2).length ? process.argv.slice(2) : ["docs"];
let blocks = 0;
const failures = [];
for (const dir of dirs) {
  for (const file of markdownFiles(resolve(ROOT, dir))) {
    const text = readFileSync(file, "utf8");
    for (const block of mermaidBlocks(text)) {
      blocks++;
      try {
        await mermaid.parse(block.source);
      } catch (e) {
        const message = String(e?.message ?? e).split("\n")[0];
        failures.push(`${file.slice(ROOT.length + 1)}:${block.startLine}: ${message}`);
      }
    }
  }
}

if (failures.length) {
  console.error(failures.join("\n"));
  console.error(`\n${failures.length} of ${blocks} mermaid blocks do not parse`);
  process.exit(1);
}
console.log(`check-mermaid: ${blocks} blocks parse`);
