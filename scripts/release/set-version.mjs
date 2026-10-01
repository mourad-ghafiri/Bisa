#!/usr/bin/env node
/**
 * Move the platform to a new version (contributing/release.md §The version
 * and the changelog): the one `version` line of every manifest that repeats
 * the workspace's — the root `Cargo.toml`, the shell's, `desktop/package.json`,
 * `tauri.conf.json`, the addon SDK's package and every built-in addon's
 * `addon.json` — by targeted replacement, the file's formatting kept; and the
 * changelog cut, Unreleased becoming the dated section the release ships as
 * its notes. A dry run by default: it says what it would change. `--write`
 * applies it, then prints the commands it leaves to you — the lockfiles and
 * the generated catalog page are rewritten by their own tools, never here.
 *
 *   node scripts/release/set-version.mjs 0.2.0            what would change
 *   node scripts/release/set-version.mjs 0.2.0 --write    change it
 */
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { AFTER_BUMP, VERSION_FILES, bumpJsonVersion, bumpTomlVersion, cutRelease, isVersion, workspaceVersion } from "./releaseModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");
const args = process.argv.slice(2);
const write = args.includes("--write");
const next = args.find((a) => !a.startsWith("--"));

const refuse = (sentence) => {
  console.error(`set-version: ${sentence}`);
  process.exit(2);
};

if (!isVersion(next)) refuse(`"${next ?? ""}" is not a version of the form <major>.<minor>.<patch>`);
const current = workspaceVersion(read("Cargo.toml"));
if (!current) refuse("Cargo.toml has no [workspace.package] version");
if (current === next) refuse(`the platform is already at ${current}`);

/** Every change, computed before any file is touched: a refusal leaves the tree as it was. */
const changes = [];
try {
  for (const { path, table } of VERSION_FILES.toml) changes.push({ path, text: bumpTomlVersion(read(path), table, current, next) });
  for (const path of VERSION_FILES.json) changes.push({ path, text: bumpJsonVersion(read(path), current, next) });
  const addons = readdirSync(join(root, VERSION_FILES.addons), { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => `${VERSION_FILES.addons}/${e.name}/addon.json`)
    .sort();
  for (const path of addons) changes.push({ path, text: bumpJsonVersion(read(path), current, next) });
  const today = new Date().toISOString().slice(0, 10);
  changes.push({ path: "CHANGELOG.md", text: cutRelease(read("CHANGELOG.md"), next, today) });
} catch (e) {
  refuse(e.message);
}

console.log(`${write ? "" : "would change "}${current} → ${next} in ${changes.length} files:`);
for (const { path } of changes) console.log(`  ${path}`);
if (!write) {
  console.log("\nnothing written: add --write to apply");
  process.exit(0);
}
for (const { path, text } of changes) writeFileSync(join(root, path), text);
console.log("\nthen, in this order — each rewrites what only its tool may:");
for (const command of AFTER_BUMP) console.log(`  ${command}`);
console.log("\nread CHANGELOG.md once more, then commit the lot as one change.");
