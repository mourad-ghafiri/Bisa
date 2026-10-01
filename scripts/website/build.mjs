#!/usr/bin/env node
/**
 * Build bisa.dev (website/README.md): every page fragment under
 * `scripts/website/pages/` inside the partials beside it, every token filled
 * from the repository's own sources — the catalog's counts, the step kinds
 * and their words, the start events, a template's steps, a harness's name,
 * the decision points, the hundred asks, the screenshots that are there —
 * the brand files copied in, the sitemap, `robots.txt` and the web manifest
 * written, and the screenshot block of `README.md` replaced. The output is
 * committed, so a host serves `website/` as it is.
 *
 *   node scripts/website/build.mjs           write website/ and the README's block
 *   node scripts/website/build.mjs --check   build in memory; exit 1 on any difference
 *
 * A token nobody answers, a reader that finds too little, a screenshot the
 * manifest does not know or one over its budget stops the build in words.
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as M from "./websiteModel.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const read = (rel) => readFileSync(join(root, rel), "utf8");
const bytes = (rel) => readFileSync(join(root, rel));
const exists = (rel) => existsSync(join(root, rel));
const dirs = (rel) => readdirSync(join(root, rel), { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort();
const files = (rel, ext) => readdirSync(join(root, rel)).filter((f) => f.endsWith(ext)).sort();
const esc = M.escapeHtml;
const checking = process.argv.includes("--check");

// --- the repository's facts -------------------------------------------------

const workspace = read("Cargo.toml").split("[workspace.package]")[1] ?? "";
const homepage = /^homepage = "([^"]+)"/m.exec(workspace)?.[1];
const REPO = /^repository = "([^"]+)"/m.exec(workspace)?.[1];
if (!homepage || !REPO) throw new Error("Cargo.toml's [workspace.package] names no homepage or repository");
const SITE = homepage.endsWith("/") ? homepage : `${homepage}/`;
const BLOB = `${REPO}/blob/main`;

/** A path of the repository as a link on GitHub — refused when the file is not there. */
function onGitHub(rel) {
  if (!exists(rel)) throw new Error(`a link names ${rel}, which is not in the repository`);
  return `${BLOB}/${rel}`;
}

const LINKS = {
  site: SITE,
  repo: REPO,
  releases: `${REPO}/releases`,
  latest: `${REPO}/releases/latest`,
  docs: `${REPO}/tree/main/docs`,
  guides: `${REPO}/tree/main/docs/guide`,
  "guide-start": onGitHub("docs/guide/getting-started.md"),
  architecture: onGitHub("docs/architecture/README.md"),
  cli: onGitHub("docs/reference/cli.md"),
  changelog: onGitHub("CHANGELOG.md"),
  "security-policy": onGitHub("SECURITY.md"),
  license: onGitHub("LICENSE"),
  notices: onGitHub("THIRD-PARTY-NOTICES.md"),
  "feature-status": onGitHub("docs/feature-status.md"),
  issues: `${REPO}/issues`,
};

const ftl = M.parseFtl(read("locales/en/desktop/workflow.ftl"));
const kindsSource = read("desktop/src/views/_workflow/stepKinds.mjs");
const KINDS = M.parseStepKinds(kindsSource, ftl);
const STARTS = M.parseStartEvents(kindsSource, ftl);

const catalog = (kind) => files(`library/catalog/${kind}`, ".toml");
/** One catalog folder's records by slug, each the string values of its `[table]`. */
function catalogTable(kind, table) {
  const out = new Map();
  for (const f of catalog(kind)) {
    const record = M.tomlTable(read(`library/catalog/${kind}/${f}`), table);
    if (!record.name) throw new Error(`library/catalog/${kind}/${f} names nothing under [${table}]`);
    out.set(f.replace(/\.toml$/, ""), record);
  }
  return out;
}
const AGENTS = catalogTable("agents", "agent");
const TEAMS = catalogTable("teams", "team");
const SKILLS = catalogTable("skills", "skill");
M.atLeast([...AGENTS.keys()], 20, "catalog agents");
M.atLeast([...TEAMS.keys()], 5, "catalog teams");
const CONNECTORS = M.atLeast([...catalogTable("connectors", "connector").values()].map((c) => c.name), 10, "connectors");
const PETS = M.atLeast(
  dirs("library/pets").map((d) => JSON.parse(read(`library/pets/${d}/pet.json`)).displayName),
  5,
  "pets",
);
const ADDONS = M.atLeast(
  dirs("library/addons").map((d) => JSON.parse(read(`library/addons/${d}/addon.json`)).name),
  10,
  "addons",
).sort((a, b) => a.localeCompare(b));
const decisionPoints = M.decisionPointCount(read("crates/bisa-core/src/decision.rs"));
if (!decisionPoints) throw new Error("the reader of the decision points is broken: no DecisionPoint::ALL");
const COUNTS = {
  agents: catalog("agents").length,
  skills: catalog("skills").length,
  teams: catalog("teams").length,
  channels: catalog("channels").length,
  templates: catalog("workflows").length,
  connectors: CONNECTORS.length,
  addons: ADDONS.length,
  pets: PETS.length,
  "step-kinds": KINDS.length,
  starts: STARTS.length,
  "decision-points": decisionPoints,
};
for (const [what, n] of Object.entries(COUNTS)) if (!(n > 0)) throw new Error(`the reader of ${what} is broken: it counted none`);

