#!/usr/bin/env node
/**
 * The repository a release is published on is the one the workspace declares
 * (`[workspace.package] repository` in Cargo.toml) — never the clone's remote,
 * which may be a fork or a mirror (contributing/release.md §Publishing).
 *
 *   node scripts/release/repository.mjs               prints `owner/repo`
 *   git remote -v | node scripts/release/repository.mjs --remote
 *                                                     prints the name of the first
 *                                                     remote whose URL is that
 *                                                     repository; exit 1 when none is
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { remoteNamed, repositorySlug } from "./releaseModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const slug = repositorySlug(readFileSync(join(root, "Cargo.toml"), "utf8"));
if (!slug) {
  console.error("repository: Cargo.toml's [workspace.package] repository is not a github.com URL");
  process.exit(2);
}

if (process.argv[2] === "--remote") {
  const remote = remoteNamed(readFileSync(0, "utf8"), slug);
  if (!remote) {
    console.error(`repository: no git remote points at ${slug}`);
    process.exit(1);
  }
  process.stdout.write(`${remote}\n`);
} else {
  process.stdout.write(`${slug}\n`);
}
