/**
 * Appearance: which theme, which accent, how dense, how large, which faces.
 *
 * The values all live in CSS (`src/theme/*`); this file only decides which
 * sets apply, by stamping attributes on <html>. Six dials, deliberately
 * independent — a theme is a palette, an accent replaces four of its roles, a
 * density moves three heights, the type scale multiplies one virtual rem, and
 * the two font dials swap one face each. None of them can reach into another,
 * so "compact Dune dark in Atkinson with a teal accent" is a combination
 * nobody had to author.
 *
 *   data-theme      absent | glass | glass-dark | harbor | harbor-dark | orchard | orchard-dark
 *                   | dune | dune-dark | suede | suede-dark
 *   data-accent     absent | violet | teal | rose
 *   data-density    absent | compact
 *   data-font-ui    absent | inter | atkinson
 *   data-font-mono  absent | jetbrains | plex
 *   data-scheme     light | dark
 *   --type-scale    a number
 *
 * "system" removes `data-theme` so the default family — Glass, the one that
 * owns bare `:root` — follows the OS through the `prefers-color-scheme` block
 * in `theme/themes/glass.css`: a choice, not a snapshot, so a machine that
 * flips at sunset keeps following.
 *
 * `data-scheme` is the one attribute that is not a preference: it is the
 * light/dark side the mounted theme actually landed on, which the accent
 * overlays need in order to pick a fill that reads against the surface they
 * are sitting on. It is derived, never stored, re-derived when the system
 * flips, and handed to the desktop shell so the window's own chrome lands on
 * the same side — together with the mounted family's **material**, so a
 * glass family gets a transparent window with the OS's own effect behind
 * it, and an opaque one gets a window that is only a window.
 */

import { readPref, webStorage, writePref } from "./storedPrefModel.mjs";
import { errorFields, log } from "../log";
import { t as tr } from "../i18n/l10n.mjs";
export type ThemeChoice = "system" | "glass" | "glass-dark" | "harbor" | "harbor-dark" | "orchard" | "orchard-dark" | "dune" | "dune-dark" | "suede" | "suede-dark";
export type AccentChoice = "amber" | "violet" | "teal" | "rose";
export type DensityChoice = "comfortable" | "compact";
export type FontUiChoice = "system" | "inter" | "atkinson";
export type FontMonoChoice = "system" | "jetbrains" | "plex";

type Scheme = "light" | "dark";

/**
 * What a family's surfaces are made of. `opaque` paints its `bg` fully and
 * the window is only a window; `glass` carries alpha in its surfaces and
 * frosts them (`theme/material.css`), and the desktop shell puts the OS's
 * own under-window effect behind the page for it.
 */
export type Material = "opaque" | "glass";

/** One family: a light and a dark side, its material, and the one line that says what it is for. */
export interface ThemeFamily {
  id: "glass" | "harbor" | "orchard" | "dune" | "suede";
  label: string;
  mood: string;
  material: Material;
  light: ThemeChoice;
  dark: ThemeChoice;
}

/** The five families, the default first. The picker draws one card per family, two tiles each. */
export const FAMILIES: ReadonlyArray<ThemeFamily> = [
  { id: "glass", label: tr("shell-theme-glass"), mood: tr("shell-theme-frosted-panes-over-whatever-behind-window"), material: "glass", light: "glass", dark: "glass-dark" },
  { id: "harbor", label: tr("shell-theme-harbor"), mood: tr("shell-theme-cool-greys-whisper-blue-low-glare"), material: "opaque", light: "harbor", dark: "harbor-dark" },
  { id: "orchard", label: tr("shell-theme-orchard"), mood: tr("shell-theme-green-tinted-neutrals-calmest-opaque-families"), material: "opaque", light: "orchard", dark: "orchard-dark" },
  { id: "dune", label: tr("shell-theme-dune"), mood: tr("shell-theme-warm-sand-light-umber-dark-twilight"), material: "opaque", light: "dune", dark: "dune-dark" },
  { id: "suede", label: tr("shell-theme-suede"), mood: tr("shell-theme-matte-warm-undyed-paper-light-warm"), material: "opaque", light: "suede", dark: "suede-dark" },
];

