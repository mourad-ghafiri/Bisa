/**
 * The Git mark — git's own logo, worn wherever git is the tool: the
 * Project IDE's Git tab, Settings' git identity, the repository doors.
 *
 * A git *concept* keeps its kit glyph (`ICON.branch`, `ICON.merge`,
 * `ICON.checkout`, `ICON.pullRequest`, `ICON.stash` …): the mark names the
 * tool, never one of its verbs.
 *
 * | Mark | Source | Licence |
 * |---|---|---|
 * | Git | git-scm.com, `Git-Icon-Black.svg` (community/logos) | Git Logo by Jason Long, CC BY 3.0 Unported — credited in `NOTICES.md` and the About dialog |
 *
 * It stands among the kit's lucide glyphs (the IDE's rail: folder, hammer,
 * bot, info), so it is drawn the way they are rather than filled: an
 * outline on lucide's 24-unit grid — `fill: none`, a stroke of 2, round caps
 * and joins — after the official logo: its rounded square turned on its
 * corner, the stem that enters from the upper-left edge to the top node and
 * on to the right one, the branch down to the bottom node, at the logo's own
 * proportions (its 58-unit square scaled to 14 units). Inline like the
 * harness marks (the desktop bundles no image files), in `currentColor`, so
 * it takes the ink, hover and selection of whatever holds it; `tone="brand"`
 * draws it in git's own orange. It takes the props a lucide icon takes
 * (`size`, `strokeWidth`, `className`, `aria-hidden`), so an icon slot typed
 * `LucideIcon | Mark` holds it as it holds any glyph.
 */

import type { MarkProps } from "./harnessMarks";

/** Git's orange, from the same page (`Git-Icon-1788C.svg`). */
const GIT_ORANGE = "#F05032";

export function GitMark({ size = 14, strokeWidth = 2, className, title, tone, ...rest }: MarkProps & { strokeWidth?: number }) {
  const ink = tone === "brand" ? GIT_ORANGE : "currentColor";
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={ink}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : rest["aria-hidden"] ?? true}
    >
      {title && <title>{title}</title>}
      {/* The logo's square, upright, turned on its corner as the logo is. */}
      <g transform="rotate(-45 12 12)">
        <rect x="5" y="5" width="14" height="14" rx="2.5" />
        <path d="M14.75 5v9.9M14.75 9.35 9.75 14.9" />
        {/* The three nodes: round-capped dots, as lucide draws a dot, a little wider than the stroke. */}
        <path d="M14.75 9.35h.01M14.75 14.9h.01M9.75 14.9h.01" strokeWidth={strokeWidth * 1.75} />
      </g>
    </svg>
  );
}
