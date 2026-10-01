/**
 * The one door every word the desktop says goes through (17 — Internationalisation):
 * `t(id, args)` renders a message of the catalog (`locales/<lang>/**.ftl`) in
 * the language installed, `attr(id, name, args)` one attribute of it — a
 * widget's `.placeholder`, a setting's `.help` — and `tx(text)` a `Text` the
 * node sent: its id and its arguments, the same shape the crates render.
 *
 * Plain `.mjs`, so `node --test` reads it and every model may say its words
 * through it; the catalog is installed once — by `boot.ts` in the window from
 * the bundled files, by `testing.mjs` in a test from `locales/en` on disk
 * (`npm test` preloads it). A message the catalog lacks renders as its id —
 * visible on screen, greppable, and told once to whoever listens
 * (`onMissing`) — never as an empty string and never as a throw. A number
 * argument renders as digits, never grouped; `ORDINAL($n)` selects 1st · 2nd
 * · 3rd. Isolation
 * marks are off: a placeable is its characters, not FSI…PDI around them;
 * direction is the document's (`<html dir>`), not the sentence's.
 */

import { FluentBundle, FluentNumber, FluentResource } from "@fluent/bundle";

/** @type {{ locale: string, bundle: FluentBundle | null }} */
let current = { locale: "en", bundle: null };
/** @type {Set<string>} */
const missing = new Set();
/** @type {Set<(id: string) => void>} */
const listeners = new Set();

/**
 * Install a language: its sources, parsed into one bundle. Answers the
 * syntax errors, per message — a broken message is left out, the rest stand.
 * @param {string} locale
 * @param {readonly string[]} sources the `.ftl` texts
 * @returns {Error[]}
 */
export function install(locale, sources) {
  const bundle = new FluentBundle(locale, {
    useIsolating: false,
    functions: {
      // `{ ORDINAL($n) -> [one] { $n }st … }`: the number under ordinal plural rules (1st, 2nd, 3rd, 4th).
      ORDINAL: (args) => new FluentNumber(Number(args[0]?.valueOf() ?? 0), { type: "ordinal", useGrouping: false }),
    },
  });
  /** @type {Error[]} */
  const errors = [];
  for (const source of sources) errors.push(...bundle.addResource(new FluentResource(source), { allowOverrides: false }));
  current = { locale, bundle };
  missing.clear();
  return errors;
}

/** The language installed — `en` before any is. */
export function locale() {
  return current.locale;
}

/** Whether the catalog has a message `id`. */
export function has(id) {
  return current.bundle?.hasMessage(id) ?? false;
}

function miss(id) {
  if (missing.has(id)) return;
  missing.add(id);
  for (const l of listeners) l(id);
}

/**
 * Hear each id the catalog lacked, once — `boot.ts` logs it.
 * @param {(id: string) => void} listener
 */
export function onMissing(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * @param {import("@fluent/bundle").Pattern} pattern
 * @param {Record<string, unknown> | null | undefined} args
 */
/**
 * A number argument is digits — a port, a pid, a count — never grouped
 * (`4173`, not `4,173`); a plural still selects on it. What wants the
 * locale's grouping goes through `format.number` first, as a string.
 * @param {Record<string, unknown> | null | undefined} args
 */
function plain(args) {
  if (!args) return null;
  /** @type {Record<string, unknown>} */
  const out = {};
  for (const [k, v] of Object.entries(args)) out[k] = typeof v === "number" ? new FluentNumber(v, { useGrouping: false, maximumFractionDigits: 20 }) : v;
  return out;
}

function format(pattern, args) {
  /** @type {Error[]} */
  const errors = [];
  const out = /** @type {FluentBundle} */ (current.bundle).formatPattern(pattern, /** @type {any} */ (plain(args)), errors);
  return out;
}

/**
 * The sentence for a message, with its arguments; the id when there is none.
 * @param {string} id
 * @param {Record<string, unknown>} [args]
 */
export function t(id, args) {
  const message = current.bundle?.getMessage(id);
  if (!message?.value) {
    miss(id);
    return id;
  }
  return format(message.value, args);
}

/**
 * One attribute of a message — `.help`, `.placeholder`; `id.name` when there is none.
 * @param {string} id
 * @param {string} name
 * @param {Record<string, unknown>} [args]
 */
export function attr(id, name, args) {
  const message = current.bundle?.getMessage(id);
  const pattern = message?.attributes?.[name];
  if (!pattern) {
    miss(`${id}.${name}`);
    return `${id}.${name}`;
  }
  return format(pattern, args);
}

/**
 * A `Text` the node sent — `{ id, args }` — rendered here, in this language.
 * @param {{ id: string, args?: Record<string, unknown> | null } | null | undefined} text
 */
export function tx(text) {
  if (!text || typeof text.id !== "string") return "";
  return t(text.id, text.args ?? undefined);
}
