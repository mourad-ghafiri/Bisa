/**
 * The release rules: versions, tags and asset names; the SHA-256 line both
 * ways; the changelog's sections and the cut of a release; the declared
 * repository and the remote that is it; the targeted version bumps. Run with
 * `node --test scripts/release/releaseModel.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  AFTER_BUMP,
  VERSION_FILES,
  assetNames,
  bumpJsonVersion,
  bumpHakariRequirement,
  bumpTomlVersion,
  cutRelease,
  isVersion,
  parseSha256,
  remoteMatches,
  remoteNamed,
  repositorySlug,
  sectionFor,
  sha256Line,
  tagFor,
  workspaceVersion,
} from "./releaseModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");

const CHANGELOG = `# Changelog

Words about the format.

## [Unreleased]

### Added

- A thing that is new.

### Fixed

- A thing that was wrong.

## [0.1.0] - 2026-10-01

The first release.

### Added

- Everything.

[Unreleased]: https://github.com/owner/repo/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/owner/repo/releases/tag/v0.1.0
`;

test("a version is <major>.<minor>.<patch> and nothing else; its tag wears a v; the two assets are named from it", () => {
  for (const ok of ["0.1.0", "1.0.0", "10.20.30"]) assert.ok(isVersion(ok), ok);
  for (const bad of ["1.0", "v1.0.0", "1.0.0-rc1", "01.0.0", "1.0.0 ", "", undefined, "a.b.c"]) assert.ok(!isVersion(bad), String(bad));
  assert.equal(tagFor("0.1.0"), "v0.1.0");
  assert.deepEqual(assetNames("0.1.0"), { dmg: "Bisa-0.1.0-macos-universal.dmg", sha256: "Bisa-0.1.0-macos-universal.dmg.sha256" });
});

test("a SHA-256 line is the hash, two spaces and the bare name — what shasum -c reads — and parses back", () => {
  const hash = "a".repeat(64);
  const line = sha256Line(hash, "Bisa-0.1.0-macos-universal.dmg");
  assert.equal(line, `${hash}  Bisa-0.1.0-macos-universal.dmg\n`);
  assert.deepEqual(parseSha256(line), { hash, name: "Bisa-0.1.0-macos-universal.dmg" });
  assert.equal(parseSha256(`${hash} Bisa.dmg`), null, "one space is not the form");
  assert.equal(parseSha256(`${"g".repeat(64)}  Bisa.dmg`), null, "not a hex hash");
  assert.equal(parseSha256(""), null);
});

test("the section of a version is its body, trimmed; a missing section is null and an empty one is an empty string", () => {
  assert.equal(sectionFor(CHANGELOG, "0.1.0"), "The first release.\n\n### Added\n\n- Everything.");
  assert.equal(sectionFor(CHANGELOG, "Unreleased"), "### Added\n\n- A thing that is new.\n\n### Fixed\n\n- A thing that was wrong.");
  assert.equal(sectionFor(CHANGELOG, "0.2.0"), null);
  assert.equal(sectionFor("# Changelog\n\n## [0.3.0] - 2026-01-01\n\n[0.3.0]: https://x/y\n", "0.3.0"), "", "the link block is no body");
  assert.equal(sectionFor("## [0.3.0] - 2026-01-01\n\n\n", "0.3.0"), "");
});

test("a release is cut from Unreleased: a dated section first among the released, Unreleased left empty, the links moved on", () => {
  const cut = cutRelease(CHANGELOG, "0.2.0", "2026-11-02");
  assert.equal(sectionFor(cut, "Unreleased"), "", "Unreleased is empty");
  assert.equal(sectionFor(cut, "0.2.0"), "### Added\n\n- A thing that is new.\n\n### Fixed\n\n- A thing that was wrong.");
  assert.equal(sectionFor(cut, "0.1.0"), "The first release.\n\n### Added\n\n- Everything.", "the older section is untouched");
  assert.ok(cut.indexOf("## [Unreleased]") < cut.indexOf("## [0.2.0] - 2026-11-02") && cut.indexOf("## [0.2.0]") < cut.indexOf("## [0.1.0]"), "newest first");
  assert.ok(cut.includes("[Unreleased]: https://github.com/owner/repo/compare/v0.2.0...HEAD"));
  assert.ok(cut.includes("[0.2.0]: https://github.com/owner/repo/compare/v0.1.0...v0.2.0"));
  assert.ok(cut.includes("[0.1.0]: https://github.com/owner/repo/releases/tag/v0.1.0"));
  assert.ok(cut.startsWith("# Changelog\n\nWords about the format.\n\n## [Unreleased]\n\n## [0.2.0] - 2026-11-02\n\n### Added"), "the head is kept and the spacing is one blank line");
  assert.ok(!cut.includes("\n\n\n"), "no run of blank lines");
  // A changelog whose only section is Unreleased: the first release's link is the tag's page.
  const first = "# C\n\n## [Unreleased]\n\n- Born.\n\n[Unreleased]: https://github.com/o/r/compare/v0.0.0...HEAD\n";
  const born = cutRelease(first, "1.0.0", "2027-01-01");
  assert.ok(born.includes("[1.0.0]: https://github.com/o/r/releases/tag/v1.0.0") && born.includes("[Unreleased]: https://github.com/o/r/compare/v1.0.0...HEAD"));
});

test("a cut is refused by sentence when there is nothing to release, when the version exists, and when the form is wrong", () => {
  const empty = CHANGELOG.replace("### Added\n\n- A thing that is new.\n\n### Fixed\n\n- A thing that was wrong.\n\n## [0.1.0]", "## [0.1.0]");
  assert.throws(() => cutRelease(empty, "0.2.0", "2026-11-02"), /Unreleased section is empty/);
  assert.throws(() => cutRelease(CHANGELOG, "0.1.0", "2026-11-02"), /already has a section for 0.1.0/);
  assert.throws(() => cutRelease(CHANGELOG, "0.2", "2026-11-02"), /not <major>\.<minor>\.<patch>/);
  assert.throws(() => cutRelease(CHANGELOG, "0.2.0", "2 Nov 2026"), /YYYY-MM-DD/);
  assert.throws(() => cutRelease("# C\n\n## [0.1.0] - 2026-01-01\n\n- x\n", "0.2.0", "2026-11-02"), /no `## \[Unreleased\]` section/);
  assert.throws(() => cutRelease("# C\n\n## [Unreleased]\n\n- x\n", "0.2.0", "2026-11-02"), /link block/);
});

test("the repository is the workspace's declared one, read as owner/repo; the remote that is it is found in any of git's three spellings", () => {
  const toml = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.1.0"\nrepository = "https://github.com/owner/repo"\n\n[workspace.lints]\n';
  assert.equal(repositorySlug(toml), "owner/repo");
  assert.equal(repositorySlug(toml.replace('repo"', 'repo.git"')), "owner/repo");
  assert.equal(repositorySlug('[package]\nrepository = "https://github.com/a/b"\n'), null, "the workspace table, not a package's");
  assert.equal(repositorySlug('[workspace.package]\nrepository = "https://gitlab.com/a/b"\n'), null, "a release goes to GitHub");
  assert.equal(workspaceVersion(toml), "0.1.0");
  for (const url of ["https://github.com/owner/repo", "https://github.com/owner/repo.git", "https://github.com/owner/repo/", "git@github.com:owner/repo.git", "git@github.com:owner/repo", "ssh://git@github.com/owner/repo.git"]) assert.ok(remoteMatches(url, "owner/repo"), url);
  for (const url of ["https://github.com/owner/other", "https://github.com/other/repo", "https://github.com/owner/repository", "https://gitlab.com/owner/repo", "http://github.com/owner/repo", ""]) assert.ok(!remoteMatches(url, "owner/repo"), url || "(empty)");
  const remotes = "origin\thttps://github.com/owner/fork.git (fetch)\norigin\thttps://github.com/owner/fork.git (push)\nbisa\tgit@github.com:owner/repo.git (fetch)\nbisa\tgit@github.com:owner/repo.git (push)\n";
  assert.equal(remoteNamed(remotes, "owner/repo"), "bisa");
  assert.equal(remoteNamed(remotes, "owner/none"), null);
  assert.equal(remoteNamed("", "owner/repo"), null);
});

test("a JSON bump changes the one version line and nothing else; two lines or another value are refused", () => {
  const pkg = '{\n  "name": "bisa-desktop",\n  "private": true,\n  "version": "0.1.0",\n  "scripts": {\n    "dev": "vite"\n  }\n}\n';
  assert.equal(bumpJsonVersion(pkg, "0.1.0", "0.2.0"), pkg.replace('"version": "0.1.0"', '"version": "0.2.0"'));
  assert.throws(() => bumpJsonVersion(pkg, "0.0.9", "0.2.0"), /is "0.1.0", not "0.0.9"/);
  assert.throws(() => bumpJsonVersion(pkg.replace('"private": true,', '"private": true,\n  "version": "0.1.0",'), "0.1.0", "0.2.0"), /found 2/);
  assert.throws(() => bumpJsonVersion('{"name": "x"}', "0.1.0", "0.2.0"), /found 0/);
});

test("a TOML bump changes the version inside its table alone — a dependency's version line is never touched", () => {
  const manifest = '[package]\nname = "bisa-desktop"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nserde = { version = "1" }\ntokio = "1"\n\n[dev-dependencies]\nversion = "9.9.9"\n';
  const bumped = bumpTomlVersion(manifest, "package", "0.1.0", "0.2.0");
  assert.equal(bumped, manifest.replace('version = "0.1.0"', 'version = "0.2.0"'));
  assert.ok(bumped.includes('version = "9.9.9"'), "another table's line stands");
  assert.throws(() => bumpTomlVersion(manifest, "package", "0.0.9", "0.2.0"), /is "0.1.0", not "0.0.9"/);
  assert.throws(() => bumpTomlVersion(manifest, "workspace.package", "0.1.0", "0.2.0"), /no \[workspace.package\] table/);
  assert.throws(() => bumpTomlVersion("[package]\nname = \"x\"\n", "package", "0.1.0", "0.2.0"), /found 0/);
});

test("the workspace-hack requirement moves with the major and minor: the one hakari line alone, a manifest without it untouched, another minor or two lines refused", () => {
  const manifest = '[package]\nname = "x"\n\n[dependencies]\nbisa-deps = { version = "0.3", path = "../bisa-deps" }\nserde = { version = "1.0" }\n';
  const bumped = bumpHakariRequirement(manifest, "0.3.0", "0.4.0");
  assert.equal(bumped, manifest.replace('version = "0.3", path', 'version = "0.4", path'));
  assert.ok(bumped.includes('serde = { version = "1.0" }'), "another dependency's version is kept");
  assert.equal(bumpHakariRequirement(manifest, "0.3.0", "0.3.1"), manifest, "a patch moves nothing");
  assert.equal(bumpHakariRequirement('[package]\nname = "bisa-deps"\n', "0.3.0", "0.4.0"), null);
  assert.throws(() => bumpHakariRequirement(manifest, "0.2.0", "0.4.0"), /is "0.3", not "0.2"/);
  assert.throws(() => bumpHakariRequirement(manifest + manifest, "0.3.0", "0.4.0"), /found 2/);
});

test("every crate that requires the workspace-hack crate requires it at the workspace's own major and minor today", () => {
  const version = workspaceVersion(read("Cargo.toml"));
  const crates = readdirSync(join(root, VERSION_FILES.crates), { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => `${VERSION_FILES.crates}/${e.name}/Cargo.toml`);
  let carrying = 0;
  for (const path of crates) {
    const text = read(path);
    if (bumpHakariRequirement(text, version, version) === null) continue;
    carrying += 1;
    assert.doesNotThrow(() => bumpHakariRequirement(text, version, "9.9.9"), path);
  }
  assert.ok(carrying >= 20, `the member crates carry the requirement: ${carrying}`);
});

test("the files a bump touches are the ones platformIdentity.test.mjs holds to the workspace, and every one is at the workspace's version today", () => {
  assert.deepEqual(VERSION_FILES.toml.map((f) => f.path), ["Cargo.toml", "desktop/src-tauri/Cargo.toml"]);
  assert.deepEqual(VERSION_FILES.json, ["desktop/package.json", "desktop/src-tauri/tauri.conf.json", "addons/sdk/package.json"]);
  assert.equal(VERSION_FILES.addons, "library/addons");
  const version = workspaceVersion(read("Cargo.toml"));
  assert.ok(isVersion(version));
  for (const { path, table } of VERSION_FILES.toml) assert.doesNotThrow(() => bumpTomlVersion(read(path), table, version, "9.9.9"), path);
  for (const path of VERSION_FILES.json) assert.doesNotThrow(() => bumpJsonVersion(read(path), version, "9.9.9"), path);
  const addons = readdirSync(join(root, VERSION_FILES.addons), { withFileTypes: true }).filter((e) => e.isDirectory());
  assert.equal(addons.length, 13);
  for (const a of addons) assert.doesNotThrow(() => bumpJsonVersion(read(`${VERSION_FILES.addons}/${a.name}/addon.json`), version, "9.9.9"), a.name);
  assert.ok(AFTER_BUMP.includes("cargo update --workspace") && AFTER_BUMP.includes("just gen-catalog-docs") && AFTER_BUMP.at(-1) === "just verify", "the lockfiles and the generated page are left to their tools, the gate last");
});

test("the repository's own changelog has an Unreleased section and a section with words for the version the tree wears", () => {
  const changelog = read("CHANGELOG.md");
  const version = workspaceVersion(read("Cargo.toml"));
  assert.ok(changelog.startsWith("# Changelog\n"));
  assert.notEqual(sectionFor(changelog, "Unreleased"), null, "an Unreleased section exists");
  const section = sectionFor(changelog, version);
  assert.ok(section && section.length > 0, `a section for ${version} with words in it`);
  assert.ok(changelog.includes(`[Unreleased]: https://github.com/${repositorySlug(read("Cargo.toml"))}/compare/v${version}...HEAD`), "Unreleased compares from the latest tag");
  assert.ok(changelog.includes(`[${version}]: https://github.com/`), "the version has its link");
});
