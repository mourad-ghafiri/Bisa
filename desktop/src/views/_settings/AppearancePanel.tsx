/**
 * Six dials, shown rather than listed.
 *
 * A picker that offers only words — "Orchard", "Dune dark" — makes you try
 * each one to find out what it is, which on a theme control means repainting
 * the whole app to make one decision. So every dial here previews itself: the
 * theme tiles are the window in miniature with a line of real text, the
 * accent tiles are the row that is waiting on you, the type slider recaptions
 * itself in pixels, and each face is shown set in itself.
 *
 * **The previews read their values out of the live stylesheet.** A tile
 * stamps `data-theme` (or `data-font-ui`) on `<html>`, reads the resolved
 * value back, and restores the attribute before the browser can paint — all
 * inside one synchronous task, so nothing flickers. The alternative was a
 * table of colours copied out of `theme/themes/*.css`, which is a second
 * definition of every palette that goes wrong silently the first time
 * somebody adjusts a neutral.
 *
 * The dials are independent by design, so this panel does not group them into
 * presets: "compact Dune dark in Atkinson with a teal accent" is a combination
 * nobody had to author, and a preset list would take it away. The facts —
 * what a sample stamps, what the scale means in pixels, how families become
 * cards — are `appearanceModel.mjs`'s.
 */

import { useEffect, useLayoutEffect, useState } from "react";
import {
  ACCENTS,
  DEFAULT_FAMILY,
  DENSITIES,
  FAMILIES,
  FONTS_MONO,
  FONTS_UI,
  TYPE_SCALE,
  getAppearance,
  preloadFonts,
  schemeOf,
  setAccent,
  setDensity,
  setFontMono,
  setFontUi,
  setTheme,
  setTypeScale,
  watchAppearance,
  type AccentChoice,
  type Appearance,
  type DensityChoice,
  type FontMonoChoice,
  type FontUiChoice,
  type ThemeChoice,
} from "../../shell/theme";
import { href } from "../../router";
import { Button, Card, cn, ICON, Section, SegmentedControl, Slider, Tile } from "../../ui";
import { resetNavOrder, useNavOrderIsDefault } from "../../shell/navOrderStore";
import { SPECIMEN, familyCards, isDefaultScale, sampleAttributes, typeSpecimen } from "./appearanceModel.mjs";
import { settingsPath, settingsSearch } from "./settingsLink.mjs";
import { t as tr } from "../../i18n/l10n.mjs";
import { percent } from "../../i18n/format.mjs";
import { rich } from "../../i18n/rich";

// ---------------------------------------------------------------------------
// Reading the live stylesheet
// ---------------------------------------------------------------------------

/**
 * The roles a preview draws with — a subset of the role contract. A tile that
 * used every role would be a colour chart; these nine are the ones the app's
 * own chrome is actually made of, so the tile looks like a small version of
 * the window you are about to get.
 */
const PREVIEW_ROLES = ["bg", "surface", "surface-2", "border", "text", "text-dim", "accent", "accent-soft", "accent-ink"] as const;

type Palette = Record<(typeof PREVIEW_ROLES)[number], string>;

/**
 * Stamp attributes on `<html>`, read custom properties back, put everything
 * as it was. Nothing awaits between the write and the restore, so the browser
 * has no opportunity to paint the intermediate state — this is a read, not a
 * temporary theme change. `null` removes an attribute.
 */
function sample<K extends string>(attrs: Record<string, string | null>, props: readonly K[]): Record<K, string> {
  const root = document.documentElement;
  const before = Object.fromEntries(Object.keys(attrs).map((k) => [k, root.getAttribute(k)]));
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null) root.removeAttribute(k);
    else root.setAttribute(k, v);
  }
  const cs = getComputedStyle(root);
  const out = Object.fromEntries(props.map((p) => [p, cs.getPropertyValue(p).trim()])) as Record<K, string>;
  for (const [k, v] of Object.entries(before)) {
    if (v === null) root.removeAttribute(k);
    else root.setAttribute(k, v);
  }
  return out;
}

