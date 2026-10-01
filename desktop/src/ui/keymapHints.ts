/**
 * The kit shows chords beside menu items but does not own the keymap — the
 * shell does (`shell/shortcuts.ts`), and `ui/` never imports from `shell/`.
 * So the shell provides a lookup here whenever the keymap resolves, and a
 * menu asks for the chord of a command id. Nothing is bound through this
 * file; a chord shown is one the shell already fires.
 */

let lookup: (command: string) => string | null = () => null;

/** The shell's lookup, replaced on every keymap reload; a menu asks at draw time, so nothing subscribes. */
export function provideChords(next: (command: string) => string | null): void {
  lookup = next;
}

/** The chord for a command, in the shell's current keymap; null when unbound or unknown. */
export function chordHint(command: string | null | undefined): string | null {
  return command ? lookup(command) : null;
}
