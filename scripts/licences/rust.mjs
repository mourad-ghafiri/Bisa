/**
 * The Rust side of the licence work: every third-party crate the two
 * workspaces resolve — the root (every platform: the `bisa` binary ships on
 * each) and the desktop shell (the macOS targets, the platform shipped
 * today) — with its declared licence, the text and notice files beside its
 * manifest, and the branch this distribution takes under `deny.toml`'s list.
 * Reads `cargo metadata --offline --locked`; nothing is fetched.
 */
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { chooseBranch, copyrightLines, needsSourcePointer } from "./licencesModel.mjs";

const LICENCE_FILE = /^(LICEN[CS]E|COPYING|COPYRIGHT)(\b|[-_.])/i;
const NOTICE_FILE = /^NOTICE(\b|[-_.])/i;

function metadata(manifestDir, platforms) {
  const args = ["metadata", "--format-version", "1", "--offline", "--locked"];
  for (const p of platforms) args.push("--filter-platform", p);
  const out = execFileSync("cargo", args, { cwd: manifestDir, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "pipe"] });
  return JSON.parse(out);
}

/** The licence-like files beside a manifest, as `{name, text}`, sorted by name. */
export function textsBeside(manifestPath) {
  const dir = dirname(manifestPath);
  let names = [];
  try {
    names = readdirSync(dir);
  } catch {
    return { licences: [], notices: [] };
  }
  const isFile = (n) => {
    try {
      return statSync(join(dir, n)).isFile();
    } catch {
      return false;
    }
  };
  const read = (n) => ({ name: n, text: readFileSync(join(dir, n), "utf8") });
  const licences = names.filter((n) => LICENCE_FILE.test(n) && isFile(n)).sort().map(read);
  const notices = names.filter((n) => NOTICE_FILE.test(n) && isFile(n)).sort().map(read);
  return { licences, notices };
}

/**
 * The third-party packages of one workspace, judged under `allow`.
 * @param {string} manifestDir
 * @param {readonly string[]} allow
 * @param {readonly string[]} platforms `--filter-platform` targets; none for every platform
 * @param {string} where a word for the notices — which binary carries them
 */
export function rustPackages(manifestDir, allow, platforms, where) {
  const meta = metadata(manifestDir, platforms);
  const own = new Set(meta.workspace_members);
  const out = [];
  for (const p of meta.packages) {
    if (own.has(p.id)) continue;
    if (!p.source) continue; // a path dependency of the workspace: ours
    const declared = p.license ?? (p.license_file ? `see ${p.license_file}` : null);
    const branch = declared ? chooseBranch(declared, allow) : null;
    const { licences, notices } = textsBeside(p.manifest_path);
    const text = licences.map((l) => l.text).join("\n");
    out.push({
      kind: "crate",
      where,
      name: p.name,
      version: p.version,
      declared: declared ?? "(none declared)",
      branch,
      copyrights: copyrightLines(text),
      licences,
      notices,
      source: needsSourcePointer(branch) ? `https://crates.io/crates/${p.name}/${p.version}` : null,
    });
  }
  return out;
}

/** The two workspaces, as the shipped binaries carry them. */
export function allRustPackages(root, allow) {
  const macos = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
  return [
    ...rustPackages(root, allow, [], "the bisa binary"),
    ...rustPackages(join(root, "desktop", "src-tauri"), allow, macos, "the desktop shell (macOS)"),
  ];
}
