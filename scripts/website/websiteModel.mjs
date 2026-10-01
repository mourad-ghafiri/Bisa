/**
 * The website's rules, pure (website/README.md): how a page fragment says
 * what it is, how a token is filled and an unknown one refused, how an
 * image's size is read from its own bytes, how the repository's facts are
 * read off their sources — the step kinds and their words, the start events,
 * a template's steps, a harness's name, the decision points, the hundred
 * asks — and how the README's generated block is replaced. Also the two
 * rules the site is held to: its palette (no colour outside the platform's
 * hues) and its budgets. No I/O here: `build.mjs` reads and writes, and
 * `node --test` reads this.
 */

/** A reader that finds fewer than it must is a broken reader, never an empty page. */
export function atLeast(items, n, what) {
  if (!Array.isArray(items) || items.length < n) {
    throw new Error(`the reader of ${what} is broken: found ${Array.isArray(items) ? items.length : 0}, expected at least ${n}`);
  }
  return items;
}

/** `&`, `<`, `>`, `"` and `'` as entities — every word read from the repository passes through here. */
export function escapeHtml(text) {
  return String(text ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
}

/**
 * A page fragment opens with its facts as JSON in a comment —
 * `<!--{"path": "/", "title": "…", "description": "…"}-->` — then its body.
 * Refused without one, or with a path, title or description missing.
 */
export function parseFragment(text, name) {
  const m = /^<!--(\{[\s\S]*?\})-->\n?/.exec(String(text));
  if (!m) throw new Error(`${name}: a page fragment opens with its facts as <!--{…}-->`);
  let meta;
  try {
    meta = JSON.parse(m[1]);
  } catch (e) {
    throw new Error(`${name}: its facts are not JSON — ${e.message}`);
  }
  for (const key of ["path", "title", "description"]) {
    if (typeof meta[key] !== "string" || !meta[key].trim()) throw new Error(`${name}: its facts name no ${key}`);
  }
  return { meta, body: String(text).slice(m[0].length) };
}

const TOKEN = /\{\{([a-z][a-z0-9-]*)(?::([^{}\s]+))?\}\}/g;

/**
 * Every `{{name}}` or `{{name:arg}}` replaced by what `resolve(name, arg)`
 * answers. A token nobody answers — `resolve` returns `undefined` — stops
 * the build with its name and where it stood; a typo is never printed on the
 * site.
 */
export function fillTokens(text, resolve, where) {
  return String(text).replace(TOKEN, (whole, name, arg) => {
    const value = resolve(name, arg);
    if (value === undefined || value === null) throw new Error(`${where}: unknown token ${whole}`);
    return String(value);
  });
}

/** The tokens a text holds, as `name` or `name:arg`, in order. */
export function tokensIn(text) {
  return [...String(text).matchAll(TOKEN)].map((m) => (m[2] ? `${m[1]}:${m[2]}` : m[1]));
}

/**
 * An image's size from its own header: PNG (IHDR), JPEG (the first SOF
 * marker), WebP (VP8, VP8L or VP8X). `null` for anything else, or bytes cut
 * short — never a guess.
 * @param {Uint8Array} b
 * @returns {{type: "png" | "jpeg" | "webp", width: number, height: number} | null}
 */
export function imageSize(b) {
  if (!b || b.length < 16) return null;
  const u16be = (i) => (b[i] << 8) | b[i + 1];
  const u32be = (i) => ((b[i] << 24) >>> 0) + (b[i + 1] << 16) + (b[i + 2] << 8) + b[i + 3];
  const u24le = (i) => b[i] | (b[i + 1] << 8) | (b[i + 2] << 16);
  // PNG: the signature, then IHDR's width and height.
  if (b[0] === 0x89 && b[1] === 0x50 && b[2] === 0x4e && b[3] === 0x47) {
    if (b.length < 24) return null;
    return { type: "png", width: u32be(16), height: u32be(20) };
  }
  // JPEG: walk the markers to the first start-of-frame.
  if (b[0] === 0xff && b[1] === 0xd8) {
    let i = 2;
    while (i + 9 < b.length) {
      if (b[i] !== 0xff) return null;
      const marker = b[i + 1];
      if (marker === 0xd8 || marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) {
        i += 2;
        continue;
      }
      const length = u16be(i + 2);
      const sof = marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc;
      if (sof) return { type: "jpeg", width: u16be(i + 7), height: u16be(i + 5) };
      i += 2 + length;
    }
    return null;
  }
  // WebP: RIFF….WEBP, then the first chunk.
  const ascii = (i, n) => String.fromCharCode(...b.slice(i, i + n));
  if (ascii(0, 4) === "RIFF" && ascii(8, 4) === "WEBP" && b.length >= 30) {
    const chunk = ascii(12, 4);
    if (chunk === "VP8X") return { type: "webp", width: u24le(24) + 1, height: u24le(27) + 1 };
    if (chunk === "VP8L") {
      const bits = b[21] | (b[22] << 8) | (b[23] << 16) | (b[24] << 24);
      return { type: "webp", width: (bits & 0x3fff) + 1, height: ((bits >> 14) & 0x3fff) + 1 };
    }
    if (chunk === "VP8 ") return { type: "webp", width: (b[26] | (b[27] << 8)) & 0x3fff, height: (b[28] | (b[29] << 8)) & 0x3fff };
  }
  return null;
}

/** The one-line messages of a Fluent file, id → words; a message that runs over lines is left out. */
export function parseFtl(text) {
  const out = new Map();
  for (const line of String(text).split("\n")) {
    const m = /^([a-z][a-z0-9-]*) = (.+)$/.exec(line);
    if (m) out.set(m[1], m[2].trim());
  }
  return out;
}

/**
 * The palette's step kinds, in its order, with their words: read from
 * `desktop/src/views/_workflow/stepKinds.mjs` (the core's `StepKind::NAMES`)
 * and the catalog the desktop speaks from. A kind whose words are missing is
 * refused — the site never shows a message id.
 */
export function parseStepKinds(source, ftl) {
  const kinds = [...String(source).matchAll(/\{ kind: "([a-z_]+)", family: "([a-z]+)", label: t\("([a-z0-9-]+)"\), explain: t\("([a-z0-9-]+)"\) \}/g)].map((m) => {
    const label = ftl.get(m[3]);
    const explain = ftl.get(m[4]);
    if (!label || !explain) throw new Error(`the step kind ${m[1]} has no words in the catalog (${!label ? m[3] : m[4]})`);
    return { kind: m[1], family: m[2], label, explain };
  });
  return atLeast(kinds, 18, "step kinds");
}

/** The ways a run begins, in the palette's order, with their words. */
export function parseStartEvents(source, ftl) {
  const block = String(source).split("export const START_EVENTS")[1]?.split("];")[0] ?? "";
  const events = [...block.matchAll(/\{ event: "([a-z_]+)", label: t\("([a-z0-9-]+)"\) \}/g)].map((m) => {
    const label = ftl.get(m[2]);
    if (!label) throw new Error(`the start event ${m[1]} has no words in the catalog (${m[2]})`);
    return { event: m[1], label };
  });
  return atLeast(events, 10, "start events");
}

/** A string value of a TOML line `key = "…"`, unescaped as far as the catalog needs. */
const tomlString = (line, key) => {
  const m = new RegExp(`^${key} = "((?:[^"\\\\]|\\\\.)*)"`).exec(line);
  return m ? m[1].replace(/\\"/g, '"').replace(/\\\\/g, "\\") : null;
};

/**
 * A catalog workflow: its name, its description and its steps in order —
 * each `[[workflow.steps]]` table's `id`, `name` and `kind`.
 */
export function parseWorkflowToml(text) {
  let name = null;
  let description = null;
  const steps = [];
  let section = "";
  let step = null;
  for (const raw of String(text).split("\n")) {
    const line = raw.trim();
    const table = /^\[\[?([^\]]+)\]\]?$/.exec(line);
    if (table) {
      section = table[1];
      if (line === "[[workflow.steps]]") {
        step = {};
        steps.push(step);
      } else if (!section.startsWith("workflow.steps")) {
        step = null;
      }
      continue;
    }
    if (section === "workflow") {
      name ??= tomlString(line, "name");
      description ??= tomlString(line, "description");
    } else if (section === "workflow.steps" && step) {
      for (const key of ["id", "name", "kind"]) step[key] ??= tomlString(line, key);
    }
  }
  if (!name) throw new Error("a catalog workflow names itself under [workflow]");
  for (const s of steps) if (!s.id || !s.name || !s.kind) throw new Error(`${name}: a step lacks its id, name or kind`);
  return { name, description, steps: atLeast(steps, 2, `${name}'s steps`) };
}

/**
 * The string and string-list values of one table of a TOML file —
 * `[agent]`, `[team]`, `[connector]` — as the catalog writes them: one
 * `key = "…"` or `key = ["…", "…"]` per line. A multi-line value (a prompt)
 * is not read. Empty when the table is not there.
 * @returns {Record<string, string | string[]>}
 */
export function tomlTable(text, table) {
  const out = {};
  let inside = false;
  for (const raw of String(text).split("\n")) {
    const line = raw.trim();
    const header = /^\[\[?([^\]]+)\]\]?$/.exec(line);
    if (header) {
      inside = header[1] === table && !line.startsWith("[[");
      continue;
    }
    if (!inside) continue;
    const kv = /^([a-z_]+) = (.+)$/.exec(line);
    if (!kv || kv[1] in out) continue;
    const value = kv[2].trim();
    if (value.startsWith('"') && !value.startsWith('"""')) {
      const str = tomlString(line, kv[1]);
      if (str !== null) out[kv[1]] = str;
    } else if (value.startsWith("[") && value.endsWith("]")) {
      out[kv[1]] = [...value.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]);
    }
  }
  return out;
}

