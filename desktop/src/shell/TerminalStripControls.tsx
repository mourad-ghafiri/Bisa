/**
 * The controls that act on the focused terminal, drawn in the centre strip's
 * trailing slot while a terminal tab is active: new shell here, send to agent,
 * split, close pane, restart. They act on the terminal store only — the
 * layer that draws the shells is the panel's, mounted in `App.tsx`.
 */

import { useEffect, useState } from "react";
import { FOCUS_RING, ICON, StripControlButton, Tooltip, cn } from "../ui";
import { attachContext } from "../views/_workbench/agentPaneStore";
import { terminalChip } from "../views/_workbench/contextChips.mjs";
import { terminalTail } from "../terminal/tails";
import { leaves } from "./paneTreeModel.mjs";
import { terminalChipName } from "./terminalChipModel.mjs";
import { isLive, terminalTitle } from "./terminalsModel.mjs";
import {
  closeTerminalPane,
  openTerminalIn,
  restartTerminalTab,
  splitTerminalPane,
  useTerminals,
} from "./useTerminals";
import { t } from "../i18n/l10n.mjs";


/**
 * Whether the tab has printed nothing yet — *Send to agent* has nothing to
 * send. The buffer is read lazily (`terminal/tails.ts`), so it is asked
 * once a second only while it is blank, and never again once it has
 * printed: a fresh shell's prompt arrives within a breath.
 */
function useBlankTail(key: string | null): boolean {
  const [blank, setBlank] = useState(false);
  useEffect(() => {
    if (!key) return;
    const check = () => {
      const lines = terminalTail(key);
      const nothing = !lines || lines.every((l) => l.trim() === "");
      setBlank(nothing);
      return nothing;
    };
    if (!check()) return;
    const timer = window.setInterval(() => {
      if (!check()) window.clearInterval(timer);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [key]);
  return blank;
}

export function TerminalStripControls() {
  const { sessions, active, panes, focusedPane } = useTerminals();
  const activeSession = sessions.find((s) => s.key === active) ?? null;
  const manyPanes = leaves(panes).length > 1;
  const blank = useBlankTail(activeSession?.key ?? null);
  const nothingToSend = t("shell-terminal-strip-controls-nothing-to-send");
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
      {activeSession &&
        (blank ? (
          // Disabled takes no pointer and no focus, so the reason rides a
          // focusable wrapper — the kit Button's `disabledReason`, in the strip's shape.
          <Tooltip label={nothingToSend}>
            <span tabIndex={0} className={cn("inline-flex shrink-0 rounded-control", FOCUS_RING)}>
              <StripControlButton label={t("shell-terminal-strip-controls-send-agent")} disabled onClick={() => undefined}>
                <ICON.agent size={13} aria-hidden />
              </StripControlButton>
              <span className="sr-only">{nothingToSend}</span>
            </span>
          </Tooltip>
        ) : (
          <Tooltip label={t("shell-terminal-strip-controls-attach-terminal-s-last-lines-next")}>
            <StripControlButton label={t("shell-terminal-strip-controls-send-agent")}
              onClick={() => {
                const lines = terminalTail(activeSession.key);
                if (lines) attachContext(terminalChip(terminalChipName(activeSession, sessions), lines));
              }}>
              <ICON.agent size={13} aria-hidden />
            </StripControlButton>
          </Tooltip>
        ))}
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
