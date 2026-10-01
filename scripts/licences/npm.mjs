/**
 * The npm side of the licence work: the desktop's runtime closure — every
 * package Vite may bundle, reached from `dependencies` through
 * `dependencies`, `optionalDependencies` and `peerDependencies` with Node's
 * own resolution through nested `node_modules` — each with its declared
 * licence, the text and notice files beside its `package.json`, and the
 * branch this distribution takes. The dev tools (vite, eslint, tsc…) do not
 * ship and are reported apart. Reads `desktop/node_modules`; nothing is fetched.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { chooseBranch, copyrightLines, needsSourcePointer } from "./licencesModel.mjs";

const LICENCE_FILE = /^(LICEN[CS]E|COPYING|COPYRIGHT)(\b|[-_.])/i;
const NOTICE_FILE = /^(NOTICE|ThirdPartyNotices?)(\b|[-_.])/i;

/**
 * Two packages whose manifests say less than their licence files do; the
 * gate would otherwise stop on them, so the words are here, once.
 */
export const KNOWN = Object.freeze({
  duck: "BSD-2-Clause", // `"license": "BSD"`; the LICENSE file is the two-clause text
  khroma: "MIT", // no `license` field; the `license` file is the MIT text
});

/** The declared licence of a package.json, or null. */
export function declaredOf(pkg) {
  if (typeof pkg.license === "string") return pkg.license;
  if (pkg.license && typeof pkg.license.type === "string") return pkg.license.type;
  if (Array.isArray(pkg.licenses)) return pkg.licenses.map((l) => (typeof l === "string" ? l : l.type)).filter(Boolean).join(" OR ") || null;
  return null;
}

/** Node's resolution: `name` as seen from `from`, walking up through nested `node_modules`. */
function resolvePackage(from, name, root) {
  let dir = from;
  for (;;) {
    const candidate = join(dir, "node_modules", name, "package.json");
    if (existsSync(candidate)) return candidate;
    if (dir === root || dir === dirname(dir)) return null;
    dir = dirname(dir);
  }
}

function textsBeside(pkgPath) {
  const dir = dirname(pkgPath);
  const names = readdirSync(dir);
  const isFile = (n) => {
    try {
      return statSync(join(dir, n)).isFile();
    } catch {
      return false;
    }
  };
  const read = (n) => ({ name: n, text: readFileSync(join(dir, n), "utf8") });
  return {
    licences: names.filter((n) => LICENCE_FILE.test(n) && isFile(n)).sort().map(read),
    notices: names.filter((n) => NOTICE_FILE.test(n) && isFile(n)).sort().map(read),
  };
}

/**
 * The runtime closure of `desktop/`, judged under `allow`.
 * @param {string} desktopDir
 * @param {readonly string[]} allow
 * @returns {{packages: object[], missing: string[], optionalMissing: string[]}}
 */
export function npmRuntimePackages(desktopDir, allow) {
  const rootPkg = JSON.parse(readFileSync(join(desktopDir, "package.json"), "utf8"));
  const seen = new Map();
  const missing = [];
  const optionalMissing = [];
  // A `dependency` must be here; an optional or a peer dependency is followed
  // when installed (another platform's binary, a peer the app never
  // installed) and named apart when not — it does not ship from here.
  const queue = Object.keys(rootPkg.dependencies ?? {}).map((name) => ({ name, from: desktopDir, required: true }));
  while (queue.length) {
    const { name, from, required } = queue.shift();
    const pkgPath = resolvePackage(from, name, desktopDir);
    if (!pkgPath) {
      const list = required ? missing : optionalMissing;
      if (!list.includes(name)) list.push(name);
      continue;
    }
    if (seen.has(pkgPath)) continue;
    const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
    seen.set(pkgPath, pkg);
    const dir = dirname(pkgPath);
    for (const dep of Object.keys(pkg.dependencies ?? {})) queue.push({ name: dep, from: dir, required: true });
    for (const dep of [...Object.keys(pkg.optionalDependencies ?? {}), ...Object.keys(pkg.peerDependencies ?? {})]) queue.push({ name: dep, from: dir, required: false });
  }
  const packages = [];
  for (const [pkgPath, pkg] of seen) {
    const declared = KNOWN[pkg.name] ?? declaredOf(pkg);
    const branch = declared ? chooseBranch(declared, allow) : null;
    const { licences, notices } = textsBeside(pkgPath);
    packages.push({
      kind: "npm",
      where: "the desktop",
      name: pkg.name,
      version: pkg.version,
      declared: declared ?? "(none declared)",
      branch,
      copyrights: copyrightLines(licences.map((l) => l.text).join("\n")),
      licences,
      notices,
      source: needsSourcePointer(branch) ? `https://www.npmjs.com/package/${pkg.name}/v/${pkg.version}` : null,
    });
  }
  return { packages, missing, optionalMissing };
}

/** Every package installed under `desktop/node_modules` that the runtime closure does not reach: the build's tools. */
export function npmDevOnly(desktopDir, runtime) {
  const reached = new Set(runtime.map((p) => `${p.name}@${p.version}`));
  const out = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
      const p = join(dir, entry.name);
      if (entry.name.startsWith("@")) {
        for (const sub of readdirSync(p, { withFileTypes: true })) if (sub.isDirectory()) visit(join(p, sub.name));
      } else visit(p);
    }
  };
  const visit = (dir) => {
    const pkgPath = join(dir, "package.json");
    if (existsSync(pkgPath)) {
      try {
        const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
        if (pkg.name && !reached.has(`${pkg.name}@${pkg.version}`)) out.push({ name: pkg.name, version: pkg.version, declared: KNOWN[pkg.name] ?? declaredOf(pkg) ?? "(none declared)" });
      } catch {
        // not a package
      }
    }
    const nested = join(dir, "node_modules");
    if (existsSync(nested)) walk(nested);
  };
  walk(join(desktopDir, "node_modules"));
  return out;
}
