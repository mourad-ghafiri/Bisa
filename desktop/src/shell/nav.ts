/**
 * The primary nav, as data — one list, three readers.
 *
 * The sidebar draws it, the top chrome reads it to say where you are, and the
 * command palette turns it into "Go to …" entries. Before this file each of
 * those three kept its own copy, which is how "Agents & Teams" came to be one
 * sidebar row pointing at a screen that was really two screens: adding a
 * destination meant editing three lists, so nobody did, and destinations got
 * folded into each other instead.
 *
 * This list says *which* destinations there are, in the order they come by
 * default. The order a person sees is theirs: dragged in the sidebar or its
 * rail and remembered per viewer (`navOrderStore.ts` over
 * `navOrderModel.mjs`) — every surface that draws the destinations reads
 * `usePrimaryNav()` there, never this list's order.
 *
 * Splitting agents from teams is the point of the change. They are different
 * questions — "who can do this work" against "who is on this" — and a reader
 * looking for one had to know it was hiding inside the other.
 */

import type { Route } from "../router";
import { ICON, type IconName, type LucideIcon } from "../ui";
import { t } from "../i18n/l10n.mjs";

export interface NavEntry {
  /** Matches what `section(route)` returns, so detail screens stay lit. */
  key: Route["name"];
  label: string;
  /** The `ICON` key of `icon` — what a drag ghost names, since it draws no component. */
  glyph: IconName;
  icon: LucideIcon;
  route: Route;
}

/**
 * The catalog is deliberately absent. It is where staff and procedures *come
 * from* rather than a place you work, visited a handful of times in a
 * workspace's life, and a nav row spent on it competed with Goals and the
 * roster every other day. It lives under Settings › Library, one panel per kind
 * (`settings?tab=catalog-agent` …); the palette
 * pushes its own "Go to Catalog" entry, since this list no longer generates
 * one.
 */
export const PRIMARY_NAV: readonly NavEntry[] = [
  // The file's order reads top to bottom as a day does: what concerns you,
  // then who does the work and who is on it, then where it lives, the shapes
  // it runs, the goals it serves, and what happened. A person's own order is
  // theirs (`navOrderStore.ts`); this is what *Reset order* returns to.
  { key: "inbox", glyph: "inbox", label: t("shell-nav-inbox"), icon: ICON.inbox, route: { name: "inbox" } },
  { key: "agents", glyph: "agent", label: t("shell-omnibox-agents"), icon: ICON.agent, route: { name: "agents" } },
  { key: "teams", glyph: "team", label: t("shell-omnibox-teams"), icon: ICON.team, route: { name: "teams" } },
  { key: "projects", glyph: "project", label: t("shell-omnibox-projects"), icon: ICON.project, route: { name: "projects" } },
  { key: "workflows", glyph: "workflow", label: t("shell-nav-workflows"), icon: ICON.workflow, route: { name: "workflows" } },
  { key: "goals", glyph: "goal", label: t("shell-omnibox-goals"), icon: ICON.goal, route: { name: "goals" } },
  { key: "pulse", glyph: "pulse", label: t("shell-nav-pulse"), icon: ICON.pulse, route: { name: "pulse" } },
];

/**
 * The label and glyph for wherever you currently are, including the places
 * that are not primary nav entries — a channel, a DM, the settings screen.
 * `section()` has already folded a detail route into its section, so this
 * only has to name the sections the nav list does not.
 */
export function whereYouAre(sectionKey: Route["name"]): { label: string; icon: LucideIcon } {
  const entry = PRIMARY_NAV.find((e) => e.key === sectionKey);
  if (entry) return { label: entry.label, icon: entry.icon };
  switch (sectionKey) {
    case "channels":
      return { label: t("shell-sidebar-channels"), icon: ICON.channel };
    case "messages":
      return { label: t("shell-sidebar-messages"), icon: ICON.dm };
    case "settings":
      return { label: t("shell-keymap-settings"), icon: ICON.settings };
    default:
      return { label: "Bisa", icon: ICON.inbox };
  }
}
