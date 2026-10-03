/**
 * A tab strip. The panel below it stays the caller's.
 *
 * Radix owns the roving tabindex, `aria-selected`, Home/End and the
 * arrow-key wrap that the hand-rolled version had to keep re-deriving. The
 * strip is deliberately still just a strip: `Tabs.Root` here has no
 * `Tabs.Content`, because every caller already renders its own panel from
 * `active` and switching them to Radix's content model would mean touching
 * every one of them for no behaviour they lack.
 *
 * A tab may wear a glyph before its label. A strip whose labels would not
 * fit its container folds to the glyphs, in stages, in the order the tabs
 * declare (`tabsFitModel`): a tab says the turn in which it yields its word
 * (`fold`, lower first — the project rail's Workflows before Goals before
 * Workspace); tabs that say none yield together, first, so a strip nobody
 * ordered folds all at once. A folded tab keeps its glyph and its badge, the
 * label moves into a tooltip and the accessible name, and the labels come
 * back in reverse when there is room again. A tab with no glyph keeps its
 * label always, so a strip nobody gave glyphs to is unchanged. The strip is
 * one row at every width: nothing wraps, nothing is hidden.
 *
 * The strip measures itself at every stage it draws — the width its tabs
 * take, not the container's — and the container on every resize; the
 * stage is settled inside layout effects, so a stage drawn only to be
 * measured never reaches the screen.
 *
 * The underline slides between tabs instead of cutting. That is the one place
 * animation is doing work rather than decoration — it says *which* tab you
 * came from, which a hard cut throws away — and it is skipped entirely under
 * reduced motion, where the underline simply appears under the new tab. It
 * is drawn in `text`, not the accent: the tab you are on is where you are,
 * not something waiting on you (`theme/tokens.css`, the colour rules).
 */

import * as T from "@radix-ui/react-tabs";
import { motion } from "motion/react";
import { useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { cn } from "./cn";
import type { LucideIcon } from "./icons";
import { useMotionTiming } from "./motion";
import { fitStage, foldGroups, foldedIds, glyphTitle, stripKey } from "./tabsFitModel.mjs";
import { Tooltip } from "./Tooltip";

export interface TabDef {
  id: string;
  label: string;
  badge?: ReactNode;
  /** The glyph before the label — and the whole tab when the strip folds. */
  icon?: LucideIcon;
  /** What the badge counts, for the folded tab's tooltip. */
  count?: number;
  /** When the strip is too tight, the turn in which this tab yields its word — lower first; tabs that declare none yield together, first. A tab with no glyph never folds. */
  fold?: number;
}

/** The space between two tabs, as `gap-1` draws it. */
const GAP_PX = 4;

/** The width the strip's tabs take, whatever the container gives. */
function contentWidth(el: HTMLElement): number {
  const children = Array.from(el.children);
  const widths = children.reduce((sum, c) => sum + c.getBoundingClientRect().width, 0);
  return widths + GAP_PX * Math.max(0, children.length - 1);
}

export function Tabs({
  tabs,
  active,
  onChange,
  className,
  bare = false,
  label,
}: {
  tabs: TabDef[];
  active: string;
  onChange: (id: string) => void;
  className?: string;
  /** In a `ScreenBar`: no line of its own — the strip stands the bar's height and its underline lands on the bar's hairline. */
  bare?: boolean;
  /** The strip's name for a screen reader, when the tabs alone do not say what they choose between. */
  label?: string;
}) {
  // Scoped so two tab strips on one screen do not animate into each other.
  const group = useId();
  const timing = useMotionTiming("fast");
  const list = useRef<HTMLDivElement>(null);
  const key = stripKey(tabs);
  const groups = foldGroups(tabs);
  const foldable = groups.length > 0;
  const [measured, setMeasured] = useState<{ key: string; needed: (number | null)[] }>({ key, needed: [] });
  const needed = measured.key === key ? measured.needed : [];
  const [available, setAvailable] = useState<number | null>(null);
  const stage = foldable ? fitStage(needed, available, groups.length) : 0;
  const folded = foldedIds(groups, stage);

  // The width the tabs take is read at every stage the strip draws — and
  // whenever the tabs are given again, since a badge may change without its
  // count — and kept under the tabs' key; the width the container gives is
  // read on every resize. Both bail when nothing moved, so the strip settles.
  useLayoutEffect(() => {
    const el = list.current;
    if (!el || !foldable) return;
    const w = contentWidth(el);
    setMeasured((prev) => {
      const kept = prev.key === key ? prev.needed : [];
      if (prev.key === key && kept[stage] === w) return prev;
      const next = kept.slice();
      next[stage] = w;
      return { key, needed: next };
    });
    setAvailable(el.clientWidth);
  }, [foldable, key, stage, tabs]);
  useLayoutEffect(() => {
    const el = list.current;
    if (!el || !foldable) return;
    const ro = new ResizeObserver(() => setAvailable(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, [foldable]);

  return (
    <T.Root value={active} onValueChange={onChange} className={cn(bare && "flex min-w-0", className)}>
      <T.List ref={list} aria-label={label} className={cn("flex gap-1 overflow-hidden", bare ? "min-w-0 flex-1 items-stretch" : "items-center border-b border-hairline")}>
        {tabs.map((t) => {
          const on = t.id === active;
          const isFolded = folded.has(t.id);
          const trigger = (
            <T.Trigger
              key={t.id}
              value={t.id}
              aria-label={isFolded ? glyphTitle(t.label, t.count) : undefined}
              className={cn(
                "anim relative flex shrink-0 items-center gap-1.5 py-2 text-xs font-medium outline-none focus-visible:text-text",
                isFolded ? "px-2" : "px-3",
                on ? "text-text" : "text-text-dim hover:text-text",
              )}
            >
              {t.icon && <t.icon size={13} aria-hidden className="shrink-0" />}
              {!isFolded && t.label}
              {t.badge}
              {on && (
                <motion.span
                  layoutId={`tab-underline-${group}`}
                  transition={timing}
                  className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-text"
                />
              )}
            </T.Trigger>
          );
          return isFolded ? (
            <Tooltip key={t.id} label={glyphTitle(t.label, t.count)}>
              {trigger}
            </Tooltip>
          ) : (
            trigger
          );
        })}
      </T.List>
    </T.Root>
  );
}
