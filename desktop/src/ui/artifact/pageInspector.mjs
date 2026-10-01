/**
 * The inspector a rendered page runs when the IDE annotates it (ide/03
 * §Annotate): one inline script, injected by `pageDocument` right after the
 * page's policy metas, and the vocabulary the two sides of the frame speak.
 *
 * The page is a sandboxed frame with an opaque origin (`PAGE_SANDBOX`), so
 * the app cannot reach its DOM and the page cannot reach the app. What
 * crosses is `postMessage` alone: the app tells the frame to inspect and
 * which elements wear a badge; the frame tells the app what the pointer is
 * over, what was clicked — the element's path, its tag, the head of its
 * HTML, its text, its rectangle — the note the person typed for it, and
 * when a badge's element is gone. The frame draws the outline, the tag
 * label, the badges **and the note box** itself, in its own DOM: only it
 * knows its scroll, and only a box in the page sits over the element on a
 * native page too (ide/18) — one implementation, so the IDE's frame and the
 * browser tab annotate exactly alike; the app draws the tray, because only
 * it has the kit and the doors. A pick carries the element's ancestors up
 * to `html` — the crumbs the box draws — and each crumb re-picks that
 * element in the page, so the root, the body or any container is one click
 * from any element; `html` and `body` are picks like any other.
 *
 * The inspector is in one of two **modes** (`INSPECT_MODES`): `off`, the
 * page is the page's own; `picking`, the pointer outlines and a click
 * picks. A box open is the page's own held state: the pointer outlines
 * nothing and a click outside the box picks nothing (it is still swallowed,
 * because a link under the box must not navigate while a note is typed),
 * while the outline stays on the pick, a scroll keeps the box with it, and
 * a click inside the box is the box's own. Enter or *Add* says the note
 * (`NOTE`) with the element it is about; Escape or *Never mind* closes the
 * box alone (`CLOSED`); Escape with no box open leaves the inspector.
 *
 * Trust is the window, never the origin: an opaque origin reads `"null"`,
 * so both sides check `event.source` — the frame accepts its parent alone,
 * the app accepts the frame's window alone — and the app re-bounds every
 * field it receives (`parseInspectorMessage`), since the page's own scripts
 * can post anything. The script names no network, no storage and no cookie;
 * a test holds it to that. It adds no capability the policy did not already
 * grant (`script-src 'unsafe-inline'`) and is never injected for an agent's
 * artifact card or stage.
 *
 * The same wire carries **find**: the app says what to look for and which
 * match to stand on, the frame highlights every match in its own text with
 * the highlight registry (the app cannot see the page's DOM) and answers
 * where it landed. The script rides in every page the IDE opens; a page
 * outside a workstream carries it and is never asked to inspect.
 *
 * Plain JavaScript so `node --test` runs the script in a `vm` against a
 * small fake document, and the parser without one.
 */

import { BOX_WIDTH, inspectorStyles } from "./inspectorTheme.mjs";
import { t as tr } from "../../i18n/l10n.mjs";


/** The head of an element's HTML a pick carries, in UTF-8 bytes; the tail says when it was cut. */
export const MAX_EXCERPT_BYTES = 2048;
/** How far up a path climbs before it gives up on a locator. */
const MAX_SELECTOR_DEPTH = 16;
export const MAX_SELECTOR_CHARS = 512;
const MAX_TAG_CHARS = 64;
/** An element's text, whitespace collapsed, at most this long. */
export const MAX_TEXT_CHARS = 200;
/** What a cut excerpt ends with — the same words a cut selection chip uses. */
export const CUT_TAIL = "\n… (cut)";
/** The most text a read answers (ide/18) — characters; the agent's bound is bytes on the engine. */
export const MAX_READ_CHARS = 16384;
/** How many matches a find-text answers, each with this many characters around it. */
const MAX_MATCHES = 20;
const MAX_AROUND = 80;
/** How many lines a snapshot's outline holds at most (ide/18); the rest is counted. */
const MAX_OUTLINE_LINES = 400;
/** How many dialogs and console lines one answer carries, and how long each line may be. */
export const MAX_DIALOGS = 20;
export const MAX_CONSOLE_LINES = 100;
export const MAX_LINE_CHARS = 500;
/** How long a wait waits at most, and how quiet a page must be to be idle. */
export const MAX_WAIT_MS = 30000;
export const IDLE_MS = 500;
/** How often a wait looks again. */
const WAIT_POLL_MS = 100;
/** The most a page's title is carried as. */
const MAX_TITLE_CHARS = 200;
const MAX_URL_CHARS = 2048;
const MAX_ID_CHARS = 64;
const MAX_ERROR_CHARS = 512;

/** The most a note typed in the box is carried as. */
export const MAX_NOTE_CHARS = 1000;

/** The inspector's modes: the page's own, or the pointer picking. A word off this list is `off`. */
const INSPECT_MODES = Object.freeze(["off", "picking"]);

/** The message types, both ways. */
export const INSPECTOR_MESSAGES = Object.freeze({
  /** App → frame: which mode to be in. `{mode: "off" | "picking"}` — `off` closes a box open too. */
  INSPECT: "bisa:inspect",
  /** App → frame: the badges to draw, each with its note — what a box on that element opens with. `{marks: [{selector, n, note}]}` */
  MARKS: "bisa:marks",
  /** App → frame: the overlay's dress, said when the theme moves — `{styles}`, `inspectorStyles(theme)`; a box open re-paints in place. */
  THEME: "bisa:theme",
  /** Frame → app: the element under the pointer. `{tag, rect}` */
  HOVER: "bisa:hover",
  /** Frame → app: an element was picked — clicked, or a crumb of the box — and the box opened on it. `{selector, tag, excerpt, text, rect, ancestors: [{tag, selector}]}` — the ancestors outermost first, `html` first. */
  PICK: "bisa:pick",
  /** Frame → app: the note typed for an element — *Add* or Enter in the box, which closed. `{selector, tag, excerpt, text, note}` */
  NOTE: "bisa:note",
  /** Frame → app: the box closed with no note — *Never mind* or Escape in it. */
  CLOSED: "bisa:closed",
  /** Frame → app: badges whose element the page no longer has. `{lost: number[]}` */
  MARKED: "bisa:marked",
  /** Frame → app: Escape was pressed inside the page with no box open — leave the inspector. */
  ESCAPE: "bisa:escape",
  /** App → frame: find this in the page's text and stand on the `index`th match. `{query, regex, caseSensitive, index}` */
  FIND: "bisa:find",
  /** Frame → app: what the find landed on. `{index, count}` — `-1` and `0` for nothing. */
  FOUND: "bisa:found",
  /** Frame → app: where the page is scrolled to, once a frame while it moves. `{top, left}` — the frame is sandboxed, so its scroll is the page's to say. */
  PLACE: "bisa:place",
  /** App → frame: scroll back to a kept place, said after every load (a `srcdoc` change reloads at the top). `{top, left}` */
  PLACE_TO: "bisa:place-to",
  /** App → page (the browser's driver, ide/18): the page's text, or an element's — as text or HTML. `{id, target?, format?}`; answered by an `ANSWER`. A target is a CSS selector or a snapshot's ref (`e12`). */
  READ: "bisa:read",
  /** App → page: the matches of some words, each with its surroundings and the selector holding it. `{id, query}` */
  FIND_TEXT: "bisa:find-text",
  /** App → page: the page's outline — headings, landmarks, links, buttons, fields — each with a ref. `{id, target?, all?}` */
  SNAPSHOT: "bisa:snapshot",
  /** App → page: click the element. `{id, target}` */
  CLICK: "bisa:click",
  /** App → page: set the field's value in one go. `{id, target, text}` */
  FILL: "bisa:fill",
  /** App → page: type into the field a keystroke at a time. `{id, target, text, clear?, submit?}` */
  TYPE: "bisa:type",
  /** App → page: press a key on the element, else the focused one. `{id, key, target?, modifiers?}` */
  PRESS: "bisa:press",
  /** App → page: choose an option of a select. `{id, target, value?, label?}` */
  SELECT: "bisa:select",
  /** App → page: move the pointer over the element. `{id, target}` */
  POINT: "bisa:point",
  /** App → page: scroll the page or an element. `{id, to?, byX?, byY?}` */
  SCROLL: "bisa:scroll",
  /** App → page: wait for an element, some text, its going, or quiet. `{id, until, target?, query?, timeoutMs}` */
  WAIT: "bisa:wait",
  /** App → page: what the page logged and the errors it raised. `{id, clear?}` */
  CONSOLE: "bisa:console",
  /** App → page: evaluate an expression and answer its value. `{id, expression}` */
  EVAL: "bisa:eval",
  /** Page → app: a driver request's answer. `{id, ok, url, title, text?, error?, selector?, count?, waitedMs?, scroll?, value?, dialogs?, console?}` */
  ANSWER: "bisa:answer",
  /** Page → app: the page loaded or moved — its URL and title. `{url, title}` */
  TITLE: "bisa:title",
  /** Page → app (the browser tab, ide/18): a browser chord pressed inside the page. `{command}`, one of `BROWSER_COMMANDS`. */
  KEY: "bisa:key",
});

/** The name the browser tab's page posts to: `window.webkit.messageHandlers.bisa` — the desktop's one door in from every page (ide/18). */
export const BROWSER_DOOR = "bisa";

/**
 * The commands a chord pressed inside a browser tab's page relays to the
 * app, since a key pressed in the native view never reaches the main
 * window: the keymap's own ids, so the bar acts on them as it does on the
 * chord typed in the main window.
 */
export const BROWSER_COMMANDS = Object.freeze(["focus_address", "browser_reload", "browser_back", "browser_forward", "new_browser_tab", "close_tab"]);