/** The default family — bare `:root` and System's dark side are its two faces. */
export const DEFAULT_FAMILY = FAMILIES[0];

/**
 * The themes a picker should offer, with the side each one lands on.
 * `themes.test.mjs` holds this list equal to the stylesheets and the
 * registry, so an id here is an id everywhere.
 */
export const THEMES: ReadonlyArray<{ id: ThemeChoice; label: string; scheme: Scheme | null }> = [
  { id: "system", label: tr("shell-theme-system"), scheme: null },
  { id: "glass", label: tr("shell-theme-glass"), scheme: "light" },
  { id: "glass-dark", label: tr("shell-theme-glass-dark"), scheme: "dark" },
  { id: "harbor", label: tr("shell-theme-harbor"), scheme: "light" },
  { id: "harbor-dark", label: tr("shell-theme-harbor-dark"), scheme: "dark" },
  { id: "orchard", label: tr("shell-theme-orchard"), scheme: "light" },
  { id: "orchard-dark", label: tr("shell-theme-orchard-dark"), scheme: "dark" },
  { id: "dune", label: tr("shell-theme-dune"), scheme: "light" },
  { id: "dune-dark", label: tr("shell-theme-dune-dark"), scheme: "dark" },
  { id: "suede", label: tr("shell-theme-suede"), scheme: "light" },
  { id: "suede-dark", label: tr("shell-theme-suede-dark"), scheme: "dark" },
];

/** Amber is absent from the DOM on purpose — see `theme/accents.css`. */
export const ACCENTS: ReadonlyArray<{ id: AccentChoice; label: string }> = [
  { id: "amber", label: tr("shell-theme-family-s-own") },
  { id: "violet", label: tr("shell-theme-violet") },
  { id: "teal", label: tr("shell-theme-teal") },
  { id: "rose", label: tr("shell-theme-rose") },
];

export const DENSITIES: ReadonlyArray<{ id: DensityChoice; label: string }> = [
  { id: "comfortable", label: tr("shell-theme-comfortable") },
  { id: "compact", label: tr("shell-theme-compact") },
];

/** The UI faces; `system` is absent from the DOM, the others are bundled (`theme/fonts.css`). */
export const FONTS_UI: ReadonlyArray<{ id: FontUiChoice; label: string; note: string }> = [
  { id: "system", label: tr("shell-theme-system"), note: tr("shell-theme-whatever-os-reads-fastest-paint") },
  { id: "inter", label: tr("shell-theme-inter"), note: tr("shell-theme-modern-grotesque-tabular-numerals-open-counters") },
  { id: "atkinson", label: tr("shell-theme-atkinson-hyperlegible"), note: tr("shell-theme-drawn-so-every-letter-told-apart") },
];

/** The code faces, for the editor, the terminal and every `font-mono` in the app. */
export const FONTS_MONO: ReadonlyArray<{ id: FontMonoChoice; label: string; note: string }> = [
  { id: "system", label: tr("shell-theme-system"), note: tr("shell-theme-os-s-monospace") },
  { id: "jetbrains", label: tr("shell-theme-jetbrains-mono"), note: tr("shell-theme-tall-wide-letterforms-made-long-lines") },
  { id: "plex", label: tr("shell-theme-ibm-plex-mono"), note: tr("shell-theme-quieter-slab-serif-mono-reads-well") },
];

/**
 * The type scale is a multiplier rather than a list of sizes, because the
 * webview's own zoom is already multiplying the real rem and two competing
 * ladders would drift apart. At 1 the body is 14px and the meta text 12px;
 * 0.9 is the floor below which the meta text stops being legible, 1.5 the
 * ceiling where the rail's rows stop fitting a label.
 */
export const TYPE_SCALE = { min: 0.9, max: 1.5, step: 0.05, default: 1 } as const;

