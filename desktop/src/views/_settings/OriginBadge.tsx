/**
 * The origin badge a setting's control carries (ide/13 — *Visible in the
 * UI*): which scope the value on screen came from, with the sentence behind
 * it on hover or keyboard focus — the badge is a stop in the tab order, so
 * the sentence is never a pointer's alone. One component for the registry's
 * rows and every hand-written panel that draws a key of its own; the words
 * are `settingOriginModel.mjs`'s.
 */

import type { ResolvedSetting } from "../../types";
import { Chip, Tooltip } from "../../ui";
import { originBadge } from "./settingOriginModel.mjs";

export function OriginBadge({ origin }: { origin: ResolvedSetting["origin"] }) {
  const badge = originBadge(origin);
  return (
    <Tooltip label={badge.hint}>
      <span tabIndex={0} className="inline-flex rounded-full">
        <Chip tone={badge.tone}>{badge.word}</Chip>
      </span>
    </Tooltip>
  );
}
