#!/usr/bin/env node
/**
 * The licence gate (docs/contributing/release.md §Licence): every Rust crate
 * the two binaries carry and every npm package the desktop bundles must take
 * a branch of its licence that `deny.toml` allows — the same rule cargo-deny
 * applies, so the gate runs on a machine without it. A package with no
 * allowed branch fails the gate by name — copyleft named as such; an
 * unknown or undeclared licence named as something to read. The build's
 * npm tools do not ship: they are listed, never failed.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { isCopyleftOnly, parseAllow } from "./licencesModel.mjs";
import { npmDevOnly, npmRuntimePackages } from "./npm.mjs";
import { allRustPackages } from "./rust.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const allow = parseAllow(readFileSync(join(root, "deny.toml"), "utf8"));
const FONT_ALLOW = ["OFL-1.1"];

const rust = allRustPackages(root, allow);
const { packages: npm, missing } = npmRuntimePackages(join(root, "desktop"), [...allow, ...FONT_ALLOW]);
if (missing.length) {
  console.error(`packages the runtime closure names but node_modules lacks (run npm ci): ${missing.join(", ")}`);
  process.exit(1);
}

const copyleft = [];
const unknown = [];
for (const p of [...rust, ...npm]) {
  if (p.branch) continue;
  const line = `${p.kind} ${p.name} ${p.version}: ${p.declared} (${p.where})`;
  if (isCopyleftOnly(p.declared, allow)) copyleft.push(line);
  else unknown.push(line);
}
const dev = npmDevOnly(join(root, "desktop"), npm);
const devOff = dev.filter((p) => !allow.includes(p.declared) && !FONT_ALLOW.includes(p.declared));

console.log(`licences: ${rust.length} crates and ${npm.length} npm packages judged against deny.toml's ${allow.length} ids (+ OFL-1.1 for fonts)`);
if (devOff.length) console.log(`build-time npm tools off the list (they do not ship):\n  ${devOff.map((p) => `${p.name} ${p.version}: ${p.declared}`).join("\n  ")}`);
if (unknown.length) console.error(`licences with no allowed branch — read them, then allow the id in deny.toml or drop the package:\n  ${unknown.join("\n  ")}`);
if (copyleft.length) console.error(`copyleft with no permissive alternative:\n  ${copyleft.join("\n  ")}`);
if (unknown.length || copyleft.length) process.exit(1);
console.log("licences: every shipped package takes an allowed branch");
