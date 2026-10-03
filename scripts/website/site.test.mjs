/**
 * bisa.dev as it is committed (website/README.md): built from its sources
 * and nothing else; every page with one heading, its title, description,
 * canonical address, social tags and policy; every link and anchor
 * resolving; nothing loaded from another host; every image sized and
 * described; the sitemap and robots.txt in step with the pages; the palette
 * the platform's, the text legible on every surface; the budgets kept; the
 * slogan where it belongs; and none of the words the project does not say.
 * Run with `node --test scripts/website/site.test.mjs`.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BUDGET, README_SLIDESHOW, WEB_SHOT, imageSize, paletteOffences, parseFragment, slideshowFrames } from "./websiteModel.mjs";
import { composite, contrast } from "../../desktop/src/theme/contrast.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const site = join(root, "website");
const read = (rel) => readFileSync(join(root, rel), "utf8");
const SITE = "https://bisa.dev/";
const SLOGAN = "A Decentralized Agentic IDE";
const REPO = /^repository = "([^"]+)"/m.exec(read("Cargo.toml").split("[workspace.package]")[1])[1];

/** Every page under website/, by its address. */
function pages() {
  const out = [];
  const walk = (dir) => {
    for (const e of readdirSync(join(site, dir), { withFileTypes: true })) {
      const rel = dir ? `${dir}/${e.name}` : e.name;
      if (e.isDirectory()) walk(rel);
      else if (e.name.endsWith(".html")) out.push({ file: rel, path: rel === "index.html" ? "/" : rel.endsWith("/index.html") ? `/${rel.slice(0, -"index.html".length)}` : `/${rel}`, html: readFileSync(join(site, rel), "utf8") });
    }
  };
  walk("");
  return out.sort((a, b) => a.path.localeCompare(b.path));
}
const PAGES = pages();
const page = (path) => PAGES.find((p) => p.path === path);
const meta = (html, attr, name) => new RegExp(`<meta ${attr}="${name}" content="([^"]*)"`).exec(html)?.[1];
const text = (html) => html.replace(/<script[\s\S]*?<\/script>/g, "").replace(/<[^>]+>/g, " ").replace(/&amp;/g, "&").replace(/&#39;/g, "'").replace(/&quot;/g, '"').replace(/\s+/g, " ");
const ids = (html) => [...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);

test("the site is what scripts/website/ builds — nothing edited by hand, nothing left behind", () => {
  const run = spawnSync(process.execPath, [join(root, "scripts/website/build.mjs"), "--check"], { cwd: root, encoding: "utf8" });
  assert.equal(run.status, 0, run.stderr || run.stdout);
  assert.deepEqual(PAGES.map((p) => p.path), ["/", "/404.html", "/download/", "/features/", "/harnesses/", "/security/"]);
});

test("every page has one heading, a title and a description of its own, its canonical address and the social tags", () => {
  const titles = new Set();
  const descriptions = new Set();
  for (const { path, html } of PAGES) {
    assert.equal((html.match(/<h1[\s>]/g) ?? []).length, 1, `${path}: one h1`);
    assert.match(html, /^<!doctype html>\n<html lang="en"/, `${path}: a document in English`);
    const title = /<title>([^<]+)<\/title>/.exec(html)?.[1];
    assert.ok(title && title.length >= 10 && title.length <= 70, `${path}: a title of 10 to 70 characters — ${title}`);
    const description = meta(html, "name", "description");
    assert.ok(description && description.length >= 70 && description.length <= 170, `${path}: a description of 70 to 170 characters — ${description?.length}`);
    titles.add(title);
    descriptions.add(description);
    assert.equal(/<link rel="canonical" href="([^"]+)">/.exec(html)?.[1], `${SITE}${path.slice(1)}`, `${path}: its canonical address`);
    for (const p of ["og:type", "og:title", "og:description", "og:url", "og:image", "og:image:width", "og:image:height", "og:image:alt"]) assert.ok(meta(html, "property", p), `${path}: ${p}`);
    assert.equal(meta(html, "property", "og:url"), `${SITE}${path.slice(1)}`);
    assert.equal(meta(html, "name", "twitter:card"), "summary_large_image");
    assert.ok(!/\sstyle="/.test(html), `${path}: no style attribute — the look is the stylesheet's`);
    for (const [tag] of html.matchAll(/<script\b[^>]*>/g)) assert.ok(/^<script src="(\.\.?\/|\/)assets\/site\.js" defer>$|type="application\/ld\+json"/.test(tag), `${path}: one plain deferred script and no inline one — ${tag}`);
    const dupes = ids(html).filter((id, i, all) => all.indexOf(id) !== i);
    assert.deepEqual(dupes, [], `${path}: every id once`);
  }
  assert.equal(titles.size, PAGES.length, "every title its own");
  assert.equal(descriptions.size, PAGES.length, "every description its own");
  assert.match(page("/404.html").html, /<meta name="robots" content="noindex">/);
});

test("the home page says what Bisa is to a search engine — a site and a free application, with no rating nobody gave", () => {
  const { html } = page("/");
  const blocks = [...html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)].map((m) => JSON.parse(m[1]));
  assert.equal(blocks.length, 1);
  const [website, app] = blocks[0];
  assert.deepEqual(website, { "@context": "https://schema.org", "@type": "WebSite", name: "Bisa", url: SITE });
  assert.equal(app["@type"], "SoftwareApplication");
  assert.equal(app.applicationCategory, "DeveloperApplication");
  assert.deepEqual(app.offers, { "@type": "Offer", price: "0", priceCurrency: "USD" });
  assert.match(app.operatingSystem, /^macOS \d+(\.\d+)? or later$/);
  assert.equal(app.downloadUrl, `${REPO}/releases/latest`);
  for (const key of ["aggregateRating", "review", "reviews"]) assert.ok(!(key in app), `no ${key} — none exists`);
  for (const p of PAGES) assert.ok(!/aggregateRating|"@type":\s*"Review"/.test(p.html), `${p.path}: no rating or review — none exists`);
  for (const p of PAGES.filter((x) => x.path !== "/")) assert.ok(!p.html.includes("application/ld+json"), `${p.path}: the site's facts live on the home page alone`);
});