const COLOR_PROPS = PREVIEW_ROLES.map((r) => `--color-${r}`);
function palette(attrs: Record<string, string | null>): Palette {
  const raw = sample(attrs, COLOR_PROPS);
  return Object.fromEntries(PREVIEW_ROLES.map((r) => [r, raw[`--color-${r}`]])) as Palette;
}

/**
 * Every palette the picker needs, sampled together. The theme tiles keep the
 * reader's *current* accent rather than each theme's own, because that is
 * what choosing the theme would actually give them.
 */
function samplePalettes(accent: AccentChoice) {
  const themed = (id: ThemeChoice) => palette(sampleAttributes({ theme: id, scheme: schemeOf(id), accent }));
  const ids: ThemeChoice[] = ["system", ...FAMILIES.flatMap((f) => [f.light, f.dark])];
  return {
    themes: Object.fromEntries(ids.map((id) => [id, themed(id)])) as Record<ThemeChoice, Palette>,
    accents: Object.fromEntries(ACCENTS.map((a) => [a.id, palette({ "data-accent": a.id === "amber" ? null : a.id })])) as Record<AccentChoice, Palette>,
  };
}

/** Each face's stack, read from `theme/fonts.css` by stamping its dial. */
function sampleFonts() {
  return {
    ui: Object.fromEntries(FONTS_UI.map((f) => [f.id, sample({ "data-font-ui": f.id === "system" ? null : f.id }, ["--font-sans"])["--font-sans"]])) as Record<FontUiChoice, string>,
    mono: Object.fromEntries(FONTS_MONO.map((f) => [f.id, sample({ "data-font-mono": f.id === "system" ? null : f.id }, ["--font-mono"])["--font-mono"]])) as Record<FontMonoChoice, string>,
  };
}

// ---------------------------------------------------------------------------
// The tiles
// ---------------------------------------------------------------------------

/**
 * A window in miniature: a sidebar, a row that is waiting on you, and a card
 * with a line of real text at the real body size. The accent appears exactly
 * once, on the row that is asking for something — the rule the tokens file
 * states, drawn so a reader picking a colour can see what the colour is for.
 */
function MiniWindow({ p, className }: { p: Palette; className?: string }) {
  return (
    <div aria-hidden className={cn("flex h-full w-full overflow-hidden", className)} style={{ background: p.bg, color: p.text }}>
      <div className="flex w-[30%] flex-col gap-1.5 p-2" style={{ background: p["surface-2"], borderRight: `1px solid ${p.border}` }}>
        <div className="h-1.5 w-3/4 rounded-full" style={{ background: p["text-dim"] }} />
        <div className="h-1.5 w-1/2 rounded-full" style={{ background: p.border }} />
        <div className="h-1.5 w-2/3 rounded-full" style={{ background: p.border }} />
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5 p-2">
        <div className="flex items-center gap-1.5 rounded px-1.5 py-1 text-3xs font-medium" style={{ background: p["accent-soft"], color: p["accent-ink"] }}>
          <span className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ background: p.accent }} />
          <span className="truncate">{tr("settings-appearance-panel-waiting")}</span>
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-1 rounded p-1.5" style={{ background: p.surface, border: `1px solid ${p.border}` }}>
          <span className="truncate text-sm leading-tight" style={{ color: p.text }}>{tr("settings-appearance-panel-ship-landing-page")}</span>
          <span className="truncate text-3xs" style={{ color: p["text-dim"] }}>{tr("settings-appearance-panel-12-30-steps-reviewed-yesterday")}</span>
        </div>
      </div>
    </div>
  );
}