const KEY = "bisa.theme";
const ACCENT_KEY = "bisa.accent";
const DENSITY_KEY = "bisa.density";
const TYPE_KEY = "bisa.type_scale";
const FONT_UI_KEY = "bisa.font_ui";
const FONT_MONO_KEY = "bisa.font_mono";

const appearanceWatchers = new Set<(a: Appearance) => void>();

export interface Appearance {
  theme: ThemeChoice;
  accent: AccentChoice;
  density: DensityChoice;
  typeScale: number;
  fontUi: FontUiChoice;
  fontMono: FontMonoChoice;
}

function isChoice(v: unknown): v is ThemeChoice {
  return typeof v === "string" && THEMES.some((t) => t.id === v);
}
function isAccent(v: unknown): v is AccentChoice {
  return typeof v === "string" && ACCENTS.some((a) => a.id === v);
}
function isDensity(v: unknown): v is DensityChoice {
  return typeof v === "string" && DENSITIES.some((d) => d.id === v);
}
function isFontUi(v: unknown): v is FontUiChoice {
  return typeof v === "string" && FONTS_UI.some((f) => f.id === v);
}
function isFontMono(v: unknown): v is FontMonoChoice {
  return typeof v === "string" && FONTS_MONO.some((f) => f.id === v);
}

function read(key: string): string | null {
  return readPref(webStorage(), key, (raw) => raw, null);
}

/** A locked-down webview still gets the visual change for this session. */
function write(key: string, value: string): void {
  writePref(webStorage(), key, value);
}

function getTheme(): ThemeChoice {
  const v = read(KEY);
  return isChoice(v) ? v : "system";
}
function getAccent(): AccentChoice {
  const v = read(ACCENT_KEY);
  return isAccent(v) ? v : "amber";
}
function getDensity(): DensityChoice {
  const v = read(DENSITY_KEY);
  return isDensity(v) ? v : "comfortable";
}
function getTypeScale(): number {
  const n = Number(read(TYPE_KEY));
  if (!Number.isFinite(n) || n <= 0) return TYPE_SCALE.default;
  return Math.min(TYPE_SCALE.max, Math.max(TYPE_SCALE.min, n));
}
function getFontUi(): FontUiChoice {
  const v = read(FONT_UI_KEY);
  return isFontUi(v) ? v : "system";
}
function getFontMono(): FontMonoChoice {
  const v = read(FONT_MONO_KEY);
  return isFontMono(v) ? v : "system";
}

export function getAppearance(): Appearance {
  return {
    theme: getTheme(),
    accent: getAccent(),
    density: getDensity(),
    typeScale: getTypeScale(),
    fontUi: getFontUi(),
    fontMono: getFontMono(),
  };
}

function systemPrefersDark(): boolean {
  try {
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  } catch {
    return false;
  }
}

/** Which side of light/dark a choice actually lands on, right now. */
export function schemeOf(choice: ThemeChoice): Scheme {
  if (choice === "system") return systemPrefersDark() ? "dark" : "light";
  return THEMES.find((t) => t.id === choice)?.scheme ?? "light";
}

export function setTheme(choice: ThemeChoice): void {
  write(KEY, choice);
  persist("appearance.theme", choice);
  applyTheme(choice);
  notify();
}

export function setAccent(choice: AccentChoice): void {
  write(ACCENT_KEY, choice);
  persist("appearance.accent", choice);
  applyAccent(choice);
  notify();
}

export function setDensity(choice: DensityChoice): void {
  write(DENSITY_KEY, choice);
  persist("appearance.density", choice);
  applyDensity(choice);
  notify();
}

export function setTypeScale(scale: number): void {
  const clamped = Math.min(TYPE_SCALE.max, Math.max(TYPE_SCALE.min, Math.round(scale * 100) / 100));
  write(TYPE_KEY, String(clamped));
  persist("appearance.type_scale", clamped);
  applyTypeScale(clamped);
  notify();
}

/** One step up or down the scale — the palette's *Text: larger / smaller*. */
export function stepTypeScale(direction: 1 | -1): void {
  setTypeScale(getTypeScale() + direction * TYPE_SCALE.step);
}

