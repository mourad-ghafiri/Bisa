/**
 * One review already given, as the Review step and the agent request both
 * draw it: its verdict as a chip, when, and its words folded to a first
 * sentence (`FoldedText`).
 */

import type { ReviewSummary } from "../../types";
import { Chip, FoldedText } from "../../ui";
import { relative } from "../../i18n/format.mjs";
import { reviewTone } from "./prReviewModel.mjs";
import { unixOf } from "./reviewStepModel.mjs";

export function When({ iso }: { iso: string | null | undefined }) {
  const at = unixOf(iso);
  if (at === null) return null;
  return (
    <span className="shrink-0 text-2xs text-text-dim" title={iso ?? undefined}>
      {relative(at)}
    </span>
  );
}

export function ReviewGiven({ review, words, verdict, storeKey }: { review: ReviewSummary; words: string; verdict: string; storeKey: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex items-center gap-2">
        <Chip tone={reviewTone(review.state)}>{verdict}</Chip>
        <span className="flex-1" />
        <When iso={review.submitted_at} />
      </div>
      {words.trim() && <FoldedText text={words} storeKey={storeKey} className="text-text-dim" />}
    </div>
  );
}
