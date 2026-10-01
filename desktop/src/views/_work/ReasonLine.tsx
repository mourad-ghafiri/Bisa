/**
 * Why a control is off, or what the next thing to do is — one dim line under
 * the control, said once, with the door beside it when there is one (ide/04).
 * A reason that lived only in a disabled button's tooltip was a reason a
 * person on a keyboard or a touchpad never read; here it is a line.
 */
import type { ReactNode } from "react";
import { Button, ICON } from "../../ui";

export function ReasonLine({
  children,
  tone = "dim",
  door,
  className = "",
}: {
  children: ReactNode;
  /** `dim` for a fact, `warn` for a condition to know about, `danger` for what blocks. */
  tone?: "dim" | "warn" | "danger";
  /** A way to fix it, beside the sentence. */
  door?: { label: string; onClick: () => void } | null;
  className?: string;
}) {
  const color = tone === "danger" ? "text-danger" : tone === "warn" ? "text-warn" : "text-text-dim";
  return (
    <div className={`flex flex-wrap items-center gap-x-2 gap-y-0.5 text-2xs ${color} ${className}`} role={tone === "dim" ? undefined : "status"}>
      <span className="min-w-0">{children}</span>
      {door && (
        <Button size="sm" variant="ghost" onClick={door.onClick} className="h-5 px-1 text-2xs">
          {door.label}
          <ICON.forward size={10} aria-hidden />
        </Button>
      )}
    </div>
  );
}