test("every link inside the site resolves to a page or a file — written from where the page stands, the 404 page's from the root — and every anchor to an id there", () => {
  /** The site's own address a link reaches from a page: `../download/#build` on /features/ → /download/#build. */
  const reach = (from, url) => new URL(url, `https://bisa.dev${from}`).pathname + new URL(url, `https://bisa.dev${from}`).hash;
  const exists = (address) => {
    const clean = address.split("#")[0];
    return clean.endsWith("/") ? existsSync(join(site, clean.slice(1), "index.html")) : existsSync(join(site, clean.slice(1)));
  };
  for (const { path, html } of PAGES) {
    for (const [, url] of html.matchAll(/(?:href|src)="([^"]+)"/g)) {
      if (/^(https?:|mailto:)/.test(url)) continue;
      if (url.startsWith("#")) {
        assert.ok(ids(html).includes(url.slice(1)), `${path}: ${url} names an id on the page`);
        continue;
      }
      if (path === "/404.html") assert.ok(url.startsWith("/"), `${path}: ${url} is root-absolute — the 404 page is served at any depth`);
      else assert.ok(!url.startsWith("/"), `${path}: ${url} is written from where the page stands, so the page reads whole from the disk too`);
      const address = reach(path, url);
      assert.ok(exists(address), `${path}: ${url} resolves (${address})`);
      if (address.includes("#")) {
        const [target, anchor] = address.split("#");
        const on = PAGES.find((p) => p.path === target);
        assert.ok(on && ids(on.html).includes(anchor), `${path}: ${url} names an id on ${target}`);
      }
    }
    for (const [, url] of html.matchAll(/href="(https:\/\/github\.com\/[^"]+)"/g)) assert.ok(url.startsWith(REPO), `${path}: a GitHub link is the declared repository's — ${url}`);
  }
});

