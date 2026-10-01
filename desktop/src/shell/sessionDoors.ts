/**
 * The one way to open a terminal tab or a harness session from anywhere —
 * the project rail, the footer, a menu. A tab is focused in the terminal
 * layer and, when it is rooted at a place the workbench can show, the
 * route moves there with the tab as the document (`?doc=terminal:<key>`);
 * a machine-rooted tab lives in the terminal layer alone. A session opened
 * in a terminal opens as its tab; an engine session is followed, its
 * workstream shown and the Agents pane opened on it.
 */

import { navigate } from "../router";
import type { WorkbenchScope } from "../router";
import { showRightPanel } from "../views/_workbench/rightPanelStore";
import { followSession } from "./followedSessionStore";
import type { TerminalSessionState } from "./terminalsModel.mjs";
import { focusTerminalTab } from "./useTerminals";

type TerminalScope = TerminalSessionState["scope"];

/** The tab's place, as the workbench roots: every terminal scope but the machine's. */
function workbenchScopeOf(scope: TerminalScope): WorkbenchScope | null {
  return scope === "machine" ? null : scope;
}

/** Focus a terminal tab and show the place it is rooted at, with the tab as the document. */
export function openTerminalTab(tab: { key: string; scope: TerminalScope; id: string }): void {
  focusTerminalTab(tab.key);
  const scope = workbenchScopeOf(tab.scope);
  if (scope) navigate({ name: "workbench", scope, id: tab.id }, { doc: `terminal:${tab.key}` });
}

/**
 * Open a harness session: its tab when a person opened it in a terminal
 * (`terminalKey`), else the workstream with the Agents pane on it and the
 * session followed. A session in no checkout has nowhere to open: false.
 */
export function openHarnessSession(session: { id: string; workstream: string | null; terminalKey?: string | null }): boolean {
  if (!session.workstream) return false;
  if (session.terminalKey) {
    openTerminalTab({ key: session.terminalKey, scope: "workstream", id: session.workstream });
    return true;
  }
  followSession(session.workstream, session.id);
  navigate({ name: "workbench", scope: "workstream", id: session.workstream });
  showRightPanel("agents", `workstream:${session.workstream}`);
  return true;
}
