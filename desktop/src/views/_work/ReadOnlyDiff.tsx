/**
 * A patch as history: the hunk view without its write controls. The commit
 * inspector and the Stashes view both render one, so the two say the same
 * thing about a change that is not the working tree's.
 */

import { HunkDiff } from "./HunkDiff";

export function ReadOnlyDiff({ diff }: { diff: string }) {
  // HunkDiff stages through a project id; passing an empty one with `busy`
  // set disables every control, and no note composer opens on history.
  return <HunkDiff pid="" wid="" path="" staged={false} diff={diff} busy onApplied={() => {}} onNoted={() => {}} />;
}