test("nothing is loaded from another host: scripts, styles, icons, images and fonts are the site's own", () => {
  for (const { path, html } of PAGES) {
    assert.ok(!/<script[^>]+src="(https?:)?\/\//.test(html), `${path}: no script from elsewhere`);
    assert.ok(!/<link[^>]+href="(https?:)?\/\/[^"]+"[^>]*>/.test(html.replace(/<link rel="canonical"[^>]+>/, "")), `${path}: no stylesheet, icon or manifest from elsewhere`);
    assert.ok(!/<(img|source|iframe|video|audio)[^>]+src="(https?:)?\/\//.test(html), `${path}: no media from elsewhere`);
    assert.ok(!/<iframe/.test(html), `${path}: no frame`);
  }
  const css = read("website/assets/site.css");
  assert.ok(![...css.matchAll(/url\(\s*["']?([^"')]+)/g)].some((m) => /^(https?:)?\/\//.test(m[1])), "the stylesheet reaches nothing outside");
  assert.ok(!/@import/.test(css), "and imports nothing");
  const js = read("website/assets/site.js");
  assert.ok(!/fetch\(|XMLHttpRequest|sendBeacon|WebSocket|document\.cookie|localStorage/.test(js), "the script calls no one, sets no cookie and keeps nothing");
});

test("every image is described and sized, and every screenshot of the manifest has its place on a page", () => {
  const manifest = JSON.parse(read("website/screenshots/manifest.json"));
  const shots = manifest.shots.map((s) => s.id);
  assert.equal(new Set(shots).size, shots.length, "every screenshot named once");
  const all = PAGES.map((p) => p.html).join("\n");
  for (const { path, html } of PAGES) {
    for (const [tag] of html.matchAll(/<img\b[^>]*>/g)) {
      assert.match(tag, /\salt="[^"]*"/, `${path}: ${tag} has an alt`);
      assert.match(tag, /\swidth="\d+"/, `${path}: ${tag} has a width`);
      assert.match(tag, /\sheight="\d+"/, `${path}: ${tag} has a height`);
    }
  }
  for (const id of shots) assert.ok(all.includes(`id="shot-${id}"`) || all.includes(`id="slide-${id}"`), `the screenshot ${id} has a place on the site`);
  for (const s of manifest.shots) assert.ok(s.alt && s.title && s.capture, `${s.id}: an alt, a title and what to capture`);
  for (const f of readdirSync(join(root, "website/screenshots"))) {
    if (["manifest.json", "placeholder.svg", "README.md"].includes(f) || f.startsWith(".")) continue;
    const m = /^(.+)\.(webp|jpg|png)$/.exec(f);
    assert.ok(m && shots.includes(m[1]), `website/screenshots/${f} is a screenshot the manifest names — the build leaves it out until it is`);
    // Made for the web: a visitor downloads every one of them as they scroll.
    const bytes = readFileSync(join(root, "website/screenshots", f));
    const size = imageSize(bytes);
    assert.ok(bytes.length <= BUDGET.screenshotWarn && size && size.width <= WEB_SHOT.width, `website/screenshots/${f} weighs ${Math.round(bytes.length / 1024)} KB at ${size?.width} px wide — run \`just website-shots\``);
  }
  for (const { path, html } of PAGES) {
    for (const [fig] of html.matchAll(/<figure class="shot[^"]*"[\s\S]*?<\/figure>/g)) {
      const shown = /<img /.test(fig);
      assert.equal(/class="window"/.test(fig), !shown, `${path}: a screenshot stands bare — it carries its own title bar — and only one still to come is outlined: ${fig.slice(0, 120)}`);
    }
  }
  const readme = read("README.md");
  // The overview under the slogan is required; the gallery and the IDE blocks are checked only where the README carries them.
  assert.ok(readme.includes("<!-- website:overview"), "README: the overview block under the slogan");
  for (const name of ["overview", "screenshots", "ide"].filter((n) => readme.includes(`<!-- website:${n}`))) {
    const block = readme.split(`<!-- website:${name}`)[1].split(`<!-- /website:${name} -->`)[0];
    assert.match(block, /<img src="/, `README: the ${name} block holds its screenshots`);
    for (const [, src] of block.matchAll(/<img src="([^"]+)"/g)) assert.ok(existsSync(join(root, src)), `README: ${src} is there — never a broken image`);
  }
  assert.ok(readme.indexOf("<!-- website:overview") > readme.indexOf("**A Decentralized Agentic IDE.**"), "README: the overview follows the slogan");
  // The README's slideshow is the website's opening: the same screens, in the same order, made from the screenshots that are here now.
  if (existsSync(join(root, README_SLIDESHOW.frames))) {
    const slides = parseFragment(read("scripts/website/pages/home.html"), "home.html").meta.slides;
    const weight = (id) => (existsSync(join(root, "website/screenshots", `${id}.webp`)) ? statSync(join(root, "website/screenshots", `${id}.webp`)).size : null);
    assert.deepEqual(JSON.parse(read(README_SLIDESHOW.frames)).frames, slideshowFrames(slides, weight), "the README's slideshow is behind its screens — run `just website-shots`");
    const overview = readme.split("<!-- website:overview")[1].split("<!-- /website:overview -->")[0];
    assert.ok(overview.includes(`<img src="${README_SLIDESHOW.image}"`), "README: the slideshow stands under the slogan");
  }
});

test("the sitemap lists exactly the pages to index, at their canonical addresses, and robots.txt names it", () => {
  const sitemap = read("website/sitemap.xml");
  const listed = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]).sort();
  const expected = PAGES.filter((p) => !p.html.includes('content="noindex"')).map((p) => `${SITE}${p.path.slice(1)}`).sort();
  assert.deepEqual(listed, expected);
  assert.ok(!/<priority>|<changefreq>/.test(sitemap), "nothing Google ignores");
  assert.match(read("website/robots.txt"), new RegExp(`^Sitemap: ${SITE}sitemap\\.xml$`, "m"));
});

/** The custom properties of the first block matching `selector` in the stylesheet. */
function tokens(css, opener) {
  const at = css.indexOf(opener);
  assert.ok(at >= 0, `${opener} is in the stylesheet`);
  const body = css.slice(at, css.indexOf("}", at));
  return Object.fromEntries([...body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

test("the palette is the platform's — blues, cyans and the status hues, never purple — and every text is legible on every surface", () => {
  const css = read("website/assets/site.css");
  assert.deepEqual(paletteOffences(css), []);
  const light = tokens(css, ":root {");
  const dark = { ...light, ...tokens(css, "@media (prefers-color-scheme: dark) {\n  :root {") };
  for (const [scheme, t] of [["light", light], ["dark", dark]]) {
    const ground = t.page;
    const surfaces = { page: ground, surface: composite(t["color-surface"], ground), solid: t["color-surface-solid"] };
    for (const [name, under] of Object.entries(surfaces)) {
      for (const role of ["color-text", "color-text-dim", "brand-cyan-ink", "color-accent-ink"]) {
        const ratio = contrast(t[role], under);
        assert.ok(ratio >= 4.5, `${scheme}: ${role} on ${name} is ${ratio.toFixed(2)}:1, under 4.5`);
      }
    }
    assert.ok(contrast(t["color-accent-contrast"], t["color-accent"]) >= 4.5, `${scheme}: the primary button's words on its accent`);
  }
});

test("the site keeps its budgets: a stylesheet, a script and pages a visitor downloads at once", () => {
  assert.ok(statSync(join(site, "assets/site.css")).size <= BUDGET.css, "the stylesheet");
  const js = statSync(join(site, "assets/site.js")).size;
  assert.ok(js <= BUDGET.js, `the script, ${js} bytes`);
  for (const p of PAGES) assert.ok(Buffer.byteLength(p.html) <= BUDGET.page, `${p.path} weighs ${Buffer.byteLength(p.html)} bytes`);
});

test("the slogan opens the home page, names it, and closes every page — and the README says it too", () => {
  const { html } = page("/");
  assert.equal(text(/<h1[^>]*>([\s\S]*?)<\/h1>/.exec(html)[1]).trim(), SLOGAN);
  assert.ok(/<title>[^<]*A Decentralized Agentic IDE[^<]*<\/title>/.test(html));
  for (const p of PAGES) assert.ok(p.html.includes(`<p>${SLOGAN}.</p>`), `${p.path}: the footer says it`);
  assert.ok(read("README.md").includes(`**${SLOGAN}.**`));
  // The name is not glossed and carries no second tagline: *bisa* already means *can*.
  for (const [where, words] of [...PAGES.map((p) => [p.path, text(p.html)]), ["README.md", read("README.md")]]) {
    assert.ok(!/Bisa can\b|in Indonesian|Whatever you want/i.test(words), `${where}: the name is explained, or the old tagline is back`);
  }
  assert.ok(text(html).includes("Linux and Windows coming soon"), "where it runs today, and what comes next");
});

test("the tour follows the app's sidebar, and its rail names every stop in that order", () => {
  const { html } = page("/");
  const order = ["inbox", "agents", "teams", "projects", "workflows", "goals", "pulse", "channels", "direct-messages", "settings", "more", "your-move"];
  assert.deepEqual([...html.matchAll(/<section class="tour[^"]*" id="([a-z-]+)" data-step="(\d+)"/g)].map((m) => [m[1], Number(m[2])]), order.map((id, i) => [id, i]));
  assert.deepEqual([...html.matchAll(/<a href="#([a-z-]+)" data-rail="(\d+)"/g)].map((m) => [m[1], Number(m[2])]), order.map((id, i) => [id, i]));
  for (const id of ["collaboration", "connectors", "notes", "drawing", "pets", "security", "decision-making", "addons"]) assert.ok(ids(html).includes(id), `the box holds ${id}`);
});

test("the served script is one plain script — no module, no import, nothing it fetches — so the site runs from the disk too", () => {
  const js = read("website/assets/site.js");
  assert.ok(js.startsWith("/* Built by scripts/website/build.mjs"), "built, and says so");
  assert.ok(!/^\s*(import|export)\b/m.test(js), "no module syntax");
  assert.ok(!existsSync(join(site, "assets/railModel.mjs")), "the rules are bundled, never served on their own");
  for (const p of PAGES) assert.ok(!/type="module"/.test(p.html), `${p.path}: no module script`);
});

test("the opening shows the app screen by screen", () => {
  const { html } = page("/");
  const hero = html.split('<section class="hero"')[1].split("</section>")[0];
  const slides = [...hero.matchAll(/<figure class="slide( is-missing)?" id="slide-([^"]+)"[^>]*>([\s\S]*?)<\/figure>/g)];
  assert.ok(slides.length >= 2, "a slideshow, not one screen");
  for (const [, missing, id, inner] of slides) {
    if (missing) assert.match(inner, new RegExp(`<code>website/screenshots/${id}\\.png</code>`), `the ${id} slide, still to come, names the file to add`);
    else assert.match(inner, /<img src="\.\/screenshots\//, `the ${id} slide is its screenshot`);
  }
  assert.ok(slides.some(([, missing, id]) => id === "ide" && !missing), "Projects has its slide: the IDE's own screenshot");
  assert.ok(!/slides-pause|slides-tabs|data-slide=/.test(hero), "the slideshow plays by itself — no Pause, no tabs");
  const js = read("website/assets/site.js");
  assert.ok(js.includes('"Previous screen"') && js.includes('"Next screen"'), "two arrows, back and on, named for a screen reader");
});

test("the site draws no screen of the app — the app is shown by its screenshots alone", () => {
  for (const { path, html } of PAGES) {
    for (const drawn of ['class="designer', 'class="capture', 'class="canvas', 'data-designer', 'data-capture']) assert.ok(!html.includes(drawn), `${path}: ${drawn} — a screen drawn on the site`);
  }
  const css = read("website/assets/site.css");
  for (const drawn of [".designer {", ".capture {", ".canvas {", ".node {"]) assert.ok(!css.includes(drawn), `the stylesheet styles ${drawn}`);
});

test("the site shows no command to type — the app is presented by its screens and its words", () => {
  for (const { path, html } of PAGES) {
    assert.ok(!/<pre\b/.test(html), `${path}: a block of commands`);
    assert.ok(!/data-copy|In a terminal</.test(html), `${path}: a command panel`);
  }
});

test("the site names no kinds — of a step, or of what a relay carries", () => {
  for (const { path, html } of PAGES) {
    const words = html.replace(/<script\b[\s\S]*?<\/script>/g, "").replace(/\sclass="[^"]*"/g, "");
    const said = /\bkinds?\b|1059/i.exec(words);
    assert.ok(!said, `${path}: says “${said?.[0]}” — ${said && words.slice(Math.max(0, said.index - 60), said.index + 40)}`);
  }
});

test("the agents and the teams of the catalog are each folded away under one line, never listed on the page", () => {
  const home = PAGES.find((p) => p.path === "/").html;
  for (const [what, list] of [["agent", 'class="gallery"'], ["team", 'class="teams"']]) {
    const fold = new RegExp(`<details class="more-list">\\s*<summary>Every ${what} in the catalog</summary>\\s*<(ul|div) ${list}`);
    assert.match(home, fold, `every ${what} of the catalog sits under its own fold`);
    assert.equal(home.split(list).length - 1, 1, `the ${what}s are listed once`);
  }
});

test("none of the words the project does not say", () => {
  const banned = /\b(autonomous|hands-off|telemetry|plugins?|whiteboard|triggers?|triggered|chats?)\b/i;
  // The hundred asks are people's own sentences, quoted from the scenarios guide as they are written.
  const ours = (html) => text(html.replace(/<div class="asks"[\s\S]*?<\/div>\n<p class="fine">[\s\S]*?<\/div>/, ""));
  const files = [...PAGES.map((p) => [p.path, ours(p.html)]), ["site.js", read("website/assets/site.js")], ["manifest", read("website/screenshots/manifest.json")]];
  for (const [name, body] of files) {
    assert.equal(banned.exec(body)?.[0], undefined, `${name} says a word the vocabulary refuses`);
    assert.ok(!/\b\d[\d,]* tests\b/.test(body), `${name}: no test count on a public page`);
    for (const m of body.matchAll(/\bsandboxed\b/gi)) assert.equal(body.slice(m.index, m.index + 15).toLowerCase(), "sandboxed frame", `${name}: only an addon's frame is called sandboxed`);
  }
});
