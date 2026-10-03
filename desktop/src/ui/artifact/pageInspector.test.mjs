/**
 * The page inspector: what its script may do, what it says, and what the
 * app accepts. Run with `node --test desktop/src/ui/artifact/pageInspector.test.mjs`.
 */
import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { relayChords, resolveKeymap } from "../../shell/keymapModel.mjs";
import vm from "node:vm";

import {
  BROWSER_COMMANDS,
  BROWSER_DOOR,
  browserScript,
  MAX_CONSOLE_LINES,
  MAX_DIALOGS,
  MAX_LINE_CHARS,
  MAX_WAIT_MS,
  consoleMessage,
  evalMessage,
  pointMessage,
  pressMessage,
  scrollMessage,
  selectMessage,
  snapshotMessage,
  typeMessage,
  waitMessage,
  CUT_TAIL,
  INSPECTOR_MESSAGES,
  MAX_EXCERPT_BYTES,
  MAX_NOTE_CHARS,
  MAX_READ_CHARS,
  MAX_SELECTOR_CHARS,
  MAX_TEXT_CHARS,
  clickMessage,
  cutBytes,
  fillMessage,
  findMessage,
  findTextMessage,
  inspectMessage,
  placeToMessage,
  marksMessage,
  parseInspectorMessage,
  readMessage,
  inspectorScript,
  inspectorWords,
  themeMessage,
} from "./pageInspector.mjs";
import { STYLE_PARTS, inspectorStyles, inspectorTheme } from "./inspectorTheme.mjs";

/** The overlay's theme the scripts under test are dressed in — the light fallbacks. */
const THEME = inspectorTheme({}, "light");
const INSPECTOR_SCRIPT = inspectorScript(THEME);

/** A theme whose every value is a sentinel — what the built script carries came from the theme or from nowhere. */
function sentinelTheme(prefix) {
  const t = inspectorTheme({}, "light");
  const mark = (group, key) => `${prefix}-${group}-${key}`.toUpperCase();
  return {
    scheme: "light",
    color: Object.fromEntries(Object.keys(t.color).map((k) => [k, mark("color", k)])),
    radius: Object.fromEntries(Object.keys(t.radius).map((k) => [k, mark("radius", k)])),
    shadow: Object.fromEntries(Object.keys(t.shadow).map((k) => [k, mark("shadow", k)])),
    font: Object.fromEntries(Object.keys(t.font).map((k) => [k, mark("font", k)])),
    motion: Object.fromEntries(Object.keys(t.motion).map((k) => [k, mark("motion", k)])),
  };
}

test("the script names no network, no storage and no cookie — postMessage is its one way out", () => {
  for (const word of ["fetch(", "XMLHttpRequest", "localStorage", "sessionStorage", "document.cookie", "indexedDB", "import(", "WebSocket", "sendBeacon", "eval(", "`"]) {
    assert.ok(!INSPECTOR_SCRIPT.includes(word), `the script must not contain ${word}`);
  }
  assert.ok(INSPECTOR_SCRIPT.includes('window.parent.postMessage(msg, "*")'), "an opaque origin cannot be named: the target is *");
  assert.ok(INSPECTOR_SCRIPT.includes("e.source !== window.parent"), "the frame answers its parent alone");
  assert.ok(INSPECTOR_SCRIPT.includes("stopImmediatePropagation"), "a click while inspecting never reaches the page");
});

test("the overlay is dressed by the theme alone: no colour, no font of its own, and every part it draws is a part the theme dresses", () => {
  const sentinel = sentinelTheme("S");
  for (const script of [inspectorScript(sentinel), browserScript([], sentinel)]) {
    for (const bad of [/#[0-9a-fA-F]{3,8}\b/, /\brgba?\(/, /oklch\(/, /system-ui/, /ui-monospace/, /-apple-system/]) {
      assert.ok(!bad.test(script), `nothing of the overlay's look is written into the script: ${bad}`);
    }
    for (const word of ["S-COLOR-ACCENT", "S-COLOR-SURFACE", "S-RADIUS-CONTROL", "S-RADIUS-CARD", "S-SHADOW-FLOATING", "S-FONT-UI", "S-FONT-MONO", "S-MOTION-FAST"]) {
      assert.ok(script.includes(word), `the theme's ${word} reached the script`);
    }
  }
  // Every `part("tag", "what")` and every `STYLES[...]` the core names is a part the theme dresses.
  const source = readFileSync(new URL("./pageInspector.mjs", import.meta.url), "utf8");
  const core = source.slice(source.indexOf("const INSPECTOR_CORE"), source.indexOf("function bakedStyles"));
  const named = [...core.matchAll(/part\("[a-z]+", "([a-z-]+)"\)/g)].map((m) => m[1]);
  assert.ok(named.length >= 10, "the core draws its parts through part()");
  for (const what of named) assert.ok(STYLE_PARTS.includes(what), `${what} is a part the theme dresses`);
  assert.ok(!/style\.cssText = "/.test(core), "no part is dressed by a literal");
  assert.ok(Object.keys(inspectorStyles(THEME)).every((k) => STYLE_PARTS.includes(k)), "the theme dresses only known parts");
});

test("the note box says the catalog's words, baked into the script — an element's tag where %TAG% stands", () => {
  const words = inspectorWords();
  for (const [key, value] of Object.entries(words)) assert.ok(typeof value === "string" && value.trim().length > 0, `${key} has words`);
  assert.equal(words.box, "Annotate <%TAG%>");
  assert.equal(words.instead, "Annotate <%TAG%> instead");
  assert.deepEqual([words.add, words.change, words.close, words.ask], ["Add", "Change", "Never mind", "What should change here?"]);
  for (const script of [inspectorScript(THEME), browserScript([], THEME)]) assert.ok(script.includes(`var WORDS = ${JSON.stringify(words)};`), "the words ride the script as its dress does");
});

test("run in a page: the first paint wears the baked theme, a THEME word re-dresses an open box in place, and a stranger's THEME is ignored", () => {
  const page = fakeDocument();
  const save = page.node("button", { id: "save", text: "Save" });
  page.body.appendChild(save);
  vm.runInNewContext(inspectorScript(sentinelTheme("S")), { window: page.window, document: page.document });
  page.fromParent(inspectMessage("picking"));
  page.fromParent(marksMessage([{ selector: "#save", n: 1, note: "bigger" }]));
  page.fire("click", { target: save });
  const box = boxOf(page);
  assert.ok(box.open());
  assert.ok(box.el.style.cssText.includes("S-COLOR-SURFACE"), "the box wears the baked surface");
  assert.ok(box.el.style.cssText.includes("color-scheme:light"), "the box's form control follows the app's side, not the page's");
  assert.ok(box.add().style.cssText.includes("S-COLOR-ACCENT"), "Add is the accent");
  const badges = page.document.documentElement.children.find((c) => c.attrs["data-bisa-inspector"] === "badges");
  assert.ok(badges.children[0].style.cssText.includes("S-COLOR-ACCENT"), "a badge is the accent");
  const badgeTop = `${save.rect.top - 9}px`;
  assert.equal(badges.children[0].style.top, badgeTop, "a badge stands by its element");

  box.input().value = "half typed";
  page.fromParent(themeMessage(inspectorTheme({ "--color-surface": "T-SURFACE", "--color-accent": "T-ACCENT", "--color-accent-ink": "T-INK", "--color-text": "T-TEXT" }, "dark")));
  assert.ok(box.open(), "the box stays open");
  assert.equal(box.input().value, "half typed", "the words stay typed");
  assert.ok(box.el.style.cssText.includes("T-SURFACE") && !box.el.style.cssText.includes("S-COLOR-SURFACE"), "the box wears the new surface");
  assert.ok(box.el.style.cssText.includes("color-scheme:dark"));
  assert.ok(box.add().style.cssText.includes("T-ACCENT"), "Add follows");
  assert.ok(box.current().style.cssText.includes("T-TEXT"), "the crumbs follow — the element in full ink, neutral");
  assert.ok(badges.children[0].style.cssText.includes("T-ACCENT"), "the badge follows");
  assert.equal(badges.children[0].style.top, badgeTop, "and keeps its place");
  assert.equal(box.el.style.display, "block");

  const before = box.el.style.cssText;
  page.fire("message", { source: { not: "the parent" }, data: themeMessage(inspectorTheme({ "--color-surface": "X-SURFACE" }, "light")) });
  assert.equal(box.el.style.cssText, before, "a stranger dresses nothing");
  page.fromParent({ type: INSPECTOR_MESSAGES.THEME, styles: "not an object" });
  assert.equal(box.el.style.cssText, before, "a malformed word dresses nothing");
});

test("the vocabulary is frozen, the app's builders say what they mean, and the parser takes the frame's words alone", () => {
  assert.ok(Object.isFrozen(INSPECTOR_MESSAGES));
  assert.deepEqual(inspectMessage("picking"), { type: "bisa:inspect", mode: "picking" });
  assert.deepEqual(inspectMessage("held"), { type: "bisa:inspect", mode: "off" }, "held is the page's own state, never the app's word");
  assert.deepEqual(inspectMessage("off"), { type: "bisa:inspect", mode: "off" });
  assert.deepEqual(inspectMessage("sideways"), { type: "bisa:inspect", mode: "off" }, "a word off the list is off");
  assert.deepEqual(marksMessage([{ selector: "#a", n: 1, note: "bigger" }, { selector: "#b", n: 2 }]), { type: "bisa:marks", marks: [{ selector: "#a", n: 1, note: "bigger" }, { selector: "#b", n: 2, note: "" }] }, "every badge carries its note, blank when it has none");
  const rect = { top: 1, left: 2, width: 3, height: 4 };
  assert.deepEqual(parseInspectorMessage({ type: "bisa:hover", tag: "div.card", rect }), { type: "bisa:hover", tag: "div.card", rect });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:pick", selector: "#save", tag: "button#save", excerpt: "<button>", text: "Save", rect }), {
    type: "bisa:pick",
    selector: "#save",
    tag: "button#save",
    excerpt: "<button>",
    text: "Save",
    rect,
    ancestors: [],
  });
  // The ancestors ride bounded: a malformed one is dropped, the chain cut at the path's depth.
  const ancestors = [{ tag: "html", selector: "html" }, { tag: "body.dark", selector: "body" }, { tag: 3, selector: "x" }, { selector: "" }, "nope"];
  assert.deepEqual(parseInspectorMessage({ type: "bisa:pick", selector: "#save", tag: "b", excerpt: "", text: "", rect, ancestors }).ancestors, [
    { tag: "html", selector: "html" },
    { tag: "body.dark", selector: "body" },
  ]);
  const deep = Array.from({ length: 40 }, (_, i) => ({ tag: "div", selector: `d${i}` }));
  assert.equal(parseInspectorMessage({ type: "bisa:pick", selector: "#save", tag: "b", excerpt: "", text: "", rect, ancestors: deep }).ancestors.length, 16);
  assert.deepEqual(parseInspectorMessage({ type: "bisa:note", selector: "#save", tag: "button#save", excerpt: "<button>", text: "Save", note: "  make it blue " }), {
    type: "bisa:note",
    selector: "#save",
    tag: "button#save",
    excerpt: "<button>",
    text: "Save",
    note: "make it blue",
  }, "a note rides with the element it is about, trimmed");
  assert.equal(parseInspectorMessage({ type: "bisa:note", selector: "#save", tag: "b", excerpt: "", text: "", note: "x".repeat(MAX_NOTE_CHARS + 5) }).note.length, MAX_NOTE_CHARS, "a note is bounded");
  assert.deepEqual(parseInspectorMessage({ type: "bisa:closed" }), { type: "bisa:closed" });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:marked", lost: [2, "x", 3.5, 4] }), { type: "bisa:marked", lost: [2, 4] });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:escape" }), { type: "bisa:escape" });
  for (const bad of [null, "x", 1, {}, { type: "bisa:inspect", on: true }, { type: "bisa:marks", marks: [] }, { type: "bisa:select", selector: "body" }, { type: "bisa:rect", selector: "#save", rect }, { type: "bisa:note", selector: "#save", tag: "b", excerpt: "", text: "", note: "   " }, { type: "bisa:note", selector: 1, tag: "b", excerpt: "", text: "", note: "n" }, { type: "bisa:hover", tag: 3, rect }, { type: "bisa:pick", selector: 1, tag: "b", excerpt: "", text: "", rect }, { type: "bisa:pick", selector: "#a", tag: "b", excerpt: "", text: "", rect: { top: NaN, left: 0, width: 0, height: 0 } }, { type: "bisa:marked", lost: "2" }]) {
    assert.equal(parseInspectorMessage(bad), null, JSON.stringify(bad));
  }
});

