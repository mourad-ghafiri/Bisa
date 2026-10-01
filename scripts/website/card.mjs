#!/usr/bin/env node
/**
 * Render the social card (website/README.md): `social-card.html` beside this
 * file, at 1200 × 630, by this machine's Chrome — headless, over its own
 * DevTools protocol, in a profile of its own under `target/` — into
 * `website/assets/social-card.png`. Run by `just website-card` after the
 * card's words change; the PNG is committed, so nothing renders it at build.
 *
 *   node scripts/website/card.mjs [path to Chrome]
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const CHROMES = [
  process.argv[2],
  process.env.CHROME,
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
].filter(Boolean);
const chromePath = CHROMES.find((p) => existsSync(p));
if (!chromePath) {
  console.error("website-card: no Chrome here — pass its path, or set CHROME; the committed card stays as it is");
  process.exit(2);
}

const PORT = 9400 + Math.floor(Math.random() * 400);
const profile = join(root, "target", "website-card-profile");
mkdirSync(profile, { recursive: true });
const chrome = spawn(chromePath, ["--headless=new", `--remote-debugging-port=${PORT}`, `--user-data-dir=${profile}`, "--no-first-run", "--hide-scrollbars", "--window-size=1200,630", "about:blank"], { stdio: "ignore" });
const end = (code, words) => {
  chrome.kill("SIGTERM");
  if (words) console.error(`website-card: ${words}`);
  process.exit(code);
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let socket;
for (let i = 0; i < 50 && !socket; i++) {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
    const target = targets.find((t) => t.type === "page");
    if (target) socket = new WebSocket(target.webSocketDebuggerUrl);
  } catch {
    // Chrome is still starting.
  }
  if (!socket) await sleep(200);
}
if (!socket) end(1, "Chrome did not answer on its debugging port");
await new Promise((r) => socket.addEventListener("open", r));
let next = 0;
const waiting = new Map();
socket.addEventListener("message", (m) => {
  const msg = JSON.parse(m.data);
  waiting.get(msg.id)?.(msg);
  waiting.delete(msg.id);
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    next += 1;
    waiting.set(next, resolve);
    socket.send(JSON.stringify({ id: next, method, params }));
  });

await send("Emulation.setDeviceMetricsOverride", { width: 1200, height: 630, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }] });
await send("Page.navigate", { url: pathToFileURL(join(root, "scripts/website/social-card.html")).href });
await sleep(1200);
const shot = await send("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: 1200, height: 630, scale: 1 } });
if (!shot.result?.data) end(1, "Chrome returned no picture");
writeFileSync(join(root, "website/assets/social-card.png"), Buffer.from(shot.result.data, "base64"));
console.log("website-card: website/assets/social-card.png, 1200 × 630");
socket.close();
end(0);