function ThemeTile({
  label,
  palette: p,
  /** The System tile: two palettes, split on the diagonal. */
  halves,
  active,
  onSelect,
}: {
  label: string;
  palette: Palette;
  halves?: { light: Palette; dark: Palette };
  active: boolean;
  onSelect: () => void;
}) {
  return (
    <Tile
      active={active}
      onSelect={onSelect}
      name={label}
      className="flex-1"
      previewClass="block h-[104px]"
      preview={
        <>
          <MiniWindow p={halves ? halves.light : p} />
          {halves && (
            // The diagonal says "both, depending" in a way two side-by-side
            // tiles cannot: it is one choice, not two.
            <span className="absolute inset-0" style={{ clipPath: "polygon(100% 0, 100% 100%, 0 100%)" }}>
              <MiniWindow p={halves.dark} />
            </span>
          )}
        </>
      }
    />
  );
}

/**
 * An accent, shown as the thing it is for. A row of coloured dots asks "which
 * do you like"; this asks "which of these would you notice", which is the
 * question the accent actually settles.
 */
function AccentTile({ label, palette: p, active, onSelect }: { label: string; palette: Palette; active: boolean; onSelect: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onSelect}
      className={cn("anim group flex items-center gap-2 rounded-control border px-2 py-1.5 text-left outline-none focus-visible:border-accent", active ? "border-accent ring-2 ring-accent/40" : "border-border hover:border-text-dim/40 hover:bg-surface-2")}
    >
      <span aria-hidden className="flex h-6 shrink-0 items-center gap-1.5 rounded-full px-2.5" style={{ background: p["accent-soft"] }}>
        <span className="h-2 w-2 rounded-full" style={{ background: p.accent }} />
        <span className="whitespace-nowrap text-2xs font-medium" style={{ color: p["accent-ink"] }}>{tr("settings-appearance-panel-waiting")}</span>
      </span>
      <span className={cn("text-xs", active ? "font-medium text-text" : "text-text-dim")}>{label}</span>
    </button>
  );
}

/** A face, set in itself, with the one line that says what it is for. */
function FontSpecimen({ label, note, stack, text, mono, active, onSelect }: { label: string; note: string; stack: string; text: string; mono?: boolean; active: boolean; onSelect: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onSelect}
      className={cn("anim flex min-w-0 flex-col gap-1 rounded-control border px-3 py-2 text-left outline-none focus-visible:border-accent", active ? "border-accent ring-2 ring-accent/40" : "border-border hover:border-text-dim/40 hover:bg-surface-2")}
    >
      <span className="flex items-center gap-1 text-2xs">
        {active && <ICON.check size={12} aria-hidden className="shrink-0 text-text" />}
        <span className={active ? "font-medium text-text" : "text-text-dim"}>{label}</span>
      </span>
      <span className={cn("truncate text-text", mono ? "text-xs" : "text-sm")} style={{ fontFamily: stack || undefined }}>
        {text}
      </span>
      <span className="text-2xs text-text-dim">{note}</span>
    </button>
  );
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

