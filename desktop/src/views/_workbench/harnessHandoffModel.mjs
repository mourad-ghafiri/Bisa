/**
 * Annotations typed into a harness of the person's own (ide/18 §Annotating a
 * page, ide/06 §Start words): the third door under an annotated page,
 * beside the agent's.
 *
 * An agent gets the request as a conversation message whose chips the engine
 * frames (`framing.rs context_block`). A harness in a terminal has no engine
 * between it and the words, so the tray renders them itself — the same edit
 * sentence, the same block the engine would print, byte for byte (a test
 * reads the Rust) — and the tab types them: into a **new** terminal once
 * the harness has drawn its prompt, or into a **live** one of the workstream
 * once its harness listens — held while it works, never typed into a turn.
 * Which terminals there are, whether one may take the words right now, and
 * what the toast says are decided here; the tray draws.
 *
 * Never to a session the engine runs — only to the person's own terminal, by
 * their choice (the hand-off rule's one exception, ide/09).
 */

import { t } from "../../i18n/l10n.mjs";
import { liveChildren, stateOf, waitWords } from "../../ui/sessionState.mjs";
import { annotationChips, annotationsContent, annotationsTarget } from "./annotationModel.mjs";
import { pageWords } from "./contextChips.mjs";
import { standingHarnesses } from "./railHarnessesModel.mjs";

// for the machine
const CONTEXT_HEADER = "--- context the person attached ---";

/**
 * The engine's `context_block` for annotation chips, as `framing.rs` prints
 * it: the header, then each chip as `[annotation] <page> · <selector>`, its
 * excerpt fenced as html, its note; empty for none.
 * @param {readonly object[]} chips
 */
export function contextBlock(chips) {
  const lines = [];
  for (const chip of chips ?? []) {
    if (!chip || chip.kind !== "annotation") continue;
    // for the machine
    lines.push(`[annotation] ${pageWords(chip.page)} · ${chip.selector}`, "```html", String(chip.excerpt ?? "").trimEnd(), "```", String(chip.note ?? "").trim());
  }
  return lines.length ? `${CONTEXT_HEADER}\n${lines.join("\n")}\n` : "";
}

/** The instruction a harness gets with the edit — the person's words, then the contract, kept or undone by hand. */
export function handoffContent(instruction, target) {
  const what = String(instruction ?? "").trim();
  const lead = what ? `${what}\n\n` : "";
  return t("workbench-harness-handoff-edit-directly-file-do-not-commit", { lead, target });
}

/**
 * The text typed into the harness: the request as the agent would get it, then the block.
 * @param {{message: string, count: number, page: import("./contextChips.mjs").PageRef, annotations: readonly import("./annotationModel.mjs").Annotation[]}} draft
 */
export function harnessWords({ message, count, page, annotations }) {
  const words = handoffContent(annotationsContent(message, count), annotationsTarget(count, page));
  const block = contextBlock(annotationChips(page, annotations));
  return block ? `${words}\n\n${block.trimEnd()}` : words;
}

/** A sub-agent's dialog, when the session itself is not waiting but one of its live children is: the row's wait is that child's. */
function childWait(sessions, sessionId) {
  const row = sessionId ? (sessions ?? []).find((s) => s.id === sessionId) : null;
  if (!row || stateOf(row.state) === "waiting") return null;
  return liveChildren(row).find((c) => stateOf(c.state) === "waiting")?.state ?? null;
}

/**
 * The terminals the words can go to, in the menu's order: a new one per
 * harness with an interactive form — installed first, the rest kept and
 * disabled — then the workstream's live harness tabs, each read with its
 * sub-agents' dialogs as its own (a child's question is still a dialog the
 * Enter would answer). `null` while the harness list is unknown: the door
 * waits, as every picker does.
 * @param {readonly import("../../types").HarnessRow[] | null} harnesses
 * @param {readonly import("../../types").SessionRow[]} sessions
 * @param {readonly import("../../shell/terminalsModel.mjs").TerminalSessionState[]} terminals
 * @param {string} wid
 * @returns {import("./harnessHandoffModel.d.mts").HarnessTarget[] | null}
 */
export function harnessTargets(harnesses, sessions, terminals, wid) {
  if (!Array.isArray(harnesses)) return null;
  const fresh = harnesses
    .filter((h) => h && h.launch !== null && h.launch !== undefined)
    .slice()
    .sort((a, b) => (a.installed !== b.installed ? (a.installed ? -1 : 1) : String(a.label).localeCompare(String(b.label))))
    .map((h) => ({ kind: "new", harness: h.id, label: h.label, installed: h.installed === true, detail: typeof h.detail === "string" && h.detail ? h.detail : null }));
  const live = standingHarnesses(sessions, terminals, wid)
    .filter((s) => s.terminalKey)
    .map((s) => ({ kind: "tab", key: s.terminalKey, harness: s.harness, state: childWait(sessions, s.sessionId) ?? s.state, sessionId: s.sessionId }));
  return [...fresh, ...live];
}

