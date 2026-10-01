export const PLAIN_FOLDER: string;
export const INIT_LABEL: string;

export function initOffer(facts: { kind: string | null | undefined; exists: boolean | null | undefined; git: boolean | null | undefined }): { shown: boolean };
export function initConsequence(facts: { adopted: boolean; path?: string | null }): { needsConfirm: boolean; body: string };
export function initDoneWords(committer: string | null | undefined): string;
