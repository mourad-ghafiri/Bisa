/**
 * A command's chord, as the platform writes it — or nothing when the person
 * unbound it. The one way a screen shows a shortcut, so the hint and the
 * handler cannot disagree.
 */
import { KeyHint } from "../ui";
import { useChord } from "./useKeymap";

export function CommandHint({ id, className }: { id: string; className?: string }) {
  const chord = useChord(id);
  return chord ? <KeyHint combo={chord} className={className} /> : null;
}
