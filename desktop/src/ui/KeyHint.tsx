/** A keyboard shortcut, rendered the way the platform writes it. */

const MAC = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/** The named keys a chord may spell, as the platform draws them. */
const KEY_GLYPHS: Record<string, string> = {
  Comma: ",",
  Backslash: "\\",
  Backquote: "`",
  Space: "␣",
  Left: "←",
  Right: "→",
  Up: "↑",
  Down: "↓",
  Enter: "↩",
  Backspace: "⌫",
  Delete: "⌦",
  Tab: "⇥",
  Escape: "⎋",
};

export function keyLabel(combo: string): string {
  const parts = combo.split("+").map((p) => KEY_GLYPHS[p] ?? p);
  return MAC
    ? parts.map((p) => p.replace("Mod", "⌘").replace("Shift", "⇧").replace("Alt", "⌥").replace("Ctrl", "⌃")).join("")
    : parts.map((p) => p.replace("Mod", "Ctrl")).join("+");
}

export function KeyHint({ combo, className = "" }: { combo: string; className?: string }) {
  return (
    <kbd
      className={`rounded border border-border bg-surface-2 px-1 text-2xs font-medium text-text-dim ${className}`}
    >
      {keyLabel(combo)}
    </kbd>
  );
}

export const isMac = MAC;