test("the parser bounds every field: the excerpt by bytes with the cut tail, the rest by characters, a negative size to nothing", () => {
  const long = "é".repeat(MAX_EXCERPT_BYTES);
  const cut = cutBytes(long, MAX_EXCERPT_BYTES);
  assert.ok(cut.endsWith(CUT_TAIL));
  assert.ok(new TextEncoder().encode(cut).length <= MAX_EXCERPT_BYTES, "the bound holds in bytes");
  assert.ok(!cut.includes("�"), "a cut inside a character is dropped, never invented");
  assert.equal(cutBytes("short", 100), "short");
  const rect = { top: 0, left: 0, width: -5, height: 10 };
  const pick = parseInspectorMessage({ type: "bisa:pick", selector: "x".repeat(MAX_SELECTOR_CHARS + 50), tag: "t", excerpt: long, text: "y".repeat(MAX_TEXT_CHARS + 50), rect });
  assert.equal(pick.selector.length, MAX_SELECTOR_CHARS);
  assert.equal(pick.text.length, MAX_TEXT_CHARS);
  assert.ok(pick.excerpt.endsWith(CUT_TAIL));
  assert.equal(pick.rect.width, 0);
});

/** A document small enough to hold in a test: nodes with what the script reads and touches. */
function fakeDocument() {
  const listeners = new Map();
  const posted = [];
  let seq = 0;
  const node = (tag, over = {}) => {
    const el = {
      nodeType: 1,
      tagName: tag.toUpperCase(),
      id: over.id ?? "",
      className: over.className ?? "",
      children: [],
      parentElement: null,
      // As in a document: an id given at birth is an attribute too.
      attrs: over.id ? { id: over.id } : {},
      style: {},
      _text: over.text ?? "",
      // Own text when set, else the children's — as `textContent` reads in a document.
      get textContent() {
        return this._text || this.children.map((c) => c.textContent).join("");
      },
      set textContent(v) {
        this._text = String(v);
      },
      rect: over.rect ?? { top: 10 * ++seq, left: 5, width: 100, height: 20 },
      firstChild: null,
      get outerHTML() {
        return over.html ?? `<${tag}${this.id ? ` id="${this.id}"` : ""}>${this.textContent}</${tag}>`;
      },
      getBoundingClientRect() {
        return this.rect;
      },
      setAttribute(k, v) {
        this.attrs[k] = v;
        if (k === "id") this.id = v;
      },
      getAttribute(k) {
        return k in this.attrs ? this.attrs[k] : null;
      },
      focus() {
        document.activeElement = this;
      },
      closest(sel) {
        const want = sel.slice(1, -1);
        for (let n = this; n; n = n.parentElement) if (want in n.attrs) return n;
        return null;
      },
      appendChild(c) {
        c.parentElement = this;
        this.children.push(c);
        this.firstChild = this.children[0];
        return c;
      },
      removeChild(c) {
        this.children = this.children.filter((x) => x !== c);
        this.firstChild = this.children[0] ?? null;
        return c;
      },
    };
    return el;
  };
  const html = node("html");
  const body = node("body");
  html.appendChild(body);
  const all = (root, out = []) => {
    for (const c of root.children) {
      out.push(c);
      all(c, out);
    }
    return out;
  };
  /** `#id`, or a ` > ` chain of `tag` / `tag:nth-of-type(k)` from body. */
  const querySelectorAll = (sel) => {
    if (sel.startsWith("#") && !sel.includes(" ")) return all(html).filter((n) => n.id === sel.slice(1));
    if (sel === "html") return [html];
    const parts = sel.split(" > ");
    let current = [html];
    for (const part of parts) {
      const m = /^([a-z]+)(?::nth-of-type\((\d+)\))?$/.exec(part);
      if (!m) return [];
      const next = [];
      for (const parent of current) {
        const same = parent.children.filter((c) => c.tagName === m[1].toUpperCase());
        if (m[2]) {
          const k = same[Number(m[2]) - 1];
          if (k) next.push(k);
        } else next.push(...same);
      }
      current = next;
    }
    return current;
  };
  const document = {
    documentElement: html,
    body,
    activeElement: null,
    createElement: (t) => node(t),
    querySelector: (sel) => querySelectorAll(sel)[0] ?? null,
    querySelectorAll,
  };
  const parent = { postMessage: (msg) => posted.push(msg) };
  const window = {
    parent,
    innerWidth: 800,
    innerHeight: 600,
    CSS: { escape: (s) => s },
    // Every page has a console; the browser's program wraps it.
    console: { log() {}, error() {}, warn() {}, info() {}, debug() {} },
    addEventListener(type, fn) {
      listeners.set(type, [...(listeners.get(type) ?? []), fn]);
    },
    removeEventListener(type, fn) {
      listeners.set(type, (listeners.get(type) ?? []).filter((f) => f !== fn));
    },
  };
  const fire = (type, event) => {
    for (const fn of listeners.get(type) ?? []) fn({ type, preventDefault() {}, stopImmediatePropagation() {}, ...event });
  };
  const fromParent = (data) => fire("message", { source: parent, data });
  return { window, document, body, node, posted, listeners, fire, fromParent };
}

