/**
 * A theme token as the pixels it currently resolves to.
 *
 * Three virtual lists need a row height as a number before they paint — a
 * virtual list positions rows by arithmetic, not by layout — and the number
 * has to be the theme's, or the rows stop fitting their labels the moment the
 * type scale moves. So they read `--spacing-row-sm` here rather than carrying
 * a pixel constant.
 *
 * The value is re-read when anything on `<html>` that can move a token
 * changes: the same attribute filter the terminal watches, plus `style`,
 * which is where `--type-scale` lives. A `MutationObserver` rather than
 * `watchAppearance`, because `ui/` does not reach into `shell/` for this and
 * because the observer also sees a change made by the inline boot script or
 * the devtools.
 */

import { useEffect, useState } from "react";

/** Everything on `<html>` that can move a token's value. */
export const THEME_ATTRIBUTES = ["data-theme", "data-accent", "data-scheme", "data-density", "data-font-ui", "data-font-mono", "style"];

function readPx(token: string, fallback: number): number {
  if (typeof document === "undefined") return fallback;
  const probe = document.createElement("div");
  probe.style.position = "absolute";
  probe.style.visibility = "hidden";
  probe.style.height = `var(${token})`;
  document.documentElement.appendChild(probe);
  const px = Number.parseFloat(getComputedStyle(probe).height);
  probe.remove();
  return Number.isFinite(px) && px > 0 ? Math.round(px) : fallback;
}

/**
 * A counter that ticks whenever the theme attributes on `<html>` change —
 * for the effects that render something from the roles (a diagram, a canvas)
 * and have to do it again after a switch.
 */
export function useThemeNonce(): number {
  const [nonce, setNonce] = useState(0);
  useEffect(() => {
    if (typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => setNonce((n) => n + 1));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: THEME_ATTRIBUTES });
    return () => observer.disconnect();
  }, []);
  return nonce;
}

/**
 * The computed pixel value of a length token, live.
 *
 * `fallback` is for the frame before the stylesheet has resolved and for a
 * test DOM without layout; a component should pass the value the token holds
 * at scale 1, so the fallback is only ever a frame late, never wrong.
 */
export function useTokenPx(token: string, fallback: number): number {
  const nonce = useThemeNonce();
  const [px, setPx] = useState(() => readPx(token, fallback));
  useEffect(() => {
    setPx(readPx(token, fallback));
  }, [token, fallback, nonce]);
  return px;
}