/** The first sentence of a description — up to its first full stop followed by a space, or all of it. */
export function firstSentence(text) {
  const s = String(text ?? "").trim();
  const m = /^(.+?[.!?])(\s|$)/.exec(s);
  return m ? m[1] : s;
}

/**
 * The website's worked example held to what ships: every step's kind is one
 * of the palette's, every agent one of the catalog's, every skill one that
 * agent carries, every id once. Refused in words otherwise — the example can
 * only name what a person could really get.
 * @param {{steps: any[]}} example
 * @param {{kinds: Set<string>, agents: Map<string, {skills?: string[]}>}} catalog
 */
export function checkExample(example, { kinds, agents }) {
  const steps = Array.isArray(example?.steps) ? example.steps : [];
  atLeast(steps, 2, "the example's steps");
  const ids = new Set();
  for (const s of steps) {
    if (!s.id || ids.has(s.id)) throw new Error(`the example has a step with no id, or ${s.id} twice`);
    ids.add(s.id);
    if (!s.name) throw new Error(`the example's step ${s.id} has no name`);
    if (!kinds.has(s.kind)) throw new Error(`the example's step ${s.id} is a ${s.kind}, which is no kind of the designer's`);
    if (s.agent !== undefined) {
      const agent = agents.get(s.agent);
      if (!agent) throw new Error(`the example's step ${s.id} names the agent ${s.agent}, which the catalog does not ship`);
      for (const skill of s.skills ?? []) {
        if (!(agent.skills ?? []).includes(skill)) throw new Error(`the example's step ${s.id} gives ${s.agent} the skill ${skill}, which it does not carry`);
      }
    }
  }
  return example;
}

