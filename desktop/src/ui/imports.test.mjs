import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, relative } from "node:path";
import { sourceFiles } from "../testWalk.mjs";

/**
 * The kit rule (`docs/architecture/07-layering.md`): a screen imports from
 * `../ui` and never reaches for a UI library itself. Swapping an
 * implementation is then a one-file change, and a glyph or a primitive cannot
 * quietly acquire a second source.
 */
const here = dirname(fileURLToPath(import.meta.url));
const src = join(here, "..");

const BANNED = [
  "lucide-react",
  "@radix-ui/",
  "motion",
  "monaco-editor",
  "mermaid",
  "@xyflow/",
  "@dnd-kit/",
  // The canvas (19 — Drawings), wrapped once as the kit's third entry `ui/excalidraw.ts`.
  "@excalidraw/",
  // The artifact viewers' libraries (ide/12), each wrapped once under `ui/artifact/`.
  "pdfjs-dist",
  "xlsx",
  "mammoth",
  "dompurify",
  "jszip",
];

/** The directories the rule covers; `ui/` is where these libraries are allowed. */
const COVERED = ["views", "shell", "notes", "draw", "pet", "addons", "terminal"];

/** The one place outside the kit that animates directly, named so it stays one. */
// The setup gate is an alert dialog Escape does not close — the primitive itself, as `scenarios/setup.test.mjs` holds.
const ALLOWED = new Set(["shell/SidebarSection.tsx", "shell/SetupGate.tsx"]);

test("no screen imports a UI library the kit wraps", () => {
  const offences = [];
  for (const dir of COVERED) {
    for (const file of sourceFiles(join(src, dir), (p) => /\.(ts|tsx)$/.test(p))) {
      const rel = relative(src, file);
      if (ALLOWED.has(rel)) continue;
      const text = readFileSync(file, "utf8");
      for (const m of text.matchAll(/from\s+"([^"]+)"/g)) {
        const spec = m[1];
        if (BANNED.some((b) => spec === b || spec.startsWith(b.endsWith("/") ? b : `${b}/`))) offences.push(`${rel}: ${spec}`);
      }
    }
  }
  assert.deepEqual(offences, [], "import these through ../ui");
});

/**
 * The canvas is the kit's second entry (`ui/flow.ts`) so the graph
 * library leaves the main chunk. The index must not re-export it, and only
 * the designer's own files may reach for the second entry.
 */
test("the flow canvas is not on the kit index, and only the designer imports it", () => {
  const index = readFileSync(join(src, "ui/index.ts"), "utf8");
  assert.doesNotMatch(index, /FlowCanvas|FlowHandle|from "\.\/flow"/, "ui/index.ts re-exports the canvas");
  const flow = readFileSync(join(src, "ui/flow.ts"), "utf8");
  assert.match(flow, /FlowCanvas/);
  const outside = [];
  for (const dir of COVERED) {
    for (const file of sourceFiles(join(src, dir), (p) => /\.(ts|tsx)$/.test(p))) {
      const rel = relative(src, file);
      const text = readFileSync(file, "utf8");
      if (/from\s+"[./]*ui\/flow"/.test(text) && !rel.startsWith("views/_workflow/")) outside.push(rel);
    }
  }
  assert.deepEqual(outside, [], "only views/_workflow/ draws graphs");
});

/**
 * The canvas is the kit's third entry (`ui/excalidraw.ts`), for the reason
 * the flow canvas is the second: the largest chunk the app ships stays out
 * of the main one. The index must not re-export it, and only `draw/` may
 * reach for the entry.
 */
test("the canvas is not on the kit index, and only draw/ imports it", () => {
  const index = readFileSync(join(src, "ui/index.ts"), "utf8");
  assert.doesNotMatch(index, /excalidraw/i, "ui/index.ts re-exports the canvas");
  const entry = readFileSync(join(src, "ui/excalidraw.ts"), "utf8");
  assert.match(entry, /loadExcalidraw/);
  assert.ok(entry.indexOf("EXCALIDRAW_ASSET_PATH") < entry.indexOf('import("@excalidraw/excalidraw")'), "the assets are pointed home before the module is asked for");
  assert.match(entry, /import\("@excalidraw\/excalidraw\/index\.css"\)/, "the stylesheet rides in with the module");
  const outside = [];
  for (const dir of COVERED) {
    for (const file of sourceFiles(join(src, dir), (p) => /\.(ts|tsx)$/.test(p))) {
      const rel = relative(src, file);
      const text = readFileSync(file, "utf8");
      if (/from\s+"[./]*ui\/excalidraw"/.test(text) && !rel.startsWith("draw/")) outside.push(rel);
    }
  }
  assert.deepEqual(outside, [], "only draw/ draws on the canvas");
});
