/**
 * The workbench's top bar, as facts: the two side panels' toggles — the
 * project rail on the left, the right panel — each with its words and its
 * keymap command, and the centre's mode switch — Project · Agent · Board —
 * each mode with its words and glyph, so the header only draws
 * (`PanelDoors.tsx`, `Workbench.tsx`) and the words are checked once. The
 * right panel's occupants are its own vocabulary (`rightPanelModel.mjs`).
 */

import { availableModes, nextMode } from "./ideModeModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const WORDS = Object.freeze({
  rail: Object.freeze({ noun: t("workbench-workbench-chrome-project-rail"), command: "toggle_rail" }),
  right: Object.freeze({ noun: t("workbench-workbench-chrome-right-panel"), command: "toggle_right_panel" }),
});

/**
 * A toggle's words: what pressing it does now, and the keymap command whose
 * chord it shows.
 * @param {"rail" | "right"} side
 * @param {boolean} open whether that side is showing
 * @returns {{label: string, command: string}}
 */
export function toggleWords(side, open) {
  const w = WORDS[side] ?? WORDS.right;
  return { label: `${open ? "Hide" : t("workbench-rail-badges-show")} ${w.noun}`, command: w.command };
}

/** The keymap command that cycles the centre's mode. */
export const MODE_COMMAND = "toggle_ide_mode";

/** The keymap command that puts the centre on the Board, from anywhere. */
export const BOARD_COMMAND = "board";

const MODE_WORDS = Object.freeze({
  project: Object.freeze({ label: t("workbench-right-panel-project"), icon: "file", hint: t("workbench-workbench-chrome-documents-terminals-centre") }),
  agent: Object.freeze({ label: t("workbench-agent-pane-agent"), icon: "agent", hint: t("workbench-workbench-chrome-conversation-agents-centre") }),
  board: Object.freeze({ label: t("workbench-workbench-chrome-board"), icon: "board", hint: t("workbench-workbench-chrome-every-workstream-card-column-centre") }),
});

/**
 * A mode's words on the switch: its label, its glyph (a name in `ICON`),
 * what choosing it does, and the command whose chord the switch shows — the
 * cycle's for Project and Agent, the Board's own for the Board.
 * @param {"project" | "agent" | "board"} mode
 * @returns {{label: string, icon: string, hint: string, command: string}}
 */
export function modeWords(mode) {
  const w = MODE_WORDS[mode] ?? MODE_WORDS.project;
  return { label: w.label, icon: w.icon, hint: w.hint, command: mode === "board" ? BOARD_COMMAND : MODE_COMMAND };
}

/**
 * The sentence for the cycle's palette entry and its command: where the
 * next press goes, from where you are, over the modes offered.
 * @param {"project" | "agent" | "board"} mode the mode you are in
 * @param {boolean} [boardEnabled]
 */
export function modeSwitchLabel(mode, boardEnabled = true) {
  const next = nextMode(mode, boardEnabled);
  return t("workbench-workbench-chrome-switch-mode", { next: modeWords(next).label });
}

/**
 * The switch's segments, in order, for the modes offered.
 * @param {boolean} [boardEnabled]
 * @returns {{id: string, label: string, icon: string, hint: string}[]}
 */
export function modeSegments(boardEnabled = true) {
  return availableModes(boardEnabled).map((id) => ({ id, ...modeWords(id) }));
}