export function setFontUi(choice: FontUiChoice): void {
  write(FONT_UI_KEY, choice);
  persist("appearance.font_ui", choice);
  applyFontUi(choice);
  notify();
}

export function setFontMono(choice: FontMonoChoice): void {
  write(FONT_MONO_KEY, choice);
  persist("appearance.font_mono", choice);
  applyFontMono(choice);
  notify();
}

/** Stamp or clear one attribute; the "default" word means absent. */
function stamp(attr: string, value: string, absent: string): void {
  const root = document.documentElement;
  if (value === absent) root.removeAttribute(attr);
  else root.setAttribute(attr, value);
}

function applyAccent(choice: AccentChoice = getAccent()): void {
  stamp("data-accent", choice, "amber");
}
function applyDensity(choice: DensityChoice = getDensity()): void {
  stamp("data-density", choice, "comfortable");
}
/**
 * The bundled faces, loaded on demand. Each entry injects one
 * family's `@fontsource` CSS the first time it is asked for; a face that is
 * never chosen is never parsed. `system` has no bundled face and no entry.
 */
const FONT_LOADERS: Record<string, () => Promise<unknown>> = {
  "ui:inter": () => import("@fontsource-variable/inter/wght.css"),
  "ui:atkinson": () =>
    Promise.all([import("@fontsource/atkinson-hyperlegible/400.css"), import("@fontsource/atkinson-hyperlegible/700.css")]),
  "mono:jetbrains": () => import("@fontsource-variable/jetbrains-mono/wght.css"),
  "mono:plex": () =>
    Promise.all([
      import("@fontsource/ibm-plex-mono/400.css"),
      import("@fontsource/ibm-plex-mono/500.css"),
      import("@fontsource/ibm-plex-mono/700.css"),
    ]),
};
const fontsLoaded = new Set<string>();

/** Inject a bundled face's CSS once; a no-op for `system` and for a repeat. */
function ensureFont(key: string): void {
  if (fontsLoaded.has(key) || !FONT_LOADERS[key]) return;
  fontsLoaded.add(key);
  void FONT_LOADERS[key]().catch(() => {
    // The face degrades to the fallback stack the dial rule names; try again later.
    fontsLoaded.delete(key);
  });
}

/** Load every bundled face — the appearance settings call this to preview them. */
export function preloadFonts(): void {
  for (const key of Object.keys(FONT_LOADERS)) ensureFont(key);
}

function applyFontUi(choice: FontUiChoice = getFontUi()): void {
  if (choice !== "system") ensureFont(`ui:${choice}`);
  stamp("data-font-ui", choice, "system");
}
function applyFontMono(choice: FontMonoChoice = getFontMono()): void {
  if (choice !== "system") ensureFont(`mono:${choice}`);
  stamp("data-font-mono", choice, "system");
}
function applyTypeScale(scale: number = getTypeScale()): void {
  document.documentElement.style.setProperty("--type-scale", String(scale));
}

/** The family a choice belongs to; *System* is the default family's. */
export function familyOf(choice: ThemeChoice): ThemeFamily {
  return FAMILIES.find((f) => f.light === choice || f.dark === choice) ?? DEFAULT_FAMILY;
}

/**
 * The side the window landed on and the material it is made of: the side is
 * stamped for the overlays, and both are handed to the desktop shell so the
 * title bar is dark over a dark page and the desktop is blurred under a
 * glass one. A browser dev session has no shell and skips the second half.
 */
function applyWindow(choice: ThemeChoice): void {
  const scheme = schemeOf(choice);
  const { material } = familyOf(choice);
  document.documentElement.setAttribute("data-scheme", scheme);
  void import("../api").then(({ setWindowAppearance }) => setWindowAppearance(scheme, material));
}

/**
 * Stamp the whole appearance.
 *
 * This is the app's single boot-time call, so it applies every dial rather
 * than only the theme: the six are one visual state, and a boot that set the
 * palette but left the accent and density unstamped would show the user a
 * settings screen that disagreed with the window it was in. The inline script
 * in `index.html` has already stamped the cache before the first paint; this
 * confirms it and takes over.
 */
