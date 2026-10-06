/**
 * A collapsible group header for dense lists — the sidebar's sections and
 * anywhere else a list needs a title, a count and one action.
 *
 * The chevron rotates rather than being swapped for a second glyph, so the
 * open and closed states are visibly the same control in two positions. It is
 * `aria-hidden`; `aria-expanded` on the button is what actually says which
 * way it is.
 *
 * `useCollapsed` lives here rather than in the shell because the header and
 * the state it draws belong together: a *view* may not import from `shell/`,
 * so leaving the hook there meant a screen could have the control and not the
 * memory of it.
 */

import type { ReactNode } from "react";
import { cn } from "./cn";
import { ICON } from "./icons";

/**
 * Whether a named section is collapsed, remembered across reloads.
 *
 * Both the read and the write are guarded: a webview with storage denied
 * (private mode, a locked-down embedder) should render an expanded section,
 * not a blank one, and a section that fails to remember it was collapsed is
 * not worth an error anyone sees.
 *
 * `initial` is the section's own default. It matters for a group whose
 * ordinary state is closed — an outcome list nobody needs open every time —
 * where treating "nothing stored" as "expanded" would be the wrong answer on
 * a first run and every run after a storage wipe.
 */
export function SectionHeader({
  title,
  count,
  showZero = false,
  open = true,
  onToggle,
  action,
  alwaysAction = false,
  trailing,
  flush = false,
}: {
  title: string;
  count?: number;
  /** Draw the count at zero too — a list that is empty on purpose says *0*, not nothing. */
  showZero?: boolean;
  open?: boolean;
  onToggle?: () => void;
  /** One verb for the whole section; revealed on hover unless `alwaysAction`. */
  action?: ReactNode;
  /** The action at rest — for a bulk verb a person reaches for first (*Stage all*), or a destructive one that must never appear under a sweeping pointer. */
  alwaysAction?: boolean;
  /** A fact at the right edge, always visible — a summary, a status word. */
  trailing?: ReactNode;
  /**
   * Flush with its content's edge. The header is inset by default to line up
   * with rows that carry the same inset (a list, a tree, the sidebar); over
   * prose, a field or a card — an inspector's sections — it stands flush, so
   * the heading and what it heads share one edge.
   */
  flush?: boolean;
}) {
  // The chevron is the fold's control; a header that does not fold draws none,
  // so a plain section never promises a disclosure it has not got.
  const label = (
    <>
      {onToggle && (
        <ICON.collapsed
          size={11}
          aria-hidden
          className={cn("anim shrink-0 text-text-dim", open && "rotate-90")}
        />
      )}
      {/* Sentence case: a group's name, not a label in capitals — forty rows
          under five shouted headings read as five alarms. */}
      <span className="text-2xs font-semibold text-text-dim">{title}</span>
      {count !== undefined && (count > 0 || showZero) && (
        <span className="tnum text-2xs font-normal text-text-dim">{count}</span>
      )}
    </>
  );
  return (
    // `min-h-6` and `flex-wrap`, not a fixed `h-6`: at a panel's narrowest a
    // header's actions (the remotes' layout switch, *Fetch* and *Add remote…*)
    // did not share the line with its name and were cut at the panel's edge;
    // wrapped, they keep the right (`ml-auto`) on a line of their own.
    <div className={cn("group flex min-h-6 shrink-0 flex-wrap items-center gap-1.5", flush ? "px-0" : "px-2")}>
      {onToggle ? (
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          className="flex flex-1 items-center gap-1.5 text-left"
        >
          {label}
        </button>
      ) : (
        <span className="flex flex-1 items-center gap-1.5">{label}</span>
      )}
      {trailing && <span className="text-2xs text-text-dim">{trailing}</span>}
      {action && <span className={cn("ml-auto", alwaysAction ? "anim" : "row-actions anim")}>{action}</span>}
    </div>
  );
}
