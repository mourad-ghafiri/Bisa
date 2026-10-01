/**
 * Context chips (ide/09): what a person attaches to a message in the agent
 * pane, and the one rule that makes them intuitive — **nothing is injected
 * that is not a chip**. If the agent will see it, it is here, with a label
 * and a remove button.
 *
 * Plain JavaScript so `node --test` runs it: the labels, the bounds and the
 * deduplication are what a reader trusts, and the component is paint.
 */

import { t } from "../../i18n/l10n.mjs";

/** The core's bound on serialised chips per message, mirrored. */
export const MAX_CONTEXT_BYTES = 64 * 1024;
/** `agents.context.terminal_lines` default. */
const DEFAULT_TERMINAL_LINES = 80;
/** `agents.context.selection_max_lines` default. */
const DEFAULT_SELECTION_LINES = 400;

/** @param {string} path */
export function fileChip(path) {
  return { kind: "file", path };
}

/**
 * @param {string} path @param {number} start @param {number} end @param {string} text @param {number} [maxLines]
 */
export function selectionChip(path, start, end, text, maxLines = DEFAULT_SELECTION_LINES) {
  const lines = text.split("\n");
  const cut = lines.length > maxLines;
  const kept = cut ? lines.slice(0, maxLines).join("\n") + "\n… (cut)" : text;
  const s = Math.max(1, start);
  return {
    kind: "selection",
    path,
    range: { start: s, end: Math.max(s, cut ? s + maxLines - 1 : end) },
    text: kept,
  };
}

/**
 * @param {string} path @param {boolean} staged @param {string} hunkText @param {string} id
 */
export function hunkChip(path, staged, hunkText, id) {
  return { kind: "diff_hunk", path, scope: staged ? { scope: "staged" } : { scope: "unstaged" }, hunk: id, patch: hunkText };
}

/**
 * The last `maxLines` lines of a terminal, trailing blanks dropped.
 * @param {string} session @param {string[]} lines @param {number} [maxLines]
 */
export function terminalChip(session, lines, maxLines = DEFAULT_TERMINAL_LINES) {
  let end = lines.length;
  while (end > 0 && lines[end - 1].trim() === "") end--;
  const tail = lines.slice(Math.max(0, end - maxLines), end).join("\n");
  return { kind: "terminal", session, tail };
}

/** @param {string} id */
export function workItemChip(id) {
  return { kind: "work_item", id };
}

/** @param {string} id */
export function commitChip(id) {
  return { kind: "commit", id };
}

/**
 * Where an annotated element lives (ide/18): a file of the project, or a
 * URL the IDE's browser showed whose source is not a file the agent can
 * name.
 * @param {string} path
 */
export function filePage(path) {
  return { kind: "file", path };
}
/** @param {string} url */
export function urlPage(url) {
  return { kind: "url", url };
}

/** The page as a sentence names it: the path, or the URL. @param {{kind: "file", path: string} | {kind: "url", url: string}} page */
export function pageWords(page) {
  return page.kind === "file" ? page.path : page.url;
}

/**
 * An element of a rendered page the person pointed at (ide/03 §Annotate):
 * where the page is (`page` — a file, or a URL), where the element is
 * (`selector`, a locator hint), what it was (`excerpt`, the head of its
 * HTML — the identity, since a page may outgrow its locator) and the change
 * wanted (`note`).
 * @param {{kind: "file", path: string} | {kind: "url", url: string}} page @param {string} selector @param {string} excerpt @param {string} note
 */
export function annotationChip(page, selector, excerpt, note) {
  return { kind: "annotation", page, selector, excerpt, note };
}

/**
 * What a chip is called on screen.
 * @param {import("../../types").ContextRef} ref
 */
