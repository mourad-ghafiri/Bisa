/**
 * The licence rules, pure (contributing/release.md §Licence): the one allow
 * list is `deny.toml`'s; an SPDX expression is judged by choosing the first
 * allowed alternative of an `OR`, and by needing every term of an `AND`;
 * copyleft that has no permissive alternative is a failure by name. The
 * same rules render the third-party notices, so what the gate lets through
 * is exactly what the notices say. No I/O here: `node --test` reads it.
 */

/** The `[licenses] allow = [...]` list of a deny.toml, in order. */
export function parseAllow(denyToml) {
  const at = denyToml.indexOf("[licenses]");
  if (at < 0) return [];
  const block = denyToml.slice(at);
  const open = block.indexOf("allow = [");
  if (open < 0) return [];
  const close = block.indexOf("]", open);
  return [...block.slice(open, close).matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

/** The terms that make an expression copyleft when no permissive branch is offered. */
const COPYLEFT = /^(A?GPL|LGPL|SSPL|EUPL|CPAL|CC-BY-SA|CC-BY-NC|OSL|CECILL)/i;

/**
 * Tokenise an SPDX expression: ids, `WITH` exceptions kept with their id,
 * `OR`/`AND` (a `/` is the legacy `OR`), parentheses.
 */
function tokens(expression) {
  return String(expression)
    .replace(/\//g, " OR ")
    .replace(/\(/g, " ( ")
    .replace(/\)/g, " ) ")
    .split(/\s+/)
    .filter(Boolean);
}

/**
 * Parse an expression into a tree: `{id}` | `{or: [...]}` | `{and: [...]}`.
 * Precedence as SPDX: `WITH` binds tightest, then `AND`, then `OR`.
 */
export function parseExpression(expression) {
  const ts = tokens(expression);
  let i = 0;
  const peek = () => ts[i];
  const next = () => ts[i++];
  function primary() {
    const t = next();
    if (t === undefined) throw new Error(`an expression ended early: ${expression}`);
    if (t === "(") {
      const e = or();
      if (next() !== ")") throw new Error(`a parenthesis was not closed: ${expression}`);
      return e;
    }
    let id = t;
    if (peek() && peek().toUpperCase() === "WITH") {
      next();
      id = `${id} WITH ${next()}`;
    }
    return { id };
  }
  function and() {
    const parts = [primary()];
    while (peek() && peek().toUpperCase() === "AND") {
      next();
      parts.push(primary());
    }
    return parts.length === 1 ? parts[0] : { and: parts };
  }
  function or() {
    const parts = [and()];
    while (peek() && peek().toUpperCase() === "OR") {
      next();
      parts.push(and());
    }
    return parts.length === 1 ? parts[0] : { or: parts };
  }
  const tree = or();
  if (i !== ts.length) throw new Error(`an expression did not end where it should: ${expression}`);
  return tree;
}

/** Every id an expression names, in reading order. */
export function idsOf(expression) {
  const out = [];
  const walk = (n) => {
    if (n.id) out.push(n.id);
    for (const c of n.or ?? n.and ?? []) walk(c);
  };
  walk(parseExpression(expression));
  return out;
}

/**
 * The branch of `expression` this distribution takes under `allow`: the
 * first alternative of an `OR` whose every term is allowed, as a normalised
 * expression (`A AND B`), or `null` when no branch is allowed.
 * @param {string} expression
 * @param {readonly string[]} allow
 * @returns {string | null}
 */
export function chooseBranch(expression, allow) {
  const allowed = new Set(allow);
  const pick = (n) => {
    if (n.id) return allowed.has(n.id) ? n.id : null;
    if (n.or) {
      for (const c of n.or) {
        const chosen = pick(c);
        if (chosen) return chosen;
      }
      return null;
    }
    const parts = n.and.map(pick);
    return parts.every(Boolean) ? parts.join(" AND ") : null;
  };
  let tree;
  try {
    tree = parseExpression(expression);
  } catch {
    return null;
  }
  return pick(tree);
}

/** An expression with no allowed branch that names a copyleft term: the failure the gate names. */
export function isCopyleftOnly(expression, allow) {
  if (chooseBranch(expression, allow)) return false;
  let ids;
  try {
    ids = idsOf(expression);
  } catch {
    return false;
  }
  return ids.some((id) => COPYLEFT.test(id));
}

/** The copyright lines of a licence text, trimmed and deduplicated, at most `max`. */
export function copyrightLines(text, max = 6) {
  const seen = new Set();
  const out = [];
  for (const raw of String(text ?? "").split("\n")) {
    const line = raw.replace(/^[\s#*/-]+/, "").trim();
    if (!/^copyright\b/i.test(line) && !/^\(c\)\s/i.test(line) && !/^©/.test(line)) continue;
    if (line.length > 200) continue;
    if (seen.has(line)) continue;
    seen.add(line);
    out.push(line);
    if (out.length >= max) break;
  }
  return out;
}

/**
 * Packages grouped by the branch chosen, each group sorted by name then
 * version; a package with no branch goes under `null`.
 * @param {{name: string, version: string, branch: string | null}[]} packages
 * @returns {Map<string | null, typeof packages>}
 */
export function groupByLicence(packages) {
  const groups = new Map();
  const sorted = [...packages].sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  for (const p of sorted) {
    const list = groups.get(p.branch) ?? [];
    list.push(p);
    groups.set(p.branch, list);
  }
  return new Map(
    [...groups.entries()].sort(([a], [b]) => {
      if (a === null) return 1;
      if (b === null) return -1;
      return a.localeCompare(b);
    }),
  );
}

/**
 * One package's line of the notices: name, version, the licence as declared
 * and the branch taken when they differ, its copyright lines, and — for a
 * file-level copyleft branch — where its source is.
 */
export function renderPackage(p) {
  const licence = p.branch && p.branch !== p.declared ? `${p.declared} — taken as ${p.branch}` : p.declared;
  const parts = [`- **${p.name}** ${p.version} — ${licence}`];
  if (p.copyrights?.length) parts.push(`  ${p.copyrights.join("; ")}`);
  if (p.source) parts.push(`  source: ${p.source}`);
  return parts.join("\n");
}

/** Whether a branch carries an obligation to point at the source (MPL, file-level copyleft). */
export function needsSourcePointer(branch) {
  return typeof branch === "string" && /\bMPL-/.test(branch);
}
