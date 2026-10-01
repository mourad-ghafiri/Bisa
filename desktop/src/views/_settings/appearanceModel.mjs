/**
 * The facts behind Settings › Appearance: which attributes a
 * preview stamps to sample a palette, what the type scale means in pixels,
 * how the families become cards, and the words the specimens are set
 * in. Plain `.mjs` with a `.d.mts` beside it, so `node --test` imports the
 * real module; the panel draws, this decides.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * The body, secondary and meta sizes at scale 1, in pixels — the three steps
 * a reader actually meets (`--text-sm`, `--text-2xs`, `--text-3xs` in
 * `theme/tokens.css`). The slider's caption is computed from these so the
 * numbers it shows are the numbers the stylesheet uses.
 */
export const BASE_PX = { body: 14, secondary: 12, meta: 11 };

/** The pixel sizes at a scale, to one decimal, and the percentage a person reads. */
export function typeSpecimen(scale) {
  const s = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const px = (n) => Math.round(n * s * 10) / 10;
  return {
    percent: Math.round(s * 100),
    body: px(BASE_PX.body),
    secondary: px(BASE_PX.secondary),
    meta: px(BASE_PX.meta),
  };
}

/** Whether a scale is the default, allowing for the float the slider hands back. */
export function isDefaultScale(scale, fallback = 1) {
  return Math.abs((Number(scale) || 0) - fallback) < 0.001;
}

/**
 * The attributes to stamp on `<html>` to sample one palette: `null` removes
 * an attribute, which is how "system", the amber accent and the system fonts
 * are expressed in the DOM. `scheme` is stamped explicitly because the accent
 * overlays read it, and a sample must land on the side the theme would.
 */
export function sampleAttributes({ theme, scheme, accent }) {
  return {
    "data-theme": !theme || theme === "system" ? null : theme,
    "data-scheme": scheme === "dark" ? "dark" : "light",
    "data-accent": !accent || accent === "amber" ? null : accent,
  };
}

/**
 * The picker's cards: one per family with a light and a dark tile, and the
 * System card first, whose two halves are the default family's. `active`
 * marks the tile the current theme is.
 */
export function familyCards(families, defaultFamily, current) {
  const tile = (id, label) => ({ id, label, active: current === id });
  return [
    {
      id: "system",
      label: t("settings-appearance-panel-system"),
      mood: t("settings-appearance-follows-machine-light-dark-after-dark", { defaultFamily: defaultFamily.label }),
      tiles: [tile("system", t("settings-appearance-follows-os"))],
      halves: { light: defaultFamily.light, dark: defaultFamily.dark },
    },
    ...families.map((f) => ({
      id: f.id,
      label: f.label,
      mood: f.mood,
      tiles: [tile(f.light, t("settings-appearance-light")), tile(f.dark, t("settings-appearance-dark"))],
      halves: null,
    })),
  ];
}

/** The words a specimen is set in: real sentences, with the glyphs that tell faces apart. */
export const SPECIMEN = {
  ui: t("settings-appearance-ship-landing-page-before-review-thursday"),
  mono: "const ratio = (l1 + 0.05) / (l2 + 0.05); // Il1| O0o {} [] => 0xFF",
  body: t("settings-appearance-theme-palette-accent-means-attention-type"),
};