/** What an adapter's `fn display_name` answers — the harness's own name, as the app shows it. */
export function displayName(rustSource) {
  const m = /fn display_name\(&self\) -> &(?:'static )?str \{\s*"([^"]+)"/.exec(String(rustSource));
  return m ? m[1] : null;
}

/** A preset harness's label (`PRESET_HARNESSES` in the harness catalog), by its id without `preset:`. */
export function presetLabel(rustSource, id) {
  const m = new RegExp(`id: "preset:${id}",\\s*label: "([^"]+)"`).exec(String(rustSource));
  return m ? m[1] : null;
}

/** How many decision points the core declares (`DecisionPoint::ALL`). */
export function decisionPointCount(rustSource) {
  const m = /pub const ALL: \[DecisionPoint; (\d+)\]/.exec(String(rustSource));
  return m ? Number(m[1]) : null;
}

/** The minimum macOS the bundle declares, as a person reads it: `11.0` → `11`. */
export function macosVersion(tauriConf) {
  const v = JSON.parse(tauriConf)?.bundle?.macOS?.minimumSystemVersion;
  if (typeof v !== "string" || !/^\d+(\.\d+)*$/.test(v)) throw new Error("tauri.conf.json names no bundle.macOS.minimumSystemVersion");
  return v.replace(/\.0$/, "");
}

/** The three words a scenario row's *Today* column may say. */
export const TODAY = Object.freeze(["runs", "with you", "custom connector"]);

/**
 * The hundred asks of `docs/guide/real-world-scenarios.md`: each row's id,
 * its section's title, the person's sentence and the *Today* word — markdown
 * emphasis and code marks taken off the sentence, nothing else changed.
 */
export function parseScenarios(markdown) {
  const rows = [];
  let section = null;
  for (const line of String(markdown).split("\n")) {
    const heading = /^## [A-Z]\. (.+)$/.exec(line);
    if (heading) {
      section = heading[1].trim();
      continue;
    }
    const row = /^\| ([A-Z]\d+) \| (.+?) \| .+ \| ([^|]+) \|\s*$/.exec(line);
    if (!row || !section) continue;
    const today = row[3].replace(/[*_]/g, "").trim().toLowerCase();
    const word = TODAY.find((w) => today.startsWith(w));
    if (!word) throw new Error(`scenario ${row[1]}: its Today column says "${row[3].trim()}", not one of ${TODAY.join(" · ")}`);
    rows.push({ id: row[1], section, ask: row[2].replace(/[*_`]/g, "").trim(), today: word });
  }
  return atLeast(rows, 90, "scenarios");
}

/** How many asks say each of the three words. */
export function tally(asks) {
  const out = Object.fromEntries(TODAY.map((w) => [w, 0]));
  for (const a of asks) out[a.today] += 1;
  return out;
}

/**
 * The text between `start` and `end` replaced by `content`, the markers
 * kept; everything else byte for byte. Refused when either marker is
 * missing or out of order — a README without its markers is never
 * rewritten.
 */
export function replaceBlock(text, start, end, content) {
  const s = String(text);
  const a = s.indexOf(start);
  const b = s.indexOf(end);
  if (a < 0 || b < 0 || b < a) throw new Error(`the markers ${start} … ${end} are not both there, in order`);
  return `${s.slice(0, a + start.length)}\n${content}\n${s.slice(b)}`;
}

/**
 * The platform's hues: the Glass blue, the logo's cyan and its blues — its
 * navy sits at 260 — between 180 and 262, and the three status hues — ok
 * 155, warn 90, danger 22 — each within 12°. Dune's indigo (265) and the
 * violet accent (295) fall outside. A colour with chroma under 0.03 is a
 * grey and passes whatever its hue.
 */
export const PALETTE = Object.freeze({ band: [180, 262], status: [22, 90, 155], spread: 12, grey: 0.03 });

/**
 * Every colour a stylesheet writes that the palette refuses: one that is not
 * `oklch()` (a hex, `rgb()`, `hsl()`, `hwb()`, `lab()`, `lch()`), and an
 * `oklch()` whose hue is outside the platform's. Purple — the violet accent
 * at 295, Dune's indigo at 265 — cannot enter.
 * @returns {string[]} what is refused, in words
 */
export function paletteOffences(css) {
  const text = String(css).replace(/\/\*[\s\S]*?\*\//g, "");
  const out = [];
  for (const m of text.matchAll(/#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch)\(/g)) {
    // A `url(#id)` fragment is an address, never a colour.
    if (m[0].startsWith("#") && /url\(\s*$/.test(text.slice(Math.max(0, m.index - 6), m.index))) continue;
    out.push(`${m[0]} — write colours as oklch()`);
  }
  for (const m of text.matchAll(/oklch\(\s*([\d.]+)%?\s+([\d.]+)\s+([\d.]+)/g)) {
    const c = Number(m[2]);
    const h = Number(m[3]);
    if (c < PALETTE.grey) continue;
    const inBand = h >= PALETTE.band[0] && h <= PALETTE.band[1];
    const status = PALETTE.status.some((s) => Math.min(Math.abs(h - s), 360 - Math.abs(h - s)) <= PALETTE.spread);
    if (!inBand && !status) out.push(`${m[0]}…) — hue ${h} is outside the platform's palette`);
  }
  return out;
}

/**
 * A page's addresses written from where it stands: every root-absolute `href`
 * and `src` (`/features/`, `/assets/site.css`) made relative by `prefix` —
 * `./` at the root, `../` a folder down — so a page reads whole from a host
 * and straight from the disk alike. `/` itself is the prefix. A full address
 * (`https://…`, `//…`) and an anchor are left as they are.
 */
export function relativeUrls(html, prefix) {
  return String(html).replace(/\b(href|src)="\/(?!\/)([^"]*)"/g, (_, attr, rest) => `${attr}="${prefix}${rest}"`);
}

/** How far down the root a page's address is, as the prefix its links need: `/` → `./`, `/features/` → `../`. */
export function prefixFor(path) {
  const depth = String(path).split("/").length - 2;
  return depth <= 0 ? "./" : "../".repeat(depth);
}

/**
 * The served script: the rail's rules and the behaviours in one plain script —
 * no module, so it runs from a `file:` address too. The rules' `export`s
 * become plain declarations and the behaviours' import of them goes; both
 * run in one function's scope. Refused when the behaviours no longer import
 * the rules the way this expects.
 */
export function bundleScript(modelSource, siteSource) {
  const importLine = /^import \{[^}]+\} from "\.\/railModel\.mjs";\n/m;
  if (!importLine.test(siteSource)) throw new Error('site.src.js imports its rules as `import { … } from "./railModel.mjs";` — the bundle cannot find that line');
  const model = String(modelSource).replace(/^export (const|function) /gm, "$1 ");
  if (/^export\b/m.test(model)) throw new Error("railModel.mjs exports something other than a const or a function");
  const site = String(siteSource).replace(importLine, "");
  return `/* Built by scripts/website/build.mjs from railModel.mjs and site.src.js — edit those, never this. */\n(() => {\n"use strict";\n${model}\n${site}})();\n`;
}

/** The site's budgets, in bytes: what a visitor downloads before anything else. */
export const BUDGET = Object.freeze({ css: 64 * 1024, js: 16 * 1024, page: 160 * 1024, screenshot: 2 * 1024 * 1024, screenshotWarn: 600 * 1024 });

/** The screenshot extensions the build looks for, in the order it prefers them. */
export const SHOT_EXTENSIONS = Object.freeze(["webp", "jpg", "png"]);

/**
 * A screenshot made for the web: WebP, which keeps a window capture's transparent shadow, at most
 * twice as wide as the widest the site shows one (about 1000 CSS pixels), at a quality where the
 * app's smallest text reads as in the capture.
 */
export const WEB_SHOT = Object.freeze({ width: 2000, quality: 88, alphaQuality: 90 });

/**
 * The words `cwebp` (Google's WebP encoder, `cwebp -longhelp`) is run with for one screenshot:
 * the slowest, smallest method, the sharper colour conversion text needs, no metadata, and a
 * resize only when the capture is wider than the web needs — never an enlargement.
 */
export function cwebpArgs(input, output, width) {
  const resize = width > WEB_SHOT.width ? ["-resize", String(WEB_SHOT.width), "0"] : [];
  return ["-quiet", "-q", String(WEB_SHOT.quality), "-alpha_q", String(WEB_SHOT.alphaQuality), "-m", "6", "-sharp_yuv", "-metadata", "none", ...resize, input, "-o", output];
}

/**
 * The README's slideshow: the website's opening, as one animated WebP a page with no script can show —
 * each screen a frame, as wide as the README shows it twice over, held a few seconds, looping.
 */
export const README_SLIDESHOW = Object.freeze({ width: 1280, quality: 82, holdMs: 3000, image: "website/assets/readme-slideshow.webp", frames: "website/assets/readme-slideshow.json" });

/**
 * The frames the README's slideshow holds: for each screen of the opening, the first of its
 * screenshots that is here — with its weight, so a screenshot that changed makes the slideshow stale.
 * A screen with none is left out: an animation has no outlined placeholder.
 * @param {{label: string, shots: string[]}[]} slides the home page's `slides`
 * @param {(id: string) => number | null} weight a screenshot's bytes, or null when it is not here
 */
export function slideshowFrames(slides, weight) {
  return (slides ?? []).flatMap((s) => {
    const id = (s.shots ?? []).find((shot) => weight(shot) !== null);
    return id === undefined ? [] : [{ label: s.label, id, bytes: weight(id) }];
  });
}

/** The words `img2webp` is run with: looping for ever, lossy, each frame held, written to `output`. */
export function img2webpArgs(framePaths, output) {
  const each = framePaths.flatMap((path) => ["-d", String(README_SLIDESHOW.holdMs), path]);
  return ["-loop", "0", "-sharp_yuv", "-lossy", "-q", String(README_SLIDESHOW.quality), "-m", "6", ...each, "-o", output];
}
