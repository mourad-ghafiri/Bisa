/**
 * The root crash, as a fact the ways out can read — no React.
 *
 * When the shell's own tree throws — a hook of `App`'s, a provider, the
 * toast rail above every inner boundary — the root boundary in `main.tsx`
 * draws one card where the window was. Before this that card was a trap:
 * it unmounted `App`, and with it the listeners for the window's close and
 * the OS's quit, while the shell still held every close for the webview's
 * answer — the red button did nothing, ⌘Q and the Dock's Quit hung, and
 * only a Force Quit ended Bisa, leaving the node alive on the workspace.
 *
 * Now the ways out stand above the boundary (`installCloseGuard`, from
 * `main.tsx`), and read this: while the root has crashed the flow is
 * **bare** — no question (the dialog host is gone), no save (the editors
 * are gone), what is remembered kept, then the quit. The card's doors are
 * listed here too, so a test can hold them without a renderer: the two
 * every build has, and the four the desktop shell adds, none of which needs
 * the node — the node may be the thing that is not there.
 *
 * Plain `.mjs` with a `.d.mts` beside it, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * One crash fact: set while the root card is mounted, clear otherwise.
 * `createRootCrash` for a test; `rootCrash` is the window's.
 */
export function createRootCrash() {
  /** @type {unknown} */
  let error = null;
  return Object.freeze({
    /** @param {unknown} e */
    markCrashed(e) {
      error = e ?? new Error("the root crashed");
    },
    clearCrash() {
      error = null;
    },
    crashed() {
      return error !== null;
    },
    error() {
      return error;
    },
  });
}

export const rootCrash = createRootCrash();

/** Every door the card may offer, in the order they are drawn. */
export const ROOT_CRASH_DOORS = Object.freeze(["try_again", "reload", "restart_node", "reveal_log", "open_data_folder", "quit"]);

/**
 * The doors for this build: in the desktop shell all six; in a browser dev
 * session the two the page itself can do.
 * @param {boolean} inShell
 * @returns {string[]}
 */
export function rootCrashDoors(inShell) {
  return inShell ? [...ROOT_CRASH_DOORS] : ["try_again", "reload"];
}

/**
 * How a door is drawn: the one most likely to bring the window back is the
 * card's primary; the other recoveries are plain; the ways to the folders
 * and out are quiet.
 * @param {string} door
 * @returns {"primary" | "default" | "ghost"}
 */
export function doorRank(door) {
  if (door === "reload") return "primary";
  if (door === "try_again" || door === "restart_node") return "default";
  return "ghost";
}

/** The card's two sentences. */
export function rootCrashWords() {
  return { title: t("shell-root-crash-title"), body: t("shell-root-crash-body") };
}

/**
 * A door's label.
 * @param {string} door
 */
export function rootDoorWords(door) {
  switch (door) {
    case "try_again":
      return t("shell-root-crash-try-again");
    case "reload":
      return t("shell-root-crash-reload");
    case "restart_node":
      return t("shell-root-crash-restart-node");
    case "reveal_log":
      return t("shell-root-crash-reveal-log");
    case "open_data_folder":
      return t("shell-root-crash-open-data-folder");
    case "quit":
      return t("shell-root-crash-quit");
    default:
      return door;
  }
}

/**
 * The line under a door that did not work: which, and why.
 * @param {string} door @param {unknown} reason
 */
export function rootDoorFailedWords(door, reason) {
  return t("shell-root-crash-door-failed", { door: rootDoorWords(door), reason: reason instanceof Error ? reason.message : String(reason) });
}
