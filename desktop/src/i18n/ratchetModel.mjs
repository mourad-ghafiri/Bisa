/**
 * The ratchet: how many bare sentences a desktop source still carries
 * (17 §Guards). This scanner counts what reads as a sentence to a person and
 * leaves out what may stay English.
 *
 * What it counts:
 * - **every word a screen draws** (`jsxProse`, `.tsx` only) — a JSX text
 *   node with a letter in it, whatever else it holds (`,` `;` `(` `)` `|`
 *   `=`) and whatever stands beside it (`{n} more`, `Hosted by {name}`); a
 *   literal an expression child renders (`{n === 1 ? "agent" : "agents"}`,
 *   through `?:`, `||`, `??`, `&&`, `+` and a template's own placeables); and
 *   the same for a `label=`/`title=`/`placeholder=`-shaped prop (`isProseProp`). These are
 *   read off the TypeScript parser's tree, so a generic (`useState<X>(null)`)
 *   is never mistaken for markup and a `className` on the same line hides
 *   nothing;
 * - a words table — an object key named as a prop is (`title: "Quit Bisa?"`);
 * - a string or template literal that reads as a sentence (`isProse`): a
 *   space and a few letters — two words, or one word beside a placeable
 *   (`${n} more`, `rule ${id}`). Each literal is read off the parser's tree
 *   too, in a `.ts` and a `.mjs` as in a `.tsx`, so a template nested in a
 *   template's placeable is seen on its own.
 *
 * What it leaves out: a log line, a thrown error's developer text, a class
 * list, an identifier compared or matched, an import, a comment, a URL, a
 * catalog id said through `t()`, and what stands in a prop that holds
 * identifiers (`className`, `data-*`, `name` — `isIdentifierProp`) or in a class helper's
 * arguments — by where the literal stands in the tree, so a class on a line
 * never excuses the sentence beside it — and a line **marked**
 * `for the agent` (a model reads English), `for the machine` (a word of a
 * protocol, which another program parses), `for the log` or `content, never
 * translated`, in a trailing comment or on a comment line of its own just
 * above (`MARKS`, `marked`).
 *
 * The count per file is `ratchet.baseline.json`; `scenarios/i18n.test.mjs`
 * holds the sources to it exactly, so a file that gained a sentence fails and
 * one that lost a sentence asks for the baseline to be lowered (`just
 * i18n-baseline`). The baseline is empty — `{}` — and stays so, apart from
 * what is not for a person at all: the prompts an agent reads
 * (`NOT_FOR_A_PERSON`). `node src/i18n/ratchet.mjs --list` prints
 * each sentence as `path:line: literal`.
 *
 * Plain `.mjs`, so `node --test` reads it and `ratchet.mjs` runs it; this
 * module reads sources and never writes.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

import ts from "typescript";

/** Props and object keys whose string value is a sentence for a person. */
export const PROSE_KEYS = Object.freeze(["label", "title", "aria-label", "aria-description", "placeholder", "description", "hint", "blurb", "confirmLabel", "tooltip", "heading", "subtitle", "caption", "note", "body", "word", "words", "text", "empty", "lead", "path", "switch"]);

/** The marks: a line that carries one — or stands under a comment line that does — is not for a person. */
export const MARKS = Object.freeze(["for the agent", "for the machine", "for the log", "content, never translated"]);

/**
 * Whether a prop's value is a sentence for a person: one of `PROSE_KEYS`, a
 * name that ends as one does (`keyLabel`, `valuePlaceholder`, `emptyHint`), or
 * one of the few a component takes its words through (`what`, `subject`,
 * `error`, `emptyLibrary`).
 * @param {string} name
 */
export function isProseProp(name) {
  return PROSE_KEYS.includes(name) || /(Label|Placeholder|Title|Hint|Words|Text|Note|Message|Tooltip|Blurb|Description|Library)$/.test(name) || ["what", "subject", "error"].includes(name);
}