/** The app's word to the page's driver: read it, or the element — as text or HTML. @param {string} id @param {string | null} [target] @param {"text" | "html"} [format] */
export function readMessage(id, target = null, format = "text") {
  return { type: INSPECTOR_MESSAGES.READ, id: String(id), target: target ? String(target) : null, format: format === "html" ? "html" : "text" };
}
/** @param {string} id @param {string} query */
export function findTextMessage(id, query) {
  return { type: INSPECTOR_MESSAGES.FIND_TEXT, id: String(id), query: String(query ?? "") };
}
/** @param {string} id @param {string | null} [target] @param {boolean} [all] */
export function snapshotMessage(id, target = null, all = false) {
  return { type: INSPECTOR_MESSAGES.SNAPSHOT, id: String(id), target: target ? String(target) : null, all: !!all };
}
/** @param {string} id @param {string} target */
export function clickMessage(id, target) {
  return { type: INSPECTOR_MESSAGES.CLICK, id: String(id), target: String(target) };
}
/** @param {string} id @param {string} target @param {string} text */
export function fillMessage(id, target, text) {
  return { type: INSPECTOR_MESSAGES.FILL, id: String(id), target: String(target), text: String(text ?? "") };
}
/** @param {string} id @param {string} target @param {string} text @param {boolean} [clear] @param {boolean} [submit] */
export function typeMessage(id, target, text, clear = false, submit = false) {
  return { type: INSPECTOR_MESSAGES.TYPE, id: String(id), target: String(target), text: String(text ?? ""), clear: !!clear, submit: !!submit };
}
/** @param {string} id @param {string} key @param {string | null} [target] @param {readonly string[]} [modifiers] */
export function pressMessage(id, key, target = null, modifiers = []) {
  return { type: INSPECTOR_MESSAGES.PRESS, id: String(id), key: String(key), target: target ? String(target) : null, modifiers: (modifiers ?? []).map(String) };
}
/** @param {string} id @param {string} target @param {string | null} [value] @param {string | null} [label] */
export function selectMessage(id, target, value = null, label = null) {
  return { type: INSPECTOR_MESSAGES.SELECT, id: String(id), target: String(target), value: value == null ? null : String(value), label: label == null ? null : String(label) };
}
/** @param {string} id @param {string} target */
export function pointMessage(id, target) {
  return { type: INSPECTOR_MESSAGES.POINT, id: String(id), target: String(target) };
}
/** @param {string} id @param {string | null} [to] @param {number} [byX] @param {number} [byY] */
export function scrollMessage(id, to = null, byX = 0, byY = 0) {
  return { type: INSPECTOR_MESSAGES.SCROLL, id: String(id), to: to ? String(to) : null, byX: Number(byX) || 0, byY: Number(byY) || 0 };
}
/** @param {string} id @param {"selector" | "text" | "gone" | "idle"} until @param {string | null} [target] @param {string | null} [query] @param {number} [timeoutMs] */
export function waitMessage(id, until, target = null, query = null, timeoutMs = 10000) {
  const ms = Math.min(MAX_WAIT_MS, Math.max(1, Math.floor(Number(timeoutMs) || 10000)));
  return { type: INSPECTOR_MESSAGES.WAIT, id: String(id), until: String(until), target: target ? String(target) : null, query: query == null ? null : String(query), timeoutMs: ms };
}
/** @param {string} id @param {boolean} [clear] */
export function consoleMessage(id, clear = false) {
  return { type: INSPECTOR_MESSAGES.CONSOLE, id: String(id), clear: !!clear };
}
/** @param {string} id @param {string} expression */
export function evalMessage(id, expression) {
  return { type: INSPECTOR_MESSAGES.EVAL, id: String(id), expression: String(expression ?? "") };
}

/** The app's word to the frame: be in this mode. A word off the list is `off`. @param {string} mode */
export function inspectMessage(mode) {
  return { type: INSPECTOR_MESSAGES.INSPECT, mode: INSPECT_MODES.includes(mode) ? mode : "off" };
}

/** The app's word to the frame: these elements wear these numbers, with these notes. @param {readonly {selector: string, n: number, note?: string}[]} marks */
export function marksMessage(marks) {
  return { type: INSPECTOR_MESSAGES.MARKS, marks: (marks ?? []).map((m) => ({ selector: String(m.selector), n: Number(m.n), note: String(m.note ?? "") })) };
}

/**
 * The app's word to the frame: wear this theme — every part's `cssText` from
 * `inspectorStyles`, so the page never resolves a token itself.
 * @param {import("./inspectorTheme.mjs").InspectorTheme} theme
 */
export function themeMessage(theme) {
  return { type: INSPECTOR_MESSAGES.THEME, styles: inspectorStyles(theme) };
}

/**
 * The app's word to the frame: find this, and stand on this match. An empty
 * query clears the highlights.
 * @param {{query: string, regex: boolean, caseSensitive: boolean} | null | undefined} find
 * @param {number} index
 */
/**
 * App → frame: put the page back where it was. Whole, non-negative pixels.
 * @param {{top: number, left: number} | null | undefined} place
 */
export function placeToMessage(place) {
  const px = (n) => (Number.isFinite(n) && n > 0 ? Math.round(n) : 0);
  return { type: INSPECTOR_MESSAGES.PLACE_TO, top: px(place?.top), left: px(place?.left) };
}

export function findMessage(find, index) {
  return {
    type: INSPECTOR_MESSAGES.FIND,
    query: typeof find?.query === "string" ? find.query.slice(0, MAX_TEXT_CHARS * 5) : "",
    regex: !!find?.regex,
    caseSensitive: !!find?.caseSensitive,
    index: Number.isInteger(index) ? index : 0,
  };
}

/**
 * A string cut to at most `max` UTF-8 bytes, the tail appended when it was
 * cut — so the bound holds on the wire, where bytes are what count.
 * @param {string} s
 * @param {number} max
 */
export function cutBytes(s, max) {
  const enc = new TextEncoder();
  const bytes = enc.encode(s);
  if (bytes.length <= max) return s;
  const tail = enc.encode(CUT_TAIL);
  const room = Math.max(0, max - tail.length);
  // A cut inside a multi-byte character is dropped by the decoder, never invented.
  const head = new TextDecoder("utf-8", { fatal: false }).decode(bytes.slice(0, room)).replace(/�+$/u, "");
  return `${head}${CUT_TAIL}`;
}

const chars = (v, max) => (typeof v === "string" ? v.slice(0, max) : null);
/** A non-negative whole number, or null. */
const wholeOf = (v) => (typeof v === "number" && Number.isInteger(v) && v >= 0 ? v : null);
/** Where a page stands: four whole numbers, or null. */
function scrollOf(v) {
  if (!v || typeof v !== "object") return null;
  const s = /** @type {Record<string, unknown>} */ (v);
  const n = (x) => (typeof x === "number" && Number.isFinite(x) ? Math.round(x) : null);
  const x = n(s.x), y = n(s.y), width = n(s.width), height = n(s.height);
  return x === null || y === null || width === null || height === null ? null : { x, y, width, height };
}
/** A script's value: JSON as the page said it, cut to a string when it is too long; nothing for nothing. */
function valueOf(v) {
  if (v === undefined) return null;
  let json;
  try {
    json = JSON.stringify(v);
  } catch {
    return null;
  }
  if (json === undefined) return null;
  return json.length > MAX_READ_CHARS ? json.slice(0, MAX_READ_CHARS) + CUT_TAIL : v;
}
/** The dialogs a page raised, each bounded; at most `MAX_DIALOGS`. */
function dialogsOf(v) {
  if (!Array.isArray(v)) return [];
  const out = [];
  for (const d of v) {
    if (out.length >= MAX_DIALOGS) break;
    if (!d || typeof d !== "object") continue;
    const kind = typeof d.kind === "string" && ["alert", "confirm", "prompt"].includes(d.kind) ? d.kind : null;
    const message = chars(d.message, MAX_LINE_CHARS);
    if (kind === null || message === null) continue;
    out.push({ kind, message, answer: chars(d.answer, MAX_LINE_CHARS) });
  }
  return out;
}
/** The console's lines, each bounded; at most `MAX_CONSOLE_LINES`. */
function consoleOf(v) {
  if (!Array.isArray(v)) return [];
  const out = [];
  for (const l of v) {
    if (out.length >= MAX_CONSOLE_LINES) break;
    if (!l || typeof l !== "object") continue;
    const level = chars(l.level, 16);
    const text = chars(l.text, MAX_LINE_CHARS);
    const at = wholeOf(l.at);
    if (level === null || text === null) continue;
    out.push({ level, text, at: at ?? 0 });
  }
  return out;
}
const finite = (v) => (typeof v === "number" && Number.isFinite(v) ? v : null);

/**
 * A pick's ancestors, each a tag word and a locator, bounded like the pick's
 * own: a malformed entry is dropped, the list cut at the path's depth.
 * @param {unknown} v
 */
function ancestorsOf(v) {
  if (!Array.isArray(v)) return [];
  /** @type {{tag: string, selector: string}[]} */
  const out = [];
  for (const item of v) {
    if (!item || typeof item !== "object") continue;
    const tag = chars(/** @type {Record<string, unknown>} */ (item).tag, MAX_TAG_CHARS);
    const selector = chars(/** @type {Record<string, unknown>} */ (item).selector, MAX_SELECTOR_CHARS);
    if (tag === null || selector === null || !selector) continue;
    out.push({ tag, selector });
    if (out.length >= MAX_SELECTOR_DEPTH) break;
  }
  return out;
}

function rectOf(v) {
  if (!v || typeof v !== "object") return null;
  const top = finite(v.top);
  const left = finite(v.left);
  const width = finite(v.width);
  const height = finite(v.height);
  if (top === null || left === null || width === null || height === null) return null;
  return { top, left, width: Math.max(0, width), height: Math.max(0, height) };
}

/**
 * A message from the frame, typed and bounded — or `null` for anything
 * else: a message of the app's own vocabulary sent back, a missing field, a
 * field of the wrong shape. The frame is untrusted; nothing it says is used
 * before it passed here.
 * @param {unknown} data
 */