const workflows = new Map();
function workflow(slug) {
  if (!workflows.has(slug)) {
    const rel = `library/catalog/workflows/${slug}.toml`;
    if (!exists(rel)) throw new Error(`no catalog template ${slug}`);
    workflows.set(slug, M.parseWorkflowToml(read(rel)));
  }
  return workflows.get(slug);
}

const EXAMPLES = new Map(
  files("scripts/website/examples", ".json").map((f) => {
    const example = JSON.parse(read(`scripts/website/examples/${f}`));
    M.checkExample(example, { kinds: new Set(KINDS.map((k) => k.kind)), agents: AGENTS });
    return [f.replace(/\.json$/, ""), example];
  }),
);
function example(name) {
  const found = EXAMPLES.get(name);
  if (!found) throw new Error(`no example ${name} under scripts/website/examples/`);
  return found;
}

const ASKS = M.parseScenarios(read("docs/guide/real-world-scenarios.md"));
const TALLY = M.tally(ASKS);
const MACOS = M.macosVersion(read("desktop/src-tauri/tauri.conf.json"));
const SETTINGS = new Set([...read("docs/reference/settings-keys.md").matchAll(/^\| `([a-z0-9_.]+)` \|/gm)].map((m) => m[1]));
M.atLeast([...SETTINGS], 100, "settings keys");
const harnessCatalog = read("crates/bisa-harness/src/catalog.rs");

function harness(stem) {
  const rel = `crates/bisa-adapters/src/${stem}.rs`;
  const name = exists(rel) ? M.displayName(read(rel)) : null;
  if (!name) throw new Error(`no adapter ${stem} with a display name`);
  return name;
}
function preset(id) {
  const name = M.presetLabel(harnessCatalog, id);
  if (!name) throw new Error(`no preset harness ${id}`);
  return name;
}

// --- screenshots ------------------------------------------------------------

const SHOTS_DIR = "website/screenshots";
const manifest = JSON.parse(read(`${SHOTS_DIR}/manifest.json`));
const SHOTS = new Map(manifest.shots.map((s) => [s.id, s]));
const KNOWN_FILES = new Set(["manifest.json", "placeholder.svg", "README.md", ".gitkeep"]);
// A file the manifest does not name is said and left out: one stray name never stops the rest of the site.
// (`just website-check` still fails on it, so it is not forgotten.)
for (const f of readdirSync(join(root, SHOTS_DIR))) {
  if (KNOWN_FILES.has(f) || f.startsWith(".")) continue;
  const m = /^(.+)\.(webp|jpg|png)$/.exec(f);
  if ((!m || !SHOTS.has(m[1])) && !checking) {
    console.warn(`note: ${SHOTS_DIR}/${f} is not shown — manifest.json names no screenshot "${m ? m[1] : f}". Rename it to one of: ${[...SHOTS.keys()].join(", ")}; or add an entry for it.`);
  }
}

