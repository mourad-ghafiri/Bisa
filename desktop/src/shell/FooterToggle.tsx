/**
 * A footer switch for an overlay — the notes dock's, the drawings dock's —
 * tinted when its overlay is on. One component, so the two switches read
 * alike and `StatusBar.tsx` draws each as one line.
 */

import { ICON, Tooltip, cn } from "../ui";

export function FooterToggle({ icon: Icon, on, label, onClick, disabled = false }: { icon: (typeof ICON)[keyof typeof ICON]; on: boolean; label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <Tooltip label={label}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={on}
        disabled={disabled}
        onClick={onClick}
        className={cn(
          "anim flex h-6 w-6 shrink-0 items-center justify-center rounded-control disabled:opacity-45",
          on ? "bg-selected text-text" : "text-text-dim hover:bg-surface-2 hover:text-text",
        )}
      >
        <Icon size={13} aria-hidden />
      </button>
    </Tooltip>
  );
}
