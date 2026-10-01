export type MessageVerb = "reply" | "copy" | "copy-link" | "retract";
export declare const VERBS: readonly MessageVerb[];
export declare function messageVerbs(message: { mine: boolean; retracted: boolean }): MessageVerb[];
export declare function verbWords(id: MessageVerb): { label: string; icon: "reply" | "copy" | "link" | "delete"; danger: boolean; apart: boolean };