/** The screenshots over the weight that loads fast — said once, together, at the end of a build. */
const warnedHeavy = new Set();
/** The file a screenshot has, with its size from its own header; `null` while it is not there. */
function shotFile(id) {
  for (const ext of M.SHOT_EXTENSIONS) {
    const rel = `${SHOTS_DIR}/${id}.${ext}`;
    if (!exists(rel)) continue;
    const b = bytes(rel);
    if (b.length > M.BUDGET.screenshot) {
      throw new Error(`${rel} is ${(b.length / 1048576).toFixed(1)} MB, over the 2 MB a screenshot may weigh — run \`just website-shots\`, which makes it a WebP for the web and moves the original aside`);
    }
    if (b.length > M.BUDGET.screenshotWarn) warnedHeavy.add(`${id}.${ext} ${Math.round(b.length / 1024)} KB`);
    const size = M.imageSize(new Uint8Array(b));
    if (!size) throw new Error(`${rel} is not a PNG, JPEG or WebP this build can read`);
    return { file: `${id}.${ext}`, ...size };
  }
  return null;
}

function figure(id, { hero = false } = {}) {
  const shot = SHOTS.get(id);
  if (!shot) return undefined;
  const found = shotFile(id);
  const place = `shot${hero ? " hero-shot" : ""}`;
  // A screenshot is the app's own window, caught with its title bar and its shadow: it is shown as it is,
  // never framed again. Only a screenshot still to come is drawn as an outline of the window, labelled.
  if (found) {
    const img = `<img src="/screenshots/${found.file}" width="${found.width}" height="${found.height}" alt="${esc(shot.alt)}"${hero ? ' fetchpriority="high"' : ' loading="lazy"'} decoding="async">`;
    // The app's screens are dense: each opens at its own size, in the browser, on a click.
    return `<figure class="${place}" id="shot-${esc(id)}"><a class="shot-open" href="/screenshots/${found.file}" title="Open the screenshot at full size">${img}</a></figure>`;
  }
  const bar = `<div class="window-bar" aria-hidden="true"><span></span><span></span><span></span><b>${esc(shot.title)}</b></div>`;
  const skeleton = `<div class="skeleton" role="img" aria-label="${esc(`Screenshot to come: ${shot.alt}`)}"><span class="sk-side"></span><span class="sk-main"><i></i><i></i><i></i><i></i></span><span class="sk-note"><strong>${esc(shot.title)}</strong><span>Screenshot to come</span><code>website/screenshots/${esc(id)}.png</code></span></div>`;
  return `<figure class="${place} is-missing" id="shot-${esc(id)}"><div class="window">${bar}${skeleton}</div></figure>`;
}

// --- the opening: a slideshow of the app ---------------------------------------------------

/** The screens the opening shows, from the page's own front matter: a label, and the screenshots that may show it. */
function slidesOf(meta) {
  const list = meta.slides;
  if (!Array.isArray(list) || list.length === 0) throw new Error(`${meta.path}: the opening names its screens in the front matter, as "slides"`);
  for (const s of list) {
    if (!s.label || !Array.isArray(s.shots) || s.shots.length === 0) throw new Error(`${meta.path}: a slide of the opening needs a label and its shots — ${JSON.stringify(s)}`);
    for (const id of s.shots) if (!SHOTS.has(id)) throw new Error(`${meta.path}: the slide ${s.label} names ${id}, which manifest.json does not`);
  }
  return list;
}

/**
 * One slide for each screen: the first of its screenshots that is here, or — while none is — the
 * outlined window of the first, labelled with the file to add, as everywhere else on the site.
 */
function heroSlides(meta) {
  const slides = slidesOf(meta).map((f) => ({ f, id: f.shots.find((id) => shotFile(id)) ?? f.shots[0] }));
  const items = slides
    .map(({ f, id }, i) => {
      const found = shotFile(id);
      const shot = SHOTS.get(id);
      const label = `${i + 1} of ${slides.length}: ${esc(f.label)}`;
      if (!found) {
        const bar = `<div class="window-bar" aria-hidden="true"><span></span><span></span><span></span><b>${esc(shot.title)}</b></div>`;
        const skeleton = `<div class="skeleton" role="img" aria-label="${esc(`Screenshot to come: ${shot.alt}`)}"><span class="sk-side"></span><span class="sk-main"><i></i><i></i><i></i><i></i></span><span class="sk-note"><strong>${esc(shot.title)}</strong><span>Screenshot to come</span><code>website/screenshots/${esc(id)}.png</code></span></div>`;
        return `<figure class="slide is-missing" id="slide-${esc(id)}" role="group" aria-roledescription="slide" aria-label="${label}"><div class="window">${bar}${skeleton}</div></figure>`;
      }
      const img = `<img src="/screenshots/${found.file}" width="${found.width}" height="${found.height}" alt="${esc(shot.alt)}"${i === 0 ? ' fetchpriority="high"' : ' loading="lazy"'} decoding="async">`;
      return `<figure class="slide" id="slide-${esc(id)}" role="group" aria-roledescription="slide" aria-label="${label}"><a class="shot-open" href="/screenshots/${found.file}" title="Open the screenshot at full size">${img}</a></figure>`;
    })
    .join("");
  return `<div class="slides" data-slides aria-roledescription="carousel" aria-label="The app, screen by screen"><div class="slides-track" tabindex="0">${items}</div></div>`;
}