export function parseInspectorMessage(data) {
  if (!data || typeof data !== "object") return null;
  const d = /** @type {Record<string, unknown>} */ (data);
  switch (d.type) {
    case INSPECTOR_MESSAGES.HOVER: {
      const tag = chars(d.tag, MAX_TAG_CHARS);
      const rect = rectOf(d.rect);
      return tag !== null && rect ? { type: INSPECTOR_MESSAGES.HOVER, tag, rect } : null;
    }
    case INSPECTOR_MESSAGES.PICK: {
      const selector = chars(d.selector, MAX_SELECTOR_CHARS);
      const tag = chars(d.tag, MAX_TAG_CHARS);
      const text = chars(d.text, MAX_TEXT_CHARS);
      const rect = rectOf(d.rect);
      if (selector === null || tag === null || text === null || typeof d.excerpt !== "string" || !rect) return null;
      return { type: INSPECTOR_MESSAGES.PICK, selector, tag, excerpt: cutBytes(d.excerpt, MAX_EXCERPT_BYTES), text, rect, ancestors: ancestorsOf(d.ancestors) };
    }
    case INSPECTOR_MESSAGES.NOTE: {
      const selector = chars(d.selector, MAX_SELECTOR_CHARS);
      const tag = chars(d.tag, MAX_TAG_CHARS);
      const text = chars(d.text, MAX_TEXT_CHARS);
      const note = chars(d.note, MAX_NOTE_CHARS)?.trim() || null;
      if (selector === null || tag === null || text === null || typeof d.excerpt !== "string" || note === null) return null;
      return { type: INSPECTOR_MESSAGES.NOTE, selector, tag, excerpt: cutBytes(d.excerpt, MAX_EXCERPT_BYTES), text, note };
    }
    case INSPECTOR_MESSAGES.CLOSED:
      return { type: INSPECTOR_MESSAGES.CLOSED };
    case INSPECTOR_MESSAGES.MARKED: {
      if (!Array.isArray(d.lost)) return null;
      return { type: INSPECTOR_MESSAGES.MARKED, lost: d.lost.filter((n) => Number.isInteger(n)) };
    }
    case INSPECTOR_MESSAGES.ESCAPE:
      return { type: INSPECTOR_MESSAGES.ESCAPE };
    case INSPECTOR_MESSAGES.ANSWER: {
      const id = chars(d.id, MAX_ID_CHARS);
      const url = chars(d.url, MAX_URL_CHARS);
      const title = chars(d.title, MAX_TITLE_CHARS);
      if (id === null || !id || url === null || title === null) return null;
      const ok = d.ok !== false;
      const text = typeof d.text === "string" ? d.text.slice(0, MAX_READ_CHARS) : null;
      const error = ok ? null : (chars(d.error, MAX_ERROR_CHARS) ?? tr("ui-page-inspector-page-refused"));
      const selector = chars(d.selector, MAX_SELECTOR_CHARS);
      const count = wholeOf(d.count);
      const waitedMs = wholeOf(d.waitedMs);
      const scroll = scrollOf(d.scroll);
      const value = valueOf(d.value);
      const dialogs = dialogsOf(d.dialogs);
      const console = consoleOf(d.console);
      return { type: INSPECTOR_MESSAGES.ANSWER, id, ok, url, title, text, error, selector, count, waitedMs, scroll, value, dialogs, console };
    }
    case INSPECTOR_MESSAGES.TITLE: {
      const url = chars(d.url, MAX_URL_CHARS);
      const title = chars(d.title, MAX_TITLE_CHARS);
      return url !== null && title !== null ? { type: INSPECTOR_MESSAGES.TITLE, url, title } : null;
    }
    case INSPECTOR_MESSAGES.KEY: {
      const command = typeof d.command === "string" && BROWSER_COMMANDS.includes(d.command) ? d.command : null;
      return command ? { type: INSPECTOR_MESSAGES.KEY, command } : null;
    }
    case INSPECTOR_MESSAGES.PLACE: {
      const top = finite(d.top);
      const left = finite(d.left);
      if (top === null || left === null) return null;
      return { type: INSPECTOR_MESSAGES.PLACE, top: Math.max(0, Math.round(top)), left: Math.max(0, Math.round(left)) };
    }
    case INSPECTOR_MESSAGES.FOUND: {
      const index = finite(d.index);
      const count = finite(d.count);
      if (index === null || count === null || !Number.isInteger(index) || !Number.isInteger(count)) return null;
      return { type: INSPECTOR_MESSAGES.FOUND, index: Math.max(-1, index), count: Math.max(0, count) };
    }
    default:
      return null;
  }
}

/**
 * The program a page runs, whatever carries its words: the inspector — the
 * outline, the picks, the badges, the find — and the driver the browser's
 * tools speak to (ide/18): a read, a find in the text, a snapshot of the
 * page's outline with a ref per element, a click, a fill, typing a
 * keystroke at a time, a key pressed, an option chosen, a hover, a scroll, a
 * wait, the console, a script evaluated — each answered with the page's URL
 * and title, and the dialogs the page raised meanwhile. An element is named
 * by a ref while the page that gave it stands, else by a CSS selector. `post` is the transport's, defined
 * before this; `handle` takes the app's words. `String.raw`, so a `\n` in
 * it is the two characters a script's string literal needs, and no backtick
 * may appear.
 */
