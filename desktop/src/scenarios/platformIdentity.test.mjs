/**
 * One version, one website, one repository (contributing/release.md §One
 * version): the Cargo workspace declares them once, and every manifest and
 * page that repeats them says the same — the desktop's package, the Tauri
 * shell, the addon SDK, every crate's inheritance, every built-in addon's
 * manifest, the README, the getting-started clone line, the licence, the
 * Vite defines and About's doors. Source assertions, no DOM. Run with
 * `node --test desktop/src/scenarios/platformIdentity.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");
const json = (rel) => JSON.parse(read(rel));
/** A `key = "value"` line of a TOML table, the first one after `[table]`. */
const tomlValue = (text, table, key) => {
  const at = text.indexOf(`[${table}]`);
  assert.ok(at >= 0, `${table} is declared`);
  const block = text.slice(at, text.indexOf("\n[", at + 1) === -1 ? undefined : text.indexOf("\n[", at + 1));
  const m = block.match(new RegExp(`^${key} = "([^"]+)"`, "m"));
  assert.ok(m, `${table}.${key} is declared`);
  return m[1];
};

const workspace = read("Cargo.toml");
const VERSION = tomlValue(workspace, "workspace.package", "version");
const HOMEPAGE = tomlValue(workspace, "workspace.package", "homepage");
const REPOSITORY = tomlValue(workspace, "workspace.package", "repository");

test("the workspace declares the three facts once, and they are the official ones", () => {
  assert.match(VERSION, /^\d+\.\d+\.\d+$/);
  assert.equal(HOMEPAGE, "https://bisa.dev");
  assert.equal(REPOSITORY, "https://github.com/mourad-ghafiri/Bisa");
});

test("every crate inherits the repository and the website from the workspace", () => {
  const crates = readdirSync(join(root, "crates"), { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name).sort();
  assert.ok(crates.length >= 26);
  for (const c of crates) {
    if (c === "bisa-deps") continue; // hakari's, generated
    const manifest = read(`crates/${c}/Cargo.toml`);
    assert.ok(manifest.includes("version.workspace = true"), `${c} inherits the version`);
    assert.ok(manifest.includes("repository.workspace = true"), `${c} inherits the repository`);
    assert.ok(manifest.includes("homepage.workspace = true"), `${c} inherits the website`);
  }
});

test("the desktop, the Tauri shell and the addon SDK repeat the same version and the same two URLs", () => {
  const pkg = json("desktop/package.json");
  assert.equal(pkg.version, VERSION);
  assert.equal(pkg.homepage, HOMEPAGE);
  assert.equal(pkg.repository?.url, REPOSITORY);
  assert.equal(pkg.bugs?.url, `${REPOSITORY}/issues`);
  const shell = read("desktop/src-tauri/Cargo.toml");
  assert.equal(tomlValue(shell, "package", "version"), VERSION);
  assert.equal(tomlValue(shell, "package", "homepage"), HOMEPAGE);
  assert.equal(tomlValue(shell, "package", "repository"), REPOSITORY);
  assert.equal(json("desktop/src-tauri/tauri.conf.json").version, VERSION);
  const sdk = json("addons/sdk/package.json");
  assert.equal(sdk.version, VERSION);
  assert.equal(sdk.homepage, HOMEPAGE);
  assert.equal(sdk.repository?.url, REPOSITORY);
  const vite = read("desktop/vite.config.ts");
  for (const name of ["__APP_VERSION__", "__APP_HOMEPAGE__", "__APP_REPOSITORY__"]) assert.ok(vite.includes(name), `${name} is baked in`);
  assert.ok(read("desktop/src/version.d.ts").includes("__APP_REPOSITORY__"), "declared once, in version.d.ts");
  assert.ok(!read("desktop/src/addons/addonBridge.ts").includes("declare const __APP_VERSION__"), "and nowhere else");
  assert.ok(read("desktop/src/shell/AboutDialog.tsx").includes("aboutLinks(__APP_HOMEPAGE__, __APP_REPOSITORY__)"), "About's doors are the baked-in URLs");
});

test("every built-in addon wears the platform's version, website and repository", () => {
  const folders = readdirSync(join(root, "library/addons"), { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name).sort();
  assert.equal(folders.length, 13);
  for (const f of folders) {
    const m = json(`library/addons/${f}/addon.json`);
    assert.equal(m.version, VERSION, `${f}: the platform's version`);
    assert.equal(m.homepage, HOMEPAGE, `${f}: the platform's website`);
    assert.equal(m.repo, REPOSITORY, `${f}: the platform's repository`);
    assert.equal(m.author, "Bisa", `${f}: the platform's own`);
  }
  const template = json("addons/template/addon.json");
  assert.notEqual(template.author, "Bisa", "the starter stays a third party's example");
});

test("the README, the getting-started clone line and the licence name the same website, repository and MIT", () => {
  const readme = read("README.md");
  assert.ok(readme.includes(HOMEPAGE) && readme.includes(REPOSITORY) && readme.includes("MIT"));
  assert.ok(read("docs/guide/getting-started.md").includes(`git clone ${REPOSITORY}`));
  assert.ok(read("docs/contributing/setup.md").includes(REPOSITORY) && read("docs/contributing/setup.md").includes(HOMEPAGE));
  assert.ok(existsSync(join(root, "LICENSE")), "the licence file exists");
  assert.match(read("LICENSE"), /^MIT License/);
  assert.ok(read("NOTICES.md").includes("`LICENSE`"));
});

test("the placeholder repository appears in no manifest, page or doc", () => {
  const files = ["Cargo.toml", "README.md", "NOTICES.md", "desktop/package.json", "desktop/src-tauri/Cargo.toml", "desktop/src-tauri/tauri.conf.json", "addons/sdk/package.json"];
  const walk = (dir, ext) => readdirSync(join(root, dir), { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? (e.name === "node_modules" || e.name === "target" ? [] : walk(join(dir, e.name), ext)) : ext.some((x) => e.name.endsWith(x)) ? [join(dir, e.name)] : []));
  // The placeholder, spelt in parts so this file does not name it whole.
  const placeholder = ["github.com", "bisa", "bisa"].join("/");
  for (const rel of [...files, ...walk("docs", [".md"]), ...walk("library/addons", [".json"]), ...walk("desktop/src", [".ts", ".tsx", ".mjs"]), ...walk("crates", ["Cargo.toml"])]) {
    assert.ok(!read(rel).includes(placeholder), `${rel} names the official repository, not the placeholder`);
  }
});
