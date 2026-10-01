/**
 * The pure half of the Mermaid viewer (ide/11): where the fenced blocks are
 * in a Markdown document, and how an error Mermaid reports against a
 * snippet becomes a line in *your* file.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * Every ```mermaid fence in `markdown`, with the 1-based line its first
 * source line sits on — the offset an error needs.
 */
export function mermaidBlocks(markdown) {
  // A document saved with CRLF is the same document: a `\r` left on a line
  // would ride into the diagram's source and onto its last word.
  const lines = String(markdown ?? "").split(/\r?\n/);
  const out = [];
  let open = null;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (open === null) {
      const m = /^\s*(```+|~~~+)\s*mermaid\s*$/i.exec(line);
      if (m) open = { fence: m[1], start: i + 2, body: [] };
      continue;
    }
    if (line.trim().startsWith(open.fence)) {
      out.push({ source: open.body.join("\n"), startLine: open.start });
      open = null;
      continue;
    }
    open.body.push(line);
  }
  return out;
}

/**
 * Mermaid's message names a line relative to the snippet. Add the offset so
 * the number is the file's, and say which. `startLine` is the file line the
 * snippet's line 1 sits on.
 */
export function offsetError(message, startLine) {
  const text = String(message ?? "").trim();
  const m = /line\s+(\d+)/i.exec(text);
  if (!m) return { line: null, message: text || t("ui-mermaid-mermaid-could-parse-diagram") };
  const inSnippet = Number(m[1]);
  const line = inSnippet + Math.max(0, (startLine ?? 1) - 1);
  return {
    line,
    message: text.replace(/line\s+\d+/i, `line ${line}`), // content, never translated: a piece of Mermaid's own message, its line number moved to the file's
  };
}

/**
 * Mermaid's theme name: one of its own when `diagrams.theme` pins it, and
 * otherwise `base` — the one Mermaid theme that takes every colour from
 * `themeVariables`, which is how a diagram wears the app's roles instead of
 * Mermaid's idea of light or dark.
 */
export function mermaidTheme(setting) {
  if (typeof setting === "string" && setting && setting !== "follow_app") return setting;
  return "base";
}

/**
 * The roles a diagram is drawn from. Exported so the viewer resolves exactly
 * these and the test can hold them against the contract.
 */
export const DIAGRAM_ROLES = [
  "--color-bg",
  "--color-surface",
  "--color-surface-2",
  "--color-border",
  "--color-text",
  "--color-text-dim",
  "--color-accent",
  "--color-accent-soft",
  "--color-accent-ink",
  "--color-danger",
  "--color-ok",
  "--color-warn",
];

/**
 * Mermaid's `themeVariables` from the resolved roles, so a diagram is made of
 * the same surfaces, borders and ink as the panel it sits in.
 *
 * `resolved` maps a role to a colour Mermaid can parse (`#rrggbb`); resolving
 * is the viewer's job, because it needs a DOM. A role that did not resolve is
 * left out, and Mermaid's `base` theme fills the gap from `darkMode`, which is
 * the one variable that is always set. Nodes are `surface` on a `bg` canvas
 * with `border` edges; the accent is kept for the things that ask for
 * attention — the active state, the highlighted node — and never spent on
 * every box.
 */
export function themeVariablesFor(resolved, scheme, fontFamily) {
  const v = { darkMode: scheme === "dark", fontFamily: fontFamily || "inherit", fontSize: "14px" };
  const c = (role) => {
    const value = typeof resolved?.[role] === "string" ? resolved[role].trim() : "";
    return value || null;
  };
  const put = (name, role) => {
    const value = c(role);
    if (value) v[name] = value;
  };
  put("background", "--color-bg");
  put("mainBkg", "--color-surface");
  put("primaryColor", "--color-surface");
  put("primaryTextColor", "--color-text");
  put("primaryBorderColor", "--color-border");
  put("secondaryColor", "--color-surface-2");
  put("secondaryTextColor", "--color-text");
  put("secondaryBorderColor", "--color-border");
  put("tertiaryColor", "--color-accent-soft");
  put("tertiaryTextColor", "--color-accent-ink");
  put("tertiaryBorderColor", "--color-accent");
  put("textColor", "--color-text");
  put("titleColor", "--color-text");
  put("lineColor", "--color-text-dim");
  put("nodeBorder", "--color-border");
  put("clusterBkg", "--color-surface-2");
  put("clusterBorder", "--color-border");
  put("edgeLabelBackground", "--color-surface-2");
  put("noteBkgColor", "--color-accent-soft");
  put("noteTextColor", "--color-text");
  put("noteBorderColor", "--color-accent");
  put("actorBkg", "--color-surface");
  put("actorBorder", "--color-border");
  put("actorTextColor", "--color-text");
  put("signalColor", "--color-text-dim");
  put("signalTextColor", "--color-text");
  put("labelBoxBkgColor", "--color-surface-2");
  put("labelTextColor", "--color-text");
  put("errorBkgColor", "--color-danger");
  put("errorTextColor", "--color-danger");
  put("taskBkgColor", "--color-accent-soft");
  put("taskBorderColor", "--color-accent");
  put("taskTextColor", "--color-text");
  put("doneTaskBkgColor", "--color-surface-2");
  put("doneTaskBorderColor", "--color-ok");
  put("critBkgColor", "--color-danger");
  put("critBorderColor", "--color-danger");
  put("activeTaskBkgColor", "--color-accent");
  put("activeTaskBorderColor", "--color-accent");
  put("pie1", "--color-accent");
  put("pie2", "--color-ok");
  put("pie3", "--color-warn");
  put("pie4", "--color-danger");
  put("pie5", "--color-text-dim");
  put("pie6", "--color-accent-soft");
  put("gridColor", "--color-border");
  put("todayLineColor", "--color-accent");
  return v;
}

/** Whether a document is a Mermaid file by name. */
export function isMermaidPath(path) {
  return /\.(mmd|mermaid)$/i.test(path ?? "");
}

/**
 * `diagrams.export.scale` as a rasterisation factor: an integer from 1 to 4,
 * and 2 when the setting is unset or unreadable — the registry's own default.
 */
export function exportScale(setting) {
  const n = typeof setting === "number" ? Math.round(setting) : Number.parseInt(String(setting ?? ""), 10);
  if (!Number.isFinite(n)) return 2;
  return Math.min(4, Math.max(1, n));
}