test("run in a page: a click answers with the element's path, tag, excerpt and text; inspecting off removes what it added; a lost badge is said; Escape is relayed; a stranger is ignored", () => {
  const page = fakeDocument();
  const ul = page.node("ul");
  page.body.appendChild(page.node("h1", { text: "Title" }));
  page.body.appendChild(ul);
  const items = ["one", "two", "three"].map((t) => page.node("li", { text: t, className: "item wide extra" }));
  for (const li of items) ul.appendChild(li);
  const save = page.node("button", { id: "save", text: "Save" });
  page.body.appendChild(save);
  const twins = [page.node("p", { id: "twin" }), page.node("p", { id: "twin" })];
  for (const p of twins) page.body.appendChild(p);
  const huge = page.node("pre", { html: `<pre>${"x".repeat(10_000)}</pre>` });
  page.body.appendChild(huge);

  vm.runInNewContext(INSPECTOR_SCRIPT, { window: page.window, document: page.document });
  assert.ok(page.listeners.get("message")?.length === 1, "one message listener, and nothing else until asked");
  assert.ok(!page.listeners.get("click"), "nothing is intercepted before inspecting");

  page.fromParent(inspectMessage("picking"));
  page.fire("click", { target: items[2] });
  const pick = page.posted.find((m) => m.type === INSPECTOR_MESSAGES.PICK);
  // Every click below opens the box; Escape in it closes it so the next click picks again.
  const again = (target) => {
    page.fire("keydown", { key: "Escape", target: boxOf(page).input() });
    page.fire("click", { target });
  };
  assert.equal(pick.selector, "body > ul > li:nth-of-type(3)", "no unique id: the chain of tags with their position");
  assert.equal(pick.tag, "li.item.wide", "at most two classes");
  assert.equal(pick.text, "three");
  assert.equal(pick.excerpt, "<li>three</li>");
  assert.deepEqual(Object.keys(pick.rect), ["top", "left", "width", "height"]);

  assert.deepEqual([...pick.ancestors].map((c) => ({ ...c })), [
    { tag: "html", selector: "html" },
    { tag: "body", selector: "body" },
    { tag: "ul", selector: "body > ul" },
  ], "the ancestors ride outermost first, html to the parent");

  // The document itself and its body are picks like any other.
  again(page.document.documentElement);
  assert.equal(page.posted.at(-1).selector, "html", "the document element is a pick, not rewritten to body");
  assert.equal(page.posted.at(-1).tag, "html");
  assert.deepEqual([...page.posted.at(-1).ancestors], []);
  again(page.body);
  assert.equal(page.posted.at(-1).selector, "body");
  assert.deepEqual([...page.posted.at(-1).ancestors].map((c) => c.selector), ["html"]);

  // The box opened on the body: its crumb for the root re-picks the root, and the box says so.
  const crumb = boxOf(page).crumbs().find((c) => c.attrs["data-bisa-selector"] === "html");
  page.fire("click", { target: crumb });
  const selected = page.posted.at(-1);
  assert.equal(selected.type, INSPECTOR_MESSAGES.PICK, "a crumb is a pick");
  assert.equal(selected.selector, "html");
  assert.equal(selected.tag, "html");

  again(save);
  assert.equal(page.posted.at(-1).selector, "#save", "a unique id is the whole path");
  again(twins[1]);
  assert.equal(page.posted.at(-1).selector, "body > p:nth-of-type(2)", "a duplicated id is no locator");
  again(huge);
  assert.ok(page.posted.at(-1).excerpt.endsWith("… (cut)"), "a big element's excerpt is cut in the page already");
  page.fire("keydown", { key: "Escape", target: boxOf(page).input() });
  page.fire("mousemove", { target: items[0] });
  assert.equal(page.posted.at(-1).type, INSPECTOR_MESSAGES.HOVER);
  assert.equal(page.posted.at(-1).tag, "li.item.wide");
  page.fire("keydown", { key: "Escape" });
  assert.equal(page.posted.at(-1).type, INSPECTOR_MESSAGES.ESCAPE);

  page.fromParent(marksMessage([{ selector: "#save", n: 1, note: "bigger" }, { selector: "#nope", n: 2, note: "" }]));
  // What the script posts was made in the vm's realm: compare values, not prototypes.
  const marked = page.posted.at(-1);
  assert.equal(marked.type, INSPECTOR_MESSAGES.MARKED);
  assert.deepEqual([...marked.lost], [2], "the badge whose element is gone is said");
  const badges = page.document.documentElement.children.find((c) => c.attrs["data-bisa-inspector"] === "badges");
  assert.equal(badges.children.length, 1, "one badge drawn, for the element that is there");
  assert.equal(badges.children[0].textContent, "1");

  page.fromParent(inspectMessage("off"));
  assert.equal((page.listeners.get("click") ?? []).length, 0, "inspecting off: the interceptors are gone");
  assert.equal((page.listeners.get("mousemove") ?? []).length, 0);
  const before = page.posted.length;
  page.fire("click", { target: save });
  assert.equal(page.posted.length, before, "a click is the page's own again");

  page.fire("message", { source: { not: "the parent" }, data: inspectMessage("picking") });
  page.fire("click", { target: save });
  assert.equal(page.posted.length, before, "a message not from the parent is ignored");
});

/** The note box the script drew, by its parts. */
function boxOf(page) {
  const root = page.document.documentElement;
  const el = root.children.find((c) => c.attrs["data-bisa-inspector"] === "box");
  const all = (n, out = []) => {
    for (const c of n.children) {
      out.push(c);
      all(c, out);
    }
    return out;
  };
  const part = (what) => (el ? all(el).find((c) => c.attrs["data-bisa-inspector"] === what) : undefined);
  return {
    el,
    open: () => !!el && el.style.display === "block",
    crumbs: () => (el ? all(part("crumbs")).filter((c) => c.attrs["data-bisa-act"] === "crumb") : []),
    current: () => part("crumb-current"),
    text: () => part("box-text"),
    input: () => part("input"),
    add: () => part("add"),
    close: () => part("close"),
    hint: () => part("box-hint"),
  };
}

/**
 * The same page under either program: the frame's over `postMessage`, the
 * browser tab's over the shell's door and `perform`. What the page says
 * comes back as plain values either way.
 */
/** The browser chords the default keymap relays — what a tab opens with. */
const RELAY = relayChords(resolveKeymap("default", null));
const BROWSER_SCRIPT = browserScript(RELAY, THEME);

const PROGRAMS = [
  ["the frame's script", (page) => {
    vm.runInNewContext(INSPECTOR_SCRIPT, { window: page.window, document: page.document });
    return { say: page.fromParent, said: () => page.posted };
  }],
  ["the browser's script", (page) => {
    const said = [];
    page.window.webkit = { messageHandlers: { [BROWSER_DOOR]: { postMessage: (text) => said.push(JSON.parse(text)) } } };
    page.window.navigator = { platform: "MacIntel" };
    page.window.location = { href: "https://example.com/" };
    vm.runInNewContext(BROWSER_SCRIPT, { window: page.window, document: page.document });
    return { say: (m) => page.window.__bisaBrowser.perform(m), said: () => said };
  }],
];

/** A page with a list, a button and a paragraph, under one of the programs. */
function pageUnder(boot) {
  const page = fakeDocument();
  const ul = page.node("ul");
  page.body.appendChild(ul);
  const items = ["one", "two"].map((t) => page.node("li", { text: t, rect: { top: 300, left: 40, width: 200, height: 20 } }));
  for (const li of items) ul.appendChild(li);
  const save = page.node("button", { id: "save", text: "Save", rect: { top: 10, left: 700, width: 80, height: 24 } });
  page.body.appendChild(save);
  const wire = boot(page);
  const outlineOf = () => page.document.documentElement.children.find((c) => c.attrs["data-bisa-inspector"] === "outline");
  return { page, items, save, ul, outlineOf, ...wire };
}

