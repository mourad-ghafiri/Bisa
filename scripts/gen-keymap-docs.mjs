#!/usr/bin/env node
// Render docs/reference/keymap.md from the desktop's keymap model — the one
// source the shortcut handler, the palette and Settings read. `--check`
// exits 1 when the committed page is stale. The catalog is preloaded here,
// first, so the page carries the words and not the message ids wherever the
// script runs — the Justfile, CI, a shell.
import "../desktop/src/i18n/preload.mjs";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { keymapMarkdown } from "../desktop/src/shell/keymapModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const target = join(here, "..", "docs", "reference", "keymap.md");
const next = keymapMarkdown();
if (process.argv.includes("--check")) {
  let current = "";
  try {
    current = readFileSync(target, "utf8");
  } catch {
    // Missing counts as stale.
  }
  if (current !== next) {
    console.error(`${target} is stale — run \`just gen-keymap-docs\``);
    process.exit(1);
  }
  console.log("keymap.md is current");
} else {
  writeFileSync(target, next);
  console.log(`wrote ${target}`);
}
