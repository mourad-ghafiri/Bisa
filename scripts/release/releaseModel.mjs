/**
 * The release rules, pure (contributing/release.md): what a version, a tag
 * and the two artefacts are called; the SHA-256 line in the form
 * `shasum -a 256 -c` reads; the changelog's sections (Keep a Changelog
 * 1.1.0) — the one a release ships as its notes, and the cut that turns
 * Unreleased into a dated section; the repository the workspace declares and
 * the git remote that is it; and the targeted bump of one `version` line in a
 * JSON or a TOML manifest, the file's formatting kept. No I/O here:
 * `node --test` reads it, and the three small tools beside it call it.
 */

/** `<major>.<minor>.<patch>`, each a plain number (no leading zero, no suffix). */
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;
const HEADING = /^## \[([^\]]+)\](?: - (\d{4}-\d{2}-\d{2}))?\s*$/;
/** A link definition at the changelog's foot: `[name]: https://…`. */
const LINK_LINE = /^\[[^\]]+\]: https?:\/\/\S+$/;

export function isVersion(text) {
  return VERSION.test(String(text));
}

export function tagFor(version) {
  return `v${version}`;
}

/** The two files a release is: the universal disk image and its hash. */
export function assetNames(version) {
  const dmg = `Bisa-${version}-macos-universal.dmg`;
  return { dmg, sha256: `${dmg}.sha256` };
}

/**
 * One line of a `.sha256` file: the hash, two spaces, the bare file name —
 * what `shasum -a 256` prints beside the file and `shasum -a 256 -c` reads.
 */
export function sha256Line(hash, name) {
  return `${hash}  ${name}\n`;
}

/** The hash and the name of the first such line, or `null` for anything else. */
export function parseSha256(text) {
  const m = String(text).match(/^([0-9a-f]{64})  (\S[^\n]*)$/m);
  return m ? { hash: m[1], name: m[2] } : null;
}

/** The `## [name] - date` headings of a changelog, with their line numbers. */
function headings(lines) {
  const out = [];
  lines.forEach((line, i) => {
    const m = line.match(HEADING);
    if (m) out.push({ name: m[1], date: m[2] ?? null, line: i });
  });
  return out;
}

/** The lines of the k-th section's body: after its heading, before the next one or the link block. */
function bodyRange(lines, hs, k) {
  const start = hs[k].line + 1;
  let end = k + 1 < hs.length ? hs[k + 1].line : lines.length;
  for (let i = start; i < end; i++) {
    if (LINK_LINE.test(lines[i])) {
      end = i;
      break;
    }
  }
  return [start, end];
}

/**
 * The body of the section headed `## [version]`, trimmed — the release's
 * notes. `null` when the changelog has no such section; `""` when it has one
 * with nothing under it, which is no release either.
 */
export function sectionFor(changelog, version) {
  const lines = String(changelog).split("\n");
  const hs = headings(lines);
  const k = hs.findIndex((h) => h.name === version);
  if (k < 0) return null;
  const [start, end] = bodyRange(lines, hs, k);
  return lines.slice(start, end).join("\n").trim();
}

/** The repository URL the link block names, read off the `[Unreleased]` compare link. */
function repositoryOfLinks(text) {
  const m = text.match(/^\[Unreleased\]: (\S+)\/compare\/\S+\.\.\.HEAD$/m);
  return m ? m[1] : null;
}

/**
 * Cut a release from Unreleased: its body moves under a new `## [version] - date`
 * heading placed first among the released sections, Unreleased is left empty,
 * and the link block gains the version's compare link while Unreleased's
 * compares from the new tag. Refused — an `Error` with the sentence — when
 * Unreleased is missing or empty, when the version already has a section, or
 * when the link block has no `[Unreleased]` line to read the repository off.
 */
export function cutRelease(changelog, version, date) {
  if (!isVersion(version)) throw new Error(`"${version}" is not <major>.<minor>.<patch>`);
  if (!ISO_DATE.test(String(date))) throw new Error(`"${date}" is not a date of the form YYYY-MM-DD`);
  const text = String(changelog);
  const lines = text.split("\n");
  const hs = headings(lines);
  const u = hs.findIndex((h) => h.name === "Unreleased");
  if (u < 0) throw new Error("CHANGELOG.md has no `## [Unreleased]` section");
  if (hs.some((h) => h.name === version)) throw new Error(`CHANGELOG.md already has a section for ${version}`);
  const repository = repositoryOfLinks(text);
  if (!repository) throw new Error("the link block at the foot of CHANGELOG.md has no `[Unreleased]: …/compare/…HEAD` line");
  const [start, end] = bodyRange(lines, hs, u);
  const body = lines.slice(start, end).join("\n").trim();
  if (!body) throw new Error("the Unreleased section is empty: write what changed before cutting a release");
  const previous = hs[u + 1]?.name ?? null;
  const cut = [...lines.slice(0, start), "", `## [${version}] - ${date}`, "", body, "", ...lines.slice(end)].join("\n");
  const tag = tagFor(version);
  const versionLink = previous ? `${repository}/compare/${tagFor(previous)}...${tag}` : `${repository}/releases/tag/${tag}`;
  return cut.replace(/^\[Unreleased\]: \S+$/m, `[Unreleased]: ${repository}/compare/${tag}...HEAD\n[${version}]: ${versionLink}`);
}

