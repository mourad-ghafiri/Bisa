/**
 * The desktop's own page behind a Content-Security-Policy, as the sources
 * show it (11 — Security § The desktop's own page): the policy the shell
 * applies and its development twin, the one inline script the page carries
 * and no `<style>` element, the same policy sent by Vite's dev server and
 * preview, the violation listener, the pages an agent wrote loaded from the
 * shell's own scheme rather than `srcdoc`, pdf.js kept off eval, the
 * Markdown link rule, every capability free of `remote`, and the build
 * inlining no `BISA_*` variable. Source assertions, as `browser.test.mjs`
 * makes them — no DOM. Run with
 * `node --test desktop/src/scenarios/webviewPolicy.test.mjs`.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const src = join(dirname(fileURLToPath(import.meta.url)), "..");
const desktop = join(src, "..");
const read = (rel) => readFileSync(join(src, rel), "utf8");
const readDesktop = (rel) => readFileSync(join(desktop, rel), "utf8");

const security = JSON.parse(readDesktop("src-tauri/tauri.conf.json")).app.security;
/** A policy as a map: directive → its source list. */
const directives = (csp) => Object.fromEntries(csp.split(";").map((d) => d.trim()).filter(Boolean).map((d) => [d.split(/\s+/)[0], d.split(/\s+/).slice(1)]));

test("the window's policy allows the app, the node and the pages' scheme — no inline script, no eval, no object, no foreign frame", () => {
  const csp = directives(security.csp);
  assert.deepEqual(csp["default-src"], ["'self'"]);
  assert.deepEqual(csp["script-src"], ["'self'"], "no inline script, no eval: the sanitizer is no longer the only wall");
  assert.deepEqual(csp["style-src"], ["'self'", "'unsafe-inline'"], "the libraries inject style elements and React writes style attributes");
  assert.deepEqual(csp["object-src"], ["'none'"]);
  assert.deepEqual(csp["base-uri"], ["'self'"]);
  assert.deepEqual(csp["form-action"], ["'self'"]);
  assert.deepEqual(csp["worker-src"], ["'self'", "blob:"], "Monaco's and pdf.js's workers");
  for (const d of ["img-src", "media-src", "connect-src", "frame-src"]) {
    assert.ok(csp[d].includes("http://127.0.0.1:*") && csp[d].includes("http://localhost:*"), `${d} reaches the node on either name`);
  }
  assert.ok(csp["img-src"].includes("https:"), "a remote image in a document keeps loading — the person's decision");
  assert.ok(!csp["img-src"].includes("http:"), "and a plaintext one does not");
  assert.ok(csp["connect-src"].includes("ipc:") && csp["connect-src"].includes("http://ipc.localhost"), "Tauri's IPC on every platform");
  assert.ok(csp["frame-src"].includes("bisa-page:"), "the pages an agent wrote, from the shell's scheme");
  assert.ok(!csp["frame-src"].includes("https:") && !csp["frame-src"].includes("'self'"), "no foreign frame and no frame of the app's own origin");
  assert.ok(!security.csp.includes("'unsafe-eval'"), "nothing evaluates a string anywhere");
  assert.ok(!security.csp.includes("about:"), "a srcdoc navigation makes no request; nothing else loads from about:");
});

test("the development policy differs from the app's by React's inline preamble and the HMR socket alone", () => {
  const dev = directives(security.devCsp);
  const app = directives(security.csp);
  assert.deepEqual(dev["script-src"], ["'self'", "'unsafe-inline'"]);
  assert.deepEqual(dev["connect-src"].filter((s) => !s.startsWith("ws://")), app["connect-src"]);
  assert.deepEqual(dev["connect-src"].filter((s) => s.startsWith("ws://")), ["ws://127.0.0.1:*", "ws://localhost:*"]);
  for (const d of Object.keys(app)) {
    if (d === "script-src" || d === "connect-src") continue;
    assert.deepEqual(dev[d], app[d], d);
  }
  assert.deepEqual(Object.keys(dev).sort(), Object.keys(app).sort());
  assert.ok(!security.devCsp.includes("'unsafe-eval'"));
});

test("the page carries one inline script — the appearance stamp Tauri hashes — and no style element, so the style policy's inline allowance stands", () => {
  const html = readDesktop("index.html");
  const scripts = [...html.matchAll(/<script\b[^>]*>/g)].map((m) => m[0]);
  assert.deepEqual(scripts, ["<script>", '<script type="module" src="/src/main.tsx">'], "the stamp and the app");
  assert.ok(!/<style\b/.test(html), "a <style> in the page would make Tauri add a nonce, and a nonce makes 'unsafe-inline' inert");
  assert.ok(!/\son[a-z]+=/i.test(html), "no inline handler");
});

