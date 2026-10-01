/**
 * The holder word alone — who a goal waits on — as a chip. One vocabulary
 * for Goals, the Inbox header, Teams and the omnibox, so "whose move is it"
 * reads the same everywhere. The word comes from the node (`Goal::holder` in
 * the core); this file only labels and tones it.
 */
import { Chip, type Tone } from "../../ui";
import type { Holder, RunStrip } from "../../types";
import { HOLDER_LABEL, HOLDER_TONE, failureWords, finishedTone } from "./goalStripModel.mjs";

const TONE: Record<string, Tone> = {
  warn: "warn",
  accent: "accent",
  "text-dim": "quiet",
  ok: "ok",
  danger: "danger",
};

function holderTone(holder: Holder, strip?: RunStrip | null): Tone {
  const role = holder === "finished" ? finishedTone(strip ?? undefined) : HOLDER_TONE[holder];
  return TONE[role] ?? "quiet";
}

export function HolderBadge({ holder, strip }: { holder: Holder; strip?: RunStrip | null }) {
  return (
    <Chip tone={holderTone(holder, strip)} title={holder === "finished" ? (failureWords(strip ?? undefined) ?? undefined) : undefined}>
      {HOLDER_LABEL[holder] ?? holder}
    </Chip>
  );
}
