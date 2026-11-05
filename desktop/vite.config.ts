import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { defineConfig, searchForWorkspaceRoot } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The desktop's own version, website and repository, baked in at build time
// from package.json — the same number tauri.conf.json carries, the same two
// URLs the Cargo workspace declares — so About can name them without asking
// anyone at runtime.
const { version, homepage, repository } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string; homepage: string; repository: { url: string } };

// The window's Content-Security-Policy is the shell's (`src-tauri/tauri.conf.json`,
// 11 — Security § The desktop's own page): in the app Tauri puts `csp` on the
// page and adds the hash of its one inline script. A dev server and a preview
// run in a browser, where nobody adds it — so the same policy goes out as a
// header here, `devCsp` on the dev server (React's refresh preamble is an
// inline script, and HMR a web socket) and `csp` with the inline script's
// hash on the preview, so the capture harness sees what the app would refuse.
const security = JSON.parse(readFileSync(new URL("./src-tauri/tauri.conf.json", import.meta.url), "utf8")).app.security as { csp: string; devCsp: string };
function previewCsp(): string {
  const built = new URL("./dist/index.html", import.meta.url);
  const html = readFileSync(existsSync(built) ? built : new URL("./index.html", import.meta.url), "utf8");
  const hashes = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => `'sha256-${createHash("sha256").update(m[1]).digest("base64")}'`);
  return security.csp.replace("script-src 'self'", ["script-src 'self'", ...hashes].join(" "));
}

// BISA_API_BASE and BISA_API_TOKEN are exposed to the frontend for UI-only
// dev (`BISA_API_BASE=http://127.0.0.1:PORT npm run dev`) — and in
// development alone: a production build (`npm run build`, what the shell
// embeds) inlines no `BISA_*` variable, so a token set in the environment of
// the machine that built a release never ships inside the bundle
// (`api.ts` then asks the shell for the token, as it does in the app).
export default defineConfig(({ mode }) => ({
  plugins: [react(), tailwindcss()],
  define: { __APP_VERSION__: JSON.stringify(version), __APP_HOMEPAGE__: JSON.stringify(homepage), __APP_REPOSITORY__: JSON.stringify(repository.url) },
  envPrefix: mode === "production" ? ["VITE_"] : ["VITE_", "BISA_"],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The platform's mark is `logo/logo.svg` and the catalog is `locales/`,
    // both at the repository root, outside this app's own root. Vite's dev
    // server serves a file outside the root only from its allow list, which
    // defaults to the project root when no workspace file names a wider one
    // (vite.dev/config/server-options.html, `server.fs.allow`); the build
    // needs nothing — an imported asset is hashed into `dist/assets` wherever
    // it lives, and a `?raw` catalog file is inlined.
    fs: { allow: [searchForWorkspaceRoot(process.cwd()), "../logo", "../locales"] },
    headers: { "Content-Security-Policy": security.devCsp },
  },
  preview: {
    headers: { "Content-Security-Policy": previewCsp() },
  },
  build: {
    target: "es2022",
  },
}));