export function applyTheme(choice: ThemeChoice = getTheme()): void {
  stamp("data-theme", choice, "system");
  applyWindow(choice);
  applyAccent();
  applyDensity();
  applyTypeScale();
  applyFontUi();
  applyFontMono();
  listenToSystem();
}

/** Any dial moving, for a settings screen that shows all of them at once. */
export function watchAppearance(cb: (a: Appearance) => void): () => void {
  appearanceWatchers.add(cb);
  return () => {
    appearanceWatchers.delete(cb);
  };
}

function notify(): void {
  const a = getAppearance();
  for (const w of appearanceWatchers) w(a);
}

/**
 * Keep `data-scheme` honest while the choice is "system".
 *
 * The colours themselves are already handled by the media query in the theme
 * files; this exists so the accent overlays and the window chrome know which
 * side they are on when the machine flips underneath a running window.
 * Registered once, because `applyTheme` is called again on every change.
 */
let listening = false;
function listenToSystem(): void {
  if (listening) return;
  try {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", () => {
      if (getTheme() === "system") applyWindow("system");
    });
    listening = true;
  } catch {
    // No matchMedia: the media queries in CSS still work, only the accent
    // overlay's light/dark split falls back to its own media query.
  }
}

// ---------------------------------------------------------------------------
// The dials are settings
// ---------------------------------------------------------------------------
//
// The six dials are `appearance.*` at machine scope in the node's settings
// registry. `localStorage` keeps a copy for the first paint — a theme that
// arrives after the first frame is a flash — and nothing more: the node's
// value wins on every start and on every `settings_changed`, and every change
// made here is written there. The `bisa.*` keys above are that cache,
// not the source of truth.

/** Write one dial to the node; a failure is logged, never fatal — the cache already applied it. */
function persist(key: string, value: unknown): void {
  void import("../api").then(({ api }) =>
    api.setSettings("machine", { [key]: value }).catch((e: unknown) => {
      log.warn("appearance", `${key} was not saved to the node`, errorFields(e));
    }),
  );
}

/**
 * Read the dials from the node and apply them, overriding the cache. Called
 * at startup after the cached theme has painted, and again whenever the node
 * says a setting changed — so a second window, the CLI or the raw settings
 * panel reach this one without a reload.
 */
export async function syncAppearanceFromNode(): Promise<void> {
  try {
    const { api } = await import("../api");
    const { settings } = await api.settingsResolved(null);
    const get = (k: string) => settings.find((s) => s.key === k)?.value;
    let changed = false;
    const theme = get("appearance.theme");
    if (isChoice(theme) && theme !== getTheme()) {
      write(KEY, theme);
      applyTheme(theme);
      changed = true;
    }
    const accent = get("appearance.accent");
    if (isAccent(accent) && accent !== getAccent()) {
      write(ACCENT_KEY, accent);
      applyAccent(accent);
      changed = true;
    }
    const density = get("appearance.density");
    if (isDensity(density) && density !== getDensity()) {
      write(DENSITY_KEY, density);
      applyDensity(density);
      changed = true;
    }
    const scale = get("appearance.type_scale");
    if (typeof scale === "number" && Number.isFinite(scale) && scale !== getTypeScale()) {
      write(TYPE_KEY, String(scale));
      applyTypeScale(Math.min(TYPE_SCALE.max, Math.max(TYPE_SCALE.min, scale)));
      changed = true;
    }
    const fontUi = get("appearance.font_ui");
    if (isFontUi(fontUi) && fontUi !== getFontUi()) {
      write(FONT_UI_KEY, fontUi);
      applyFontUi(fontUi);
      changed = true;
    }
    const fontMono = get("appearance.font_mono");
    if (isFontMono(fontMono) && fontMono !== getFontMono()) {
      write(FONT_MONO_KEY, fontMono);
      applyFontMono(fontMono);
      changed = true;
    }
    if (changed) notify();
  } catch (e) {
    // No node yet, or an older one: the cache stands until there is one.
    log.warn("appearance", "the node's settings were not read", errorFields(e));
  }
}
