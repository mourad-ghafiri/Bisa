#!/usr/bin/env node
/**
 * Make the screenshots small for the web (website/README.md): every `<id>.png` or `<id>.jpg` in
 * website/screenshots/ that the manifest names becomes `<id>.webp` — at most 2000 px wide, its
 * transparent window shadow kept — by this machine's `cwebp`, Google's WebP encoder
 * (https://developers.google.com/speed/webp/docs/cwebp). The original is moved aside under
 * target/website-screenshot-originals/ (ignored by git), never deleted; a `.webp` it replaces
 * goes there too. Then the README's slideshow — the website's opening as one animated WebP, made by
 * `img2webp` from the same screens — is made again whenever its frames are no longer the screens that
 * are here (website/assets/readme-slideshow.json says which it holds). Run by `just website-shots`,
 * and by `just website` before it builds.
 *
 *   node scripts/website/web-screenshots.mjs [--if-available]
 *
 * With `--if-available`, a machine with no `cwebp` says so and leaves the files as they are
 * (`just website-check` then names the ones too heavy for the site); without it, that is a refusal.
 */
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { README_SLIDESHOW, cwebpArgs, img2webpArgs, imageSize, parseFragment, slideshowFrames } from "./websiteModel.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const SHOTS = join(root, "website/screenshots");
const ASIDE = join(root, "target/website-screenshot-originals");
const WORK = join(root, "target/website-screenshot-work");
const lenient = process.argv.includes("--if-available");

const ids = new Set(JSON.parse(readFileSync(join(SHOTS, "manifest.json"), "utf8")).shots.map((s) => s.id));
const todo = readdirSync(SHOTS)
  .map((file) => ({ file, m: /^(.+)\.(png|jpg)$/.exec(file) }))
  .filter(({ m }) => m && ids.has(m[1]))
  .map(({ file, m }) => ({ file, id: m[1] }));

const cwebp = process.env.CWEBP || "cwebp";
const img2webp = process.env.IMG2WEBP || "img2webp";
const runs = (tool) => spawnSync(tool, ["-version"], { stdio: "ignore" }).status === 0;
/** Say what this machine lacks for the work there is, and stop — or, lenient, go on without it. */
function lacking(what) {
  console.warn(`website-shots: ${what}, and this machine has no WebP tools — install them (https://developers.google.com/speed/webp/download; with Homebrew, \`brew install webp\`) and run \`just website-shots\``);
  process.exit(lenient ? 0 : 2);
}
if (todo.length > 0 && !runs(cwebp)) lacking(`${todo.length} screenshot(s) are still PNG or JPEG`);

/** Move a file out of the site into the folder of originals, under a name nothing there holds yet. */
function moveAside(path, name) {
  mkdirSync(ASIDE, { recursive: true });
  let to = join(ASIDE, name);
  if (existsSync(to)) to = join(ASIDE, name.replace(/(\.[a-z]+)$/, `-${new Date().toISOString().replace(/[:.]/g, "-")}$1`));
  renameSync(path, to);
  return to;
}

const kb = (bytes) => `${Math.round(bytes / 1024)} KB`;
let before = 0;
let after = 0;
mkdirSync(WORK, { recursive: true });
for (const { file, id } of todo) {
  const source = join(SHOTS, file);
  const bytes = readFileSync(source);
  const size = imageSize(bytes);
  if (!size) {
    console.error(`website-shots: ${file} is not a PNG or JPEG this script can read — left as it is`);
    process.exitCode = 1;
    continue;
  }
  const made = join(WORK, `${id}.webp`);
  const run = spawnSync(cwebp, cwebpArgs(source, made, size.width), { encoding: "utf8" });
  if (run.status !== 0 || !existsSync(made)) {
    console.error(`website-shots: cwebp could not encode ${file}: ${(run.stderr || "").trim() || `exit ${run.status}`} — left as it is`);
    process.exitCode = 1;
    continue;
  }
  const target = join(SHOTS, `${id}.webp`);
  if (existsSync(target)) moveAside(target, `${id}.webp`);
  renameSync(made, target);
  const aside = moveAside(source, file);
  const out = imageSize(readFileSync(target));
  before += bytes.length;
  after += statSync(target).size;
  console.log(`website-shots: ${file} ${size.width}×${size.height} ${kb(bytes.length)} → ${id}.webp ${out.width}×${out.height} ${kb(statSync(target).size)} (original: ${aside.slice(root.length + 1)})`);
}
if (after > 0) console.log(`website-shots: ${kb(before)} → ${kb(after)}`);

// --- the README's slideshow ---------------------------------------------------------------

const slides = parseFragment(readFileSync(join(root, "scripts/website/pages/home.html"), "utf8"), "home.html").meta.slides;
const weight = (id) => (existsSync(join(SHOTS, `${id}.webp`)) ? statSync(join(SHOTS, `${id}.webp`)).size : null);
const frames = slideshowFrames(slides, weight);
const said = `${JSON.stringify({ about: "The frames of readme-slideshow.webp, made by scripts/website/web-screenshots.mjs — never edited by hand.", frames }, null, 2)}\n`;
const image = join(root, README_SLIDESHOW.image);
const record = join(root, README_SLIDESHOW.frames);
const current = existsSync(record) && existsSync(image) && readFileSync(record, "utf8") === said;
if (frames.length < 2 || current) {
  if (current) console.log(`website-shots: the README's slideshow holds ${frames.map((f) => f.label).join(" · ")}`);
} else {
  if (!runs(cwebp) || !runs(img2webp)) lacking("the README's slideshow is behind its screens");
  const paths = frames.map((f, i) => {
    const frame = join(WORK, `frame-${i}.webp`);
    const shot = join(SHOTS, `${f.id}.webp`);
    const width = imageSize(readFileSync(shot)).width;
    const args = ["-quiet", "-q", "90", "-alpha_q", "90", "-m", "6", "-sharp_yuv", "-metadata", "none", ...(width > README_SLIDESHOW.width ? ["-resize", String(README_SLIDESHOW.width), "0"] : []), shot, "-o", frame];
    const run = spawnSync(cwebp, args, { encoding: "utf8" });
    if (run.status !== 0) {
      console.error(`website-shots: cwebp could not make the frame of ${f.id}: ${(run.stderr || "").trim()}`);
      process.exit(1);
    }
    return frame;
  });
  const made = join(WORK, "readme-slideshow.webp");
  const run = spawnSync(img2webp, img2webpArgs(paths, made), { encoding: "utf8" });
  if (run.status !== 0 || !existsSync(made)) {
    console.error(`website-shots: img2webp could not make the README's slideshow: ${(run.stderr || "").trim() || `exit ${run.status}`}`);
    process.exit(1);
  }
  mkdirSync(dirname(image), { recursive: true });
  if (existsSync(image)) moveAside(image, "readme-slideshow.webp");
  renameSync(made, image);
  writeFileSync(record, said);
  console.log(`website-shots: the README's slideshow, ${frames.length} screens — ${frames.map((f) => f.label).join(" · ")} — ${kb(statSync(image).size)}`);
}
