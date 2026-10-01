/** Types for `amendModel.mjs`, plain JavaScript so `node --test` reads it. */

export declare function headMessage(subject: string | null | undefined, body: string | null | undefined): string;
export declare function amendDraft(on: boolean, message: string, head: string | null): string;
export declare function amendPlaceholder(short: string, subject: string | null | undefined): string;
export interface AmendWords {
  title: string;
  body: string;
  warning: string | null;
  confirm: string;
  danger: boolean;
  kind: "commit";
}
export declare function amendWords(facts: { short: string; subject?: string | null; staged: number; upstream?: string | null; ahead: number }): AmendWords;
