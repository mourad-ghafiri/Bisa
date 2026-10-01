interface ChoiceLike {
  id: string;
  tone?: string;
  disabled?: string | null;
}
export function isOpen(choice: { disabled?: string | null } | null | undefined): boolean;
export function openingChoice(choices: readonly ChoiceLike[]): string | null;
export function stepChoice(choices: readonly ChoiceLike[], current: string | null, step: 1 | -1): string | null;