for (const [name, boot] of PROGRAMS) {
  test(`${name}: a pick opens the note box in the page — the crumbs, the element's text, the question — and while it is open the pointer outlines nothing, a click outside is swallowed without a pick, and a click inside is the box's own`, () => {
    const { page, items, save, outlineOf, say, said } = pageUnder(boot);
    say(inspectMessage("picking"));
    assert.ok(!boxOf(page).el, "nothing drawn before a pick");
    page.fire("mousemove", { target: save });
    assert.equal(outlineOf().style.top, `${save.rect.top}px`, "picking: the outline follows the pointer");
    page.fire("click", { target: items[1] });
    assert.equal(said().at(-1).type, INSPECTOR_MESSAGES.PICK, "the pick is said");
    assert.equal(said().at(-1).selector, "body > ul > li:nth-of-type(2)");
    const box = boxOf(page);
    assert.ok(box.open(), "the box is open in the page");
    assert.equal(box.el.attrs.role, "dialog");
    assert.equal(box.el.attrs["aria-label"], "Annotate <li>");
    assert.deepEqual(box.crumbs().map((c) => [c.textContent, c.attrs["data-bisa-selector"]]), [["html", "html"], ["body", "body"], ["ul", "body > ul"]], "one crumb per ancestor, outermost first");
    assert.equal(box.current().textContent, "li");
    assert.equal(box.current().attrs["aria-current"], "true");
    assert.equal(box.text().textContent, "two");
    assert.equal(box.input().attrs.placeholder, "What should change here?");
    assert.equal(box.add().textContent, "Add");
    assert.equal(box.close().attrs["aria-label"], "Never mind");
    assert.equal(box.close().textContent, "", "the close is drawn by its dress, never a glyph from a font");
    assert.equal(box.hint().textContent, "Enter adds it · Esc closes", "the foot says how the keys answer");
    assert.equal(page.document.activeElement, box.input(), "the caret lands in the box");

    const quiet = said().length;
    page.fire("mousemove", { target: save });
    assert.equal(outlineOf().style.top, `${items[1].rect.top}px`, "held: the outline stays on the pick");
    let prevented = 0;
    page.fire("click", { target: save, preventDefault: () => prevented++ });
    assert.equal(said().length, quiet, "held: a hover and a click outside say nothing");
    assert.equal(prevented, 1, "held: the click never reaches the page — a link under the box must not navigate");
    assert.ok(box.open(), "the box stays open");
    let kept = 0;
    page.fire("mousedown", { target: box.input(), preventDefault: () => kept++ });
    assert.equal(kept, 0, "a press inside the box is the box's own — the caret must land");
    assert.equal(said().length, quiet);
  });

  test(`${name}: Enter or Add says the note with the element it is about and closes the box; Escape or Never mind closes it alone and says so; Escape with no box open leaves the inspector`, () => {
    const { page, items, save, outlineOf, say, said } = pageUnder(boot);
    say(inspectMessage("picking"));
    page.fire("click", { target: items[0] });
    const box = boxOf(page);
    let blank = 0;
    page.fire("keydown", { key: "Enter", target: box.input(), preventDefault: () => blank++ });
    assert.equal(blank, 1, "Enter in the box is the box's");
    assert.ok(box.open(), "nothing typed: nothing added, the box stays");
    box.input().value = "  make it blue  ";
    page.fire("keydown", { key: "Enter", target: box.input() });
    const note = said().at(-1);
    assert.equal(note.type, INSPECTOR_MESSAGES.NOTE);
    assert.deepEqual([note.selector, note.tag, note.text, note.excerpt, note.note], ["body > ul > li:nth-of-type(1)", "li", "one", "<li>one</li>", "make it blue"], "the note rides with the whole pick, trimmed");
    assert.deepEqual(parseInspectorMessage(note).note, "make it blue", "and the app's parser takes it");
    assert.ok(!box.open(), "the box closed");
    assert.equal(outlineOf().style.display, "none", "the outline went with it");
    assert.equal(box.input().value, "", "the words are spent");

    page.fire("click", { target: save });
    box.input().value = "x".repeat(MAX_NOTE_CHARS + 20);
    page.fire("click", { target: box.add() });
    assert.equal(said().at(-1).type, INSPECTOR_MESSAGES.NOTE, "Add is Enter");
    assert.equal(said().at(-1).selector, "#save");
    assert.equal(said().at(-1).note.length, MAX_NOTE_CHARS, "bounded in the page already");

    page.fire("click", { target: items[1] });
    assert.ok(box.open());
    page.fire("keydown", { key: "Escape", target: box.input() });
    assert.equal(said().at(-1).type, INSPECTOR_MESSAGES.CLOSED, "Escape in the box closes it and says so");
    assert.ok(!box.open());
    page.fire("click", { target: items[1] });
    box.input().value = "half a thought";
    page.fire("click", { target: box.close() });
    assert.equal(said().at(-1).type, INSPECTOR_MESSAGES.CLOSED, "Never mind closes it too");
    assert.ok(!box.open());
    page.fire("click", { target: items[1] });
    assert.equal(box.input().value, "", "the words did not survive the close");
    page.fire("keydown", { key: "Escape", target: box.input() });

    page.fire("keydown", { key: "Escape" });
    assert.equal(said().at(-1).type, INSPECTOR_MESSAGES.ESCAPE, "Escape with no box open leaves the inspector");
    assert.ok((page.listeners.get("click") ?? []).length > 0, "the page decides; the script waits for the word");
  });

  test(`${name}: a crumb re-picks from inside the box and keeps the words typed; an element already annotated opens with its note and the button says Change`, () => {
    const { page, items, ul, say, said } = pageUnder(boot);
    say(marksMessage([{ selector: "#save", n: 1, note: "make it green" }]));
    say(inspectMessage("picking"));
    page.fire("click", { target: items[0] });
    const box = boxOf(page);
    box.input().value = "tighter";
    const crumb = box.crumbs().find((c) => c.attrs["data-bisa-selector"] === "body > ul");
    page.fire("click", { target: crumb });
    assert.equal(said().at(-1).type, INSPECTOR_MESSAGES.PICK, "a crumb is a fresh pick");
    assert.equal(said().at(-1).selector, "body > ul");
    assert.ok(box.open());
    assert.equal(box.current().textContent, "ul", "the box is the list's now");
    assert.deepEqual(box.crumbs().map((c) => c.textContent), ["html", "body"]);
    assert.equal(box.input().value, "tighter", "the words typed survive the re-pick");
    page.fire("keydown", { key: "Enter", target: box.input() });
    assert.equal(said().at(-1).selector, "body > ul", "and are added to the crumb's element");
    assert.equal(said().at(-1).note, "tighter");

    const save = page.document.querySelector("#save");
    page.fire("click", { target: save });
    assert.equal(box.input().value, "make it green", "an annotated element opens with its note");
    assert.equal(box.add().textContent, "Change");
    page.fire("keydown", { key: "Escape", target: box.input() });
    page.fire("click", { target: ul });
    assert.equal(box.input().value, "", "an element with no note opens blank");
    assert.equal(box.add().textContent, "Add");
  });

  test(`${name}: the box sits above the element when there is room, below it otherwise, beside it when neither fits — never on it — and follows a scroll`, () => {
    const { page, items, save, say } = pageUnder(boot);
    say(inspectMessage("picking"));
    page.fire("click", { target: items[0] });
    const box = boxOf(page);
    box.el.rect = { top: 0, left: 0, width: 320, height: 96 };
    const tag = page.document.documentElement.children.find((c) => c.attrs["data-bisa-inspector"] === "label").getBoundingClientRect().height || 18;
    page.fire("scroll", {});
    assert.equal(box.el.style.top, `${300 - tag - 2 - 6 - 96}px`, "room above: over the element and over its tag, a gap between");
    assert.equal(box.el.style.left, "40px", "flush with its left edge");
    page.fire("keydown", { key: "Escape", target: box.input() });
    page.fire("click", { target: save });
    assert.equal(box.el.style.top, `${10 + 24 + tag + 2 + 6}px`, "no room above: under it, and under the tag that went under it too");
    assert.equal(box.el.style.left, `${800 - 320}px`, "and never past the right edge");
    items[0].rect = { top: 50, left: 40, width: 200, height: 500 };
    page.fire("keydown", { key: "Escape", target: box.input() });
    page.fire("click", { target: items[0] });
    assert.deepEqual([box.el.style.top, box.el.style.left], ["50px", `${40 + 200 + 6}px`], "a tall element with no room above or below: beside it, to its right, level with its top");
    items[0].rect = { top: 50, left: 470, width: 200, height: 500 };
    page.fire("scroll", {});
    assert.deepEqual([box.el.style.top, box.el.style.left], ["50px", `${470 - 6 - 320}px`], "no room to its right either: to its left");
    items[0].rect = { top: 0, left: 0, width: 800, height: 600 };
    page.fire("scroll", {});
    assert.deepEqual([box.el.style.top, box.el.style.left], ["0px", "0px"], "an element the size of the view leaves nowhere off it: a corner of the view");
    items[0].rect = { top: 0, left: 0, width: 300, height: 600 };
    page.fire("scroll", {});
    assert.deepEqual([box.el.style.top, box.el.style.left], ["0px", `${300 + 6}px`], "a full-height column: beside it");
    items[0].rect = { top: 50, left: 40, width: 200, height: 500 };
    items[0].rect = { top: 200, left: 40, width: 200, height: 20 };
    page.fire("scroll", {});
    assert.equal(box.el.style.top, `${200 - tag - 2 - 6 - 96}px`, "a scroll moves the box with its element");
  });

  test(`${name}: the tag rides outside the element — above it, else under it — and never past the right edge`, () => {
    const { page, items, save, say } = pageUnder(boot);
    say(inspectMessage("picking"));
    page.fire("mousemove", { target: items[0] });
    const label = page.document.documentElement.children.find((c) => c.attrs["data-bisa-inspector"] === "label");
    label.rect = { top: 0, left: 0, width: 120, height: 18 };
    page.fire("mousemove", { target: items[1] });
    page.fire("mousemove", { target: items[0] });
    assert.deepEqual([label.style.top, label.style.left], [`${300 - 18 - 2}px`, "40px"], "room above: just over the element's top edge, not on it");
    page.fire("mousemove", { target: save });
    assert.equal(label.style.top, `${10 + 24 + 2}px`, "no room above: just under its bottom edge");
    assert.equal(label.style.left, `${800 - 120}px`, "and pulled in from the right edge");
    items[1].rect = { top: 0, left: 40, width: 200, height: 600 };
    page.fire("mousemove", { target: items[1] });
    assert.equal(label.style.top, "2px", "only an element the whole height of the view takes it inside its top edge");
  });

  test(`${name}: the app's word closes the box — picking again is no new word, off drops everything — and a repeated word installs nothing twice`, () => {
    const { page, items, outlineOf, say, said } = pageUnder(boot);
    say(inspectMessage("picking"));
    page.fire("click", { target: items[0] });
    const box = boxOf(page);
    say(inspectMessage("picking"));
    assert.ok(box.open(), "the same word again changes nothing");
    assert.equal((page.listeners.get("click") ?? []).length, 1, "and installs nothing twice");
    assert.equal((page.listeners.get("mousemove") ?? []).length, 1);
    const quiet = said().length;
    say(inspectMessage("off"));
    assert.ok(!box.open(), "off closes the box");
    assert.equal(said().length, quiet, "without a word — the app asked");
    assert.equal((page.listeners.get("click") ?? []).length, 0, "off clears the interceptors");
    assert.equal(outlineOf().style.display, "none");
    say(inspectMessage("picking"));
    page.fire("click", { target: items[1] });
    assert.ok(box.open(), "picking again picks again");
    assert.equal(box.input().value, "");
  });
}

