/**
 * Browser, beside Terminal (ide/18): the one button for the embedded
 * browser and everything that puts a page in it. The main click opens a
 * tab on the checkout's newest server, else its newest port, else blank
 * with the field ready. The caret lists *New tab*; on a checkout *From
 * folder…* — the root of the checkout or one folder of it, chosen in
 * `ServeFolderDialog` and served by the node on a port of this machine
 * (`serveDoors.serveAndOpen`, the Files tree's door too); every
 * server up with *Open :port* and *Stop*; the ports the rail attributes
 * here; the tabs already open here; and *Annotate the page for an agent…*
 * when the active tab here shows a page an agent can edit. The words and
 * the order are `serversModel.mjs`'s; the node hosts the servers, so
 * nothing here names a program or a path. `⌘⇧R` opens the door
 * `runCommandModel.runDoor` names: the newest server, else the project's
 * approved run command (the Terminal caret's item, `useRunCommand`), else
 * the folder picker, on the folder served last. Renders nothing outside the
 * desktop shell.
 */

import { useEffect, useState } from "react";
import { api } from "../api";
import { ICON, Menu, Tooltip, cn, sayFailure, useToast } from "../ui";
import type { MenuItem } from "../ui";
import { chordHint } from "../ui/keymapHints";
import { portUrl } from "../views/_workbench/portsModel.mjs";
import { runDoor } from "../views/_workbench/runCommandModel.mjs";
import { annotatable, browserMenu, stoppedWords } from "../views/_workbench/serversModel.mjs";
import { openBrowserAt } from "./browserDoors";
import { setBrowserInspecting } from "./browserInspectorStore";
import { browserLabel, browsersRootedAt, seenRootedAt } from "./browsersModel.mjs";
import { ServeFolderDialog } from "./ServeFolderDialog";
import { lastServedFolder, serveAndOpen } from "./serveDoors";
import { RUN_PROJECT, onDoor } from "./shortcuts";
import { canOpenBrowser, focusBrowserTab, revealBrowserTab, useBrowsers } from "./useBrowsers";
import { runTheCommand, useRunCommand } from "./useRunCommand";
import { useServing } from "./useServing";
import { t } from "../i18n/l10n.mjs";

