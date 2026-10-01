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

const button = cva(
  "anim inline-flex shrink-0 items-center justify-center rounded-control border font-medium disabled:pointer-events-none disabled:opacity-45",
  {
    variants: {
      variant: {
        primary: "border-accent bg-accent text-accent-contrast hover:opacity-90",
        default: "border-border bg-surface text-text hover:bg-surface-2",
        ghost:
          "border-transparent bg-transparent text-text-dim hover:bg-surface-2 hover:text-text",
        danger: "border-border bg-transparent text-danger hover:bg-danger-soft",
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
}

export function Button({
  variant,
  size,
  className,
  children,
  asChild = false,
  ...rest
}: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      // `asChild` hands the type down to whatever the child is; a bare
      // <button> inside a form defaults to submit, which is almost never what
      // a kit button means.
      {...(asChild ? {} : { type: "button" as const })}
      className={cn(button({ variant, size }), className)}
      {...rest}
    >
      {children}
    </Comp>
  );
}
