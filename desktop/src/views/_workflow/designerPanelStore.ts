/**
 * The Workflow Designer's right panel: whether its column is showing and
 * which pane — Properties or Agent. Window furniture, in `localStorage`,
 * remembered once for the machine the way the sidebar's mode is: the rail is
 * the screen's, not a workflow's. The vocabulary and the rules are
 * `designerPanelModel.mjs`; this file only keeps.
 *
 * One module store rather than component state because the keymap shows a
 * pane from outside the screen's tree (`designer_agent`, `designer_properties`,
 * `toggle_designer_panel`), and a link lands on one (`?panel=agent`).
 */

import { useSyncExternalStore } from "react";
import { readPref, switchPref, switchWord, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { DEFAULT_PANE, isPane, paneForSelection, pressPane, type DesignerPane, type DesignerPanelState } from "./designerPanelModel.mjs";

const OPEN_KEY = "bisa.workflow.panel.open";
const TAB_KEY = "bisa.workflow.panel.tab";

let state: DesignerPanelState = {
  open: readPref(webStorage(), OPEN_KEY, switchPref, true),
  tab: readPref(webStorage(), TAB_KEY, (raw) => (isPane(raw) ? raw : undefined), DEFAULT_PANE),
};
const listeners = new Set<() => void>();

function set(next: DesignerPanelState): void {
  if (next.open === state.open && next.tab === state.tab) return;
  state = next;
  writePref(webStorage(), OPEN_KEY, switchWord(next.open));
  writePref(webStorage(), TAB_KEY, next.tab);
  for (const l of listeners) l();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useDesignerPanel(): DesignerPanelState {
  return useSyncExternalStore(subscribe, () => state, () => state);
}

/** Show the column on a pane — opens, never closes. */
export function showDesignerPane(tab: DesignerPane): void {
  set({ open: true, tab });
}

/** A rail tab pressed: open · switch · close. */
export function pressDesignerPane(tab: DesignerPane): void {
  set(pressPane(state, tab));
}

/** Show or hide the column, on the pane it last showed. */
export function toggleDesignerPanel(): void {
  set({ open: !state.open, tab: state.tab });
}

/** A step was picked on the canvas: Properties shows. */
export function revealSelection(selected: string | null): void {
  set(paneForSelection(state, selected));
}
