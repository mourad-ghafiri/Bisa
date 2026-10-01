/**
 * The overlay's theme, read off `<html>` — the DOM half of
 * `inspectorTheme.mjs`, the way `terminal/tokens.ts` is of `xtermTheme.mjs`.
 *
 * Colours, radii, shadows, fonts and motion come back from
 * `getPropertyValue` as the theme spells them (`oklch(…)` included): they go
 * into real CSS inside the page, parsed by the same engine, so no hex
 * round-trip is needed. The two type sizes are `calc()` over a virtual rem
 * and resolve only on a live element, so a hidden probe wears each as its
 * `font-size` and is measured — the `useTokenPx` technique.
 */

import { useMemo } from "react";
import { useThemeNonce } from "../useTokenPx";
import { FALLBACK_SIZES, INSPECTOR_ROLES, inspectorTheme } from "./inspectorTheme.mjs";
import type { InspectorTheme } from "./inspectorTheme.mjs";

/** The side the mounted theme landed on: `data-scheme` first, the machine's preference before any script has run. */
function currentScheme(): "light" | "dark" {
  const stamped = document.documentElement.getAttribute("data-scheme");
  if (stamped === "dark" || stamped === "light") return stamped;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** A type token as the px it resolves to, or the fallback where there is no layout. */
function probePx(token: string, fallback: number): number {
  const probe = document.createElement("span");
  probe.style.position = "absolute";
  probe.style.visibility = "hidden";
  probe.style.fontSize = `var(${token})`;
  document.documentElement.appendChild(probe);
  const px = Number.parseFloat(getComputedStyle(probe).fontSize);
  probe.remove();
  return Number.isFinite(px) && px > 0 ? px : fallback;
}

/** The overlay's theme as the app wears it now. */
export function readInspectorTheme(): InspectorTheme {
  if (typeof document === "undefined") return inspectorTheme({}, "light");
  const style = getComputedStyle(document.documentElement);
  const resolved: Record<string, string> = {};
  for (const role of INSPECTOR_ROLES) resolved[role] = style.getPropertyValue(role).trim();
  return inspectorTheme(resolved, currentScheme(), { size: probePx("--text-xs", FALLBACK_SIZES.size), small: probePx("--text-2xs", FALLBACK_SIZES.small) });
}

/** The overlay's theme, live — read again whenever anything on `<html>` that can move a token changes. */
export function useInspectorTheme(): InspectorTheme {
  const nonce = useThemeNonce();
  return useMemo(readInspectorTheme, [nonce]);
}
