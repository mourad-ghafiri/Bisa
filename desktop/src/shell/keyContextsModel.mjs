/**
 * Which keymap scopes are live for a key (ide/15 §Dispatch), from what the
 * DOM said about where the key landed — `keyContexts.ts` reads the DOM, this
 * decides. `global` is left implicit, as the keymap adds it.
 *
 * `root` — the IDE is on a root; `strip` — a root's tab strip is on screen;
 * `designer` — the Workflow screen or a goal's Workflow tab is on screen;
 * `within` — what the key's target sits inside.
 */

/** The selectors a target is tested against, by the fact each one answers. */
export const WITHIN = Object.freeze({
  monaco: ".monaco-editor",
  rendered: "[data-rendered-doc]",
  conflict: "[data-conflict-doc]",
  terminal: "[data-terminal-panel]",
  browser: "[data-browser-doc]",
  files: "[data-files-tree]",
});

/**
 * @param {{root: boolean, strip: boolean, designer: boolean, within: Partial<Record<keyof typeof WITHIN, boolean>>}} facts
 * @returns {string[]}
 */
export function scopesOf({ root, strip, designer, within }) {
  const w = within ?? {};
  const out = [];
  if (root) out.push("workbench");
  // `tabs` is live while a root's strip is on screen — Ctrl+Tab from the
  // editor cycles documents the way it does everywhere else.
  if (root && strip) out.push("tabs");
  if (root && w.monaco) out.push("editor");
  // A document is read two ways — the editor, or a rendering — and find is
  // the document's whichever way, wherever it is drawn, on any route.
  if (w.monaco || w.rendered || w.conflict) out.push("document");
  if (w.terminal) out.push("terminal");
  // A browser tab's body — the IDE's centre or the Browser pane (ide/18).
  if (w.browser) out.push("browser");
  // A Files tree owns its own keys wherever it is drawn — the goal inspector's too.
  if (w.files) out.push("files");
  // The Workflow Designer owns its panel's chords.
  if (!root && designer) out.push("designer");
  return out;
}
