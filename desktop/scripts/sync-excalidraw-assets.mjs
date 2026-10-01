#!/usr/bin/env node
/**
 * Copy the canvas's fonts out of the package into `public/excalidraw/fonts`
 * (19 — Drawings), so the app serves them from its own origin and the
 * canvas asks no CDN for anything (`ui/excalidraw.ts` points
 * `EXCALIDRAW_ASSET_PATH` here). Run by `predev` and `prebuild`; idempotent;
 * fails loudly when the package is not installed, so a broken install is a
 * broken build rather than a canvas that reaches for the network at runtime.
 */
import { cpSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const desktop = join(here, "..");
const source = join(desktop, "node_modules", "@excalidraw", "excalidraw", "dist", "prod", "fonts");
const target = join(desktop, "public", "excalidraw", "fonts");

if (!existsSync(source)) {
  process.stderr.write(`sync-excalidraw-assets: ${source} is missing — run \`npm install\` first\n`);
  process.exit(1);
}
mkdirSync(target, { recursive: true });
cpSync(source, target, { recursive: true });
const families = readdirSync(target).length;
process.stdout.write(`sync-excalidraw-assets: ${families} font families in public/excalidraw/fonts\n`);
