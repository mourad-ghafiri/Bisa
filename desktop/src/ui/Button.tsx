/**
 * The one button.
 *
 * Four variants, and the split between them is about consequence, not looks:
 * `primary` is the single action a screen is asking for, `default` is
 * everything else you might do, `ghost` is an action that lives inside a row
 * and should not compete with the row's content, and `danger` is anything
 * that destroys something. A screen with two primaries is a screen that has
 * not decided.
 *
 * `primary` fills with the accent, which is the colour that means "your
 * attention" — so a primary button on a screen that is not waiting for you is
 * the same mistake as an accent-coloured badge there.
 */

import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";
import { Tooltip } from "./Tooltip";

/*
 * `primary` and `default` are raised (`shadow-sm`, the family's own
 * `shadow-raised`: a light edge on glass, nothing on a matte family), so a
 * button reads as a thing you press and a ghost reads as part of its row.
 * A press darkens the ground rather than moving the button: in a dense
 * toolbar a control that shifts under the pointer reads as a layout jump.
 */
const button = cva(
  "anim inline-flex shrink-0 select-none items-center justify-center rounded-control border font-medium disabled:pointer-events-none disabled:opacity-45",
  {
    variants: {
      variant: {
        primary: "border-accent bg-accent text-accent-contrast shadow-sm hover:opacity-90 active:opacity-80",
        default: "border-border bg-surface text-text shadow-sm hover:bg-surface-2 active:bg-selected",
        ghost:
          "border-transparent bg-transparent text-text-dim hover:bg-surface-2 hover:text-text active:bg-selected",
        danger: "border-border bg-transparent text-danger hover:bg-danger-soft active:bg-danger-soft",
      },
      size: {
        sm: "h-7 gap-1 px-2 text-xs",
        md: "h-8 gap-1.5 px-3 text-sm",
        /** Square, for a button whose whole label is its glyph. */
        icon: "h-7 w-7 gap-0 p-0 text-sm",
      },
    },
    defaultVariants: { variant: "default", size: "md" },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof button> {
  children?: ReactNode;
  /**
   * Render the child element instead of a `<button>`, keeping these styles.
   * For the cases where the thing being styled must be an anchor, or must be
   * the element a Radix trigger hands down — never as a way to make a
   * non-interactive element look clickable.
   */
  asChild?: boolean;
  /**
   * Why the button is disabled, while it is. A disabled button takes no
   * pointer and no focus, so a `title` on it is never seen; the reason goes
   * on a focusable wrapper instead — a tooltip for the pointer and the
   * keyboard, and words a screen reader reads beside the button.
   */
  disabledReason?: string;
}

export function Button(props: ButtonProps) {
  const { variant, size, className, children, asChild = false, disabledReason, ...rest } = props;
  // A caller that passes the prop at all — even as `undefined` while there
  // is no reason yet — gets the same tree in every state.
  const reasoned = "disabledReason" in props;
  const Comp = asChild ? Slot : "button";
  const element = (
    <Comp
      // `asChild` hands the type down to whatever the child is; a bare
      // <button> inside a form defaults to submit, which is almost never what
      // a kit button means.
      {...(asChild ? {} : { type: "button" as const })}
      // Said on the element, so a surface with a rule about its buttons can
      // read it — an empty state's door is never a ghost (`EmptyState`).
      data-variant={variant ?? "default"}
      className={cn(button({ variant, size }), className)}
      {...rest}
    >
      {children}
    </Comp>
  );
  if (!reasoned || asChild) return element;
  // The wrapper stands whether or not the button is disabled, so a button
  // that disables under the keyboard (a Save while it checks) is never
  // remounted; only the wrapper's tab stop, its tip and its words follow
  // the state.
  const explained = Boolean(rest.disabled) && Boolean(disabledReason);
  return (
    <Tooltip label={disabledReason} active={explained}>
      <span tabIndex={explained ? 0 : -1} className="inline-flex shrink-0 rounded-control">
        {element}
        {explained && <span className="sr-only">{disabledReason}</span>}
      </span>
    </Tooltip>
  );
}
