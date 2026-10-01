/**
 * A door to one Settings tab, drawn as a link — the same navigation the
 * settings rail makes, so a sentence on any screen can point at where a
 * choice is made without repeating the panel.
 */

import type { ReactNode } from "react";
import { navigate } from "../../router";
import { cn } from "../../ui";
import { settingsSearch, type SettingsTab } from "./settingsLink.mjs";

export function SettingsLink({
  tab,
  children,
  onFollow,
  className,
}: {
  tab: SettingsTab;
  children: ReactNode;
  /** Runs as the door is taken, before the navigation — a dialog that holds a question drops it. */
  onFollow?: () => void;
  className?: string;
}) {
  return (
    <button
      type="button"
      className={cn("anim text-2xs text-accent-ink underline underline-offset-2 hover:text-text", className)}
      onClick={() => {
        onFollow?.();
        navigate({ name: "settings" }, settingsSearch(tab));
      }}
    >
      {children}
    </button>
  );
}
