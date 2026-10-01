/**
 * The embedded terminal, from one import site.
 *
 * Deliberately not re-exported from `ui/index.ts`. The kit is presentation —
 * every component in it is safe to drop anywhere, twice. This is not: each
 * `<Terminal />` on screen is a real shell process on this machine, and the
 * import having its own name is the last place that fact is visible before a
 * screen renders one in a list.
 *
 * One file mounts it: `shell/TerminalPanel.tsx`. Screens ask for a shell
 * through `shell/useTerminals.ts`, which holds the state but cannot render
 * one — so the checkpoint above stays a single import site even now that
 * several surfaces can open a terminal.
 */

export { Terminal } from "./Terminal";
export type { TerminalProps } from "./Terminal";
export { openTerminal, terminalAvailable, listeningPorts, stopPort } from "./session";
export type { TerminalEvent, TerminalScope, TerminalSession, ListeningPort, PortRoot } from "./session";