test("the app asks the frame to find, bounded; the frame's answer is a pair of integers or nothing", () => {
  assert.deepEqual(findMessage({ query: "needle", regex: true, caseSensitive: false }, 2), { type: "bisa:find", query: "needle", regex: true, caseSensitive: false, index: 2 });
  assert.deepEqual(findMessage(null, NaN), { type: "bisa:find", query: "", regex: false, caseSensitive: false, index: 0 });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:found", index: 3, count: 9 }), { type: "bisa:found", index: 3, count: 9 });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:found", index: -7, count: -1 }), { type: "bisa:found", index: -1, count: 0 }, "bounded below");
  assert.equal(parseInspectorMessage({ type: "bisa:found", index: 1.5, count: 2 }), null);
  assert.equal(parseInspectorMessage({ type: "bisa:found", index: "1", count: 2 }), null);
  assert.equal(parseInspectorMessage({ type: "bisa:find", query: "x" }), null, "the app's own word is not taken back");
});

test("run in a page: a find with nothing to walk answers nothing found, and an empty or refused query clears rather than throws", () => {
  const page = fakeDocument();
  page.body.appendChild(page.node("p", { text: "hello" }));
  vm.runInNewContext(INSPECTOR_SCRIPT, { window: page.window, document: page.document });
  page.fromParent(findMessage({ query: "hello", regex: false, caseSensitive: false }, 0));
  const found = page.posted.at(-1);
  assert.equal(found.type, INSPECTOR_MESSAGES.FOUND);
  assert.equal(found.count, 0, "the fake document has no tree walker: nothing is walked, nothing thrown");
  page.fromParent(findMessage({ query: "[", regex: true, caseSensitive: false }, 0));
  assert.deepEqual({ ...page.posted.at(-1) }, { type: INSPECTOR_MESSAGES.FOUND, index: -1, count: 0 }, "a refused expression finds nothing");
  page.fromParent(findMessage(null, 0));
  assert.deepEqual({ ...page.posted.at(-1) }, { type: INSPECTOR_MESSAGES.FOUND, index: -1, count: 0 });
});

test("the browser's program is the same core over the shell's door on every page — no postMessage, no network, no IPC — inert with no door, and it says its title as it loads", () => {
  for (const word of ["fetch", "XMLHttpRequest", "localStorage", "sessionStorage", "indexedDB", "cookie", "postMessage(msg, ", "__TAURI_INTERNALS__", "browser_message"]) {
    assert.ok(!BROWSER_SCRIPT.includes(word), `the browser script must not contain ${word}`);
  }
  assert.ok(BROWSER_SCRIPT.includes(`w.messageHandlers.${BROWSER_DOOR}`), "the one door out: the shell's script message handler, on every page");
  assert.ok(BROWSER_SCRIPT.includes("window.__bisaBrowser = { perform:"), "the desktop's way to speak to it");
  // With no door — a webview the shell put none on — nothing leaves.
  const dark = fakeDocument();
  vm.runInNewContext(BROWSER_SCRIPT, { window: dark.window, document: dark.document });
  dark.fire("load", {});
  assert.equal(dark.posted.length, 0, "no door: the script is inert");
  // With it, the page says its title on load and answers the driver — the web as much as this machine.
  const page = fakeDocument();
  const said = [];
  page.window.webkit = { messageHandlers: { [BROWSER_DOOR]: { postMessage: (text) => said.push(JSON.parse(text)) } } };
  page.window.navigator = { platform: "MacIntel" };
  page.window.location = { href: "https://example.com/pricing" };
  page.document.title = "Pricing";
  page.body.textContent = "Plans and prices. Buy now.";
  page.body.appendChild(page.node("h1", { text: "Plans" }));
  const buy = page.node("button", { id: "buy", text: "Buy" });
  page.body.appendChild(buy);
  const field = page.node("input", { id: "q" });
  page.body.appendChild(field);
  vm.runInNewContext(BROWSER_SCRIPT, { window: page.window, document: page.document });
  page.fire("load", {});
  assert.deepEqual(said.at(-1), { type: INSPECTOR_MESSAGES.TITLE, url: "https://example.com/pricing", title: "Pricing" }, "posted as one JSON text");
  const perform = page.window.__bisaBrowser.perform;
  perform(readMessage("r1"));
  let a = said.at(-1);
  assert.equal(a.type, INSPECTOR_MESSAGES.ANSWER);
  assert.deepEqual([a.id, a.ok, a.url, a.title, a.text], ["r1", true, "https://example.com/pricing", "Pricing", "Plans and prices. Buy now."]);
  perform(readMessage("r2", "#buy"));
  a = said.at(-1);
  assert.deepEqual([a.id, a.ok, a.selector], ["r2", true, "#buy"]);
  assert.match(a.text, /^button "Buy"\n<button id="buy">\n\nBuy$/, "an element reads as its role and name, its attributes, then its text");
  perform(readMessage("r2h", "#buy", "html"));
  assert.match(said.at(-1).text, /<button id="buy">Buy<\/button>$/, "or as its markup");
  perform(readMessage("r3", "#nowhere"));
  a = said.at(-1);
  assert.deepEqual([a.id, a.ok, a.error], ["r3", false, "nothing matches #nowhere"]);
  perform(clickMessage("c1", "#buy"));
  a = said.at(-1);
  assert.deepEqual([a.id, a.ok, a.selector], ["c1", true, "#buy"]);
  perform(fillMessage("f1", "#q", "hats"));
  a = said.at(-1);
  assert.deepEqual([a.id, a.ok, a.selector], ["f1", true, "#q"]);
  assert.equal(field.value, "hats", "typed into the field");
  perform(fillMessage("f2", "#buy", "x"));
  a = said.at(-1);
  assert.deepEqual([a.ok, a.error], [false, "#buy takes no typing"]);
  perform(findTextMessage("t1", "plans"));
  a = said.at(-1);
  assert.equal(a.id, "t1");
  assert.ok(a.ok);
  assert.equal(a.count, 0, "a page with no text nodes to walk finds nothing");
  assert.match(a.text, /no match for "plans"/);
  perform(findTextMessage("t2", "  "));
  assert.deepEqual([said.at(-1).ok, said.at(-1).error], [false, "nothing to find"]);
  // The inspector's words still work through the same door.
  perform(inspectMessage("picking"));
  page.fire("click", { target: buy });
  assert.equal(said.at(-1).type, INSPECTOR_MESSAGES.PICK);
  assert.equal(said.at(-1).selector, "#buy");
});