// --- generated blocks -----------------------------------------------------------

const startsList = () => `<ul class="start-list">${STARTS.map((s) => `<li><code>${esc(s.event)}</code> ${esc(s.label.toLowerCase())}</li>`).join("")}</ul>`;

const names = (list) => {
  const all = list.map(esc);
  return all.length > 1 ? `${all.slice(0, -1).join(", ")} and ${all.at(-1)}` : all.join("");
};
const chips = (list) => `<p class="names">${list.map((n) => `<span>${esc(n)}</span>`).join("")}</p>`;

const slug = (text) => String(text).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
const TODAY_CLASS = { runs: "runs", "with you": "with-you", "custom connector": "custom" };

function asksBlock() {
  const sections = [...new Set(ASKS.map((a) => a.section))];
  const total = ASKS.length;
  let x = 0;
  const bar = Object.entries(TODAY_CLASS)
    .map(([word, cls]) => {
      const w = (TALLY[word] / total) * 100;
      const rect = `<rect class="t-${cls}" x="${x.toFixed(2)}" y="0" width="${w.toFixed(2)}" height="6"/>`;
      x += w;
      return rect;
    })
    .join("");
  const legend = Object.entries(TODAY_CLASS)
    .map(([word, cls]) => `<li class="t-${cls}"><span class="swatch" aria-hidden="true"></span><strong>${TALLY[word]}</strong> ${esc(word)}</li>`)
    .join("");
  const filters = [["all", "All"], ...Object.entries(TODAY_CLASS).map(([word, cls]) => [cls, word[0].toUpperCase() + word.slice(1)])]
    .map(([value, label], i) => `<button type="button" data-filter="${value}" aria-pressed="${i === 0}">${esc(label)}</button>`)
    .join("");
  const groups = sections
    .map((section) => {
      const rows = ASKS.filter((a) => a.section === section)
        .map((a) => `<li class="ask t-${TODAY_CLASS[a.today]}"><span class="ask-text">${esc(a.ask)}</span><span class="today">${esc(a.today)}</span></li>`)
        .join("");
      return `<section class="ask-group" aria-labelledby="asks-${slug(section)}"><h3 id="asks-${slug(section)}">${esc(section)}</h3><ul>${rows}</ul></section>`;
    })
    .join("");
  return `<div class="asks" data-filter="all">
<svg class="tally" viewBox="0 0 100 6" preserveAspectRatio="none" aria-hidden="true">${bar}</svg>
<ul class="tally-legend">${legend}</ul>
<div class="asks-filter" role="group" aria-label="Show the asks that" hidden>${filters}</div>
<div class="ask-groups">${groups}</div>
<p class="fine">Read every row with the parts it uses: <a href="${onGitHub("docs/guide/real-world-scenarios.md")}">the scenarios guide</a>.</p>
</div>`;
}

/** The example's steps, in order, each with its kind and the agent that takes it — text, never a drawn screen. */
/** The example's steps as a person reads them: each step's name and who holds it — the run's own start and end left out. */
function exampleSteps(name) {
  const ex = example(name);
  const items = ex.steps
    .filter((s) => s.kind !== "start" && s.kind !== "end")
    .map((s) => {
      const who = s.agent ? AGENTS.get(s.agent).name : s.who === "you" ? "You" : "";
      const holder = s.agent ? "agent" : s.who === "you" ? "you" : "flow";
      return `<li class="plan-${holder}"><span class="plan-name">${esc(s.name)}</span>${who ? `<span class="plan-who">${esc(who)}</span>` : ""}</li>`;
    })
    .join("");
  return `<ol class="plan" aria-label="${esc(`The steps proposed for: ${ex.goal}`)}">${items}</ol>`;
}

