export const GUARD_VERBS: Readonly<{ cancel: string; discard: string; save: string }>;
export function guardWords(tabs: readonly { label: string; untitled: boolean }[]): { title: string; description: string; note: string };
