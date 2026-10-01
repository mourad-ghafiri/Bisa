export declare const MASK: string;
export declare function isMasked(v: unknown): boolean;
export declare function secretView(facts: { draft: string | null | undefined; stored?: boolean | null; revealed?: boolean; what?: string }): {
  shows: "draft" | "stored" | "empty";
  value: string;
  inputType: "password" | "text";
  canReveal: boolean;
  eye: { label: string; pressed: boolean; title: string };
  placeholder: string;
  dim: boolean;
};
export declare function eyeLabel(revealed: boolean, what?: string): string;
export declare function replaceOnFocus(draft: string | null | undefined, stored: boolean | null | undefined): boolean;
export declare function foldedWords(text: string | null | undefined): string;
