/** The node's own sentence when the error is an `ApiError` with words, else `null`. */
export declare function nodeSentence(error: unknown): string | null;
/** The words for a failed act, led by what failed: the node's own reason after it, or the pointer to the diagnostic log when the error is a raw exception (`raw`). */
export declare function failureWords(what: string, error: unknown): { text: string; raw: boolean };
/** The reason alone, for a sentence that already says what failed: the node's words, or where the detail went (`raw`). */
export declare function reasonWords(error: unknown): { text: string; raw: boolean };
/** The words for a failure nothing names: the node's sentence, or that something unexpected stopped it (`raw`). */
export declare function unexpectedWords(error: unknown): { text: string; raw: boolean };
