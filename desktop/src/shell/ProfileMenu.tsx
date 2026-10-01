/**
 * The person, at the far right of the top chrome: their identicon as a
 * button, and under it the three things that are about them rather than
 * about the work — **Identity** (the Settings panel with their keys),
 * **Settings**, and **About Bisa** (the versions). The one destination
 * that used to sit under the sidebar's seven, moved out of the work's way.
 *
 * Before the workspace has answered, the person glyph stands in the same
 * place, disabled, so the bar never jumps.
 */

import { useState } from "react";
import { navigate } from "../router";
import { Avatar, ICON, Menu, Tooltip } from "../ui";
import type { MenuItem } from "../ui";
import { settingsSearch } from "../views/_settings/settingsLink.mjs";
import { AboutDialog } from "./AboutDialog";
import { useChord } from "./useKeymap";
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";

export function ProfileMenu() {
  const ws = useWorkspace();
  const settingsChord = useChord("settings");
  const [about, setAbout] = useState(false);
  const ready = Boolean(ws.info);
  const items: MenuItem[] = ready
    ? [
        { label: t("shell-profile-menu-identity"), icon: ICON.identity, onSelect: () => navigate({ name: "settings" }, settingsSearch("identity")) },
        { label: t("shell-keymap-settings"), icon: ICON.settings, shortcut: settingsChord, onSelect: () => navigate({ name: "settings" }) },
        { label: t("shell-profile-menu-about-bisa"), icon: ICON.info, separatorBefore: true, onSelect: () => setAbout(true) },
      ]
    : [];
  return (
    <>
      <Menu
        label={t("shell-profile-menu-words")}
        items={items}
        trigger={
          <Tooltip label={ready ? t("shell-profile-menu-you") : t("shell-profile-menu-connecting")}>
            <span
              aria-label={t("shell-profile-menu-words")}
              className="anim flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-full hover:bg-surface-2"
            >
              {ws.me ? <Avatar id={ws.me} name={ws.nameOf(ws.me)} photo={ws.photoOf(ws.me)} size={20} /> : <ICON.person size={14} aria-hidden className="text-text-dim" />}
            </span>
          </Tooltip>
        }
      />
      <AboutDialog open={about} onClose={() => setAbout(false)} />
    </>
  );
}