/** Every catalog agent: its name and the first sentence of what it does. */
function agentsGallery() {
  const cards = [...AGENTS.values()]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((a) => `<li><strong>${esc(a.name)}</strong><span>${esc(M.firstSentence(a.description))}</span></li>`)
    .join("");
  return `<ul class="gallery">${cards}</ul>`;
}

/** Every catalog team: its purpose and its members by name. */
function teamsGallery() {
  const cards = [...TEAMS.values()]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((t) => {
      const members = (t.agents ?? []).map((slug) => {
        const agent = AGENTS.get(slug);
        if (!agent) throw new Error(`the team ${t.name} names ${slug}, which is no catalog agent`);
        return agent.name;
      });
      return `<article class="card team"><h3>${esc(t.name)}</h3><p>${esc(M.firstSentence(t.purpose))}</p>${chips(members)}</article>`;
    })
    .join("");
  return `<div class="teams">${cards}</div>`;
}

const TALLY_WORD = { runs: "runs", "with-you": "with you", "custom-connector": "custom connector" };

// --- pages ------------------------------------------------------------------------

const partial = (name) => read(`scripts/website/partials/${name}.html`);
const PARTIALS = { head: partial("head"), header: partial("header"), footer: partial("footer") };
const PAGES = files("scripts/website/pages", ".html").map((f) => ({ name: f, ...M.parseFragment(read(`scripts/website/pages/${f}`), f) }));

const canonical = (path) => `${SITE}${path.replace(/^\//, "")}`;
function jsonld(meta) {
  if (!meta.jsonld) return "";
  const data = [
    { "@context": "https://schema.org", "@type": "WebSite", name: "Bisa", url: SITE },
    {
      "@context": "https://schema.org",
      "@type": "SoftwareApplication",
      "@id": `${SITE}#app`,
      name: "Bisa",
      description: meta.description,
      url: SITE,
      applicationCategory: "DeveloperApplication",
      operatingSystem: `macOS ${MACOS} or later`,
      isAccessibleForFree: true,
      offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
      license: LINKS.license,
      downloadUrl: LINKS.latest,
      image: `${SITE}assets/social-card.png`,
      sameAs: [REPO],
    },
  ];
  return `<script type="application/ld+json">${JSON.stringify(data).replace(/</g, "\\u003c")}</script>\n`;
}

function resolver(page) {
  const { meta } = page;
  return (name, arg) => {
    switch (name) {
      case "page":
        return {
          title: esc(meta.title),
          description: esc(meta.description),
          canonical: canonical(meta.path),
          robots: meta.noindex ? '<meta name="robots" content="noindex">\n' : "",
          jsonld: jsonld(meta),
        }[arg];
      case "current":
        return meta.path === arg ? ' aria-current="page"' : "";
      case "link":
        return LINKS[arg];
      case "guide":
        return onGitHub(`docs/guide/${arg}.md`);
      case "doc":
        return onGitHub(`docs/${arg}`);
      case "count":
        return COUNTS[arg];
      case "harness":
        return esc(harness(arg));
      case "preset":
        return esc(preset(arg));
      case "macos":
        return MACOS;
      case "figure":
        return figure(arg);
      case "hero":
        return figure(arg, { hero: true });
      case "hero-slides":
        return heroSlides(meta);
      case "starts":
        return startsList();
      case "connectors":
        return chips(CONNECTORS);
      case "agents-gallery":
        return agentsGallery();
      case "teams-gallery":
        return teamsGallery();
      case "example-steps":
        return exampleSteps(arg);
      case "example-goal":
        return esc(example(arg).goal);
      case "pets":
        return names(PETS);
      case "addons":
        return names(ADDONS);
      case "addon-chips":
        return chips(ADDONS);
      case "template-name":
        return esc(workflow(arg).name);
      case "asks":
        return asksBlock();
      case "tally":
        return TALLY_WORD[arg] === undefined ? undefined : String(TALLY[TALLY_WORD[arg]]);
      case "setting":
        if (!SETTINGS.has(arg)) throw new Error(`${page.name}: ${arg} is not a registered setting (docs/reference/settings-keys.md)`);
        return `<code>${esc(arg)}</code>`;
      default:
        return undefined;
    }
  };
}

/** Where a page is written: `/` → `index.html`, `/features/` → `features/index.html`, `/404.html` as named. */
const fileOf = (path) => (path.endsWith("/") ? `${path.slice(1)}index.html` : path.slice(1));

