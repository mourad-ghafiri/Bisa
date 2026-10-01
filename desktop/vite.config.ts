import { readFileSync } from "node:fs";
import { defineConfig, searchForWorkspaceRoot } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// The desktop's own version, website and repository, baked in at build time
// from package.json — the same number tauri.conf.json carries, the same two
// URLs the Cargo workspace declares — so About can name them without asking
// anyone at runtime.
const { version, homepage, repository } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string; homepage: string; repository: { url: string } };

// BISA_API_BASE is exposed to the frontend for UI-only dev
// (`BISA_API_BASE=http://127.0.0.1:PORT npm run dev`).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: { __APP_VERSION__: JSON.stringify(version), __APP_HOMEPAGE__: JSON.stringify(homepage), __APP_REPOSITORY__: JSON.stringify(repository.url) },
  envPrefix: ["VITE_", "BISA_"],
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
  },
  build: {
    target: "es2022",
  },
});
