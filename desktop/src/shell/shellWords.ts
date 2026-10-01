/**
 * The words the shell's own menus say — the menu bar icon's fixed lines,
 * the Edit menu's verbs — pushed to the shell in the language the window
 * speaks (17 — Internationalisation). The shell builds its menus with
 * English and holds no catalog; the webview says the words once at boot,
 * through the `shell_words` command, and the shell sets them on its items.
 * Silent in a browser.
 */

import { inDesktopShell } from "../api";
import { t } from "../i18n/l10n.mjs";
import { errorFields, log } from "../log";

/** Every word the shell's menus take, from the catalog. */
function shellWords(): Record<string, string> {
  return {
    open: t("shell-words-open-bisa"),
    dock: t("shell-words-show-in-dock"),
    quit: t("shell-words-quit-bisa"),
    cut: t("shell-words-cut"),
    copy: t("shell-words-copy"),
    paste: t("shell-words-paste"),
    select_all: t("shell-words-select-all"),
  };
}

/** Say the words to the shell, once; a shell that is not there hears nothing. */
export async function pushShellWords(): Promise<void> {
  if (!inDesktopShell()) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("shell_words", { words: shellWords() });
  } catch (e) {
    // The menus keep their English; nothing else is affected.
    log.debug("shell", "the menu words did not reach the shell", errorFields(e));
  }
}