const INSPECTOR_CORE = String.raw`  var INSPECT = "${INSPECTOR_MESSAGES.INSPECT}", MARKS = "${INSPECTOR_MESSAGES.MARKS}", THEME = "${INSPECTOR_MESSAGES.THEME}";
  var HOVER = "${INSPECTOR_MESSAGES.HOVER}", PICK = "${INSPECTOR_MESSAGES.PICK}", NOTE = "${INSPECTOR_MESSAGES.NOTE}", CLOSED = "${INSPECTOR_MESSAGES.CLOSED}";
  var MARKED = "${INSPECTOR_MESSAGES.MARKED}", ESCAPE = "${INSPECTOR_MESSAGES.ESCAPE}";
  var FIND = "${INSPECTOR_MESSAGES.FIND}", FOUND = "${INSPECTOR_MESSAGES.FOUND}";
  var PLACE = "${INSPECTOR_MESSAGES.PLACE}", PLACE_TO = "${INSPECTOR_MESSAGES.PLACE_TO}";
  var FIND_ALL = "bisa-find", FIND_CURRENT = "bisa-find-current";
  var MAX_EXCERPT = ${MAX_EXCERPT_BYTES}, MAX_DEPTH = ${MAX_SELECTOR_DEPTH}, MAX_TEXT = ${MAX_TEXT_CHARS}, MAX_NOTE = ${MAX_NOTE_CHARS};
  var ATTR = "data-bisa-inspector", ACT = "data-bisa-act", SEL = "data-bisa-selector";
  var BOX_WIDTH = ${BOX_WIDTH}, GAP = 6;
  var BLOCKED = ["click", "mousedown", "mouseup", "pointerdown", "pointerup"];
  var mode = "off", tracked = null, marks = [], held = false;
  var outline = null, label = null, badges = null;
  var box = null, crumbsEl = null, textEl = null, input = null, addBtn = null;
  function own(el) { return !!(el && el.closest && el.closest("[" + ATTR + "]")); }
  function part(tag, what) {
    var el = document.createElement(tag);
    el.setAttribute(ATTR, what);
    el.style.cssText = STYLES[what] || "";
    return el;
  }
  // A part's dress is a state: hover and focus swap the whole cssText, since
  // nothing of ours is a stylesheet a selector could live in.
  function hot(el, what, enter, leave) {
    if (typeof el.addEventListener !== "function") return;
    var state = what + ":" + (enter === "focus" ? "focus" : "hover");
    el.addEventListener(enter, function () { el.style.cssText = STYLES[state] || STYLES[what] || ""; });
    el.addEventListener(leave, function () { el.style.cssText = STYLES[what] || ""; });
  }
  function repaint(el) {
    var what = el && typeof el.getAttribute === "function" ? el.getAttribute(ATTR) : null;
    if (what && STYLES[what]) {
      var s = el.style, keep = { display: s.display, top: s.top, left: s.left, width: s.width, height: s.height };
      s.cssText = STYLES[what];
      for (var k in keep) if (keep[k]) s[k] = keep[k];
    }
    var kids = el && el.children ? el.children : [];
    for (var i = 0; i < kids.length; i++) repaint(kids[i]);
  }
  // The app's theme moved: every part drawn wears the new dress where it
  // stands — a box open stays open, its words stay typed.
  function restyle(styles) {
    if (!styles || typeof styles !== "object") return;
    STYLES = styles;
    if (!outline) return;
    repaint(outline);
    repaint(label);
    repaint(badges);
    repaint(box);
  }
  function ensure() {
    if (outline) return;
    var root = document.documentElement;
    outline = part("div", "outline");
    label = part("div", "label");
    badges = part("div", "badges");
    root.appendChild(outline);
    root.appendChild(label);
    root.appendChild(badges);
    box = part("div", "box");
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-label", "Annotate the element");
    var head = part("div", "box-head");
    crumbsEl = part("nav", "crumbs");
    crumbsEl.setAttribute("aria-label", "Where the element is in the page");
    var close = part("button", "close");
    close.setAttribute(ACT, "close");
    close.setAttribute("type", "button");
    close.setAttribute("aria-label", "Never mind");
    close.setAttribute("title", "Never mind");
    close.textContent = "\u00d7";
    hot(close, "close", "mouseenter", "mouseleave");
    head.appendChild(crumbsEl);
    head.appendChild(close);
    textEl = part("p", "box-text");
    var row = part("div", "box-row");
    input = part("input", "input");
    input.setAttribute(ACT, "input");
    input.setAttribute("type", "text");
    input.setAttribute("placeholder", "What should change here?");
    input.setAttribute("aria-label", "What should change here?");
    hot(input, "input", "focus", "blur");
    addBtn = part("button", "add");
    addBtn.setAttribute(ACT, "add");
    addBtn.setAttribute("type", "button");
    addBtn.textContent = "Add";
    hot(addBtn, "add", "mouseenter", "mouseleave");
    row.appendChild(input);
    row.appendChild(addBtn);
    box.appendChild(head);
    box.appendChild(textEl);
    box.appendChild(row);
    root.appendChild(box);
  }
  function crumbWord(tag) {
    var t = String(tag || "");
    return /^(html|body)(?=[.#]|$)/.test(t) ? t.slice(0, 4) : t;
  }
  function noteFor(selector) {
    for (var i = 0; i < marks.length; i++) {
      if (marks[i] && marks[i].selector === selector && marks[i].note) return String(marks[i].note);
    }
    return "";
  }
  function viewport() {
    var d = document.documentElement;
    return { width: Number(window.innerWidth) || (d && d.clientWidth) || 0, height: Number(window.innerHeight) || (d && d.clientHeight) || 0 };
  }
  function placeBox(el) {
    if (!box) return;
    var r = el.getBoundingClientRect();
    var b = box.getBoundingClientRect ? box.getBoundingClientRect() : null;
    var bw = b && b.width ? b.width : BOX_WIDTH, bh = b && b.height ? b.height : 96;
    var v = viewport();
    var above = r.top - bh - GAP >= 0;
    var top = above ? r.top - bh - GAP : r.top + r.height + GAP;
    var left = r.left;
    top = Math.max(0, Math.min(top, Math.max(0, v.height - bh)));
    left = Math.max(0, Math.min(left, Math.max(0, v.width - bw)));
    box.style.top = top + "px";
    box.style.left = left + "px";
  }
  function openBox(el) {
    ensure();
    var selector = cssPath(el);
    while (crumbsEl.firstChild) crumbsEl.removeChild(crumbsEl.firstChild);
    var chain = ancestorsOf(el), i;
    for (i = 0; i < chain.length; i++) {
      if (i > 0) { var sep = part("span", "crumb-sep"); sep.textContent = "\u203a"; sep.setAttribute("aria-hidden", "true"); crumbsEl.appendChild(sep); }
      var crumb = part("button", "crumb");
      hot(crumb, "crumb", "mouseenter", "mouseleave");
      crumb.setAttribute(ACT, "crumb");
      crumb.setAttribute(SEL, chain[i].selector);
      crumb.setAttribute("type", "button");
      crumb.setAttribute("title", "Annotate <" + chain[i].tag + "> instead");
      crumb.textContent = crumbWord(chain[i].tag);
      crumbsEl.appendChild(crumb);
    }
    if (chain.length > 0) { var sep2 = part("span", "crumb-sep"); sep2.textContent = "\u203a"; sep2.setAttribute("aria-hidden", "true"); crumbsEl.appendChild(sep2); }
    var current = part("span", "crumb-current");
    current.setAttribute("aria-current", "true");
    current.textContent = crumbWord(tagWords(el));
    crumbsEl.appendChild(current);
    textEl.textContent = textOf(el);
    textEl.style.display = textEl.textContent ? "block" : "none";
    var existing = noteFor(selector);
    if (!String(input.value || "").trim()) input.value = existing;
    addBtn.textContent = existing ? "Change" : "Add";
    box.setAttribute("aria-label", "Annotate <" + tagWords(el) + ">");
    held = true;
    box.style.display = "block";
    placeBox(el);
    if (typeof input.focus === "function") input.focus();
  }
  function closeBox(say) {
    if (box) { box.style.display = "none"; input.value = ""; }
    held = false;
    tracked = null;
    hideOutline();
    if (say) post({ type: CLOSED });
  }
  function submit() {
    var words = String((input && input.value) || "").trim();
    if (!words || !tracked) return;
    var el = tracked;
    post({ type: NOTE, selector: cssPath(el), tag: tagWords(el), excerpt: cut(String(el.outerHTML || ""), MAX_EXCERPT), text: textOf(el), note: words.slice(0, MAX_NOTE) });
    closeBox(false);
  }
  function act(target) {
    var b = target && target.closest ? target.closest("[" + ACT + "]") : null;
    if (!b || typeof b.getAttribute !== "function") return;
    var what = b.getAttribute(ACT);
    if (what === "close") closeBox(true);
    else if (what === "add") submit();
    else if (what === "crumb") select(b.getAttribute(SEL));
  }
  function rectOf(el) {
    var r = el.getBoundingClientRect();
    return { top: r.top, left: r.left, width: r.width, height: r.height };
  }
  function escapeCss(s) {
    if (window.CSS && typeof window.CSS.escape === "function") return window.CSS.escape(s);
    return String(s).replace(/[^a-zA-Z0-9_-]/g, function (c) { return "\\" + c; });
  }
  function unique(sel) {
    try { return document.querySelectorAll(sel).length === 1; } catch (e) { return false; }
  }
  function cssPath(el) {
    var parts = [], node = el, depth = 0;
    while (node && node.nodeType === 1 && depth < MAX_DEPTH) {
      var tag = String(node.tagName).toLowerCase();
      if (tag === "html") { parts.unshift("html"); break; }
      if (node.id) {
        var byId = "#" + escapeCss(node.id);
        if (unique(byId)) { parts.unshift(byId); return parts.join(" > "); }
      }
      if (tag === "body") { parts.unshift("body"); break; }
      var parent = node.parentElement, seg = tag;
      if (parent) {
        var same = 0, index = 0, kids = parent.children;
        for (var i = 0; i < kids.length; i++) {
          if (kids[i].tagName === node.tagName) { same++; if (kids[i] === node) index = same; }
        }
        if (same > 1) seg += ":nth-of-type(" + index + ")";
      }
      parts.unshift(seg);
      node = parent;
      depth++;
    }
    return parts.join(" > ");
  }
  function ancestorsOf(el) {
    var out = [], node = el.parentElement, depth = 0;
    while (node && node.nodeType === 1 && depth < MAX_DEPTH) {
      out.unshift({ tag: tagWords(node), selector: cssPath(node) });
      if (String(node.tagName).toLowerCase() === "html") break;
      node = node.parentElement;
      depth++;
    }
    return out;
  }
  function tagWords(el) {
    var t = String(el.tagName).toLowerCase();
    if (el.id) return t + "#" + el.id;
    var cls = (typeof el.className === "string" ? el.className : "").split(/\s+/).filter(Boolean).slice(0, 2);
    return cls.length ? t + "." + cls.join(".") : t;
  }
  function cut(s, max) { return s.length > max ? s.slice(0, max) + "\n… (cut)" : s; }
  function textOf(el) { return String(el.textContent || "").replace(/\s+/g, " ").trim().slice(0, MAX_TEXT); }
  function targetOf(e) {
    var el = e.target;
    if (el && el.nodeType !== 1) el = el.parentElement;
    if (!el || own(el)) return null;
    return el;
  }
  function pickOf(el) {
    return { type: PICK, selector: cssPath(el), tag: tagWords(el), excerpt: cut(String(el.outerHTML || ""), MAX_EXCERPT), text: textOf(el), rect: rectOf(el), ancestors: ancestorsOf(el) };
  }
  function select(selector) {
    var el = null;
    try { el = document.querySelector(String(selector)); } catch (e) { el = null; }
    if (!el || own(el)) return;
    tracked = el;
    showOutline(el);
    openBox(el);
    post(pickOf(el));
  }
  function showOutline(el) {
    ensure();
    var r = el.getBoundingClientRect();
    outline.style.display = "block";
    outline.style.top = r.top + "px";
    outline.style.left = r.left + "px";
    outline.style.width = r.width + "px";
    outline.style.height = r.height + "px";
    label.style.display = "block";
    label.textContent = tagWords(el);
    label.style.top = Math.max(0, r.top - 18) + "px";
    label.style.left = Math.max(0, r.left) + "px";
  }
  function hideOutline() {
    if (!outline) return;
    outline.style.display = "none";
    label.style.display = "none";
  }
  function layBadges() {
    ensure();
    while (badges.firstChild) badges.removeChild(badges.firstChild);
    var lost = [];
    for (var i = 0; i < marks.length; i++) {
      var m = marks[i], el = null;
      try { el = document.querySelector(m.selector); } catch (e) { el = null; }
      if (!el) { lost.push(m.n); continue; }
      var r = el.getBoundingClientRect();
      var b = part("div", "badge");
      b.textContent = String(m.n);
      b.style.top = Math.max(0, r.top - 9) + "px";
      b.style.left = Math.max(0, r.left - 9) + "px";
      badges.appendChild(b);
    }
    return lost;
  }
  function onMove(e) {
    if (held) return;
    var el = targetOf(e);
    if (!el) return;
    showOutline(el);
    post({ type: HOVER, tag: tagWords(el), rect: rectOf(el) });
  }
  function onPress(e) {
    if (own(e.target)) {
      e.stopImmediatePropagation();
      if (e.type === "click") act(e.target);
      return;
    }
    e.preventDefault();
    e.stopImmediatePropagation();
    if (e.type !== "click" || mode !== "picking" || held) return;
    var el = targetOf(e);
    if (!el) return;
    tracked = el;
    showOutline(el);
    openBox(el);
    post(pickOf(el));
  }
  function onKey(e) {
    if (held && own(e.target)) {
      if (e.key === "Enter") { e.preventDefault(); e.stopImmediatePropagation(); submit(); }
      else if (e.key === "Escape") { e.preventDefault(); e.stopImmediatePropagation(); closeBox(true); }
      else e.stopImmediatePropagation();
      return;
    }
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (held) { closeBox(true); return; }
    post({ type: ESCAPE });
  }
  function textNodes() {
    var out = [];
    if (!document.body || typeof document.createTreeWalker !== "function") return out;
    var walker = document.createTreeWalker(document.body, 4, {
      acceptNode: function (n) {
        var el = n.parentElement;
        if (!el || own(el)) return 2;
        var tag = String(el.tagName || "").toUpperCase();
        if (tag === "SCRIPT" || tag === "STYLE" || tag === "NOSCRIPT") return 2;
        return String(n.data || "").length > 0 ? 1 : 2;
      }
    });
    var n = walker.nextNode();
    while (n) { out.push(n); n = walker.nextNode(); }
    return out;
  }
  function clearFind() {
    if (window.CSS && window.CSS.highlights) {
      window.CSS.highlights["delete"](FIND_ALL);
      window.CSS.highlights["delete"](FIND_CURRENT);
    }
  }
  function runFind(d) {
    clearFind();
    var query = String(d.query || "");
    if (!query) { post({ type: FOUND, index: -1, count: 0 }); return; }
    var re = null;
    try {
      re = new RegExp(d.regex ? query : query.replace(/[.*+?^$\{}()|[\]\\]/g, "\\$&"), d.caseSensitive ? "g" : "gi");
    } catch (e) { re = null; }
    if (!re) { post({ type: FOUND, index: -1, count: 0 }); return; }
    var nodes = textNodes(), texts = [], starts = [], at = 0, i;
    for (i = 0; i < nodes.length; i++) { texts.push(String(nodes[i].data || "")); starts.push(at); at += texts[i].length; }
    var whole = texts.join(""), matches = [], m;
    re.lastIndex = 0;
    while ((m = re.exec(whole)) !== null) {
      matches.push({ start: m.index, end: m.index + m[0].length });
      if (m[0].length === 0) re.lastIndex += 1;
    }
    var ranges = [];
    for (i = 0; i < matches.length; i++) {
      var spans = [], j;
      for (j = 0; j < nodes.length; j++) {
        var from = starts[j], to = from + texts[j].length;
        if (to <= matches[i].start) continue;
        if (from >= matches[i].end) break;
        var r = document.createRange();
        r.setStart(nodes[j], Math.max(matches[i].start, from) - from);
        r.setEnd(nodes[j], Math.min(matches[i].end, to) - from);
        spans.push(r);
      }
      ranges.push(spans);
    }
    var count = ranges.length;
    var index = count ? (((Number(d.index) || 0) % count) + count) % count : -1;
    if (count && window.CSS && window.CSS.highlights && typeof window.Highlight === "function") {
      var all = [];
      for (i = 0; i < ranges.length; i++) all = all.concat(ranges[i]);
      window.CSS.highlights.set(FIND_ALL, new (Function.prototype.bind.apply(window.Highlight, [null].concat(all)))());
      window.CSS.highlights.set(FIND_CURRENT, new (Function.prototype.bind.apply(window.Highlight, [null].concat(ranges[index])))());
    }
    if (index >= 0 && ranges[index][0] && ranges[index][0].startContainer && ranges[index][0].startContainer.parentElement) {
      var el = ranges[index][0].startContainer.parentElement;
      if (typeof el.scrollIntoView === "function") el.scrollIntoView({ block: "center" });
    }
    post({ type: FOUND, index: index, count: count });
  }
  function onMoved() {
    layBadges();
    if (mode === "off" || !tracked) return;
    showOutline(tracked);
    if (held) placeBox(tracked);
  }
  function setMode(next) {
    if (next !== "picking") next = "off";
    if (next === mode) return;
    var i;
    mode = next;
    if (next === "off") {
      window.removeEventListener("mousemove", onMove, true);
      for (i = 0; i < BLOCKED.length; i++) window.removeEventListener(BLOCKED[i], onPress, true);
      window.removeEventListener("keydown", onKey, true);
      closeBox(false);
      return;
    }
    for (i = 0; i < BLOCKED.length; i++) window.addEventListener(BLOCKED[i], onPress, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousemove", onMove, true);
  }
  window.addEventListener("scroll", onMoved, true);
  window.addEventListener("resize", onMoved);
  var READ = "${INSPECTOR_MESSAGES.READ}", FIND_TEXT = "${INSPECTOR_MESSAGES.FIND_TEXT}", SNAPSHOT = "${INSPECTOR_MESSAGES.SNAPSHOT}";
  var CLICK = "${INSPECTOR_MESSAGES.CLICK}", FILL = "${INSPECTOR_MESSAGES.FILL}", TYPE = "${INSPECTOR_MESSAGES.TYPE}", PRESS = "${INSPECTOR_MESSAGES.PRESS}";
  var SELECT = "${INSPECTOR_MESSAGES.SELECT}", POINT = "${INSPECTOR_MESSAGES.POINT}", SCROLL = "${INSPECTOR_MESSAGES.SCROLL}", WAIT = "${INSPECTOR_MESSAGES.WAIT}";
  var CONSOLE = "${INSPECTOR_MESSAGES.CONSOLE}", EVAL = "${INSPECTOR_MESSAGES.EVAL}";
  var ANSWER = "${INSPECTOR_MESSAGES.ANSWER}", TITLE = "${INSPECTOR_MESSAGES.TITLE}";
  var MAX_READ = ${MAX_READ_CHARS}, MAX_MATCHES = ${MAX_MATCHES}, MAX_AROUND = ${MAX_AROUND}, MAX_OUTLINE = ${MAX_OUTLINE_LINES};
  var MAX_DIALOGS = ${MAX_DIALOGS}, MAX_CONSOLE = ${MAX_CONSOLE_LINES}, MAX_LINE = ${MAX_LINE_CHARS}, MAX_WAIT = ${MAX_WAIT_MS}, IDLE = ${IDLE_MS}, POLL = ${WAIT_POLL_MS};
  var refs = [], dialogs = [], logs = [], loadedAt = clock();
  function clock() { return typeof Date.now === "function" ? Date.now() : 0; }
  var later = typeof window.setTimeout === "function" ? function (f, ms) { window.setTimeout(f, ms); } : function (f) { f(); };
  function here() { return { url: String((window.location && window.location.href) || ""), title: String(document.title || "") }; }
  /** The page loaded again: the refs are the old page's, the console starts over. */
  function pageLoaded() { refs = []; logs = []; loadedAt = clock(); }
  function answer(id, extra) {
    var at = here(), out = { type: ANSWER, id: String(id == null ? "" : id), ok: true, url: at.url, title: at.title };
    for (var k in extra) if (Object.prototype.hasOwnProperty.call(extra, k)) out[k] = extra[k];
    if (dialogs.length) { out.dialogs = dialogs.slice(0, MAX_DIALOGS); dialogs = []; }
    post(out);
  }
  function refuse(id, why, extra) {
    var out = { ok: false, error: String(why) };
    for (var k in extra) if (Object.prototype.hasOwnProperty.call(extra || {}, k)) out[k] = extra[k];
    answer(id, out);
  }
  function elementOf(selector) {
    try { return document.querySelector(String(selector)); } catch (e) { return null; }
  }
  function isRef(t) { return /^e\d+$/.test(String(t)); }
  function attached(el) {
    if (!el) return false;
    if (typeof document.contains === "function") return document.contains(el);
    for (var n = el; n; n = n.parentElement) if (n === document.documentElement || n === document.body) return true;
    return false;
  }
  /** The element a target names: a snapshot's ref while it is still in the page, else the selector's first match. */
  function resolve(target) {
    var t = String(target == null ? "" : target).trim();
    if (!t) return null;
    if (isRef(t)) { var el = refs[Number(t.slice(1))]; return el && attached(el) ? el : null; }
    return elementOf(t);
  }
  function missing(target) {
    var t = String(target == null ? "" : target).trim();
    if (!t) return "name a target — a ref from browser_snapshot, or a CSS selector";
    return isRef(t) ? "ref " + t + " is gone — the page changed; browser_snapshot again" : "nothing matches " + t;
  }
  function trimWords(s, max) { return String(s == null ? "" : s).replace(/\s+/g, " ").trim().slice(0, max); }
  function attr(el, name) { return el && typeof el.getAttribute === "function" ? el.getAttribute(name) : null; }
  function tagOf(el) { return String((el && el.tagName) || "").toUpperCase(); }
  function inputType(el) { return String(attr(el, "type") || el.type || "text").toLowerCase(); }
  /** What a person calls the element: its ARIA role, else the role its tag plays. */
  function roleOf(el) {
    var given = attr(el, "role");
    if (given) return String(given);
    var tag = tagOf(el);
    if (tag === "A") return attr(el, "href") ? "link" : "text";
    if (tag === "BUTTON" || tag === "SUMMARY") return "button";
    if (tag === "INPUT") {
      var t = inputType(el);
      if (t === "checkbox" || t === "radio") return t;
      if (t === "button" || t === "submit" || t === "reset" || t === "image") return "button";
      if (t === "hidden") return "hidden";
      if (t === "search") return "searchbox";
      return "textbox";
    }
    if (tag === "TEXTAREA") return "textbox";
    if (tag === "SELECT") return "combobox";
    if (tag === "OPTION") return "option";
    if (/^H[1-6]$/.test(tag)) return "heading";
    if (tag === "NAV") return "navigation";
    if (tag === "MAIN") return "main";
    if (tag === "HEADER") return "banner";
    if (tag === "FOOTER") return "contentinfo";
    if (tag === "FORM") return "form";
    if (tag === "IMG") return "img";
    if (tag === "LI") return "listitem";
    if (tag === "TABLE") return "table";
    if (tag === "DIALOG") return "dialog";
    if (el.isContentEditable === true) return "textbox";
    if (attr(el, "onclick") !== null || (attr(el, "tabindex") !== null && attr(el, "tabindex") !== "-1")) return "button";
    return "text";
  }
  var CONTROLS = { link: 1, button: 1, textbox: 1, searchbox: 1, checkbox: 1, radio: 1, combobox: 1, listbox: 1, option: 1, menuitem: 1, menuitemcheckbox: 1, menuitemradio: 1, tab: 1, "switch": 1, slider: 1, spinbutton: 1, treeitem: 1 };
  var LANDMARKS = { heading: 1, navigation: 1, main: 1, banner: 1, contentinfo: 1, form: 1, dialog: 1, img: 1, table: 1 };
  /** What the element is called: its label, its alt, its title, its placeholder, its text. */
  function nameOf(el) {
    var label = attr(el, "aria-label");
    if (label) return trimWords(label, MAX_TEXT);
    var by = attr(el, "aria-labelledby");
    if (by) {
      var parts = [], ids = String(by).split(/\s+/);
      for (var i = 0; i < ids.length; i++) { var l = ids[i] ? document.getElementById && document.getElementById(ids[i]) : null; if (l) parts.push(trimWords(l.textContent, MAX_TEXT)); }
      if (parts.length) return parts.join(" ").slice(0, MAX_TEXT);
    }
    var tag = tagOf(el);
    if (tag === "IMG") return trimWords(attr(el, "alt") || attr(el, "title") || "", MAX_TEXT);
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
      if (el.id && typeof document.querySelectorAll === "function") {
        var labels = [];
        try { labels = document.querySelectorAll("label[for=" + escapeCss(el.id) + "]"); } catch (e) { labels = []; }
        if (labels && labels.length) return trimWords(labels[0].textContent, MAX_TEXT);
      }
      var wrap = el.closest ? el.closest("label") : null;
      if (wrap) return trimWords(wrap.textContent, MAX_TEXT);
      var t = inputType(el);
      if (tag === "INPUT" && (t === "button" || t === "submit" || t === "reset")) return trimWords(el.value || attr(el, "value") || t, MAX_TEXT);
      return trimWords(attr(el, "placeholder") || attr(el, "title") || attr(el, "name") || "", MAX_TEXT);
    }
    return trimWords(el.textContent || attr(el, "title") || "", MAX_TEXT);
  }
  /** A landmark's name is its label alone — a form's text is everything in it, a nav's every link. */
  function landmarkName(el) { return attr(el, "aria-label") || attr(el, "aria-labelledby") || attr(el, "title") ? nameOf(el) : ""; }
  /** Whether an option belongs to a select (directly, or through an optgroup). */
  function insideSelect(el) {
    var p = el.parentElement, t1 = p ? tagOf(p) : "";
    if (t1 === "SELECT") return true;
    return t1 === "OPTGROUP" && p.parentElement ? tagOf(p.parentElement) === "SELECT" : false;
  }
  function visible(el) {
    if (typeof el.getBoundingClientRect !== "function") return true;
    var r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return false;
    if (attr(el, "aria-hidden") === "true" || attr(el, "hidden") !== null) return false;
    return true;
  }
  function quoted(s) { return JSON.stringify(String(s == null ? "" : s)); }
  /** What a person sees of the element's state, after its name. */
  function stateWords(el, role) {
    var out = [], tag = tagOf(el);
    if (el.disabled === true || attr(el, "aria-disabled") === "true") out.push("[disabled]");
    if (role === "checkbox" || role === "radio" || role === "switch" || role === "menuitemcheckbox") out.push(el.checked || attr(el, "aria-checked") === "true" ? "[checked]" : "[unchecked]");
    var expanded = attr(el, "aria-expanded");
    if (expanded !== null) out.push(expanded === "true" ? "[expanded]" : "[collapsed]");
    if (attr(el, "aria-selected") === "true" || (tag === "OPTION" && el.selected)) out.push("[selected]");
    if (attr(el, "aria-current")) out.push("[current]");
    if (attr(el, "required") !== null) out.push("[required]");
    if ((role === "textbox" || role === "searchbox" || role === "combobox" || role === "spinbutton" || role === "slider") && tag !== "SELECT") {
      var v = typeof el.value === "string" ? el.value : (el.isContentEditable ? el.textContent : "");
      if (v) out.push("value=" + quoted(trimWords(v, 80)));
      else if (attr(el, "placeholder")) out.push("placeholder=" + quoted(trimWords(attr(el, "placeholder"), 80)));
    }
    if (tag === "SELECT") {
      var opts = el.options || el.children || [], words = [];
      for (var i = 0; i < opts.length && i < 12; i++) { var o = opts[i]; if (o && tagOf(o) === "OPTION") words.push((o.selected ? "*" : "") + quoted(trimWords(o.textContent, 40))); }
      if (words.length) out.push("options=[" + words.join(", ") + (opts.length > 12 ? ", …" : "") + "]");
    }
    if (role === "link") {
      var href = attr(el, "href");
      if (href) out.push("→ " + trimWords(href, 120));
    }
    if (role === "heading") out.push("(" + tag.toLowerCase() + ")");
    if (role === "img") { var src = attr(el, "src"); if (src) out.push("src=" + quoted(trimWords(src, 80))); }
    return out.join(" ");
  }
  function elements(root) {
    var out = [];
    if (root && root.nodeType === 1 && root !== document.documentElement && root !== document.body) out.push(root);
    var kids = root ? root.children : null;
    if (kids) for (var i = 0; i < kids.length; i++) out = out.concat(elements(kids[i]));
    return out;
  }
  /** The page's outline: a line per heading, landmark and control, each with a ref the other tools take. */
  function outlineOf(root, all) {
    refs = [];
    var lines = [], count = 0, list = elements(root || document.body || document.documentElement);
    for (var i = 0; i < list.length; i++) {
      var el = list[i], tag = tagOf(el);
      if (own(el) || tag === "SCRIPT" || tag === "STYLE" || tag === "NOSCRIPT" || tag === "TEMPLATE" || tag === "HEAD" || tag === "META" || tag === "LINK" || tag === "TITLE") continue;
      var role = roleOf(el);
      if (role === "hidden") continue;
      var control = CONTROLS[role] === 1, landmark = LANDMARKS[role] === 1;
      // An option of a select is said on the combobox's line, not as a row of its own.
      if (role === "option" && insideSelect(el)) continue;
      var name = landmark && role !== "heading" && role !== "img" ? landmarkName(el) : nameOf(el);
      if (!control && !landmark) {
        if (!all) continue;
        var ownText = "";
        for (var c = el.firstChild; c; c = c.nextSibling) if (c.nodeType === 3) ownText += String(c.data || "");
        ownText = trimWords(ownText, MAX_TEXT);
        if (!ownText && !(el.children && el.children.length)) ownText = trimWords(el.textContent, MAX_TEXT);
        if (!ownText) continue;
        name = ownText;
      }
      if (!visible(el) && role !== "option") continue;
      count++;
      if (lines.length >= MAX_OUTLINE) continue;
      var n = refs.length;
      refs.push(el);
      var words = "e" + n + " " + role + (name ? " " + quoted(name) : "");
      var state = stateWords(el, role);
      if (state) words += " " + state;
      lines.push(words);
    }
    if (count > lines.length) lines.push("… " + (count - lines.length) + " more; browser_snapshot a target to see part of the page");
    var text = lines.length ? lines.join("\n") : "nothing to point at — the page has no headings, links, buttons or fields" + (all ? " and no text" : "; browser_read for its text, or all: true");
    return { text: text.length > MAX_READ ? text.slice(0, MAX_READ) + "\n… (cut)" : text, count: count };
  }
  /** What an element is, before its text: its role, its name, the attributes that matter. */
  function describe(el) {
    var role = roleOf(el), name = nameOf(el), out = role + (name ? " " + quoted(name) : "");
    var state = stateWords(el, role);
    if (state) out += " " + state;
    var names = ["id", "type", "name", "href", "src", "value", "placeholder", "title", "alt", "role", "class"], seen = [];
    for (var i = 0; i < names.length; i++) { var v = attr(el, names[i]); if (v !== null && v !== "") seen.push(names[i] + "=" + quoted(trimWords(v, 120))); }
    if (el.attributes) for (var j = 0; j < el.attributes.length; j++) { var a = el.attributes[j]; if (a && /^(aria-|data-)/.test(String(a.name)) && seen.length < 24) seen.push(String(a.name) + "=" + quoted(trimWords(a.value, 120))); }
    if (seen.length) out += "\n<" + tagOf(el).toLowerCase() + " " + seen.join(" ") + ">";
    return out;
  }
  function readText(el) {
    var root = el || document.body || document.documentElement;
    var t = root && typeof root.innerText === "string" ? root.innerText : String((root && root.textContent) || "");
    t = t.replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
    return t.length > MAX_READ ? t.slice(0, MAX_READ) + "\n… (cut)" : t;
  }
  function readHtml(el) {
    var root = el || document.documentElement;
    var h = String((root && root.outerHTML) || "");
    return h.length > MAX_READ ? h.slice(0, MAX_READ) + "\n… (cut)" : h;
  }
  function editableOf(el) {
    var tag = tagOf(el);
    return tag === "INPUT" || tag === "TEXTAREA" || el.isContentEditable === true;
  }
  function setValue(field, value) {
    var tag = tagOf(field);
    if (tag !== "INPUT" && tag !== "TEXTAREA") { field.textContent = value; return; }
    var proto = tag === "TEXTAREA" && window.HTMLTextAreaElement ? window.HTMLTextAreaElement.prototype : window.HTMLInputElement ? window.HTMLInputElement.prototype : null;
    var desc = proto && Object.getOwnPropertyDescriptor(proto, "value");
    if (desc && desc.set) desc.set.call(field, value); else field.value = value;
  }
  function valueOf(field) { var tag = tagOf(field); return tag === "INPUT" || tag === "TEXTAREA" ? String(field.value == null ? "" : field.value) : String(field.textContent || ""); }
  function fire(el, type, init, ctor) {
    var ev = null;
    try { if (ctor && typeof window[ctor] === "function") ev = new window[ctor](type, init); } catch (e) { ev = null; }
    if (!ev) {
      try { ev = new window.Event(type, { bubbles: true, cancelable: true }); } catch (e2) { return true; }
      for (var k in init) if (Object.prototype.hasOwnProperty.call(init, k) && k !== "bubbles" && k !== "cancelable") { try { ev[k] = init[k]; } catch (e3) {} }
    }
    if (typeof el.dispatchEvent !== "function") return true;
    return el.dispatchEvent(ev) !== false;
  }
  function changed(field) { fire(field, "input", { bubbles: true, cancelable: false }, "Event"); fire(field, "change", { bubbles: true, cancelable: false }, "Event"); }
  var CODES = { Enter: "Enter", Escape: "Escape", Tab: "Tab", Backspace: "Backspace", Delete: "Delete", " ": "Space", Space: "Space", ArrowUp: "ArrowUp", ArrowDown: "ArrowDown", ArrowLeft: "ArrowLeft", ArrowRight: "ArrowRight", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown" };
  function keyInit(key, mods) {
    var m = mods || [];
    var has = function (w) { for (var i = 0; i < m.length; i++) if (String(m[i]).toLowerCase() === w) return true; return false; };
    var k = key === "Space" ? " " : key;
    var code = CODES[key] || (k.length === 1 ? (/^[a-zA-Z]$/.test(k) ? "Key" + k.toUpperCase() : /^[0-9]$/.test(k) ? "Digit" + k : "") : key);
    return { key: k, code: code, bubbles: true, cancelable: true, shiftKey: has("shift"), altKey: has("alt"), ctrlKey: has("ctrl") || has("control"), metaKey: has("meta") || has("cmd") || has("command") };
  }
  /** A form's submit the way Enter in a field does it, when the page did not prevent the key. */
  function submitFrom(el) {
    var form = el && el.form ? el.form : (el && el.closest ? el.closest("form") : null);
    if (!form) return false;
    if (typeof form.requestSubmit === "function") { form.requestSubmit(); return true; }
    if (fire(form, "submit", { bubbles: true, cancelable: true }, "Event") && typeof form.submit === "function") form.submit();
    return true;
  }
  /** One key, the way a person presses it: down, press, the default, up. */
  function press(el, key, mods) {
    var target = el || document.activeElement || document.body || document.documentElement;
    var init = keyInit(key, mods), k = init.key;
    var down = fire(target, "keydown", init, "KeyboardEvent");
    var printable = k.length === 1 && !init.ctrlKey && !init.metaKey && !init.altKey;
    if (down && (printable || k === "Enter")) fire(target, "keypress", init, "KeyboardEvent");
    if (down) {
      if (k === "Enter") {
        if (tagOf(target) === "INPUT" && target.form) submitFrom(target);
        else if (tagOf(target) === "TEXTAREA" || target.isContentEditable === true) { setValue(target, valueOf(target) + "\n"); fire(target, "input", { bubbles: true }, "Event"); }
        else if (typeof target.click === "function") target.click();
      } else if (printable && editableOf(target)) {
        setValue(target, valueOf(target) + k);
        fire(target, "input", { bubbles: true }, "Event");
      } else if (k === "Backspace" && editableOf(target)) {
        setValue(target, valueOf(target).slice(0, -1));
        fire(target, "input", { bubbles: true }, "Event");
      } else if (k === " " && (roleOf(target) === "checkbox" || roleOf(target) === "button") && typeof target.click === "function") {
        target.click();
      }
    }
    fire(target, "keyup", init, "KeyboardEvent");
    return down;
  }
  function typeInto(field, text, clear, submit) {
    if (typeof field.focus === "function") field.focus();
    if (clear) { setValue(field, ""); fire(field, "input", { bubbles: true }, "Event"); }
    var chars = String(text || "");
    for (var i = 0; i < chars.length; i++) press(field, chars[i], []);
    fire(field, "change", { bubbles: true }, "Event");
    if (submit) press(field, "Enter", []);
  }
  function hover(el) {
    if (typeof el.scrollIntoView === "function") el.scrollIntoView({ block: "center" });
    var r = typeof el.getBoundingClientRect === "function" ? el.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
    var init = { bubbles: true, cancelable: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 };
    fire(el, "pointerover", init, "PointerEvent"); fire(el, "pointerenter", { bubbles: false, cancelable: false, clientX: init.clientX, clientY: init.clientY }, "PointerEvent");
    fire(el, "mouseover", init, "MouseEvent"); fire(el, "mouseenter", { bubbles: false, cancelable: false, clientX: init.clientX, clientY: init.clientY }, "MouseEvent");
    fire(el, "pointermove", init, "PointerEvent"); fire(el, "mousemove", init, "MouseEvent");
  }
  function scrollWords() {
    var doc = document.documentElement || {}, body = document.body || {};
    return {
      x: Math.round(Number(window.scrollX || window.pageXOffset || 0)),
      y: Math.round(Number(window.scrollY || window.pageYOffset || 0)),
      width: Math.round(Number(doc.scrollWidth || body.scrollWidth || window.innerWidth || 0)),
      height: Math.round(Number(doc.scrollHeight || body.scrollHeight || window.innerHeight || 0))
    };
  }
  function pageText() { var b = document.body || document.documentElement; return b ? String(typeof b.innerText === "string" ? b.innerText : b.textContent || "") : ""; }
  /** The wait's condition, read once. */
  function satisfied(d) {
    if (d.until === "selector") return !!resolve(d.target);
    if (d.until === "gone") return !resolve(d.target);
    if (d.until === "text") return pageText().toLowerCase().indexOf(String(d.query || "").toLowerCase()) !== -1;
    return true;
  }
  function whatFor(d) {
    if (d.until === "selector") return "an element matching " + String(d.target);
    if (d.until === "gone") return String(d.target) + " to leave the page";
    if (d.until === "text") return "the words " + quoted(d.query) + " in the page";
    if (d.until === "idle") return "the page to be quiet for " + IDLE + " ms";
    return "the page";
  }
  function waitFor(d) {
    var began = clock(), limit = Math.min(MAX_WAIT, Math.max(1, Number(d.timeoutMs) || 10000));
    var untilIdle = d.until === "idle";
    var lastChange = began, observer = null;
    if (untilIdle && typeof window.MutationObserver === "function" && document.documentElement) {
      try { observer = new window.MutationObserver(function () { lastChange = clock(); }); observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, characterData: true }); } catch (e) { observer = null; }
    }
    var done = false;
    function finish(ok) {
      if (done) return;
      done = true;
      if (observer) { try { observer.disconnect(); } catch (e) {} }
      var waited = clock() - began;
      if (ok) answer(d.id, { waitedMs: waited });
      else refuse(d.id, "waited " + waited + " ms for " + whatFor(d) + " and it did not come", { waitedMs: waited });
    }
    function check() {
      if (done) return;
      var now = clock();
      if (untilIdle ? now - lastChange >= IDLE && now - began >= IDLE : satisfied(d)) return finish(true);
      if (now - began >= limit || typeof window.setTimeout !== "function") return finish(false);
      later(check, POLL);
    }
    check();
  }
  var LEVELS = ["error", "warn", "info", "log", "debug"];
  function note(level, text) {
    if (logs.length >= MAX_CONSOLE) logs.shift();
    logs.push({ level: level, text: String(text).slice(0, MAX_LINE), at: Math.max(0, clock() - loadedAt) });
  }
  /** An error from any realm — an instanceof check misses one an iframe or another window made. */
  function isError(a) { return !!a && typeof a === "object" && (Object.prototype.toString.call(a) === "[object Error]" || (typeof a.message === "string" && typeof a.stack === "string")); }
  function words(args) {
    var out = [];
    for (var i = 0; i < args.length; i++) {
      var a = args[i];
      if (isError(a)) out.push(String(a.message || a));
      else if (typeof a === "object" && a !== null) { try { out.push(JSON.stringify(a).slice(0, MAX_LINE)); } catch (e) { out.push(String(a)); } }
      else out.push(String(a));
    }
    return out.join(" ");
  }
  /** The page's console and its errors, kept for browser_console — installed once, by the browser's program. */
  function installConsole() {
    var c = window.console;
    if (c && !c.__bisaWrapped) {
      c.__bisaWrapped = true;
      for (var i = 0; i < LEVELS.length; i++) (function (level) {
        var orig = typeof c[level] === "function" ? c[level] : null;
        c[level] = function () { note(level, words(arguments)); if (orig) { try { return orig.apply(c, arguments); } catch (e) {} } };
      })(LEVELS[i]);
    }
    window.addEventListener("error", function (e) {
      if (e && e.target && e.target !== window && e.target.tagName) note("resource", tagOf(e.target).toLowerCase() + " failed to load: " + String(e.target.src || e.target.href || ""));
      else note("uncaught", String((e && e.message) || (e && e.error && e.error.message) || "error") + (e && e.filename ? " (" + e.filename + ":" + e.lineno + ")" : ""));
    }, true);
    window.addEventListener("unhandledrejection", function (e) {
      var r = e && e.reason;
      note("rejection", r && r.message ? String(r.message) : String(r));
    });
  }
  /** The page's dialogs, answered for the agent and reported — installed once, by the browser's program. */
  function installDialogs() {
    function record(kind, message, reply) { if (dialogs.length >= MAX_DIALOGS) dialogs.shift(); dialogs.push({ kind: kind, message: trimWords(message, MAX_LINE), answer: reply }); }
    window.alert = function (m) { record("alert", m, null); };
    window.confirm = function (m) { record("confirm", m, "true"); return true; };
    window.prompt = function (m, def) { var a = def == null ? "" : String(def); record("prompt", m, a); return a; };
  }
  function safe(v, depth) {
    if (v === undefined) return null;
    if (v === null || typeof v === "boolean" || typeof v === "number" || typeof v === "string") return v;
    if (typeof v === "function") return "[function " + (v.name || "") + "]";
    if (typeof v === "symbol") return String(v);
    if (typeof v === "bigint") return String(v) + "n";
    if (v && v.nodeType === 1) return "<" + tagOf(v).toLowerCase() + (v.id ? " id=" + quoted(v.id) : "") + ">";
    if (v instanceof Error) return { error: String(v.message || v) };
    if (depth > 6) return "…";
    if (Array.isArray(v)) { var arr = []; for (var i = 0; i < v.length && i < 200; i++) arr.push(safe(v[i], depth + 1)); return arr; }
    if (typeof v === "object") { var o = {}, n = 0; for (var k in v) { if (!Object.prototype.hasOwnProperty.call(v, k)) continue; if (n++ >= 100) break; o[k] = safe(v[k], depth + 1); } return o; }
    return String(v);
  }
  function evaluate(d) {
    var fn;
    try { fn = new window.Function("return (" + String(d.expression || "") + ");"); } catch (e) { return refuse(d.id, "the expression does not parse: " + String(e && e.message || e)); }
    var v;
    try { v = fn.call(window); } catch (e2) { return refuse(d.id, "the expression threw: " + String(e2 && e2.message || e2)); }
    if (v && typeof v.then === "function") {
      var settled = false;
      v.then(function (r) { if (!settled) { settled = true; answer(d.id, { value: safe(r, 0) }); } }, function (e3) { if (!settled) { settled = true; refuse(d.id, "the promise rejected: " + String(e3 && e3.message || e3)); } });
      later(function () { if (!settled) { settled = true; refuse(d.id, "the promise did not settle in time"); } }, 8000);
      return;
    }
    answer(d.id, { value: safe(v, 0) });
  }
  function drive(d) {
    if (d.type === READ) {
      if (d.target) {
        var el = resolve(d.target);
        if (!el) return refuse(d.id, missing(d.target));
        var body = d.format === "html" ? readHtml(el) : readText(el);
        return answer(d.id, { text: describe(el) + "\n\n" + body, selector: cssPath(el) });
      }
      return answer(d.id, { text: d.format === "html" ? readHtml(null) : readText(null) });
    }
    if (d.type === FIND_TEXT) {
      var q = String(d.query || "").trim().toLowerCase();
      if (!q) return refuse(d.id, "nothing to find");
      var nodes = textNodes(), lines = [], count = 0;
      for (var i = 0; i < nodes.length && lines.length < MAX_MATCHES; i++) {
        var text = String(nodes[i].data || ""), low = text.toLowerCase(), at = low.indexOf(q);
        while (at !== -1 && lines.length < MAX_MATCHES) {
          count++;
          var from = Math.max(0, at - MAX_AROUND), to = Math.min(text.length, at + q.length + MAX_AROUND);
          var holder = nodes[i].parentElement;
          lines.push((holder ? cssPath(holder) : "?") + " — " + (from > 0 ? "…" : "") + text.slice(from, to).replace(/\s+/g, " ") + (to < text.length ? "…" : ""));
          at = low.indexOf(q, at + q.length);
        }
      }
      return answer(d.id, { text: count ? lines.join("\n") : "no match for " + JSON.stringify(String(d.query)), count: count });
    }
    if (d.type === SNAPSHOT) {
      var root = null;
      if (d.target) { root = resolve(d.target); if (!root) return refuse(d.id, missing(d.target)); }
      var o = outlineOf(root, !!d.all);
      return answer(d.id, { text: o.text, count: o.count });
    }
    if (d.type === CLICK) {
      var target = resolve(d.target);
      if (!target) return refuse(d.id, missing(d.target));
      if (typeof target.scrollIntoView === "function") target.scrollIntoView({ block: "center" });
      var path = cssPath(target);
      if (typeof target.click === "function") target.click();
      else fire(target, "click", { bubbles: true, cancelable: true }, "MouseEvent");
      later(function () { answer(d.id, { selector: path }); }, 150);
      return;
    }
    if (d.type === FILL || d.type === TYPE) {
      var field = resolve(d.target);
      if (!field) return refuse(d.id, missing(d.target));
      if (!editableOf(field)) return refuse(d.id, cssPath(field) + " takes no typing");
      if (typeof field.focus === "function") field.focus();
      if (d.type === FILL) { setValue(field, String(d.text || "")); changed(field); return answer(d.id, { selector: cssPath(field) }); }
      var fieldPath = cssPath(field);
      typeInto(field, d.text, !!d.clear, !!d.submit);
      later(function () { answer(d.id, { selector: fieldPath, text: valueOf(field) }); }, d.submit ? 150 : 0);
      return;
    }
    if (d.type === PRESS) {
      var on = d.target ? resolve(d.target) : null;
      if (d.target && !on) return refuse(d.id, missing(d.target));
      var key = String(d.key || "");
      if (!key) return refuse(d.id, "name a key — Enter, Escape, Tab, an arrow, a character");
      press(on, key, d.modifiers || []);
      later(function () { answer(d.id, { selector: on ? cssPath(on) : (document.activeElement ? cssPath(document.activeElement) : null) }); }, 150);
      return;
    }
    if (d.type === SELECT) {
      var sel = resolve(d.target);
      if (!sel) return refuse(d.id, missing(d.target));
      if (tagOf(sel) !== "SELECT") return refuse(d.id, cssPath(sel) + " is not a select — browser_click chooses a " + roleOf(sel));
      var opts = sel.options || sel.children || [], chosen = null, wantValue = d.value == null ? null : String(d.value), wantLabel = d.label == null ? null : trimWords(d.label, MAX_TEXT).toLowerCase();
      for (var j = 0; j < opts.length; j++) {
        var opt = opts[j];
        if (tagOf(opt) !== "OPTION") continue;
        var v = String(opt.value != null ? opt.value : attr(opt, "value") != null ? attr(opt, "value") : opt.textContent);
        if ((wantValue !== null && v === wantValue) || (wantValue === null && wantLabel !== null && trimWords(opt.textContent, MAX_TEXT).toLowerCase() === wantLabel)) { chosen = opt; break; }
      }
      if (!chosen) return refuse(d.id, "no option " + (wantValue !== null ? "with value " + quoted(wantValue) : wantLabel !== null ? "labelled " + quoted(d.label) : "named — give a value or a label") + " in " + cssPath(sel));
      for (var k2 = 0; k2 < opts.length; k2++) if (tagOf(opts[k2]) === "OPTION") opts[k2].selected = opts[k2] === chosen;
      try { sel.value = chosen.value != null ? chosen.value : attr(chosen, "value"); } catch (e4) {}
      changed(sel);
      return answer(d.id, { selector: cssPath(sel), text: "chose " + quoted(trimWords(chosen.textContent, MAX_TEXT)) + (chosen.value != null ? " (value " + quoted(chosen.value) + ")" : "") });
    }
    if (d.type === POINT) {
      var over = resolve(d.target);
      if (!over) return refuse(d.id, missing(d.target));
      hover(over);
      later(function () { answer(d.id, { selector: cssPath(over) }); }, 150);
      return;
    }
    if (d.type === SCROLL) {
      var to = d.to == null ? "" : String(d.to);
      if (to === "top") { if (typeof window.scrollTo === "function") window.scrollTo(0, 0); }
      else if (to === "bottom") { if (typeof window.scrollTo === "function") window.scrollTo(0, scrollWords().height); }
      else if (to) { var into = resolve(to); if (!into) return refuse(d.id, missing(to)); if (typeof into.scrollIntoView === "function") into.scrollIntoView({ block: "center" }); }
      else if (typeof window.scrollBy === "function") window.scrollBy(Number(d.byX) || 0, Number(d.byY) || 0);
      later(function () { answer(d.id, { scroll: scrollWords() }); }, 100);
      return;
    }
    if (d.type === WAIT) return waitFor(d);
    if (d.type === CONSOLE) {
      var out = logs.slice(0, MAX_CONSOLE);
      if (d.clear) logs = [];
      return answer(d.id, { console: out, count: out.length, text: out.length ? "" : "the console is empty — nothing logged, no error raised since the page loaded" });
    }
    if (d.type === EVAL) return evaluate(d);
  }
  var DRIVES = {};
  DRIVES[READ] = DRIVES[FIND_TEXT] = DRIVES[SNAPSHOT] = DRIVES[CLICK] = DRIVES[FILL] = DRIVES[TYPE] = DRIVES[PRESS] = DRIVES[SELECT] = DRIVES[POINT] = DRIVES[SCROLL] = DRIVES[WAIT] = DRIVES[CONSOLE] = DRIVES[EVAL] = 1;
  function handle(d) {
    if (!d || typeof d !== "object") return;
    if (d.type === INSPECT) setMode(d.mode);
    else if (d.type === MARKS) {
      marks = Array.isArray(d.marks) ? d.marks : [];
      post({ type: MARKED, lost: layBadges() });
    }
    else if (d.type === THEME) restyle(d.styles);
    else if (d.type === FIND) runFind(d);
    else if (d.type === PLACE_TO) placeTo(d);
    else if (DRIVES[d.type] === 1) drive(d);
  }
  // Where the page is, said once a frame while it moves: the app keeps it and
  // says it back after the next load, so a page edited beside its source — a
  // new srcdoc on every keystroke — or left for another tab stays where it was.
  // Nothing is heard until the app says a place: only the IDE's document
  // frame does — after every load, the origin included — so a page of the
  // browser tab, which carries this same core, never listens to its scroll
  // nor reports one nobody keeps.
  var placeFrame = 0, placing = false, placeKept = false;
  function sayPlace() {
    placeFrame = 0;
    if (placing || !placeKept) return;
    post({ type: PLACE, top: Math.round(window.scrollY || 0), left: Math.round(window.scrollX || 0) });
  }
  function placeTo(d) {
    var top = Number(d.top) || 0, left = Number(d.left) || 0;
    if (!placeKept) {
      placeKept = true;
      window.addEventListener("scroll", function () {
        if (!placeFrame) placeFrame = window.requestAnimationFrame(sayPlace);
      }, { passive: true });
    }
    if (top === 0 && left === 0) return;
    placing = true;
    // A page still laying out — pictures, a late stylesheet — is asked again
    // on the next frames until it holds, a few at most; then the place is the person's.
    var tries = 0;
    (function again() {
      window.scrollTo(left, top);
      var there = Math.abs((window.scrollY || 0) - top) < 2 && Math.abs((window.scrollX || 0) - left) < 2;
      if (there || ++tries >= 30) { placing = false; return; }
      window.requestAnimationFrame(again);
    })();
  }

`;

