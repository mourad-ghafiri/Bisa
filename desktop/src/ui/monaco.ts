/**
 * Monaco, wrapped once and loaded lazily.
 *
 * Views never import `monaco-editor`; they import `CodeEditor` and
 * `DiffEditor` from `../ui`. This module owns the three things that are easy
 * to get wrong and expensive to get wrong twice: the worker environment (a
 * Vite `?worker` import per language service, so tokenisation and TypeScript
 * never run on the UI thread), the theme (derived from the token roles and
 * re-derived when the appearance changes), and the perf marks the spike and
 * the budgets read.
 *
 * Monaco is the single largest module in the bundle, and entering the Workbench
 * route used to parse all of it before a file was ever opened. It is now behind
 * a dynamic `import()` (`loadMonaco()`), so the editor's code is parsed only on
 * the first mount of an editor — the route opens without paying for it. Type
 * annotations use the `Monaco` namespace (erased at compile time, so free);
 * runtime code awaits `loadMonaco()` or reads the already-loaded `loadedMonaco()`.
 */

import { log } from "../log";
import type * as Monaco from "monaco-editor";
import { resolvedRoles } from "./cssColor";
// The workers are `?worker` stubs: Vite splits each into its own bundle and
// gives back a constructor, so importing them here costs the main chunk only a
// few bytes — the editor's own megabytes live behind the dynamic `import()`
// below. (A dynamic `import("…?worker")` has no ambient type, so these stay static.)
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import CssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import TsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";
import { watchAppearance } from "../shell/theme";
import { EDITOR_ROLE_MAP, editorThemeFor } from "./editorTheme.mjs";

export type { Monaco };

const THEME_NAME = "bisa";

let mod: typeof Monaco | null = null;
let loading: Promise<typeof Monaco> | null = null;
let environmentReady = false;

/**
 * The Monaco module once an editor has loaded it, or null before then. Effects
 * that run after mount (a gutter update, an annotation zone) read this; the
 * mount itself awaits {@link loadMonaco}.
 */
export function loadedMonaco(): typeof Monaco | null {
  return mod;
}

/**
 * Select the whole text of the Monaco editor that holds the focus — the
 * code editor, either side of a diff, a lens, a settings field — and answer
 * whether one did. *Select All* has no DOM event the way Cut, Copy and Paste
 * have, so where the shell's Edit menu owns ⌘A (macOS: the native verb runs
 * on Monaco's hidden input, which holds a slice of the text, and the replayed
 * key carries no key code Monaco reads) the verb never reaches the model
 * unless it is handed over here (`shell/editMenu.ts`, ide/15 §Dispatch).
 */
export function selectAllInFocusedEditor(): boolean {
  const focus = document.activeElement;
  if (!mod || !focus) return false;
  for (const editor of mod.editor.getEditors()) {
    const model = editor.getModel();
    if (!model || !editor.getDomNode()?.contains(focus)) continue;
    editor.setSelection(model.getFullModelRange());
    return true;
  }
  return false;
}

/** Install the worker factory once. Safe to call from every mount. */
function ensureMonacoEnvironment(): void {
  if (environmentReady) return;
  environmentReady = true;
  (self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
    getWorker(_id: string, label: string): Worker {
      switch (label) {
        case "json":
          return new JsonWorker();
        case "css":
        case "scss":
        case "less":
          return new CssWorker();
        case "html":
        case "handlebars":
        case "razor":
          return new HtmlWorker();
        case "typescript":
        case "javascript":
          return new TsWorker();
        default:
          return new EditorWorker();
      }
    },
  };
}

/**
 * Load Monaco once, install the worker environment and the theme, and start
 * watching the appearance. Every editor mount awaits this; concurrent mounts
 * share the one import.
 */
export function loadMonaco(): Promise<typeof Monaco> {
  if (mod) return Promise.resolve(mod);
  if (!loading) {
    loading = import("monaco-editor").then((m) => {
      mod = m;
      ensureMonacoEnvironment();
      applyEditorTheme();
      watchEditorTheme();
      return m;
    });
  }
  return loading;
}

/** The token roles as the document currently resolves them, keyed without the `--color-` prefix. */
function currentRoles(): Record<string, string> {
  const resolved = resolvedRoles(Object.keys(EDITOR_ROLE_MAP).map((role) => `--color-${role}`));
  const roles: Record<string, string> = {};
  for (const [name, hex] of Object.entries(resolved)) roles[name.slice("--color-".length)] = hex;
  return roles;
}

let warnedMissing = false;

/** Define (or redefine) the app theme from the roles and make it current. */
function applyEditorTheme(): void {
  if (!mod) return;
  const scheme = document.documentElement.getAttribute("data-scheme") === "dark" ? "dark" : "light";
  const { theme, missing } = editorThemeFor(currentRoles(), scheme);
  if (missing.length > 0 && !warnedMissing) {
    warnedMissing = true;
    log.warn("editor", "the editor theme has roles without a resolved colour", { roles: missing.join(", ") });
  }
  mod.editor.defineTheme(THEME_NAME, theme as Monaco.editor.IStandaloneThemeData);
  mod.editor.setTheme(THEME_NAME);
}

let themeWatched = false;

/** Re-derive the editor theme whenever the appearance dials move. */
function watchEditorTheme(): void {
  if (themeWatched) return;
  themeWatched = true;
  watchAppearance(() => {
    // The dials stamp attributes synchronously; the roles resolve on the next frame.
    requestAnimationFrame(applyEditorTheme);
  });
}
