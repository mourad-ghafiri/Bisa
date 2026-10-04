import type { Keymap, KeyLike } from "../shell/keymapModel.mjs";

/** What each `typed` command types into the shell. */
export declare const TYPED: Readonly<Record<"line_start" | "line_end" | "word_left" | "word_right" | "delete_to_line_start" | "delete_word_right", string>>;
/** The bytes a key typed in a focused terminal types instead of what xterm would send, or null. */
export declare function typedFor(keymap: Pick<Keymap, "bindings">, e: KeyLike, mac: boolean): string | null;
