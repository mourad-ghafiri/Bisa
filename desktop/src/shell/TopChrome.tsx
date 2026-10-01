/**
 * The strip across the top: where you are, what is running, and the search.
 *
 * It exists for five reasons now. The window needs somewhere to be dragged
 * from (a Tauri webview has no title bar of its own); navigation needs Back
 * and Forward; the reader needs one place that answers "is anything happening
 * right now, and is the node still there"; search needs to be a target you
 * can hit with the mouse rather than a shortcut you have to have been told
 * about; and the person needs one place that is about them — the profile at
 * the far right (`ProfileMenu`: Identity, Settings, About), out of the
 * sidebar's way.
 *
 * The status indicators live here rather than in the sidebar because the
 * sidebar can be closed. "The engine is paused" and "the node is unreachable"
 * both explain why nothing is moving, and an explanation you can hide is an
 * explanation nobody gets. The sidebar keeps its own offline card as the
 * fuller version — this is the part that cannot be dismissed.
 *
 * Pinned in px, not rem, so ⌘+/− zooms the content and leaves the chrome.
 */

import { toggleWords } from "./sidebarModel.mjs";
import type { SidebarMode } from "./sidebarModel.mjs";
import { useMemo, type ReactNode } from "react";
import { back, forward, href, section, useRoute } from "../router";
import { Chip, ICON, PageHeader, Tooltip, WorkingDot } from "../ui";
import { CommandHint } from "./CommandHint";
import { whereYouAre } from "./nav";
import { ProfileMenu } from "./ProfileMenu";
import { useWorkspace } from "./useWorkspaceData";
import { degradedWords } from "./workspaceLoadModel.mjs";
import { t } from "../i18n/l10n.mjs";

const SLOT_ID = "top-chrome-slot";

function ChromeButton({
  onClick,
  label,
  children,
}: {
  onClick: () => void;
  label: string;
  children: ReactNode;
}) {
  return (
    <Tooltip label={label}>
      <button
        type="button"
        onClick={onClick}
        aria-label={label}
        className="anim flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
      >
        {children}
      </button>
    </Tooltip>
  );
}

/**
 * What is running right now, in one chip.
 *
 * The three conditions are ranked, not stacked: unreachable beats paused
 * beats busy, because each one makes the next irrelevant. Showing all three
 * at once would be three chips saying the same thing — nothing is moving —
 * in a strip that has no room to say it three times.
 */
function LiveStatus() {
  const ws = useWorkspace();
  const busy = useMemo(
    () => new Set(Object.values(ws.working).flat()).size,
    [ws.working],
  );

  if (ws.offline) {
    return (
      <Chip tone="danger" icon={ICON.warn} title={ws.offline}>{t("shell-top-chrome-node-unreachable")}</Chip>
    );
  }
  const degraded = degradedWords(ws.degraded, !!ws.offline);
  if (degraded) {
    return (
      <Chip tone="warn" icon={ICON.warn} title={degraded.title}>
        {degraded.label}
      </Chip>
    );
  }
  if (ws.paused) {
    return (
      <Chip tone="warn" icon={ICON.waiting} title={t("shell-top-chrome-engine-paused-nothing-will-start")}>{t("shell-top-chrome-paused")}</Chip>
    );
  }
  if (busy === 0) return null;
  return (
    <a
      href={href({ name: "pulse" })}
      title={t("shell-top-chrome-see-what-happening")}
      className="anim inline-flex h-5 items-center gap-1.5 rounded-full border border-transparent bg-accent-soft px-2 text-2xs font-medium text-accent-ink hover:opacity-90"
    >
      <WorkingDot />
      {busy === 1 ? t("shell-top-chrome-1-agent-writing") : t("shell-top-chrome-agents-writing", { busy })}
    </a>
  );
}

export function TopChrome({
  sidebarMode,
  onToggleSidebar,
  onOmnibox,
}: {
  sidebarMode: SidebarMode;
  onToggleSidebar: () => void;
  onOmnibox: () => void;
}) {
  const route = useRoute();
  const { label, icon } = whereYouAre(section(route));

  return (
    <header
      data-tauri-drag-region
      data-pane
      className="flex h-chrome shrink-0 items-center gap-1 border-b border-border bg-surface pl-3 pr-2"
    >
      <ChromeButton onClick={onToggleSidebar} label={toggleWords(sidebarMode)}>
        {sidebarMode === "expanded" ? (
          <ICON.panelOpen size={14} aria-hidden />
        ) : (
          <ICON.panelClosed size={14} aria-hidden />
        )}
      </ChromeButton>
      <ChromeButton onClick={back} label={t("shell-browser-bar-back")}>
        <ICON.back size={14} aria-hidden />
      </ChromeButton>
      <ChromeButton onClick={forward} label={t("shell-browser-bar-forward")}>
        <ICON.forward size={14} aria-hidden />
      </ChromeButton>

      <PageHeader
        level="h1"
        title={label}
        icon={icon}
        className="min-w-0 flex-1 items-center px-2 py-0"
        actions={<LiveStatus />}
      />

      <button
        type="button"
        onClick={onOmnibox}
        className="anim flex h-6 w-56 shrink-0 items-center gap-2 rounded-control border border-border bg-bg px-2 text-2xs text-text-dim hover:border-accent/40 hover:text-text"
      >
        <ICON.search size={12} aria-hidden className="shrink-0" />
        <span className="flex-1 text-left">{t("shell-top-chrome-search-jump")}</span>
        {/* KeyHint, not a literal ⌘: the shortcut is Ctrl+K off macOS, and a
            Mac glyph there names a key the keyboard does not have. */}
        <CommandHint id="omnibox" />
      </button>

      <div id={SLOT_ID} className="flex shrink-0 items-center gap-2" data-tauri-drag-region />

      {/* The person, last on the bar: Identity, Settings, About. */}
      <ProfileMenu />
    </header>
  );
}