/**
 * Whether a prop holds identifiers and nothing a person reads: a class list,
 * a `data-*` word, a form control's name, a key, an SVG path. Its literals
 * are left out whatever they look like; every other prop's are read as any
 * literal is.
 * @param {string} name
 */
export function isIdentifierProp(name) {
  return /^(className|class|key|id|name|type|role|href|src|htmlFor|style|d|viewBox|points|transform|preserveAspectRatio|list|aria-hidden|aria-keyshortcuts)$/.test(name) || /ClassName$/.test(name) || name.startsWith("data-");
}

/** A line with one of these says nothing to a person; its literals are left out. */
const LINE_EXEMPT = Object.freeze([
  ...MARKS,
  "log.",
  "console.",
  "import ",
  'from "',
  "throw new Error",
  "new Error(",
  "assert",
  "script-src",
  "style-src",
  "font-src",
  "monospace",
  "diff --git",
  "curl ",
  "cp .",
  "gen-keymap-docs",
  "new RegExp",
  "http://",
  "https://",
  "mailto:",
  "bisa://",
  "bisa.",
  "querySelector",
  "getAttribute",
  "setAttribute",
  "localStorage",
  "@fontsource",
  "content-type",
  "application/",
  "text/",
  "image/",
  "invoke(",
  "listen(",
  "emit(",
  "navigate(",
  "href(",
]);

/** The helpers whose arguments are class lists. */
const CLASS_HELPERS = Object.freeze(["cn", "clsx", "cva", "twMerge"]);

/** What stands just before a literal that is an identifier, not a sentence: a comparison, a match, a key. */
const BEFORE_EXEMPT = Object.freeze(["===", "!==", "case", ".includes(", ".startsWith(", ".endsWith(", ".indexOf(", "key:", "id:", "kind:", "type:", "name:", "target:", "route:", "icon:", "className:", "t(", "tr(", "rich(", "attr(", "tx("]);

