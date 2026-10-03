/**
 * The two elevated surfaces, shared by every overlay in the kit.
 *
 * Menus, context menus, popovers and tooltips are four different components
 * with one job in common: they float above the page and have to look like the
 * same object doing it. Keeping the classes here rather than in each file is
 * what stops a popover from acquiring a slightly different border radius or
 * shadow than the menu that sits next to it — a difference nobody notices
 * individually and everybody feels as sloppiness. The shadow is the theme's
 * (`--shadow-floating`, through Tailwind's `shadow-lg`), and every overlay
 * is marked `data-pane` where it mounts, so a glass family frosts it
 * (`theme/material.css`).
 */

/** A floating panel: menus, popovers, comboboxes. */
export const POPOVER_SURFACE =
  "motion-pop z-50 overflow-hidden rounded-control border border-border bg-surface shadow-lg";

/** A row inside a menu. Focus is `data-highlighted`, which Radix drives for
 *  both the pointer and the keyboard, so hover and arrow-key selection cannot
 *  end up looking different. Inset from the panel's edge with its own small
 *  corner, so the highlight is a shape inside the menu rather than a band
 *  cut across it. */
export const MENU_ITEM =
  "anim mx-1 flex cursor-default select-none items-center gap-2 rounded px-2.5 py-1.5 text-left text-xs outline-none data-[disabled]:pointer-events-none data-[disabled]:opacity-40";

export const MENU_ITEM_TONE = {
  default: "text-text data-[highlighted]:bg-selected",
  danger: "text-danger data-[highlighted]:bg-danger-soft",
} as const;