export function chipLabel(ref) {
  const base = (p) => p.split("/").pop() || p;
  switch (ref.kind) {
    case "file":
      return base(ref.path);
    case "selection":
      return `${base(ref.path)}:${ref.range.start}${ref.range.end !== ref.range.start ? `–${ref.range.end}` : ""}`;
    case "diff_hunk":
      return t("workbench-context-chips-hunk", { file: base(ref.path), scope: ref.scope.scope, base: ref.scope.base ?? "" });
    case "terminal":
      return t("workbench-context-chips-terminal", { session: ref.session });
    case "work_item":
      return t("workbench-context-chips-work-item", { ref: ref.id.slice(-6) });
    case "commit":
      return t("workbench-context-chips-commit", { commit: ref.id.slice(0, 7) });
    case "annotation":
      return `${ref.page.kind === "file" ? base(ref.page.path) : pageBase(ref.page.url)} · ${ref.selector.split(" > ").pop() || ref.selector}`;
    case "capture":
      // A device's screen (ide/19): the device, and the rectangle's size or the whole screen.
      return `${ref.label} · ${ref.mark ? `${ref.mark.width}×${ref.mark.height}` : t("workbench-context-chips-screen")}`;
    default:
      return "context";
  }
}

/** A URL's last path segment, else its host — what a chip calls the page. @param {string} url */
function pageBase(url) {
  try {
    const u = new URL(url);
    const last = u.pathname.split("/").filter(Boolean).pop();
    return last ?? u.host;
  } catch {
    return url;
  }
}

/** The identity two chips share when they are the same thing. */
function identity(ref) {
  switch (ref.kind) {
    case "file":
      return `file:${ref.path}`;
    case "selection":
      return `sel:${ref.path}:${ref.range.start}:${ref.range.end}`;
    case "diff_hunk":
      return `hunk:${ref.path}:${ref.scope.scope}:${ref.hunk}`;
    case "terminal":
      return `term:${ref.session}`;
    case "work_item":
      return `wi:${ref.id}`;
    case "commit":
      return `commit:${ref.id}`;
    case "annotation":
      // The same element annotated again is one chip, its note the newer.
      return `ann:${ref.page.kind}:${pageWords(ref.page)}:${ref.selector}`;
    case "capture":
      // The same spot of the same picture is one chip, its note the newer.
      // A key, not words: the rectangle or the whole screen.
      const spot = ref.mark ? `${ref.mark.x},${ref.mark.y},${ref.mark.width},${ref.mark.height}` : "screen";
      return `cap:${ref.device}:${ref.shot.sha256}:${spot}`;
    default:
      return JSON.stringify(ref);
  }
}

/** Whether two chips are the same thing — the same file, the same hunk, the same range. */
export function sameChip(a, b) {
  return identity(a) === identity(b);
}

/**
 * Add a chip, replacing an earlier chip for the same thing (a fresher
 * terminal tail, a re-selected range) rather than stacking two.
 * @param {import("../../types").ContextRef[]} refs
 * @param {import("../../types").ContextRef} ref
 */
export function attach(refs, ref) {
  const id = identity(ref);
  return [...refs.filter((r) => identity(r) !== id), ref];
}

/** @param {import("../../types").ContextRef[]} refs */
export function contextBytes(refs) {
  return new TextEncoder().encode(JSON.stringify(refs)).length;
}

/** Whether the chips fit the wire bound. @param {import("../../types").ContextRef[]} refs */
export function fitsBudget(refs) {
  return contextBytes(refs) <= MAX_CONTEXT_BYTES;
}

/**
 * The leading, non-removable chips that state the placement (ide/09): the
 * project, then the goals it is attached to. A project attached to no goal
 * is the project alone — the agent hears nothing about goals, and the bar
 * says nothing either.
 * @param {{name: string, slug: string} | null} project
 * @param {{id: string, label: string}[]} goals
 */
export function frameChips(project, goals) {
  if (!project) return [];
  const out = [{ kind: "project", label: project.name || project.slug }];
  for (const g of goals) out.push({ kind: "goal", label: g.label, id: g.id });
  return out;
}
