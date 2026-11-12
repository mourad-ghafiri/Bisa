/**
 * The answer of this control's last write, held until the list it mirrors is
 * read again (`heldAnswerModel.mjs`): `[held, hold]`, where `held` is `null`
 * as soon as `listed` is another list than the one the answer was made over.
 */
import { useCallback, useState } from "react";
import { heldFor, holding } from "./heldAnswerModel.mjs";

export function useHeldAnswer<L, A>(listed: L): [A | null, (answer: A) => void] {
  const [held, setHeld] = useState<{ over: L; answer: A } | null>(null);
  const hold = useCallback((answer: A) => setHeld(holding(listed, answer)), [listed]);
  return [heldFor(listed, held), hold];
}
