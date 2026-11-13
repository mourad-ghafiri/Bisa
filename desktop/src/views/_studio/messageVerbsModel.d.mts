export type MessageVerb = "reply" | "copy" | "copy-link" | "retract";
export declare const VERBS: readonly MessageVerb[];
export declare function messageVerbs(message: { mine: boolean; retracted: boolean }): MessageVerb[];
export declare function verbWords(id: MessageVerb): { label: string; icon: "reply" | "copy" | "link" | "delete"; danger: boolean; apart: boolean };
/** The words a question about one message quotes; `null` for a message of files alone. */
export declare function quotedWords(message: { content?: string | null } | null | undefined): string | null;
