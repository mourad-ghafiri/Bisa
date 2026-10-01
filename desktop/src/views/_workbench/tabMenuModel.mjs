/**
 * What a right-click on a tab offers (ide/03, ide/06), as facts: a document
 * tab's verbs and a terminal tab's, in order, with the keymap command each is
 * another door to. The views map ids to actions; nothing here touches a
 * store. The strip itself knows none of this — it draws what it is handed.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * A document tab's menu.
 * @param {{
 *   pinned: boolean,
 *   preview: boolean,        a preview tab (italic) — offers *Keep open*
 *   others: number,          the other closeable documents in the strip
 *   right: number,           the closeable documents to the right of this one
 *   saved: number,           the closeable documents with nothing unsaved, this one included
 *   canSplit: boolean,       the pane has two or more documents
 *   inRoot: boolean,         a file under the root — a relative path to copy, a row in Files to reveal
 *   onDisk: boolean,         a file on this machine at all (under the root, or loose) — an absolute path to copy, the file manager to reveal in
 *   desktop: boolean,        the shell is here (reveal needs a file manager)
 *   reveal: string,          what the OS calls its file manager
 * }} ctx
 * @returns {{id: string, label: string, command?: string, danger?: boolean, disabled?: boolean, separatorBefore?: boolean}[]}
 */
export function docTabMenu({ pinned, preview, others, right, saved, canSplit, inRoot, onDisk, desktop, reveal }) {
  const items = [
    { id: "close", label: pinned ? t("workbench-tab-menu-close-pinned-unpin-first") : t("workbench-project-rail-close"), command: "close_tab", disabled: pinned },
    { id: "close-others", label: t("workbench-center-documents-close-others"), command: "close_others", disabled: others === 0 },
    { id: "close-right", label: t("workbench-tab-menu-close-right"), disabled: right === 0 },
    { id: "close-saved", label: t("workbench-tab-menu-close-saved"), command: "close_saved", disabled: saved === 0 },
    { id: "close-all", label: t("workbench-center-documents-close-all"), disabled: others === 0 && pinned },
  ];
  if (preview) items.push({ id: "keep", label: t("workbench-tab-menu-keep-open"), command: "keep_tab", separatorBefore: true });
  items.push(
    { id: "pin", label: pinned ? t("workbench-tab-menu-unpin") : t("workbench-tab-menu-pin"), separatorBefore: !preview },
    { id: "split-right", label: t("workbench-center-documents-split-right"), disabled: !canSplit },
    { id: "split-down", label: t("workbench-center-documents-split-down"), disabled: !canSplit },
  );
  if (inRoot) items.push({ id: "copy-path", label: t("workbench-tab-menu-copy-relative-path"), separatorBefore: true });
  if (onDisk) items.push({ id: "copy-absolute", label: t("workbench-tab-menu-copy-absolute-path"), separatorBefore: !inRoot });
  if (inRoot) items.push({ id: "reveal-files", label: t("workbench-tab-menu-reveal-files"), command: "reveal_in_files" });
  if (onDisk && desktop) items.push({ id: "reveal-os", label: reveal, command: "reveal_in_finder" });
  return items;
}

/**
 * A terminal tab's menu — a plain shell's or a harness shell's.
 * @param {{
 *   live: boolean,           the shell is running
 *   harness: boolean,        it runs a harness (an agent), so the Agent panel knows it
 *   others: number,          the other tabs rooted here
 *   exited: number,          the exited tabs rooted here, this one included
 * }} ctx
 */
/**
 * A browser tab's menu (ide/18): reload, its URL, the machine's browser for
 * the page, the page's annotation for an agent when the page can take one,
 * then the closes.
 * @param {{
 *   others: number,        the other browser tabs rooted here
 *   blank: boolean,        nothing loaded yet — nothing to reload, copy or open outside
 *   annotatable: boolean,  a page served on this machine, in a workstream — an agent can edit it
 * }} ctx
 */
export function browserTabMenu({ others, blank, annotatable }) {
  const items = [
    { id: "reload", label: t("workbench-device-doc-reload"), disabled: blank },
    { id: "copy-url", label: t("workbench-tab-menu-copy-url"), disabled: blank, separatorBefore: true },
    { id: "open-outside", label: t("workbench-tab-menu-open-machine-s-browser"), disabled: blank },
  ];
  if (annotatable && !blank) items.push({ id: "annotate", label: t("workbench-tab-menu-annotate-page-agent"), separatorBefore: true });
  items.push({ id: "close", label: t("workbench-project-rail-close"), command: "close_tab", separatorBefore: true });
  items.push({ id: "close-others", label: others === 0 ? t("workbench-tab-menu-close-other-browser-tabs") : t("workbench-tab-menu-close-other-browser-tab-browser-tabs", { others }), danger: true, disabled: others === 0 });
  return items;
}

export function terminalTabMenu({ live, harness, others, exited }) {
  const items = [
    { id: "focus", label: t("workbench-tab-menu-focus") },
    { id: "new-shell", label: t("workbench-rail-menu-new-shell-here"), command: "new_terminal" },
  ];
  if (!live) items.push({ id: "restart", label: t("workbench-device-doc-restart") });
  items.push({ id: "send-to-agent", label: t("workbench-tab-menu-send-last-lines-agent"), separatorBefore: true });
  if (harness) items.push({ id: "show-agents", label: t("workbench-rail-menu-show-agent-panel"), command: "panel_agents" });
  items.push(
    // A live harness is terminated — its session and its tab go together; a
    // shell is closed; anything exited is just closed.
    { id: "close", label: live ? (harness ? t("workbench-project-rail-terminate") : t("workbench-tab-menu-close-ends-shell")) : t("workbench-project-rail-close"), command: "close_tab", danger: live, separatorBefore: true },
    { id: "close-others", label: others === 0 ? t("workbench-center-documents-close-others") : t("workbench-tab-menu-close-other-tab-tabs", { others }), danger: true, disabled: others === 0 },
    { id: "close-exited", label: exited === 0 ? t("workbench-tab-menu-close-exited") : t("workbench-tab-menu-close-exited-2", { exited }), disabled: exited === 0 },
  );
  return items;
}
