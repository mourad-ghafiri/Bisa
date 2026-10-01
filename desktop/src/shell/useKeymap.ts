/**
 * The live keymap as a hook: the preset and the overrides from Settings, so a
 * chord a component shows is the chord the handler fires (ide/15). Every
 * `KeyHint` in the app reads through here; none hard-codes a chord.
 */
import { useSyncExternalStore } from "react";
import { chordFor } from "./keymapModel.mjs";
import type { Keymap } from "./keymapModel.mjs";
import { currentKeymap, onKeymapChange } from "./shortcuts";

export function useKeymap(): Keymap {
  return useSyncExternalStore(onKeymapChange, currentKeymap, currentKeymap);
}

/** The chord bound to a command right now, or null when it is unbound. */
export function useChord(id: string): string | null {
  return chordFor(useKeymap(), id);
}
