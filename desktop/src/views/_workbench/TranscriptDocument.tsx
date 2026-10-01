/**
 * A session's transcript as a document in the workbench's centre (ide/09):
 * the `{kind: "transcript"}` tab, opened from the transcript pane's *Open as
 * a tab*, drawn whole in the tab's frame with the same tail the pane uses.
 * The session is read from the roster; once the roster has dropped it, from
 * the node by id, so a tab kept across a reload still names it.
 */

import { SessionMark } from "../../ui";
import { SessionTranscript } from "../_work/SessionTranscript";
import { transcriptTitle, transcriptWords } from "../_work/sessionTranscriptModel.mjs";
import { useSessionRow } from "../../shell/useSessionRow";

export function TranscriptDocument({ session }: { session: string }) {
  const row = useSessionRow(session);
  const line = transcriptWords(row);
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-3">
      <div className="flex shrink-0 items-center gap-2 text-2xs">
        {row && <SessionMark state={row.state} />}
        <span className="font-mono text-text">{transcriptTitle(row)}</span>
        <span className="min-w-0 truncate text-text-dim">{line.words}</span>
      </div>
      <SessionTranscript session={session} live={line.live} className="min-h-0 flex-1" />
    </div>
  );
}