test("a browser chord pressed inside the page is relayed as the keymap's command, on a Mac by ⌘ and elsewhere by Ctrl; anything else is the page's own", () => {
  const relay = (platform, event) => {
    const page = fakeDocument();
    const said = [];
    page.window.webkit = { messageHandlers: { [BROWSER_DOOR]: { postMessage: (text) => said.push(JSON.parse(text)) } } };
    page.window.navigator = { platform };
    page.window.location = { href: "https://example.com/" };
    vm.runInNewContext(BROWSER_SCRIPT, { window: page.window, document: page.document });
    let prevented = false;
    page.fire("keydown", { preventDefault: () => (prevented = true), metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...event });
    return { said: said.filter((m) => m.type === INSPECTOR_MESSAGES.KEY), prevented };
  };
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "l" }), { said: [{ type: INSPECTOR_MESSAGES.KEY, command: "focus_address" }], prevented: true });
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "R" }).said, [{ type: INSPECTOR_MESSAGES.KEY, command: "browser_reload" }], "the key's case is the page's");
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "[" }).said, [{ type: INSPECTOR_MESSAGES.KEY, command: "browser_back" }]);
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "]" }).said, [{ type: INSPECTOR_MESSAGES.KEY, command: "browser_forward" }]);
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "t" }).said, [{ type: INSPECTOR_MESSAGES.KEY, command: "new_browser_tab" }]);
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "w" }).said, [{ type: INSPECTOR_MESSAGES.KEY, command: "close_browser_tab" }], "⌘W inside a page closes the browser tab");
  // The relay is the keymap's: a rebound chord rides into a tab opened after it, and the old one is nobody's.
  const rebound = (platform, event) => {
    const page = fakeDocument();
    const said = [];
    page.window.webkit = { messageHandlers: { [BROWSER_DOOR]: { postMessage: (text) => said.push(JSON.parse(text)) } } };
    page.window.navigator = { platform };
    page.window.location = { href: "https://example.com/" };
    vm.runInNewContext(browserScript(relayChords(resolveKeymap("default", { browser_reload: "Mod+Shift+R" })), THEME), { window: page.window, document: page.document });
    page.fire("keydown", { preventDefault: () => undefined, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...event });
    return said.filter((m) => m.type === INSPECTOR_MESSAGES.KEY).map((m) => m.command);
  };
  assert.deepEqual(rebound("MacIntel", { metaKey: true, shiftKey: true, key: "R" }), ["browser_reload"]);
  assert.deepEqual(rebound("MacIntel", { metaKey: true, key: "r" }), [], "the chord the keymap let go relays nothing");
  assert.deepEqual(relay("Win32", { ctrlKey: true, key: "l" }).said, [{ type: INSPECTOR_MESSAGES.KEY, command: "focus_address" }], "off a Mac, Ctrl is the modifier");
  assert.deepEqual(relay("MacIntel", { ctrlKey: true, key: "l" }), { said: [], prevented: false }, "Ctrl on a Mac is the page's own");
  assert.deepEqual(relay("MacIntel", { metaKey: true, shiftKey: true, key: "l" }), { said: [], prevented: false }, "a Shift chord is not one of the six");
  assert.deepEqual(relay("MacIntel", { metaKey: true, key: "c" }), { said: [], prevented: false }, "copy stays the page's");
  assert.deepEqual(relay("MacIntel", { key: "l" }), { said: [], prevented: false }, "typing is typing");
  for (const command of BROWSER_COMMANDS) {
    assert.deepEqual(parseInspectorMessage({ type: INSPECTOR_MESSAGES.KEY, command }), { type: INSPECTOR_MESSAGES.KEY, command });
  }
  assert.equal(parseInspectorMessage({ type: INSPECTOR_MESSAGES.KEY, command: "reboot" }), null, "a command off the list is nothing");
  assert.equal(parseInspectorMessage({ type: INSPECTOR_MESSAGES.KEY }), null);
});

test("the parser takes a page's answer and its title, bounded, and nothing without an id", () => {
  const a = parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "r1", ok: true, url: "http://x/", title: "X", text: "y".repeat(MAX_READ_CHARS + 5) });
  assert.equal(a.type, INSPECTOR_MESSAGES.ANSWER);
  assert.equal(a.text.length, MAX_READ_CHARS, "the text is cut app-side too");
  assert.equal(a.error, null);
  const refused = parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "r2", ok: false, url: "http://x/", title: "", error: "nothing matches #z" });
  assert.deepEqual([refused.ok, refused.error, refused.text], [false, "nothing matches #z", null]);
  assert.equal(parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "", ok: true, url: "http://x/", title: "" }), null, "no id: not an answer to anything");
  assert.equal(parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "r", ok: true, url: 5, title: "" }), null);
  assert.deepEqual(parseInspectorMessage({ type: INSPECTOR_MESSAGES.TITLE, url: "http://x/", title: "T" }), { type: INSPECTOR_MESSAGES.TITLE, url: "http://x/", title: "T" });
  assert.equal(parseInspectorMessage({ type: INSPECTOR_MESSAGES.TITLE, url: "http://x/" }), null);
  assert.deepEqual(readMessage("r1"), { type: INSPECTOR_MESSAGES.READ, id: "r1", target: null, format: "text" });
  assert.deepEqual(readMessage("r1", "main", "html"), { type: INSPECTOR_MESSAGES.READ, id: "r1", target: "main", format: "html" });
  assert.deepEqual(readMessage("r1", "e3", "markup"), { type: INSPECTOR_MESSAGES.READ, id: "r1", target: "e3", format: "text" }, "a format off the list is text");
  assert.deepEqual(findTextMessage("t", "price"), { type: INSPECTOR_MESSAGES.FIND_TEXT, id: "t", query: "price" });
  assert.deepEqual(snapshotMessage("s"), { type: INSPECTOR_MESSAGES.SNAPSHOT, id: "s", target: null, all: false });
  assert.deepEqual(snapshotMessage("s", "main", true), { type: INSPECTOR_MESSAGES.SNAPSHOT, id: "s", target: "main", all: true });
  assert.deepEqual(clickMessage("c", "#a"), { type: INSPECTOR_MESSAGES.CLICK, id: "c", target: "#a" });
  assert.deepEqual(fillMessage("f", "#a", "x"), { type: INSPECTOR_MESSAGES.FILL, id: "f", target: "#a", text: "x" });
  assert.deepEqual(typeMessage("k", "e2", "hats", true, true), { type: INSPECTOR_MESSAGES.TYPE, id: "k", target: "e2", text: "hats", clear: true, submit: true });
  assert.deepEqual(pressMessage("p", "Enter"), { type: INSPECTOR_MESSAGES.PRESS, id: "p", key: "Enter", target: null, modifiers: [] });
  assert.deepEqual(pressMessage("p", "a", "e1", ["shift"]), { type: INSPECTOR_MESSAGES.PRESS, id: "p", key: "a", target: "e1", modifiers: ["shift"] });
  assert.deepEqual(selectMessage("o", "#s", "eu"), { type: INSPECTOR_MESSAGES.SELECT, id: "o", target: "#s", value: "eu", label: null });
  assert.deepEqual(pointMessage("h", "e4"), { type: INSPECTOR_MESSAGES.POINT, id: "h", target: "e4" });
  assert.deepEqual(scrollMessage("w", "bottom"), { type: INSPECTOR_MESSAGES.SCROLL, id: "w", to: "bottom", byX: 0, byY: 0 });
  assert.deepEqual(scrollMessage("w", null, 0, 600), { type: INSPECTOR_MESSAGES.SCROLL, id: "w", to: null, byX: 0, byY: 600 });
  assert.deepEqual(waitMessage("z", "text", null, "Signed in", 90_000), { type: INSPECTOR_MESSAGES.WAIT, id: "z", until: "text", target: null, query: "Signed in", timeoutMs: MAX_WAIT_MS }, "a wait is held to the most");
  assert.deepEqual(waitMessage("z", "selector", "#done"), { type: INSPECTOR_MESSAGES.WAIT, id: "z", until: "selector", target: "#done", query: null, timeoutMs: 10000 });
  assert.deepEqual(consoleMessage("l", true), { type: INSPECTOR_MESSAGES.CONSOLE, id: "l", clear: true });
  assert.deepEqual(evalMessage("e", "1 + 1"), { type: INSPECTOR_MESSAGES.EVAL, id: "e", expression: "1 + 1" });
  // The answer's new fields ride bounded: a bad one is dropped, never invented.
  const rich = parseInspectorMessage({
    type: INSPECTOR_MESSAGES.ANSWER, id: "r9", ok: true, url: "http://x/", title: "",
    selector: "#a", count: 3, waitedMs: 120, scroll: { x: 0, y: 1200.4, width: 1280, height: 4800 },
    value: { n: 1 },
    dialogs: [{ kind: "confirm", message: "Sure?", answer: "true" }, { kind: "shout", message: "no" }, { kind: "alert", message: "m".repeat(MAX_LINE_CHARS + 9) }],
    console: [{ level: "error", text: "boom", at: 4 }, { level: 3, text: "x" }, "nope"],
  });
  assert.deepEqual([rich.selector, rich.count, rich.waitedMs, rich.scroll, rich.value], ["#a", 3, 120, { x: 0, y: 1200, width: 1280, height: 4800 }, { n: 1 }]);
  assert.deepEqual(rich.dialogs.map((d) => d.kind), ["confirm", "alert"], "a kind off the list is dropped");
  assert.equal(rich.dialogs[1].message.length, MAX_LINE_CHARS);
  assert.deepEqual(rich.console, [{ level: "error", text: "boom", at: 4 }], "a malformed line is dropped");
  const bare = parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "r", ok: true, url: "http://x/", title: "" });
  assert.deepEqual([bare.selector, bare.count, bare.waitedMs, bare.scroll, bare.value, bare.dialogs, bare.console], [null, null, null, null, null, [], []]);
  const huge = parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "r", ok: true, url: "http://x/", title: "", value: "v".repeat(MAX_READ_CHARS + 5), count: -1, waitedMs: 1.5 });
  assert.ok(typeof huge.value === "string" && huge.value.endsWith(CUT_TAIL), "a value too long is cut to a string");
  assert.deepEqual([huge.count, huge.waitedMs], [null, null], "a count below zero or a fraction is nothing");
  const many = parseInspectorMessage({ type: INSPECTOR_MESSAGES.ANSWER, id: "r", ok: true, url: "http://x/", title: "", dialogs: Array.from({ length: MAX_DIALOGS + 5 }, () => ({ kind: "alert", message: "m" })), console: Array.from({ length: MAX_CONSOLE_LINES + 5 }, () => ({ level: "log", text: "l", at: 0 })) });
  assert.equal(many.dialogs.length, MAX_DIALOGS);
  assert.equal(many.console.length, MAX_CONSOLE_LINES);
});