/** The first `key = "value"` line of a TOML table, or `null`. */
function tomlLine(text, table, key) {
  const at = text.indexOf(`[${table}]`);
  if (at < 0) return null;
  const next = text.indexOf("\n[", at + 1);
  const block = text.slice(at, next === -1 ? undefined : next);
  const m = block.match(new RegExp(`^${key} = "([^"]*)"$`, "m"));
  return m ? m[1] : null;
}

/** `[workspace.package] version` of the root manifest, or `null`. */
export function workspaceVersion(cargoToml) {
  return tomlLine(String(cargoToml), "workspace.package", "version");
}

/**
 * `owner/repo` of the repository the workspace declares
 * (`[workspace.package] repository`, a github.com URL), or `null`.
 */
export function repositorySlug(cargoToml) {
  const url = tomlLine(String(cargoToml), "workspace.package", "repository");
  if (!url) return null;
  const m = url.match(/^https:\/\/github\.com\/([^/\s]+)\/([^/\s]+?)(?:\.git)?\/?$/);
  return m ? `${m[1]}/${m[2]}` : null;
}

/**
 * Whether a git remote's URL is the repository `owner/repo`, in the three
 * spellings git takes: `https://github.com/o/r`, `git@github.com:o/r`,
 * `ssh://git@github.com/o/r` — each with or without `.git`.
 */
export function remoteMatches(url, slug) {
  const bare = String(url).trim().replace(/\/$/, "").replace(/\.git$/, "");
  return [`https://github.com/${slug}`, `git@github.com:${slug}`, `ssh://git@github.com/${slug}`].includes(bare);
}

/**
 * The name of the first remote in `git remote -v`'s output whose URL is the
 * repository, or `null`.
 */
export function remoteNamed(gitRemoteV, slug) {
  for (const line of String(gitRemoteV).split("\n")) {
    const m = line.match(/^(\S+)\s+(\S+)/);
    if (m && remoteMatches(m[2], slug)) return m[1];
  }
  return null;
}

/**
 * A JSON manifest's one `"version": "<from>"` line set to `to`, every other
 * byte kept. Refused when the file has no such line, another value, or more
 * than one `version` key at any depth — a lockfile is not a manifest.
 */
export function bumpJsonVersion(text, from, to) {
  const lines = [...String(text).matchAll(/^(\s*"version":\s*")([^"]*)(")/gm)];
  if (lines.length !== 1) throw new Error(`expected one "version" line, found ${lines.length}`);
  if (lines[0][2] !== from) throw new Error(`the version is "${lines[0][2]}", not "${from}"`);
  return String(text).replace(/^(\s*"version":\s*")[^"]*(")/m, `$1${to}$2`);
}

/**
 * A TOML manifest's `version = "<from>"` line inside `[table]` set to `to`,
 * the rest kept — a dependency's `version` in another table is never touched.
 */
export function bumpTomlVersion(text, table, from, to) {
  const source = String(text);
  const at = source.indexOf(`[${table}]`);
  if (at < 0) throw new Error(`no [${table}] table`);
  const next = source.indexOf("\n[", at + 1);
  const end = next === -1 ? source.length : next;
  const block = source.slice(at, end);
  const found = [...block.matchAll(/^version = "([^"]*)"$/gm)];
  if (found.length !== 1) throw new Error(`expected one version line in [${table}], found ${found.length}`);
  if (found[0][1] !== from) throw new Error(`[${table}] version is "${found[0][1]}", not "${from}"`);
  return source.slice(0, at) + block.replace(/^version = "[^"]*"$/m, `version = "${to}"`) + source.slice(end);
}

/**
 * Every file that repeats the workspace's version, by how it is read
 * (contributing/release.md §One version); `platformIdentity.test.mjs` holds
 * the same set equal to the workspace. The addons' folder holds one
 * `addon.json` per built-in.
 */
export const VERSION_FILES = Object.freeze({
  toml: Object.freeze([
    Object.freeze({ path: "Cargo.toml", table: "workspace.package" }),
    Object.freeze({ path: "desktop/src-tauri/Cargo.toml", table: "package" }),
  ]),
  json: Object.freeze(["desktop/package.json", "desktop/src-tauri/tauri.conf.json", "addons/sdk/package.json"]),
  addons: "library/addons",
});

/**
 * The commands a version bump leaves to the person — each rewrites a lockfile
 * or a generated page with its own tool, which this script never runs.
 */
export const AFTER_BUMP = Object.freeze([
  "cargo update --workspace",
  "(cd desktop/src-tauri && cargo update --workspace)",
  "(cd desktop && npm install --package-lock-only)",
  "just gen-catalog-docs",
  "just verify",
]);
