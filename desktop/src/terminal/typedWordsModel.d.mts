export declare const ENTER_AFTER_MS: number;
export declare const ENTER: string;
/** The bytes to write for `text` — bracketed as a paste when the program asked, one line otherwise — and the Enter to follow them. */
export declare function typedWords(text: string, bracketed: boolean): { text: string; enter: string };