/** A page with the controls a person meets: a form with a field and a button, a link, a select, a heading. */
function storefront() {
  const page = fakeDocument();
  const said = [];
  page.window.webkit = { messageHandlers: { [BROWSER_DOOR]: { postMessage: (text) => said.push(JSON.parse(text)) } } };
  page.window.navigator = { platform: "MacIntel" };
  page.window.location = { href: "https://shop.example/" };
  page.window.Event = class FakeEvent { constructor(type, init = {}) { this.type = type; this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable; } };
  page.window.KeyboardEvent = class FakeKey extends page.window.Event { constructor(type, init = {}) { super(type, init); Object.assign(this, init); } };
  page.window.scrollX = 0; page.window.scrollY = 0;
  page.window.scrollTo = (x, y) => { page.window.scrollX = x; page.window.scrollY = y; };
  page.window.scrollBy = (x, y) => { page.window.scrollX += x; page.window.scrollY += y; };
  page.document.documentElement.scrollWidth = 1280;
  page.document.documentElement.scrollHeight = 4800;
  page.document.title = "Shop";
  const heading = page.node("h1", { text: "Welcome" });
  const nav = page.node("nav");
  const pricing = page.node("a", { text: "Pricing" });
  pricing.setAttribute("href", "/pricing");
  nav.appendChild(pricing);
  const form = page.node("form", { id: "login" });
  const email = page.node("input", { id: "email" });
  email.setAttribute("placeholder", "Email");
  email.value = "";
  email.form = form;
  const signIn = page.node("button", { id: "go", text: "Sign in" });
  form.appendChild(email);
  form.appendChild(signIn);
  const region = page.node("select", { id: "region" });
  const eu = page.node("option", { text: "Europe" });
  eu.value = "eu";
  const us = page.node("option", { text: "America" });
  us.value = "us";
  us.selected = true;
  region.appendChild(eu);
  region.appendChild(us);
  region.options = region.children;
  const note = page.node("p", { text: "Free shipping over 50." });
  for (const el of [heading, nav, form, region, note]) page.body.appendChild(el);
  const events = [];
  for (const el of [email, signIn, form, region, pricing]) {
    el.dispatchEvent = (ev) => {
      events.push([el.id || el.tagName.toLowerCase(), ev.type, ev.key ?? null]);
      return true;
    };
  }
  let submitted = 0;
  form.requestSubmit = () => {
    submitted++;
  };
  let clicks = 0;
  signIn.click = () => {
    clicks++;
  };
  // The page's `Function` makes code of the page's realm — `document` inside an
  // evaluated expression is the page's, as it is in a real webview.
  const realm = vm.createContext({ window: page.window, document: page.document });
  page.window.Function = function (...args) {
    return vm.runInContext(`(function (${args.slice(0, -1).join(", ")}) {${args.at(-1) ?? ""}})`, realm);
  };
  vm.runInContext(BROWSER_SCRIPT, realm);
  page.fire("load", {});
  const perform = page.window.__bisaBrowser.perform;
  const last = () => said.at(-1);
  return { page, said, perform, last, events, email, signIn, region, pricing, form, submitted: () => submitted, clicks: () => clicks };
}

test("a snapshot names every heading, landmark and control with a ref, and the refs drive the other tools until the page loads again", () => {
  const s = storefront();
  s.perform(snapshotMessage("s1"));
  const a = s.last();
  assert.equal(a.ok, true);
  assert.deepEqual(a.text.split("\n"), [
    'e0 heading "Welcome" (h1)',
    "e1 navigation",
    'e2 link "Pricing" → /pricing',
    "e3 form",
    'e4 textbox "Email" placeholder="Email"',
    'e5 button "Sign in"',
    'e6 combobox options=["Europe", *"America"]',
  ]);
  assert.equal(a.count, 7);
  s.perform(snapshotMessage("s2", null, true));
  assert.ok(s.last().text.includes('text "Free shipping over 50."'), "all: the text too");
  // A ref is the element a person saw.
  s.perform(clickMessage("c1", "e5"));
  assert.deepEqual([s.last().ok, s.last().selector], [true, "#go"]);
  assert.equal(s.clicks(), 1);
  s.perform(snapshotMessage("s3", "e3"));
  assert.deepEqual(s.last().text.split("\n").map((l) => l.split(" ")[1]), ["form", "textbox", "button"], "a target outlines that element and what is in it");
  s.perform(clickMessage("c2", "e99"));
  assert.match(s.last().error, /ref e99 is gone — the page changed; browser_snapshot again/);
  // A load forgets the refs: the new page's elements are not the old ones.
  s.page.fire("load", {});
  s.perform(clickMessage("c3", "e5"));
  assert.match(s.last().error, /ref e5 is gone/);
  s.perform(clickMessage("c4", ""));
  assert.match(s.last().error, /name a target/);
});

test("typing is a keystroke at a time, Enter in a form's field submits it, a key is pressed on the target or the focused element, and a fill is one act", () => {
  const s = storefront();
  s.perform(snapshotMessage("s1"));
  s.perform(typeMessage("t1", "e4", "ab", false, false));
  assert.deepEqual([s.last().ok, s.last().selector, s.last().text], [true, "#email", "ab"]);
  assert.equal(s.email.value, "ab");
  const keys = s.events.filter(([id]) => id === "email").map(([, type, key]) => `${type}:${key ?? ""}`);
  assert.deepEqual(keys.slice(0, 4), ["keydown:a", "keypress:a", "input:", "keyup:a"], "down, press, input, up — per character");
  assert.equal(s.submitted(), 0);
  s.perform(typeMessage("t2", "e4", "c", false, true));
  assert.equal(s.email.value, "abc");
  assert.equal(s.submitted(), 1, "submit: Enter after the text, and the form submits");
  s.perform(typeMessage("t3", "#email", "new", true, false));
  assert.equal(s.email.value, "new", "clear: the field is emptied first");
  s.perform(pressMessage("p1", "Enter", "#email"));
  assert.equal(s.submitted(), 2);
  assert.deepEqual([s.last().ok, s.last().selector], [true, "#email"]);
  s.perform(pressMessage("p2", "Escape"));
  assert.equal(s.last().ok, true, "no target: the focused element takes it");
  s.perform(pressMessage("p3", ""));
  assert.match(s.last().error, /name a key/);
  s.perform(fillMessage("f1", "#email", "x@y.z"));
  assert.equal(s.email.value, "x@y.z");
  assert.deepEqual(s.events.slice(-2).map(([, type]) => type), ["input", "change"], "a fill fires input and change, no keys");
  s.perform(fillMessage("f2", "#go", "x"));
  assert.deepEqual([s.last().ok, s.last().error], [false, "#go takes no typing"]);
});

