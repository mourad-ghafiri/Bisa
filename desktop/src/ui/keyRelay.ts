/**
 * A key an addon's page passed on reaches the app through here. The kit holds
 * the door and does not own the keymap — the shell does, and `ui/` never
 * imports from it — so the shell provides the judge when it mounts and the
 * addon bridge hands it a key. Nothing is bound through this file.
 */

/** A relayed key's facts, as a keyboard event carries them. */
export interface RelayedKeyFacts {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

let relay: (frame: HTMLElement, key: RelayedKeyFacts) => void = () => {};

/** The shell's judge, set once at start. */
export function provideKeyRelay(next: (frame: HTMLElement, key: RelayedKeyFacts) => void): void {
  relay = next;
}

/** Hand the app a key the frame's page did not use. */
export function relayKey(frame: HTMLElement, key: RelayedKeyFacts): void {
  relay(frame, key);
}