export function BrowserLauncher({
  scope,
  id,
  rootLabel,
  disabledReason,
  className,
  wordClassName,
}: {
  scope: "goal" | "workstream" | "work_item";
  id: string;
  /** The root's name — what the folder picker calls the checkout's root beside its word. */
  rootLabel: string;
  disabledReason?: string | null;
  className?: string;
  /** Classes for the word beside the glyph — `TerminalLauncher`'s, for a host that folds to glyphs. */
  wordClassName?: string;
}) {
  const toast = useToast();
  const wid = scope === "workstream" ? id : null;
  const { sessions, active } = useBrowsers();
  const here = browsersRootedAt(sessions, scope, id);
  const shown = seenRootedAt(sessions, scope, id).find((s) => s.key === active) ?? null;
  const { servers, all, reload, ports } = useServing(wid);
  // The run command is read here for ⌘⇧R's door alone; its item is the Terminal caret's.
  const { run } = useRunCommand(wid);
  const [starting, setStarting] = useState(false);
  /** The folder the picker opens on, while it is open: the one served here last. */
  const [picking, setPicking] = useState<string | null>(null);
  const noCheckout = disabledReason !== null && disabledReason !== undefined;
  const door = wid ? runDoor({ run, servers }).id : null;
  const menu = browserMenu({
    checkout: wid !== null,
    servers,
    ports,
    tabs: here.map((s) => ({ key: s.key, label: browserLabel(s), headless: s.headless })),
    annotate: shown && annotatable(shown, all) ? { key: shown.key } : null,
    busy: starting || noCheckout,
    door,
  });
  const home = { scope, id };

  // A tab at home here, shown where this root's browser is: the strip in
  // Project Mode, the Details pane while the centre is the conversation or
  // the Board (ide/18 §The browser tab).
  const open = (url: string | null) => void openBrowserAt(home, url);
  const serve = async (folder: string) => {
    if (!wid) return;
    setStarting(true);
    try {
      // The door toasts and opens; the list of servers follows `server_changed`.
      await serveAndOpen(wid, folder);
    } finally {
      setStarting(false);
    }
  };
  const stop = async (serverId: string) => {
    if (!wid) return;
    setStarting(true);
    try {
      const r = await api.stopServer(wid, serverId);
      toast.ok(stoppedWords(r.stopped));
      reload();
    } catch (e) {
      toast.error(sayFailure("browser", t("shell-browser-launcher-could-not-stop"), e));
    } finally {
      setStarting(false);
    }
  };
  const act = (item: string) => {
    if (item === "blank") {
      open(null);
    } else if (item === "run") {
      // ⌘⇧R's door when no server is up and the project's command is
      // approved: the same act as the Terminal caret's item.
      if (wid && run) runTheCommand(wid, run, toast.info);
    } else if (item === "serve-folder") {
      if (wid) setPicking(lastServedFolder(wid));
    } else if (item.startsWith("open:")) {
      const server = servers.find((s) => s.id === item.slice(5));
      if (server) open(server.url);
    } else if (item.startsWith("stop:")) {
      void stop(item.slice(5));
    } else if (item.startsWith("port:")) {
      open(portUrl(Number(item.slice(5))));
    } else if (item.startsWith("tab:")) {
      revealBrowserTab(item.slice(4));
    } else if (item.startsWith("annotate:")) {
      const key = item.slice(9);
      setBrowserInspecting(key, true);
      focusBrowserTab(key);
    }
  };
  // ⌘⇧R: the run door — only where there is a checkout to run or serve.
  // `act` is a plain function remade every render; the door is re-hung on
  // the facts it reads.
  useEffect(() => {
    if (!wid || noCheckout) return;
    return onDoor(RUN_PROJECT, () => act(runDoor({ run, servers }).id));
  }, [wid, noCheckout, servers, run]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!canOpenBrowser()) return null;
  const items: MenuItem[] = menu.items.map((item) => ({
    label: item.label,
    icon: (ICON as Record<string, typeof ICON.page>)[item.icon],
    disabled: item.disabled,
    separatorBefore: item.separatorBefore,
    danger: item.danger,
    shortcut: item.command ? chordHint(item.command) : null,
    onSelect: () => act(item.id),
  }));
  // A checkout not on disk yet: the tab still opens; the menu's serves and stops are held, and the tooltip says why.
  const hint = noCheckout ? `${menu.main.hint} — ${disabledReason}` : menu.main.hint;
  return (
    <>
      <span className={cn("inline-flex items-center", className)}>
        <Tooltip label={hint}>
          <button
            type="button"
            onClick={() => open(menu.main.url)}
            className="anim inline-flex h-7 items-center gap-1.5 rounded-l-control border border-r-0 border-border px-2 text-xs text-text-dim hover:bg-surface-2 hover:text-text"
          >
            <ICON.page size={12} aria-hidden />
            <span className={wordClassName}>{t("shell-browser-launcher-browser")}</span>
            {here.length > 0 && <span className="tnum text-text">{here.length}</span>}
          </button>
        </Tooltip>
        <Menu
          label={t("shell-browser-launcher-open-serve-browser")}
          items={items}
          trigger={
            <span className="anim inline-flex h-7 items-center rounded-r-control border border-border px-1 text-text-dim hover:bg-surface-2 hover:text-text">
              <ICON.expanded size={12} aria-hidden />
            </span>
          }
        />
      </span>
      {wid && (
        <ServeFolderDialog
          open={picking !== null}
          onClose={() => setPicking(null)}
          onServe={(folder) => {
            setPicking(null);
            void serve(folder);
          }}
          wid={wid}
          rootLabel={rootLabel}
          servers={servers}
          initialFolder={picking ?? ""}
        />
      )}
    </>
  );
}
