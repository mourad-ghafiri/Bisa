/**
 * The agent a person last handed something to from a document — the
 * editor's selection toolbar, a rendered page's annotations — remembered on
 * this machine (`bisa.ide.agent`) so the next hand-off leads with it.
 * Furniture: losing it only re-defaults to the general agent.
 */

import { readPref, webStorage, writePref } from "../../shell/storedPrefModel.mjs";
import { GENERAL_AGENT } from "./editorAgentModel.mjs";

const AGENT_KEY = "bisa.ide.agent";

/** The agent to lead with: the one used last, else the general agent. */
export function rememberedAgent(): string {
  return readPref(webStorage(), AGENT_KEY, (raw) => raw || GENERAL_AGENT, GENERAL_AGENT);
}

/** The person chose: lead with this one next time. A remembered agent is a convenience; losing it just re-defaults. */
export function rememberAgent(id: string): void {
  writePref(webStorage(), AGENT_KEY, id);
}