/** The harness's name for a target: the catalog's label, else its id. */
function nameOf(target, labels) {
  return target.kind === "new" ? target.label : (labels?.[target.harness] ?? target.harness);
}

/**
 * Whether a target may take the words now, and the words beside it: a
 * harness not installed cannot (its label says so, and where to get it); a
 * live tab can while it listens (idle), with a word while it works (the
 * request rides the tab and is typed once its turn ends) or when it does not
 * report (typed as it stands); never while it is still starting or waiting
 * on the person — a dialog typed over is a dialog answered by accident.
 * @param {import("./harnessHandoffModel.d.mts").HarnessTarget} target
 * @returns {{ok: boolean, words: string | null}}
 */
export function harnessDoor(target) {
  // A harness not installed is held with no second line: the label already carries the node's word on where to get it, as the launcher's row does.
  if (target.kind === "new") return target.installed ? { ok: true, words: null } : { ok: false, words: null };
  if (target.state === null || target.state === undefined) return { ok: true, words: t("workbench-harness-handoff-does-not-report-typed-as-stands") };
  const word = stateOf(target.state);
  if (word === "idle") return { ok: true, words: null };
  if (word === "thinking" || word === "running") return { ok: true, words: t("workbench-harness-handoff-working-typed-after-turn") };
  if (word === "starting") return { ok: false, words: t("workbench-harness-handoff-still-starting-no-prompt-yet") };
  if (word === "waiting") {
    const on = waitWords(target.state);
    return { ok: false, words: on ? t("workbench-harness-handoff-waiting-on-you-answer-first", { on }) : t("workbench-harness-handoff-waiting-on-you-answer-first-2") };
  }
  return { ok: false, words: t("workbench-harness-handoff-harness-has-ended") };
}

/** A tab's state as the menu words it. */
function stateLabel(target, labels) {
  const h = nameOf(target, labels);
  if (target.state === null || target.state === undefined) return t("workbench-harness-handoff-open", { h });
  const word = stateOf(target.state);
  if (word === "idle") return t("workbench-harness-handoff-listening", { h });
  if (word === "thinking" || word === "running") return t("workbench-harness-handoff-working", { h });
  if (word === "starting") return t("workbench-harness-handoff-starting", { h });
  if (word === "waiting") return t("workbench-harness-handoff-waiting", { h });
  return t("workbench-harness-handoff-ended", { h });
}

/**
 * The menu's rows: one per target, its words, disabled with the door's reason
 * when it cannot take the words, the live tabs after a rule.
 * @param {readonly import("./harnessHandoffModel.d.mts").HarnessTarget[]} targets
 * @param {Readonly<Record<string, string>>} labels harness id → the catalog's label
 * @returns {import("./harnessHandoffModel.d.mts").HarnessTargetRow[]}
 */
export function targetRows(targets, labels) {
  let firstTab = true;
  return (targets ?? []).map((target) => {
    const door = harnessDoor(target);
    const row = {
      id: target.kind === "new" ? `new:${target.harness}` : `tab:${target.key}`,
      target,
      label:
        target.kind === "new"
          ? target.installed
            ? t("workbench-harness-handoff-new-session", { h: target.label })
            : t("workbench-harness-handoff-not-installed", { h: target.label, detail: target.detail ?? t("workbench-harness-handoff-not-installed-word") })
          : stateLabel(target, labels),
      harness: target.harness,
      disabled: !door.ok,
      hint: door.words,
      separatorBefore: target.kind === "tab" && firstTab,
    };
    if (target.kind === "tab") firstTab = false;
    return row;
  });
}

/**
 * A fresh terminal for the harness, the words typed once it listens: never
 * resumed — the request opens a session of its own.
 * @param {string} wid
 * @param {string} harness
 * @param {string} words
 */
export function handOffTarget(wid, harness, words) {
  return { scope: "workstream", id: wid, harness, resume: false, start: words };
}

/**
 * The toast once the words are on their way, naming the harness and where
 * the rows stay: a new terminal types them once it listens; a live tab at
 * work holds them until its turn ends; one listening, or not reporting, has
 * them typed now.
 */
export function sentToWords(target, labels, noun) {
  const h = nameOf(target, labels);
  if (target.kind === "new") return t("workbench-harness-handoff-opening-new-terminal-typed-once-listens", { h, noun });
  const word = target.state === null || target.state === undefined ? "idle" : stateOf(target.state);
  if (word === "thinking" || word === "running") return t("workbench-harness-handoff-handed-typed-after-turn", { h, noun });
  return t("workbench-harness-handoff-typed-into-terminal-stay-until-clear", { h, noun });
}
