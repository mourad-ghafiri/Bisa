export type When = "global" | "workbench" | "tabs" | "files" | "editor" | "document" | "terminal" | "browser";
export interface Command {
  id: string;
  label: string;
  when: When;
  chords: Partial<Record<"default" | "vscode", string>>;
  /** The chords a Mac takes in place of `chords`, where its hands differ. */
  mac?: Partial<Record<"default" | "vscode", string>>;
  always?: boolean;
  /** The shell's own text editing: the terminal types the chord as a key (`terminal/typedKeysModel.mjs`). */
  typed?: boolean;
  note?: string;
}
export interface Binding {
  id: string;
  label: string;
  when: When;
  chord: string | null;
  source: "preset" | "override";
  note: string | null;
  always: boolean;
  typed: boolean;
}
export interface Keymap {
  preset: string;
  bindings: Binding[];
  warnings: string[];
}
export interface KeyLike {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}
export declare const PRESETS: readonly string[];
export declare const WHENS: readonly When[];
export declare const COMMANDS: readonly Command[];
export declare function parseChord(chord: string): { mod: boolean; ctrl: boolean; shift: boolean; alt: boolean; key: string } | null;
export declare function canonicalChord(chord: string): string | null;
export declare function matchesEvent(e: KeyLike, chord: string, mac: boolean): boolean;
export declare function chordFromEvent(e: KeyLike, mac: boolean): string | null;
export declare function resolveKeymap(preset: unknown, overrides: unknown, mac?: boolean): Keymap;
export declare function conflictFor(keymap: Keymap, id: string, chord: string): string | null;
export declare function chordFor(keymap: Keymap, id: string): string | null;
export declare function interceptsInTerminal(binding: { chord: string | null; when?: string; always?: boolean; typed?: boolean } | null | undefined, mac: boolean): boolean;
export interface RelayChord {
  key: string;
  shift: boolean;
  alt: boolean;
  command: string;
}
export declare function relayChords(keymap: Keymap): RelayChord[];
export declare function commandForEvent(keymap: Keymap, e: KeyLike, contexts: Iterable<string>, mac: boolean): string | null;
export declare function bindingsIn(keymap: Keymap, when: When): Binding[];
export declare function filterBindings(bindings: Binding[], needle: string | null | undefined): Binding[];
export declare function keymapMarkdown(): string;