/**
 * The overlay's dress, written into a script as the page opens: the first
 * paint is the theme's with no word over the wire, and a browser tab's every
 * new document — the script runs again on each navigation — starts dressed.
 * @param {import("./inspectorTheme.mjs").InspectorTheme} theme
 */
function bakedStyles(theme) {
  return `var STYLES = ${JSON.stringify(inspectorStyles(theme))};`;
}

/**
 * The program the frame runs: the core over `postMessage`, and nothing else
 * that leaves the page — the parent is the only one it hears.
 * @param {import("./inspectorTheme.mjs").InspectorTheme} theme the overlay's theme at this paint (`bisa:theme` re-dresses it later)
 */
export function inspectorScript(theme) {
  return String.raw`(function () {
  if (window.__bisaInspector) return;
  window.__bisaInspector = true;
  ${bakedStyles(theme)}
  function post(msg) { window.parent.postMessage(msg, "*"); }
${INSPECTOR_CORE}
  window.addEventListener("message", function (e) {
    if (e.source !== window.parent) return;
    handle(e.data);
  });
})();`;
}

/**
 * The program the browser tab runs in every page (ide/18): the same core
 * over the desktop's one door out of a page — the `bisa` script message
 * handler the shell puts on every tab's webview (`BROWSER_DOOR`), which a
 * page of this machine and a page of the web reach alike; the desktop
 * bounds every word (`parseInspectorMessage`). The app's words arrive
 * through `window.__bisaBrowser.perform`, which the desktop evaluates; the
 * page says its URL and title as it loads and moves, and a load forgets the
 * refs the last snapshot handed out; the page's console and its errors are
 * kept for `browser_console`, and its dialogs — alert, confirm, prompt — are
 * answered for the agent and reported with the next answer, so nothing
 * hangs; and a browser chord pressed inside the page — the keymap's browser
 * scope, ⌘L ⌘R ⌘[ ⌘] ⌘T ⌘W by default (Ctrl off a Mac), written into the
 * script as the tab opens (`relayChords`) — is relayed as a `KEY`, so the
 * bar answers it as if typed in the main window.
 * @param {readonly {key: string, shift: boolean, alt: boolean, command: string}[]} relay
 * @param {import("./inspectorTheme.mjs").InspectorTheme} theme the overlay's theme as the tab opens
 */