test("Vite sends the same policy as a header: the development twin from the dev server, the app's with the inline script's hash from the preview", () => {
  const vite = readDesktop("vite.config.ts");
  assert.ok(vite.includes('readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8")).app.security'), "one source: the shell's config");
  assert.ok(vite.includes('headers: { "Content-Security-Policy": security.devCsp }'), "the dev server");
  assert.ok(vite.includes('headers: { "Content-Security-Policy": previewCsp() }'), "the preview");
  assert.ok(vite.includes("createHash(\"sha256\").update(m[1]).digest(\"base64\")") && vite.includes("security.csp.replace(\"script-src 'self'\""), "the inline script's hash, as Tauri would add it");
  assert.ok(vite.includes('envPrefix: mode === "production" ? ["VITE_"] : ["VITE_", "BISA_"]'), "no BISA_* variable is inlined into a production build");
});

test("a refused load is one warning in the log, with the directive, what was blocked and where from", () => {
  const log = read("log.ts");
  assert.ok(log.includes('window.addEventListener("securitypolicyviolation", (ev) => {'));
  assert.ok(log.includes('log.warn("csp", "the content security policy blocked a load", {'));
  for (const field of ["directive: ev.violatedDirective", "blocked: ev.blockedURI", "file: ev.sourceFile", "line: ev.lineNumber"]) assert.ok(log.includes(field), field);
});

test("a page an agent wrote is staged with the shell and loaded from its scheme, srcdoc in a browser, never a src on the node", () => {
  const frame = read("ui/artifact/PageFrame.tsx");
  assert.ok(frame.includes("const page = useStagedPage(doc, csp);") && frame.includes("{...page}"), "the frame loads what the hook hands it");
  assert.ok(!frame.includes("srcDoc={doc}"), "no srcdoc of its own: a srcdoc document inherits the window's policy");
  assert.ok(frame.includes('sandbox={PAGE_SANDBOX}') && frame.includes('referrerPolicy="no-referrer"'), "the sandbox and the referrer stand");
  const staged = read("ui/artifact/stagedPage.ts");
  assert.ok(staged.includes('invoke<string>("page_stage", { html: doc, csp })') && staged.includes('invoke("page_release", { id: stagedPageId(url) })'), "staged and released through the shell's two commands");
  assert.ok(staged.includes("if (!inDesktopShell()) {") && staged.includes("setPage({ srcDoc: doc })"), "a browser keeps srcdoc");
  assert.ok(!staged.includes("apiBase") && !/\/attachments|\/ide\/raw/.test(staged), "nothing of the node's is loaded as a page");
  const pages = readDesktop("src-tauri/src/pages.rs");
  assert.ok(pages.includes('pub const SCHEME: &str = "bisa-page";'));
  assert.ok(pages.includes(".header(header::CONTENT_SECURITY_POLICY, page.csp.as_str())"), "the page's own policy as a header too");
  assert.ok(pages.includes("if label != crate::tray::MAIN_WINDOW {"), "the main window alone is answered");
  const main = readDesktop("src-tauri/src/main.rs");
  assert.ok(main.includes(".register_uri_scheme_protocol(pages::SCHEME, pages::protocol)") && main.includes("pages::page_stage,") && main.includes("pages::page_release,"));
  const navigation = readDesktop("src-tauri/src/navigation.rs");
  assert.ok(navigation.includes("s if s == crate::pages::SCHEME => true,"), "the scheme may load in a frame");
});

test("pdf.js is kept off eval, and a Markdown link's vouched protocols never include a protocol-relative URL", () => {
  assert.ok(read("ui/artifact/PdfView.tsx").includes("isEvalSupported: false"), "PostScript functions through the interpreter");
  const markdown = read("ui/Markdown.tsx");
  assert.ok(markdown.includes("SAFE_PROTOCOL") && markdown.includes('from "./markdownHtmlModel.mjs"') && !/const SAFE_PROTOCOL\s*=/.test(markdown), "the rule is the model's, tested there");
  assert.ok(read("ui/markdownHtmlModel.mjs").includes("export const SAFE_PROTOCOL = /^(https?:|mailto:|#|\\/(?!\\/))/i;"));
});

test("no capability names a remote URL, and the tabs' module says Tauri's rule: remote content has no API", () => {
  const dir = join(desktop, "src-tauri", "capabilities");
  const files = readdirSync(dir).filter((f) => f.endsWith(".json"));
  assert.ok(files.length > 0);
  for (const f of files) {
    const capability = JSON.parse(readFileSync(join(dir, f), "utf8"));
    assert.equal(capability.remote, undefined, `${f}: a remote URL with IPC would be a page of the web with the app's commands`);
    assert.deepEqual(capability.windows, ["main"], `${f}: the main window alone`);
  }
  // The module doc, its line breaks and `//!` folded into spaces.
  const tabs = readDesktop("src-tauri/src/browser.rs").replace(/\n\/\/! ?/g, " ");
  assert.ok(tabs.includes("Tauri hands its API to no webview whose URL is not the app's own"), "the reason a tab has no IPC, as Tauri states it");
});