/** What a sentence may open with before its first word: a separator, a quote, a sign. */
const OPENING = /^[\s·—–,…:“"'(@#+]+/u;

/**
 * Whether a literal reads as a sentence: it opens — after a separator or a
 * quote, if any — on a letter or a placeable, holds a space and at least
 * three letters, and says two words, or one word beside a placeable
 * (`{x} more`, `rule {x}`). Code is not a sentence: an assignment, a call, a
 * generic, a regex, a class list, a CSS value, an all-caps token list.
 * @param {string} literal a string's text, a template's with each placeable written `{x}`
 */
export function isProse(literal) {
  // An escape a sentence may carry — a line break, a quote — is not code.
  const s = literal.replace(/\\n|\\t/g, " ").replace(/\\(["'`])/g, "$1").trim();
  if (!s) return false;
  const opened = s.replace(OPENING, "");
  if (!/^[\p{L}{]/u.test(opened)) return false;
  if (!s.includes(" ")) return false;
  const bare = s.replace(/\{x\}/g, "x");
  if (/[=<>[\]|\\]/.test(bare)) return false; // code: a generic, an assignment, a regex
  if (/[\p{L}\p{N}_$.)]\(|\(\)/u.test(bare)) return false; // a call — `calc(…)`, `fn()`; a sentence's bracket stands after a space
  if (/;(?! )|;$|&(?! )|&&/.test(bare)) return false; // a statement's end, an entity, a conjunction of conditions
  if (s.includes("${")) return false; // a template nested in a template: code the scanner cut short
  const tokens = bare.split(/\s+/);
  if (tokens.every((t) => /^[-@]?[[\]&:a-z0-9./%(),*>+~=_"'@-]+$/.test(t)) && tokens.some((t) => /[-[]|:./.test(t))) return false; // a class list — `hover:bg-raised`, `gap-2`, a container's `@container`, `@md:grid-cols-2` — never a word that ends on a colon or a path's slash
  if (tokens.some((t) => /^-?\d+(\.\d+)?(px|rem|em|%|vh|vw|ms|s|fr|deg|ch)$/.test(t)) || s.includes("var(--")) return false; // a CSS value
  const words = s.replace(/\{x\}/g, " ").match(/\b[\p{L}]{2,}\b/gu) ?? [];
  // One word alone is an identifier or a label's key; one word beside a placeable is a sentence cut short (`{x} more`).
  if (words.length < 2 && !/^[A-Z]/.test(opened) && !(words.length === 1 && s.includes("{x}"))) return false;
  if (words.every((w) => w === w.toUpperCase())) return false; // an all-caps token list
  return (s.replace(/\{x\}/g, "").match(/\p{L}/gu) ?? []).length >= 3;
}

/** The source with block comments and comment lines removed — a comment says nothing to a person. */
export function withoutComments(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
    .split("\n")
    .map((line) => (/^\s*(\/\/|\*)/.test(line) ? "" : line))
    .join("\n");
}

function lineOf(text, index) {
  let n = 1;
  for (let i = 0; i < index && i < text.length; i++) if (text.charCodeAt(i) === 10) n++;
  return n;
}

function exemptLine(line) {
  return LINE_EXEMPT.some((e) => line.includes(e));
}

/**
 * Whether line `n` (from 1) of a source is marked as not for a person: it
 * carries a mark, or the line just above is a comment of its own that does
 * (a `//` line, or a JSX comment in braces, saying `for the machine`).
 * @param {readonly string[]} lines the source's lines, comments kept
 * @param {number} n
 */
export function marked(lines, n) {
  const has = (line) => MARKS.some((m) => line.includes(m));
  if (has(lines[n - 1] ?? "")) return true;
  const above = (lines[n - 2] ?? "").trim();
  return /^(\/\/|\/\*|\*|\{\/\*)/.test(above) && has(above);
}

/** Whether the text right before a literal's quote names it an identifier. @param {string} text @param {number} index the quote's index */
function exemptBefore(text, index) {
  const before = text.slice(Math.max(0, index - 16), index).trimEnd();
  return BEFORE_EXEMPT.some((e) => before.endsWith(e));
}

/** A source as the TypeScript parser reads it. @param {string} source @param {"tsx" | "ts" | "mjs"} kind */
function parse(source, kind) {
  const as = kind === "tsx" ? ts.ScriptKind.TSX : kind === "ts" ? ts.ScriptKind.TS : ts.ScriptKind.JS;
  return ts.createSourceFile(`source.${kind}`, source, ts.ScriptTarget.Latest, false, as);
}

/** An entity a text node spells — `&apos;`, `&#8230;` — holds no word of its own. */
const ENTITY = /&(?:[a-z]+|#\d+|#x[0-9a-f]+);/gi;

/**
 * Every word a screen draws that is not the catalog's, as `{ line, text }`:
 * a JSX text node with a letter in it, a literal an expression child
 * renders, and the same for a prop that is a sentence (`PROSE_KEYS`). Read
 * off the parser's tree — never a guess at where markup starts — and excused
 * only by a mark (`marked`).
 * @param {string} source a `.tsx` source, comments kept
 */
export function jsxProse(source) {
  const file = parse(source, "tsx");
  const lines = source.split("\n");
  /** @type {{line: number, text: string}[]} */
  const found = [];
  const lineAt = (pos) => file.getLineAndCharacterOfPosition(pos).line + 1;
  const add = (from, to, literal) => {
    const text = literal.replace(/\s+/g, " ").trim();
    if (!/\p{L}/u.test(text.replace(ENTITY, "").replace(/\{x\}/g, ""))) return;
    const first = lineAt(from);
    const last = lineAt(to);
    for (let n = first; n <= last; n++) if (marked(lines, n)) return;
    found.push({ line: first, text });
  };
  /** The literals an expression puts on screen: through `?:`, `||`, `??`, `&&`, `+`, brackets and a template's placeables; a call's answer is its own business. */
  const rendered = (e) => {
    if (!e) return;
    if (ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e)) add(e.getStart(file), e.end, e.text);
    else if (ts.isTemplateExpression(e)) {
      add(e.getStart(file), e.end, e.head.text + e.templateSpans.map((span) => `{x}${span.literal.text}`).join(""));
      for (const span of e.templateSpans) rendered(span.expression);
    } else if (ts.isConditionalExpression(e)) {
      rendered(e.whenTrue);
      rendered(e.whenFalse);
    } else if (ts.isParenthesizedExpression(e) || ts.isAsExpression(e) || ts.isNonNullExpression(e) || ts.isSatisfiesExpression(e)) rendered(e.expression);
    else if (ts.isBinaryExpression(e)) {
      const op = e.operatorToken.kind;
      if (op === ts.SyntaxKind.BarBarToken || op === ts.SyntaxKind.QuestionQuestionToken || op === ts.SyntaxKind.PlusToken) {
        rendered(e.left);
        rendered(e.right);
      } else if (op === ts.SyntaxKind.AmpersandAmpersandToken) rendered(e.right);
    }
  };
  const visit = (node) => {
    if (ts.isJsxElement(node) || ts.isJsxFragment(node)) {
      for (const child of node.children) {
        if (ts.isJsxText(child)) {
          const raw = source.slice(child.pos, child.end);
          add(child.pos + (raw.length - raw.trimStart().length), child.pos + raw.trimEnd().length, raw);
        } else if (ts.isJsxExpression(child)) rendered(child.expression);
      }
    } else if (ts.isJsxAttribute(node) && isProseProp(node.name.getText(file))) {
      const value = node.initializer;
      if (value && ts.isStringLiteral(value)) add(value.getStart(file), value.end, value.text);
      else if (value && ts.isJsxExpression(value)) rendered(value.expression);
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return found;
}

/**
 * The sentences of one source, as `{ line, text }`, in order and once each.
 * @param {string} source
 * @param {"tsx" | "ts" | "mjs"} kind
 */
export function bareProse(source, kind) {
  const text = withoutComments(source);
  const lines = source.split("\n");
  /** @type {Map<string, {line: number, text: string}>} */
  const found = new Map();
  const put = (line, literal) => {
    const key = `${line}:${literal}`;
    if (!found.has(key)) found.set(key, { line, text: literal });
  };
  const add = (index, literal) => {
    if (!isProse(literal)) return;
    const line = lineOf(text, index);
    if (marked(lines, line)) return;
    put(line, literal.trim());
  };
  // What a screen draws: its text nodes, the literals its expressions render, its sentence-shaped props.
  if (kind === "tsx") for (const f of jsxProse(source)) put(f.line, f.text);
  // An object key whose value is a sentence — a words table, a dialog's copy.
  const keys = new RegExp(`\\b(${PROSE_KEYS.filter((k) => !k.includes("-")).join("|")}):\\s*"([^"]*)"`, "g");
  for (const m of text.matchAll(keys)) add(m.index, m[2]);
  // Any string or template literal on a line that says something to a person —
  // each read off the parser's tree, so a template inside a template's
  // placeable is a literal of its own and a quote inside a regex opens nothing.
  const file = parse(source, kind);
  const stripped = text.split("\n");
  const literal = (node) => {
    const index = node.getStart(file);
    if (ts.isStringLiteral(node) && source[index] !== '"') return; // a single-quoted word is an identifier's
    const said = ts.isTemplateExpression(node) ? node.head.text + node.templateSpans.map((span) => `{x}${span.literal.text}`).join("") : node.text;
    const line = file.getLineAndCharacterOfPosition(index).line + 1;
    if (exemptLine(stripped[line - 1] ?? "") || exemptBefore(source, index)) return;
    if (!isProse(said) || marked(lines, line)) return;
    put(line, said.replace(/\s+/g, " ").trim());
  };
  const each = (node) => {
    // A prop that holds identifiers — a class, a `data-*` word, a name — says nothing, whatever else its line says;
    // a handler in it may still say a sentence, so its functions are walked. Every other prop is read as any literal is.
    if (ts.isJsxAttribute(node) && isIdentifierProp(node.name.getText(file))) {
      if (node.initializer && ts.isJsxExpression(node.initializer)) functionsOf(node.initializer);
      return;
    }
    // A class helper's arguments are classes.
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && CLASS_HELPERS.includes(node.expression.text)) return;
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) || ts.isTemplateExpression(node)) literal(node);
    ts.forEachChild(node, each);
  };
  const functionsOf = (node) => {
    if (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) each(node);
    else ts.forEachChild(node, functionsOf);
  };
  each(file);
  return [...found.values()].sort((a, b) => a.line - b.line || a.text.localeCompare(b.text));
}

/** The kind of a source file by its name, or `null` for one the ratchet leaves alone. @param {string} file */
/**
 * Files whose English is not for a person: the crash boundary (17 — it must
 * speak with no catalog), the prompts handed to an agent, the browser tools'
 * answers to an agent.
 */
const NOT_FOR_A_PERSON = Object.freeze(["views/_work/agentReviewModel.mjs", "views/_work/prReviewModel.mjs", "ui/artifact/pageInspector.mjs"]);

export function kindOf(file) {
  if (/\.test\.mjs$/.test(file) || file.endsWith(".d.mts") || file.endsWith("types.gen.ts") || file.endsWith("types.hand.ts")) return null;
  if (NOT_FOR_A_PERSON.some((f) => file === f || file.endsWith(`/${f}`) || file.endsWith(`\\${f.replaceAll("/", "\\")}`))) return null;
  if (/[\\/]scenarios[\\/]/.test(file) || file.endsWith("testWalk.mjs") || /(^|[\\/])i18n[\\/]ratchet/.test(file)) return null;
  if (file.endsWith(".tsx")) return "tsx";
  if (file.endsWith(".ts")) return "ts";
  if (file.endsWith(".mjs")) return "mjs";
  return null;
}

function walk(dir, out) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

/**
 * Every bare sentence under `src`, file by file, as `{ file, line, text }` —
 * paths relative to `src`, with `/`.
 * @param {string} src the `desktop/src` directory
 */
export function sentences(src) {
  /** @type {{file: string, line: number, text: string}[]} */
  const out = [];
  for (const path of walk(src, []).sort()) {
    const kind = kindOf(path);
    if (!kind) continue;
    const file = relative(src, path).replaceAll("\\", "/");
    for (const found of bareProse(readFileSync(path, "utf8"), kind)) out.push({ file, ...found });
  }
  return out;
}

/**
 * The count of sentences per file under `src`, files with none left out —
 * the shape of the baseline.
 * @param {string} src the `desktop/src` directory
 * @returns {Record<string, number>}
 */
export function baseline(src) {
  /** @type {Record<string, number>} */
  const counts = {};
  for (const { file } of sentences(src)) counts[file] = (counts[file] ?? 0) + 1;
  return counts;
}

/**
 * What changed between the baseline and now.
 * @param {Record<string, number>} was
 * @param {Record<string, number>} now
 */
export function compare(was, now) {
  const up = [];
  const down = [];
  for (const [file, n] of Object.entries(now)) {
    const before = was[file] ?? 0;
    if (n > before) up.push(`${file}: ${before} → ${n}`);
    else if (n < before) down.push(`${file}: ${before} → ${n}`);
  }
  for (const [file, before] of Object.entries(was)) if (!(file in now)) down.push(`${file}: ${before} → 0`);
  return { up: up.sort(), down: down.sort() };
}
