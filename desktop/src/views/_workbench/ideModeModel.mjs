/**
 * The Project IDE's three modes, as facts (ide/09 §Agent Mode, ide/16).
 *
 * **Project** is the centre as documents and terminal tabs; **Agent** is the
 * centre as the workstream's conversation with the agents; **Board** is the
 * centre as every workstream's card in its column. The rail and the right
 * panel are the same in all three — a mode is what the middle is for. What
 * the centre *shows* — `documents`, `conversation` or `board` — is the fact
 * the rest of the desktop reads (`centreOf`): a browser tab at home in the
 * root rides the centre's strip only while it shows documents, and the
 * Details pane's Browser occupant otherwise (ide/18 §The browser tab).
 *
 * Which mode a root is in is furniture, remembered per root the way the
 * right panel remembers its occupant; which mode a root *opens* in, when
 * nothing was remembered for it, is the `ide.default_mode` setting. The
 * Board is offered only while `workstreams.board.enabled` is on: with it
 * off, a root that remembered the Board opens in the default, and the
 * switch shows two modes. Both rules are here so the store only keeps and
 * the header only draws; the memory's own rules — per key, newest last,
 * capped, read back only when real — are `shell/modeMemoryModel.mjs`, which
 * the Workflow screen's switch shares.
 */

import * as memory from "../../shell/modeMemoryModel.mjs";

/** The three modes, in the order the switch shows them. */
export const MODES = Object.freeze(["project", "agent", "board"]);

export const DEFAULT_MODE = "project";

/**
 * The mode a document is shown in. Documents and terminals live in the
 * Project centre, so opening one — a path in a message, a chip on a sent
 * message, a terminal's line, a diff's file — puts its root there: a file
 * opened from Agent Mode that stayed behind the conversation was a file
 * nobody could see.
 */
export const DOCUMENT_MODE = "project";

/** The setting the default comes from. */
export const DEFAULT_MODE_KEY = "ide.default_mode";

/**
 * What a root's centre shows under a mode: the conversation or the Board
 * only on a workstream that has a project — a goal's or a work item's root,
 * and a workstream not yet placed in a project, show documents whatever
 * was remembered — else documents.
 * @param {unknown} mode
 * @param {{scope: string, hasProject: boolean}} root
 * @returns {"documents" | "conversation" | "board"}
 */
export function centreOf(mode, { scope, hasProject }) {
  if (scope !== "workstream" || !hasProject) return "documents";
  if (mode === "agent") return "conversation";
  if (mode === "board") return "board";
  return "documents";
}

/**
 * The modes a root may be in: every one, or all but the Board while the
 * setting hides it.
 * @param {boolean} [boardEnabled]
 */
export function availableModes(boardEnabled = true) {
  return boardEnabled ? [...MODES] : MODES.filter((m) => m !== "board");
}

/**
 * A stored or typed mode made safe to render: the vocabulary can change in a
 * later version, and a value written by an older one must not select a
 * centre that no longer exists — nor one the setting hides.
 * @param {unknown} value
 * @param {boolean} [boardEnabled]
 */
export function modeOf(value, boardEnabled = true) {
  return memory.modeIn(value, availableModes(boardEnabled), DEFAULT_MODE);
}

/**
 * The mode a root is in: what was remembered for it, else the default the
 * settings name — itself clamped, since it crosses the wire.
 * @param {unknown} remembered
 * @param {unknown} fallback
 * @param {boolean} [boardEnabled]
 */
export function modeFor(remembered, fallback, boardEnabled = true) {
  return memory.modeFor(remembered, fallback, availableModes(boardEnabled), DEFAULT_MODE);
}

/**
 * The mode a cycle goes to: the next in the switch's order, round to the
 * first — Project, Agent, Board — over the modes offered.
 * @param {unknown} mode
 * @param {boolean} [boardEnabled]
 */
export function nextMode(mode, boardEnabled = true) {
  return memory.nextMode(mode, availableModes(boardEnabled), DEFAULT_MODE);
}

/**
 * Remember a root's mode, newest last, the oldest forgotten past `cap` —
 * the right panel's own rule for its occupant memory.
 * @param {Readonly<Record<string, string>>} byRoot
 * @param {string} root
 * @param {string} mode
 * @param {number} [cap]
 */
export function rememberMode(byRoot, root, mode, cap = memory.DEFAULT_CAP) {
  return memory.rememberMode(byRoot, root, modeOf(mode), cap);
}

/**
 * A stored memory read back: only real roots and real modes survive.
 * @param {unknown} raw
 * @returns {Record<string, string>}
 */
export function parseRememberedModes(raw) {
  return memory.parseRememberedModes(raw, MODES);
}
