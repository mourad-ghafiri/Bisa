#!/usr/bin/env node
/**
 * Print the changelog's section for one version — what a release ships as
 * its notes (contributing/release.md §Publishing). Exit 1 with a sentence
 * when CHANGELOG.md has no such section, or one with nothing under it; both
 * scripts/release-macos.sh and scripts/publish-release.sh ask before they do
 * anything slow.
 *
 *   node scripts/release/release-notes.mjs <version>
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { isVersion, sectionFor } from "./releaseModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const version = process.argv[2];

if (!isVersion(version)) {
  console.error(`release-notes: "${version ?? ""}" is not a version of the form <major>.<minor>.<patch>`);
  process.exit(2);
}
const section = sectionFor(readFileSync(join(root, "CHANGELOG.md"), "utf8"), version);
if (section === null) {
  console.error(`release-notes: CHANGELOG.md has no \`## [${version}]\` section`);
  process.exit(1);
}
if (section === "") {
  console.error(`release-notes: the \`## [${version}]\` section of CHANGELOG.md has nothing under it`);
  process.exit(1);
}
process.stdout.write(`${section}\n`);
