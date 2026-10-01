/**
 * The one way a folder of a checkout gets served and opened (ide/18): the
 * Browser button's *From folder…* and the Files tree's *Serve this folder*
 * both come through here, so they cannot land differently.
 *
 * The node refuses to serve a folder twice (409). Asking what is up first
 * turns that refusal into the obvious answer — open the server that is
 * already there — whoever started it, in this window or another.
 */

import { api } from "../api";
import { toaster } from "../ui";
import { MEMORY_KEY, ROOT_FOLDER, parseRemembered, rememberFolder, rememberedFolder, serverFor } from "../views/_workbench/serveFolderModel.mjs";
import { servingWords } from "../views/_workbench/serversModel.mjs";
import { openBrowserAt } from "./browserDoors";

/** The folder last served in a checkout — the root when none was, or when storage says nothing. */
export function lastServedFolder(wid: string): string {
  try {
    return rememberedFolder(parseRemembered(localStorage.getItem(MEMORY_KEY)), wid);
  } catch {
    return ROOT_FOLDER;
  }
}

function remember(wid: string, folder: string): void {
  try {
    const kept = rememberFolder(parseRemembered(localStorage.getItem(MEMORY_KEY)), wid, folder);
    localStorage.setItem(MEMORY_KEY, JSON.stringify(kept));
  } catch {
    // A remembered folder is a convenience: a full or closed storage costs the next open a click.
  }
}

/**
 * Serve `folder` of the checkout — the root for the empty path — and land on
 * it in a browser tab at home there. Answers whether a page opened; a
 * refusal is said in a toast, in the node's own words.
 */
export async function serveAndOpen(wid: string, folder: string): Promise<boolean> {
  try {
    const up = serverFor((await api.servers(wid)).servers, folder);
    const server = up ?? (await api.serveFolder(wid, folder === ROOT_FOLDER ? null : folder));
    if (!up) toaster.ok(servingWords(server));
    remember(wid, folder);
    openBrowserAt({ scope: "workstream", id: wid }, server.page);
    return true;
  } catch (e) {
    toaster.error(e instanceof Error ? e.message : String(e));
    return false;
  }
}
