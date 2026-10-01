/**
 * The Browser button beside Terminal (ide/18), as facts: what its main
 * click opens on, what its caret lists — *New tab*, *From folder…*, every
 * server up with *Open* and *Stop*, the ports the rail
 * attributes, the tabs already open here, and the page's annotation for an
 * agent when the page can take one. The node hosts the servers; the ports
 * are the footer's poll; this only orders the words. Which door ⌘⇧R opens,
 * and the Terminal caret's item for the project's run command, are
 * `runCommandModel.mjs`'s.
 */

import { BLANK_URL } from "../../shell/browsersModel.mjs";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * The words a server wears: the checkout, the folder, or the artifact's
 * file name. @param {{owner: {kind: string, folder?: string, name?: string}}} server
 */
export function servedWords(server) {
  const owner = server?.owner ?? { kind: "workstream", folder: "" };
  if (owner.kind === "artifact") return owner.name ?? tr("workbench-servers-artifact");
  return owner.folder ? `${owner.folder}/` : tr("workbench-servers-checkout");
}

/** A server's row: where and on which port. @param {{owner: object, port: number}} server */
export function serverLabel(server) {
  return `${servedWords(server)} · :${server.port}`;
}

/** *Open :4173* — a server's Open item; where it serves is the hint. @param {{port: number}} server */
export function openLabel(server) {
  return tr("workbench-serve-folder-open", { port: server.port });
}

/**
 * *Stop* while it is the only server up, *Stop :4173* when there are
 * several — one word where one word says it all.
 * @param {{port: number}} server @param {readonly object[]} servers every server up here
 */
export function stopLabel(server, servers) {
  return (servers ?? []).length > 1 ? tr("workbench-servers-stop", { port: server.port }) : tr("workbench-device-doc-stop");
}

/**
 * The Browser button: its main click opens a tab on the newest server, else
 * the newest port here, else blank with the field ready. Its caret, in
 * order and a rule at each group's start: *New tab*; on a checkout *From
 * folder…* — the root of the checkout or one folder of it, chosen in a
 * picker over its tree (`serveFolderModel.mjs`) and served on a port of this
 * machine; every server up with
 * *Open :port* and *Stop* (*Stop :port* when several are up); the ports the
 * rail attributes here; the tabs already open here; and *Annotate the page
 * for an agent…* when the active tab here shows a page that can take one.
 * `busy` — a start in flight, or no checkout on disk yet — holds the serves
 * and every stop, never an open, a tab or the annotation. `door` is ⌘⇧R's
 * target (`runCommandModel.runDoor`): the item whose id it is carries the
 * `run_project` command, so the chord shows beside the thing it does — and
 * when the door is the Terminal's run command, no item here does.
 * @param {{
 *   checkout: boolean,
 *   servers: readonly {id: string, url: string, owner: object, port: number}[],
 *   ports: readonly {port: number, process: string}[],
 *   tabs: readonly {key: string, label: string, headless?: boolean}[],
 *   annotate: {key: string} | null,
 *   busy: boolean,
 *   door: string | null,
 * }} facts
 */
export function browserMenu({ checkout, servers, ports, tabs, annotate, busy, door }) {
  const newestServer = (servers ?? []).at(-1) ?? null;
  const newestPort = (ports ?? []).at(-1) ?? null;
  const main = newestServer ? { url: newestServer.url, hint: tr("workbench-servers-open-in-browser-tab", { what: serverLabel(newestServer) }) } : newestPort ? { url: `http://localhost:${newestPort.port}/`, hint: tr("workbench-servers-open-in-browser-tab", { what: `:${newestPort.port}` }) } : { url: null, hint: tr("workbench-servers-open-browser-tab-type-url") };
  const items = [{ id: "blank", label: tr("workbench-servers-new-tab"), hint: tr("workbench-servers-blank-tab-url-field-ready"), icon: "add", disabled: false }];
  if (checkout) {
    items.push({ id: "serve-folder", label: tr("workbench-servers-from-folder"), hint: tr("workbench-servers-serve-root-one-folder-checkout-port"), icon: "folder", disabled: busy, separatorBefore: true });
    (servers ?? []).forEach((server, i) => {
      items.push({ id: `open:${server.id}`, label: openLabel(server), hint: `${serverLabel(server)} — ${server.url}`, icon: "page", disabled: false, separatorBefore: i === 0 });
      items.push({ id: `stop:${server.id}`, label: stopLabel(server, servers), hint: tr("workbench-servers-stop-serving-port-closes-nothing-disk", { server: servedWords(server), port: server.port }), icon: "stop", disabled: busy, danger: true });
    });
  }
  (ports ?? []).forEach((p, i) => {
    items.push({ id: `port:${p.port}`, label: tr("workbench-servers-open-port-process", { port: p.port, process: p.process }), hint: tr("workbench-servers-port-shell-harness-opened", { port: p.port }), icon: "port", disabled: false, separatorBefore: i === 0 });
  });
  (tabs ?? []).forEach((t, i) => {
    items.push({ id: `tab:${t.key}`, label: t.headless ? tr("workbench-servers-unseen", { t: t.label }) : t.label, hint: t.headless ? tr("workbench-servers-agent-browses-here-out-sight-show") : tr("workbench-servers-show-tab"), icon: t.headless ? "hidden" : "page", disabled: false, separatorBefore: i === 0 });
  });
  if (annotate) {
    items.push({ id: `annotate:${annotate.key}`, label: tr("workbench-tab-menu-annotate-page-agent"), hint: tr("workbench-servers-hover-element-click-say-what-should"), icon: "annotate", disabled: false, separatorBefore: true });
  }
  if (checkout && door) for (const item of items) if (item.id === door) item.command = "run_project";
  return { main, items };
}

/**
 * The server whose origin a URL is on, or null — what says whether a page
 * is the node's, and whose.
 * @param {readonly {url: string, owner: object}[]} servers @param {string} url
 */
export function serverAt(servers, url) {
  let origin;
  try {
    origin = new URL(String(url)).origin;
  } catch {
    return null;
  }
  return (
    (servers ?? []).find((s) => {
      try {
        return new URL(s.url).origin === origin;
      } catch {
        return false;
      }
    }) ?? null
  );
}

/**
 * Whether a tab's page can be annotated for an agent: any page the tab
 * shows — the checkout the node serves, a dev server, the web — and never
 * an artifact's page, which is an agent's own and nobody's to edit. A blank
 * tab shows nothing to point at.
 * @param {{home: {scope: string, id: string} | null, url: string} | null} session
 * @param {readonly {url: string, owner: {kind: string}}[]} servers every server up, artifacts included
 */
export function annotatable(session, servers) {
  if (!session || !session.url || session.url === BLANK_URL) return false;
  return serverAt(servers, session.url)?.owner?.kind !== "artifact";
}

/**
 * Where a tab's annotations go: the checkout's conversation for a tab at
 * home in a workstream — the chips file chips when the page is a file of
 * it — else the conversation on screen beside the tab.
 * @param {{home: {scope: string, id: string} | null} | null} session
 * @returns {{kind: "checkout", wid: string} | {kind: "screen"}}
 */
export function annotationHome(session) {
  const home = session?.home ?? null;
  return home && home.scope === "workstream" ? { kind: "checkout", wid: home.id } : { kind: "screen" };
}

/** The verb on a stopped server's toast. */
export function stoppedWords(server) {
  return tr("workbench-servers-stopped-serving", { server: servedWords(server), port: server.port });
}

/** The verb on a started server's toast. */
export function servingWords(server) {
  return tr("workbench-servers-serving", { server: servedWords(server), url: server.url });
}