export function browserScript(relay, theme) {
  return String.raw`(function () {
  if (window.__bisaInspector) return;
  window.__bisaInspector = true;
  ${bakedStyles(theme)}
  var KEY = "${INSPECTOR_MESSAGES.KEY}";
  function door() {
    var w = window.webkit;
    return w && w.messageHandlers && w.messageHandlers.${BROWSER_DOOR} ? w.messageHandlers.${BROWSER_DOOR} : null;
  }
  function post(msg) {
    var d = door();
    if (!d) return;
    try { d.postMessage(JSON.stringify(msg)); } catch (e) {}
  }
${INSPECTOR_CORE}
  installConsole();
  installDialogs();
  window.__bisaBrowser = { perform: function (d) { handle(d); } };
  function said() { var at = here(); post({ type: TITLE, url: at.url, title: at.title }); }
  function loaded() { pageLoaded(); said(); }
  window.addEventListener("DOMContentLoaded", said);
  window.addEventListener("load", loaded);
  window.addEventListener("popstate", said);
  window.addEventListener("hashchange", said);
  var CHORDS = ${JSON.stringify(relay.map((r) => ({ key: String(r.key).toLowerCase(), shift: Boolean(r.shift), alt: Boolean(r.alt), command: String(r.command) })))};
  var nav = window.navigator;
  var mac = !!(nav && /Mac|iP/.test(String(nav.platform || "")));
  window.addEventListener("keydown", function (e) {
    var mod = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
    if (!mod) return;
    var key = String(e.key || "").toLowerCase();
    for (var i = 0; i < CHORDS.length; i++) {
      var c = CHORDS[i];
      if (c.key === key && c.shift === !!e.shiftKey && c.alt === !!e.altKey) {
        e.preventDefault();
        post({ type: KEY, command: c.command });
        return;
      }
    }
  }, true);
})();`;
}