export function AppearancePanel() {
  const [a, setA] = useState<Appearance>(getAppearance);
  const [palettes, setPalettes] = useState(() => samplePalettes(getAppearance().accent));
  const [fonts, setFonts] = useState(sampleFonts);

  // Another surface can change any of the six — a second window, the palette,
  // the node's settings — so the panel follows rather than owning the state.
  useEffect(() => watchAppearance(setA), []);

  // The faces load on demand, so the picker previews them in their
  // real form the moment it opens rather than the fallback stack.
  useEffect(() => preloadFonts(), []);

  // Layout effect, not effect: the samples are read before the browser paints
  // the panel, so the tiles are never drawn once with empty colours and then
  // corrected. Re-sampled when the accent moves, because the theme tiles show
  // the reader's own accent, and when the theme moves, because the accent
  // overlays differ by the scheme the theme landed on.
  useLayoutEffect(() => {
    setPalettes(samplePalettes(a.accent));
  }, [a.accent, a.theme]);
  useLayoutEffect(() => {
    setFonts(sampleFonts());
  }, [a.fontUi, a.fontMono]);

  const cards = familyCards(FAMILIES, DEFAULT_FAMILY, a.theme);
  const specimen = typeSpecimen(a.typeScale);
  const atDefault = isDefaultScale(a.typeScale, TYPE_SCALE.default);

  return (
    // A size container: the tiles split by the panel's width, so a narrow window keeps whole previews.
    <div className="@container flex flex-col gap-6">
      <Section title={tr("settings-appearance-panel-theme")}>
        <p className="mb-4 max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-appearance-panel-theme-blurb", { strong: (inner) => <strong className="font-medium text-text">{inner}</strong> })}</p>
        <div className="grid gap-3 @xl:grid-cols-2">
          {cards.map((card) => (
            <Card key={card.id} className="flex flex-col gap-2">
              <div className="flex items-baseline justify-between gap-2">
                <span className="text-sm font-medium text-text">{card.label}</span>
                {card.id === DEFAULT_FAMILY.id && <span className="text-2xs text-text-dim">{tr("settings-appearance-panel-default")}</span>}
              </div>
              <p className="text-2xs leading-relaxed text-text-dim">{card.mood}</p>
              <div className="flex gap-3">
                {card.tiles.map((t) => (
                  <ThemeTile
                    key={t.id}
                    label={t.label}
                    palette={palettes.themes[t.id]}
                    halves={card.halves ? { light: palettes.themes[card.halves.light], dark: palettes.themes[card.halves.dark] } : undefined}
                    active={t.active}
                    onSelect={() => setTheme(t.id)}
                  />
                ))}
              </div>
            </Card>
          ))}
        </div>
      </Section>

      <Section title={tr("settings-appearance-panel-accent")}>
        <Card>
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
            <strong className="font-medium text-text">{tr("settings-appearance-panel-accent-means-attention")}</strong> {tr("settings-appearance-panel-whatever-waiting-on-you-wears-it")}
          </p>
          <div className="mt-3 grid gap-2 @xl:grid-cols-2">
            {ACCENTS.map((c) => (
              <AccentTile key={c.id} label={c.label} palette={palettes.accents[c.id]} active={a.accent === c.id} onSelect={() => setAccent(c.id)} />
            ))}
          </div>
          <p className="mt-3 max-w-measure text-2xs leading-relaxed text-text-dim">{tr("settings-appearance-panel-accent-replaces-four-roles-nothing-else")}</p>
        </Card>
      </Section>

      <Section title={tr("settings-appearance-panel-density")}>
        <Card>
          <div className="flex flex-wrap items-center gap-3">
            <SegmentedControl label={tr("settings-appearance-panel-density")} value={a.density} onChange={(d: DensityChoice) => setDensity(d)} options={DENSITIES.map((d) => ({ id: d.id, label: d.label }))} />
            <p className="min-w-48 flex-1 text-2xs leading-relaxed text-text-dim">{tr("settings-appearance-panel-two-settings-rather-than-slider-app")}</p>
          </div>
          {/* Density applies the moment it is picked, so the sample below is
              the preview: these rows are the app's real row height. */}
          <ul className="mt-3 overflow-hidden rounded-control bg-surface-2/50">
            {[tr("settings-appearance-panel-ship-landing-page"), tr("settings-appearance-panel-review-migration-plan"), tr("settings-appearance-panel-rotate-webhook-secret")].map((t) => (
              <li key={t} className="flex h-row items-center gap-2 border-b border-hairline px-2 text-sm last:border-b-0">
                <ICON.goal size={14} aria-hidden className="shrink-0 text-text-dim" />
                <span className="truncate">{t}</span>
              </li>
            ))}
          </ul>
        </Card>
      </Section>

      <Section title={tr("settings-appearance-panel-text-size")}>
        <Card>
          <div className="flex flex-wrap items-center gap-3">
            <Slider
              label={tr("settings-appearance-panel-scale")}
              className="min-w-64 flex-1"
              min={TYPE_SCALE.min}
              max={TYPE_SCALE.max}
              step={TYPE_SCALE.step}
              value={a.typeScale}
              onChange={setTypeScale}
              format={(v) => percent(v)}
            />
            <Button size="sm" variant="ghost" disabled={atDefault} onClick={() => setTypeScale(TYPE_SCALE.default)}>{tr("settings-appearance-panel-reset")}</Button>
          </div>
          {/* The whole app is already at the new scale; this paragraph is the
              live specimen, captioned with the pixels it is set in. */}
          <div className="mt-3 rounded-control bg-surface-2/50 p-3">
            <p className="text-sm text-text">{SPECIMEN.body}</p>
            <p className="mt-1 text-2xs text-text-dim">{tr("settings-appearance-panel-secondary-text-kind-under-name-reads")}</p>
            <p className="tnum mt-2 text-2xs text-text-dim">{tr("settings-appearance-panel-body-px-secondary-px-meta-px", { body: specimen.body, secondary: specimen.secondary, meta: specimen.meta })}</p>
          </div>
          <p className="mt-3 max-w-measure text-2xs leading-relaxed text-text-dim">{tr("settings-appearance-panel-multiplier-not-list-of-sizes")}</p>
        </Card>
      </Section>

      <Section title={tr("settings-appearance-panel-fonts")}>
        <Card>
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{rich("settings-appearance-panel-fonts-blurb", { strong: (inner) => <strong className="font-medium text-text">{inner}</strong> })}</p>
          <div className="mt-3 grid gap-2 @2xl:grid-cols-3" role="group" aria-label={tr("settings-appearance-panel-interface-font")}>
            {FONTS_UI.map((f) => (
              <FontSpecimen key={f.id} label={f.label} note={f.note} stack={fonts.ui[f.id]} text={SPECIMEN.ui} active={a.fontUi === f.id} onSelect={() => setFontUi(f.id)} />
            ))}
          </div>
          <div className="mt-2 grid gap-2 @2xl:grid-cols-3" role="group" aria-label={tr("settings-appearance-panel-code-font")}>
            {FONTS_MONO.map((f) => (
              <FontSpecimen key={f.id} label={f.label} note={f.note} stack={fonts.mono[f.id]} text={SPECIMEN.mono} mono active={a.fontMono === f.id} onSelect={() => setFontMono(f.id)} />
            ))}
          </div>
          <p className="mt-3 max-w-measure text-2xs leading-relaxed text-text-dim">
            {rich("settings-appearance-panel-editor-terminal-sizes-are-settings-under", {
              editor: <a className="anim text-accent-ink underline underline-offset-2 hover:text-text" href={href({ name: "settings" }, settingsSearch("editor"))}>{settingsPath("editor", { within: true })}</a>,
              terminal: <a className="anim text-accent-ink underline underline-offset-2 hover:text-text" href={href({ name: "settings" }, settingsSearch("terminal"))}>{settingsPath("terminal", { within: true })}</a>,
            })}
          </p>
        </Card>
      </Section>

      <SidebarOrderSection />
    </div>
  );
}

/**
 * The way back from a dragged sidebar. The destinations' order is the
 * person's — dragged in the sidebar or its rail, remembered per viewer
 * (`navOrderStore`) — and an order with no way back is a trap, so this says
 * whether it is theirs or the original and offers the original, as the
 * Notes panel offers its dock's corner.
 */
function SidebarOrderSection() {
  const original = useNavOrderIsDefault();
  return (
    <Section title={tr("settings-appearance-panel-sidebar")}>
      <Card>
        <div className="flex items-center justify-between gap-3">
          <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
            {original ? tr("settings-appearance-panel-destinations-their-original-order") : tr("settings-appearance-panel-destinations-order-dragged-them")} {tr("settings-appearance-panel-drag-row-or-icon-to-move")}
          </p>
          <Button size="sm" variant="ghost" disabled={original} onClick={resetNavOrder}>{tr("settings-appearance-panel-reset-order")}</Button>
        </div>
      </Card>
    </Section>
  );
}
