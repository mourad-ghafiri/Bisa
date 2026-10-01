/**
 * The webview's logger: the one door every screen, store and client writes
 * a diagnostic line through. In the desktop shell a line goes to the
 * shell's `log_event` command and lands in `desktop.<date>.jsonl` under the
 * workspace's `logs/` (`bisa-log`); in a browser dev session it goes to
 * the console. The level in force is the machine's `logging.level`, handed
 * in by `shell/logSettings.ts`; until the node answers, errors only.
 *
 * The facts — the level words, the gate, the shape of an event — are
 * `logModel.mjs`; this file is the wiring. A site hands over ids, statuses
 * and a typed error's words, never a body: `errorFields` keeps an error to
 * its name, message, status, path and a few stack lines.
 */

import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  DEFAULT_CONFIG,
  errorFields,
  passes,
  shapeEvent,
  type LogConfig,
  type LogEvent,
  type LogLevel,
} from "./logModel.mjs";

let config: LogConfig = DEFAULT_CONFIG;

/**
 * Take the machine's `logging.*` settings: the webview's own gate, and the
 * shell's file layer through `log_configure`. A shell that refuses is said
 * on the console — the log itself is what failed.
 */
export function configureLog(next: LogConfig): void {
  config = next;
  if (!isTauri()) return;
  void invoke("log_configure", { config: next }).catch((e: unknown) => {
    console.warn("log: the shell did not take the settings:", e);
  });
}

function toConsole(event: LogEvent): void {
  const line = `[${event.target}] ${event.message}`;
  const rest = Object.keys(event.fields).length > 0 ? [event.fields] : [];
  if (event.level === "error") console.error(line, ...rest);
  else if (event.level === "warn") console.warn(line, ...rest);
  else console.debug(line, ...rest);
}

/**
 * The outbox: one line after another, in the order they were said. A
 * fire-and-forget `invoke` per line lands them in whatever order the bridge
 * pleases, and a crash's last lines are the ones a person reads first —
 * so each waits for the one before. A line the shell refused goes to the
 * console and never stalls the next.
 */
let outbox: Promise<void> = Promise.resolve();

function emit(level: LogLevel, target: string, message: string, fields?: Record<string, unknown>): void {
  if (!config.enabled || !passes(level, config.level)) return;
  const event = shapeEvent(level, target, message, fields);
  if (!isTauri()) {
    toConsole(event);
    return;
  }
  outbox = outbox.then(() =>
    invoke("log_event", { event }).then(
      () => undefined,
      () => toConsole(event),
    ),
  );
}

/** The logger. `target` names the surface — `api`, `bus`, `view`, `shell`. */
export const log = {
  error: (target: string, message: string, fields?: Record<string, unknown>) => emit("error", target, message, fields),
  warn: (target: string, message: string, fields?: Record<string, unknown>) => emit("warn", target, message, fields),
  info: (target: string, message: string, fields?: Record<string, unknown>) => emit("info", target, message, fields),
  debug: (target: string, message: string, fields?: Record<string, unknown>) => emit("debug", target, message, fields),
};

export { errorFields };

let installed = false;

/**
 * Catch what nothing else does: an uncaught error and an unhandled
 * rejection, each one line at `error`, and the page going away — a reload,
 * a quit, the renderer taken down — one line at `info`, so a file that ends
 * without it says the webview died without a word. Installed once, at boot.
 */
export function installGlobalLogHandlers(): void {
  if (installed || typeof window === "undefined") return;
  installed = true;
  window.addEventListener("error", (ev) => {
    log.error("window", ev.message || "uncaught error", {
      ...errorFields(ev.error),
      file: ev.filename,
      line: ev.lineno,
      column: ev.colno,
    });
  });
  window.addEventListener("unhandledrejection", (ev) => {
    log.error("window", "unhandled rejection", errorFields(ev.reason));
  });
  window.addEventListener("pagehide", () => {
    log.info("window", "page hidden", { path: window.location.pathname });
  });
}
