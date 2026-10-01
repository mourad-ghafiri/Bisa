/**
 * The controls that act on the focused terminal, drawn in the centre strip's
 * trailing slot while a terminal tab is active: new shell here, send to agent,
 * split, close pane, restart. They act on the terminal store only — the
 * layer that draws the shells is the panel's, mounted in `App.tsx`.
 */

import { ICON, StripControlButton, Tooltip } from "../ui";
import { attachContext } from "../views/_workbench/agentPaneStore";
import { terminalChip } from "../views/_workbench/contextChips.mjs";
import { terminalTail } from "../terminal/tails";
import { leaves } from "./paneTreeModel.mjs";
import { isLive, terminalTitle, harnessOf } from "./terminalsModel.mjs";
import {
  closeTerminalPane,
  openTerminalIn,
  restartTerminalTab,
  splitTerminalPane,
  useTerminals,
} from "./useTerminals";
import { t } from "../i18n/l10n.mjs";


export function TerminalStripControls() {
  const { sessions, active, panes, focusedPane } = useTerminals();
  const activeSession = sessions.find((s) => s.key === active) ?? null;
  const manyPanes = leaves(panes).length > 1;
  return (
    <span className="inline-flex items-center gap-0.5 px-1">
      {activeSession && (
        <Tooltip label={t("shell-terminal-strip-controls-new-shell-in", { title: terminalTitle(activeSession) })}>
          <StripControlButton label={t("shell-terminal-launcher-new-shell-here")}
            onClick={() =>
              openTerminalIn({ scope: activeSession.scope, id: activeSession.id, label: activeSession.label })
            }>
            <ICON.add size={13} aria-hidden />
          </StripControlButton>
        </Tooltip>
      )}
      {activeSession && (
        <Tooltip label={t("shell-terminal-strip-controls-attach-terminal-s-last-lines-next")}>
          <StripControlButton label={t("shell-terminal-strip-controls-send-agent")}
            onClick={() => {
              const lines = terminalTail(activeSession.key);
              if (lines) attachContext(terminalChip(`${harnessOf(activeSession) ?? "shell"} · ${activeSession.key}`, lines));
            }}>
            <ICON.agent size={13} aria-hidden />
          </StripControlButton>
        </Tooltip>
      )}
      <Tooltip label={t("shell-terminal-strip-controls-split-focused-pane-right")}>
        <StripControlButton label={t("shell-terminal-strip-controls-split-right")} disabled={!activeSession} onClick={() => splitTerminalPane("row")}>
          <ICON.splitRight size={13} aria-hidden />
        </StripControlButton>
      </Tooltip>
      <Tooltip label={t("shell-terminal-strip-controls-split-focused-pane-downwards")}>
        <StripControlButton label={t("shell-terminal-strip-controls-split-down")} disabled={!activeSession} onClick={() => splitTerminalPane("col")}>
          <ICON.splitDown size={13} aria-hidden />
        </StripControlButton>
      </Tooltip>
      {manyPanes && (
        <Tooltip label={t("shell-terminal-strip-controls-close-focused-pane-tabs-move-next")}>
          <StripControlButton label={t("shell-terminal-strip-controls-close-pane")} onClick={() => closeTerminalPane(focusedPane)}>
            <ICON.closePane size={13} aria-hidden />
          </StripControlButton>
        </Tooltip>
      )}
      {/* Restart only once the shell is gone: a control that could end a live
          one is one misclick from a half-finished rebase. */}
      {activeSession && !isLive(activeSession) && (
        <StripControlButton label={t("shell-terminal-strip-controls-restart")} title={t("shell-terminal-strip-controls-start-shell-again-tab")} onClick={() => restartTerminalTab(activeSession.key)}>
          <span className="inline-flex items-center gap-1 px-1 text-2xs">
            <ICON.refresh size={11} aria-hidden />{t("shell-terminal-strip-controls-restart")}</span>
        </StripControlButton>
      )}
    </span>
  );
}