const outputs = new Map();
for (const page of PAGES) {
  const fill = (text, where) => M.fillTokens(text, resolver(page), `${page.name} (${where})`);
  const html = [fill(PARTIALS.head, "head"), fill(PARTIALS.header, "header"), fill(page.body, "body").trimEnd(), "\n", fill(PARTIALS.footer, "footer")].join("");
  // Every page but the 404 page reads its files from where it stands — from a host and from the disk alike;
  // the 404 page is served at any depth, so its addresses stay at the root.
  outputs.set(`website/${fileOf(page.meta.path)}`, page.meta.noindex ? html : M.relativeUrls(html, M.prefixFor(page.meta.path)));
}

// One plain script, built from the rail's rules and the behaviours.
outputs.set("website/assets/site.js", M.bundleScript(read("scripts/website/railModel.mjs"), read("scripts/website/site.src.js")));

const indexable = PAGES.filter((p) => !p.meta.noindex).map((p) => p.meta.path).sort();
outputs.set(
  "website/sitemap.xml",
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${indexable.map((p) => `  <url><loc>${canonical(p)}</loc></url>`).join("\n")}\n</urlset>\n`,
);
outputs.set("website/robots.txt", `User-agent: *\nAllow: /\n\nSitemap: ${SITE}sitemap.xml\n`);
outputs.set(
  "website/site.webmanifest",
  `${JSON.stringify(
    {
      name: "Bisa",
      short_name: "Bisa",
      description: "A Decentralized Agentic IDE",
      start_url: "/",
      display: "browser",
      background_color: "#eef3f9",
      theme_color: "#1a56a8",
      icons: [
        { src: "/favicon.svg", type: "image/svg+xml", sizes: "any" },
        { src: "/apple-touch-icon.png", type: "image/png", sizes: "256x256" },
        { src: "/assets/icon-512.png", type: "image/png", sizes: "512x512" },
      ],
    },
    null,
    2,
  )}\n`,
);

// The screenshots folder's own README, from the manifest.
outputs.set(
  `${SHOTS_DIR}/README.md`,
  [
    "# Screenshots",
    "",
    "<!-- Generated by `just website` from manifest.json — edit the manifest, not this file. -->",
    "",
    "Capture the app's whole window as macOS does it (⇧⌘4, then Space, then click the window), drop it",
    "here under its name as `.png`, and run `just website`: it becomes a `.webp` for the web — at most",
    "2000 px wide, its shadow kept, the original moved aside under `target/` — and is shown as it is;",
    "one that is not here is a labelled placeholder on the site and `placeholder.svg` in the",
    "repository's README. Leave out what is yours alone: your npub, your home folder's path, an e-mail",
    "address, a token, a usage number.",
    "",
    "| File | What to capture | Shown |",
    "|---|---|---|",
    ...manifest.shots.map((s) => `| \`${s.id}.png\` | ${s.capture} | ${shotFile(s.id) ? "yes" : "placeholder"} |`),
    "",
  ].join("\n"),
);

// The README's screenshot block: what is here is shown; what is not is the placeholder, never a broken image.
// The README shows the screenshots marked `readme`: the first as the overview under the slogan, the rest
// as a gallery in its desktop section — each block between its own markers.
// `readme: true` — the overview (the first) and the gallery of the desktop section; `readme: "ide"` — the IDE section.
const README_SHOTS = manifest.shots.filter((s) => s.readme === true);
const README_IDE_SHOTS = manifest.shots.filter((s) => s.readme === "ide");
const readmeImg = (s, width) => {
  const found = shotFile(s.id);
  const src = found ? `website/screenshots/${found.file}` : "website/screenshots/placeholder.svg";
  const alt = found ? s.alt : `Screenshot to come: ${s.alt}`;
  return `<img src="${src}" alt="${esc(alt)}" width="${width}">`;
};
/**
 * Under the slogan: the README's slideshow — the website's opening as one animated image, made by
 * `just website-shots` — with the screens it shows named beneath; until it is made, the overview.
 */
