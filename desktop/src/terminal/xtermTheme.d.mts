/**
 * Types for `xtermTheme.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step. This file is the only reason TypeScript
 * never has to read it.
 */

import type { ITheme } from "@xterm/xterm";

/** The token roles the terminal reads. All of them are in the role contract. */
export declare const TERMINAL_ROLES: readonly string[];

/**
 * @param resolved role name → a colour xterm can parse (`#rrggbb`), or `""`
 *   when the page could not resolve it.
 * @param scheme the side the mounted theme landed on; anything other than
 *   `"dark"` is treated as light.
 */
export declare function xtermTheme(
  resolved: Record<string, string>,
  scheme: string | null | undefined,
): ITheme;