test("an option is chosen by value or by label, a hover moves the pointer, a scroll answers where the page stands", () => {
  const s = storefront();
  s.perform(selectMessage("o1", "#region", "eu"));
  assert.deepEqual([s.last().ok, s.last().text], [true, 'chose "Europe" (value "eu")']);
  assert.equal(s.region.options[0].selected, true);
  assert.equal(s.region.options[1].selected, false);
  s.perform(selectMessage("o2", "#region", null, "america"));
  assert.match(s.last().text, /chose "America"/, "a label, case-folded");
  s.perform(selectMessage("o3", "#region", "asia"));
  assert.match(s.last().error, /no option with value "asia"/);
  s.perform(selectMessage("o4", "#go", "x"));
  assert.match(s.last().error, /is not a select — browser_click chooses a button/);
  s.perform(pointMessage("h1", "#go"));
  assert.deepEqual([s.last().ok, s.last().selector], [true, "#go"]);
  assert.ok(s.events.some(([id, type]) => id === "go" && type === "mouseover"), "the pointer arrives");
  s.perform(scrollMessage("w1", "bottom"));
  assert.deepEqual(s.last().scroll, { x: 0, y: 4800, width: 1280, height: 4800 });
  s.perform(scrollMessage("w2", null, 0, -600));
  assert.deepEqual(s.last().scroll, { x: 0, y: 4200, width: 1280, height: 4800 });
  s.perform(scrollMessage("w3", "top"));
  assert.equal(s.last().scroll.y, 0);
  s.perform(scrollMessage("w4", "#nowhere"));
  assert.match(s.last().error, /nothing matches #nowhere/);
});

test("a wait answers at once when its condition already holds, and says what it waited for when it cannot", () => {
  const s = storefront();
  s.perform(waitMessage("z1", "selector", "#go"));
  assert.deepEqual([s.last().ok, typeof s.last().waitedMs], [true, "number"]);
  s.perform(waitMessage("z2", "text", null, "free shipping"));
  assert.equal(s.last().ok, true, "the words are in the page, case-folded");
  s.perform(waitMessage("z3", "gone", "#nowhere"));
  assert.equal(s.last().ok, true, "an element that is not there is gone");
  // With no timer to look again, a condition that does not hold is said at once.
  s.perform(waitMessage("z4", "selector", "#later", null, 50));
  assert.equal(s.last().ok, false);
  assert.match(s.last().error, /waited \d+ ms for an element matching #later and it did not come/);
  s.perform(waitMessage("z5", "text", null, "sold out", 50));
  assert.match(s.last().error, /the words "sold out" in the page/);
});

test("the console and the errors are kept for the agent, a dialog is answered and reported with the next answer, and a script's value comes back bounded", () => {
  const s = storefront();
  s.page.window.console.log("ready", { n: 1 });
  s.page.window.console.error(new Error("boom"));
  s.page.fire("error", { message: "x is not a function", filename: "app.js", lineno: 3 });
  s.perform(consoleMessage("l1"));
  let a = s.last();
  assert.equal(a.ok, true);
  assert.deepEqual(a.console.map((l) => [l.level, l.text]), [["log", 'ready {"n":1}'], ["error", "boom"], ["uncaught", "x is not a function (app.js:3)"]]);
  assert.equal(a.count, 3);
  assert.ok(a.console.every((l) => Number.isInteger(l.at) && l.at >= 0));
  s.perform(consoleMessage("l2", true));
  s.perform(consoleMessage("l3"));
  assert.deepEqual([s.last().console, s.last().count], [[], 0], "cleared: nothing new");
  assert.match(s.last().text, /the console is empty/);
  // Dialogs: answered for the agent, reported once.
  assert.equal(s.page.window.confirm("Delete it?"), true, "a confirm is accepted");
  assert.equal(s.page.window.prompt("Name?", "Ada"), "Ada", "a prompt gets its default");
  s.page.window.alert("Saved");
  s.perform(readMessage("r1"));
  a = s.last();
  assert.deepEqual(a.dialogs, [
    { kind: "confirm", message: "Delete it?", answer: "true" },
    { kind: "prompt", message: "Name?", answer: "Ada" },
    { kind: "alert", message: "Saved", answer: null },
  ]);
  s.perform(readMessage("r2"));
  assert.equal(s.last().dialogs, undefined, "reported once");
  // A script: its value as JSON, a promise awaited, an error in words.
  s.perform(evalMessage("e1", "1 + 1"));
  assert.deepEqual([s.last().ok, s.last().value], [true, 2]);
  s.perform(evalMessage("e2", "({ title: document.title, els: [document.body], fn: function () {} })"));
  assert.deepEqual(s.last().value, { title: "Shop", els: ["<body>"], fn: "[function fn]" }, "a node and a function read as words — the function by the name the engine gives it");
  s.perform(evalMessage("e3", "nope("));
  assert.match(s.last().error, /does not parse/);
  s.perform(evalMessage("e4", "(function () { throw new Error('bad'); })()"));
  assert.match(s.last().error, /the expression threw: bad/);
  s.perform(evalMessage("e5", "undefined"));
  assert.deepEqual([s.last().ok, s.last().value], [true, null]);
});

test("a page says where it is scrolled to only once the app says a place, and goes back to the one it is told", () => {
  assert.deepEqual(placeToMessage({ top: 1200.4, left: 0 }), { type: INSPECTOR_MESSAGES.PLACE_TO, top: 1200, left: 0 });
  assert.deepEqual(placeToMessage(null), { type: INSPECTOR_MESSAGES.PLACE_TO, top: 0, left: 0 }, "nothing kept is the origin — and still said, so the page starts reporting");
  assert.deepEqual(placeToMessage({ top: -5, left: Number.NaN }), { type: INSPECTOR_MESSAGES.PLACE_TO, top: 0, left: 0 });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:place", top: 640.6, left: 12 }), { type: "bisa:place", top: 641, left: 12 });
  assert.deepEqual(parseInspectorMessage({ type: "bisa:place", top: -9, left: 0 }), { type: "bisa:place", top: 0, left: 0 }, "bounded below");
  assert.equal(parseInspectorMessage({ type: "bisa:place", top: "640", left: 0 }), null);

  const page = fakeDocument();
  const frames = [];
  Object.assign(page.window, {
    scrollX: 0,
    scrollY: 0,
    scrollTo(left, top) {
      // A page still laying out is shorter than the place it is sent to.
      page.window.scrollX = left;
      page.window.scrollY = Math.min(top, page.tall);
    },
    requestAnimationFrame(fn) {
      frames.push(fn);
      return frames.length;
    },
  });
  page.tall = 300;
  const nextFrame = () => frames.shift()?.();
  vm.runInNewContext(INSPECTOR_SCRIPT, { window: page.window, document: page.document });

  // The browser tab's pages carry this core too: nobody said a place, so a scroll there is
  // the badges' to follow and nobody's to report — nothing is posted, however far it goes.
  page.window.scrollY = 250;
  page.fire("scroll", {});
  while (frames.length) nextFrame();
  assert.equal(page.posted.filter((m) => m.type === INSPECTOR_MESSAGES.PLACE).length, 0, "no place is said until the app says one");
  page.window.scrollY = 0;

  // The IDE's frame, after a load: back to where the person was — asked again while the page grows.
  page.fromParent(placeToMessage({ top: 900, left: 0 }));
  assert.equal(page.window.scrollY, 300, "as far as the page goes for now");
  page.tall = 2000;
  nextFrame();
  assert.equal(page.window.scrollY, 900);
  assert.equal(page.posted.filter((m) => m.type === INSPECTOR_MESSAGES.PLACE).length, 0, "the app's own scroll is not the person's place");

  // The person scrolls: said once a frame, not once an event.
  page.window.scrollY = 1400;
  page.fire("scroll", {});
  page.fire("scroll", {});
  nextFrame();
  // Made in the page's own realm: compared as the JSON it is posted as.
  const said = JSON.parse(JSON.stringify(page.posted.filter((m) => m.type === INSPECTOR_MESSAGES.PLACE)));
  assert.deepEqual(said, [{ type: INSPECTOR_MESSAGES.PLACE, top: 1400, left: 0 }]);
  // The badges' listener was there from the start; a place added one, and saying one again adds none.
  const listening = page.listeners.get("scroll").length;
  assert.equal(listening, 2, "the badges' and the place's");
  page.fromParent(placeToMessage(null));
  page.fromParent(placeToMessage({ top: 10, left: 0 }));
  assert.equal(page.listeners.get("scroll").length, listening, "saying a place again adds no second listener");
});