function readmeOverview() {
  const { image, frames } = M.README_SLIDESHOW;
  if (exists(image) && exists(frames)) {
    const labels = JSON.parse(read(frames)).frames.map((f) => f.label);
    return [`<p align="center"><img src="${image}" alt="${esc(`Bisa, screen by screen: ${labels.join(", ")}`)}" width="880"></p>`, `<p align="center"><sub>${labels.map(esc).join(" · ")}</sub></p>`].join("\n");
  }
  return `<p align="center">${readmeImg(README_SHOTS[0], 880)}</p>`;
}
function readmeTable(shots) {
  const rows = [];
  for (let i = 0; i < shots.length; i += 2) {
    const cells = shots.slice(i, i + 2).map((s) => `<td width="50%">${readmeImg(s, 440)}<br><sub><b>${esc(s.title)}</b></sub></td>`);
    rows.push(`<tr>${cells.join("")}</tr>`);
  }
  return `<table>${rows.join("")}</table>`;
}
function readmeIde() {
  return readmeTable(README_IDE_SHOTS);
}
function readmeGallery() {
  const rest = README_SHOTS.slice(1);
  const rows = [];
  for (let i = 0; i < rest.length; i += 2) {
    const cells = rest.slice(i, i + 2).map((s) => `<td width="50%">${readmeImg(s, 440)}<br><sub><b>${esc(s.title)}</b></sub></td>`);
    rows.push(`<tr>${cells.join("")}</tr>`);
  }
  return `<table>${rows.join("")}</table>`;
}
const README_BLOCKS = [
  ["<!-- website:overview — generated by `just website` from website/screenshots/manifest.json -->", "<!-- /website:overview -->", readmeOverview],
  ["<!-- website:screenshots — generated by `just website` from website/screenshots/manifest.json -->", "<!-- /website:screenshots -->", readmeGallery],
  ["<!-- website:ide — generated by `just website` from website/screenshots/manifest.json -->", "<!-- /website:ide -->", readmeIde],
];
outputs.set(
  "README.md",
  README_BLOCKS.reduce((text, [start, end, block]) => M.replaceBlock(text, start, end, block()), read("README.md")),
);

// The brand's files, copied as they are.
const COPIES = {
  "website/favicon.svg": "logo/logo.svg",
  "website/favicon.ico": "desktop/src-tauri/icons/icon.ico",
  "website/apple-touch-icon.png": "desktop/src-tauri/icons/128x128@2x.png",
  "website/assets/icon-512.png": "desktop/src-tauri/icons/icon.png",
  "website/assets/moonrice.webp": "library/pets/moonrice/spritesheet.webp",
};
for (const [to, from] of Object.entries(COPIES)) outputs.set(to, bytes(from));

// --- written, or checked ------------------------------------------------------------

const same = (rel, content) => exists(rel) && Buffer.compare(bytes(rel), Buffer.isBuffer(content) ? content : Buffer.from(content)) === 0;

/** Every page this build writes, and nothing else, under `website/` — a page left from a removed fragment is a difference. */
function strayPages() {
  const found = [];
  const walk = (rel) => {
    for (const e of readdirSync(join(root, rel), { withFileTypes: true })) {
      const p = `${rel}/${e.name}`;
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith(".html") && !outputs.has(p)) found.push(p);
    }
  };
  walk("website");
  return found;
}

if (checking) {
  const stale = [...outputs].filter(([rel, content]) => !same(rel, content)).map(([rel]) => rel);
  const stray = strayPages();
  if (stale.length || stray.length) {
    for (const rel of stale) console.error(`stale: ${rel}`);
    for (const rel of stray) console.error(`not built by any fragment: ${rel}`);
    console.error("the website is not what scripts/website/ builds — run `just website-gen`");
    process.exit(1);
  }
  console.log(`website: ${outputs.size} files as built`);
} else {
  let written = 0;
  for (const [rel, content] of outputs) {
    if (same(rel, content)) continue;
    mkdirSync(dirname(join(root, rel)), { recursive: true });
    writeFileSync(join(root, rel), content);
    written += 1;
  }
  const stray = strayPages();
  for (const rel of stray) console.warn(`note: ${rel} is built by no fragment — move it aside if it is not wanted`);
  const missing = manifest.shots.filter((s) => !shotFile(s.id)).length;
  console.log(`website: ${written} of ${outputs.size} files written; ${manifest.shots.length - missing} of ${manifest.shots.length} screenshots in place`);
  if (warnedHeavy.size) console.warn(`note: ${warnedHeavy.size} screenshots weigh over 600 KB: ${[...warnedHeavy].join(", ")} — \`just website-shots\` makes them WebP for the web (\`just website-check\` fails until then)`);
}
