/**
 * Handing an editor selection (or a file, or the open tabs) to an agent,
 * as facts with no React in them.
 *
 * The person selects code, presses **Ask** or **Edit**, picks an agent (the
 * general agent by default), and types. This builds the message that goes to
 * that agent — the instruction as `content`, the code as a `Selection`/`File`
 * context chip, addressed with `mentions` — the same shape `ReviewStep` posts.
 * An **edit** tells the agent to change the file in place and leave the keep
 * or undo to the person in the conversation (ide/20); nothing is committed for them.
 *
 * Where the toolbar *sits* is arithmetic, so it is here too: above the
 * selection when there is room, below it when there is not, and never off the
 * edge of the editor it belongs to.
 */

import { addressableIn, reachableIn } from "../_studio/addressModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The agent chosen when a person has not picked another. */
export const GENERAL_AGENT = "general-agent";

/**
 * What a range reads as in a sentence: `main.rs:12` for one line, `main.rs:12–18`
 * for many. Basename only — the chip already carries the full path.
 */
export function rangeLabel(path, start, end) {
  const name = String(path ?? "").split("/").pop() || String(path ?? "");
  return start === end ? `${name}:${start}` : `${name}:${start}–${end}`;
}

/** The instruction that goes with an **edit** — the person's words, then the contract. */
export function editContent(instruction, target) {
  const what = instruction.trim();
  const lead = what ? `${what}\n\n` : "";
  return t("workbench-editor-agent-edit-directly-file-do-not-commit", { lead, target });
}

/** The instruction that goes with an **ask** — the person's question, verbatim. */
export function askContent(question, target) {
  const q = question.trim();
  return q ? t("workbench-editor-agent-question-about-target", { lead: `${q}\n\n`, target }) : t("workbench-editor-agent-tell-me-about", { target });
}

/**
 * The message body to post to the chosen agent.
 * @param {object} input
 * @param {"ask"|"edit"} input.mode
 * @param {string} input.agentId
 * @param {string} input.text the person's question or edit instruction
 * @param {string} input.target a human phrase for what is attached ("the selection main.rs:12–18", "the file main.rs", "3 open files")
 * @param {readonly object[]} input.chips the `ContextRef`s (a selection / file / files)
 * @returns {{content: string, mentions: string[], context: object[]}}
 */
export function messageBody({ mode, agentId, text, target, chips }) {
  return {
    content: mode === "edit" ? editContent(text ?? "", target) : askContent(text ?? "", target),
    mentions: [agentId],
    context: [...(chips ?? [])],
  };
}

/** A hair of air between the toolbar and the line it is about. */
const GAP = 6;

/**
 * Where the toolbar goes, in the editor box's own pixels.
 *
 * **Above the selection's first line** when it fits, because a bar under the
 * last line covers the code you are about to read; below it otherwise. Both
 * axes are clamped to the box, so a selection ending at the right edge of a
 * long line does not push the bar out of the pane. `null` when neither end of
 * the selection is on screen — the person scrolled away from it, and a bar
 * pinned at the top-left corner is worse than no bar.
 *
 * @param {{start: {top: number, left: number} | null, end: {bottom: number, left: number} | null}} anchors
 *        where the selection begins and ends, from the editor
 * @param {{width: number, height: number}} box the editor's own rectangle
 * @param {{width: number, height: number}} bar what the toolbar measures
 * @returns {{top: number, left: number, above: boolean} | null}
 */
export function toolbarPlacement({ anchors, box, bar }) {
  const start = anchors?.start ?? null;
  const end = anchors?.end ?? null;
  if (!start && !end) return null;
  const above = start !== null && start.top - bar.height - GAP >= 0;
  const top = above
    ? start.top - bar.height - GAP
    : (end ? end.bottom : (start?.top ?? 0)) + GAP;
  const left = (above ? start.left : (end ?? start).left) ?? 0;
  const clamp = (v, max) => Math.max(0, Math.min(v, Math.max(0, max)));
  return {
    top: clamp(top, box.height - bar.height),
    left: clamp(left, box.width - bar.width),
    above,
  };
}

/**
 * The agents the toolbar offers, in the order it offers them: the one you
 * used last, then everyone else this conversation can address.
 *
 * `addressableIn(…, "workstream")` is the same filter every other addressing
 * surface on a checkout applies — no core agent, nobody disabled, never the
 * Workflow Agent — so the toolbar cannot offer an agent the composer would
 * refuse. The default agent is appended when it is not among them (it may be
 * the General Agent, which is exactly who an unaddressed message reaches), so
 * the pill always names somebody who will answer — unless the default is the
 * Workflow Agent, which a workstream never reaches (`reachableIn`).
 * @param {readonly {id: string, name: string, enabled?: boolean}[]} agents
 * @param {string | null | undefined} remembered the agent last used here
 * @param {{id: string, name: string} | null} fallback the project's default agent
 */
export function agentChoices(agents, remembered, fallback) {
  const offered = addressableIn(agents ?? [], "workstream");
  const out = [...offered];
  if (fallback && reachableIn(fallback, "workstream") && !out.some((a) => a.id === fallback.id)) out.push(fallback);
  const first = out.findIndex((a) => a.id === remembered);
  if (first > 0) out.unshift(...out.splice(first, 1));
  return out;
}

/**
 * The agent a surface posts to: the one wanted when it is among the choices,
 * else the first choice — the list's lead — else nobody. A surface that
 * seeds its choice from a setting (the project's `agents.default`) or from
 * memory never names an agent the choices would refuse.
 * @template {{id: string}} T
 * @param {readonly T[]} choices
 * @param {string | null | undefined} wanted
 * @returns {T | null}
 */
export function chosenAgent(choices, wanted) {
  return choices.find((a) => a.id === wanted) ?? choices[0] ?? null;
}

/** What the toast says once a message is away — an edit names where it lands. */
export function sentWords(mode, agentName) {
  return mode === "edit"
    ? t("workbench-editor-agent-asked-edit-change-waits-conversation-keep", { agentName })
    : t("workbench-editor-agent-asked", { agentName });
}
