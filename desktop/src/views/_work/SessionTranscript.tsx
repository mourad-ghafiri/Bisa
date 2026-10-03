/**
 * A session's transcript, tailed (ide/09): the harness's own file as the
 * node serves it (`GET /sessions/{id}/transcript?from_byte=`), read by byte
 * offset so a long run streams in rather than refetching from the top, and
 * polled only while `live` and only while the window is visible — on
 * becoming visible it catches up once. The one reader: the work item's pane
 * on a goal, the transcript pane beside any screen, and the workbench tab
 * all draw this.
 */

import { useEffect, useRef, useState } from "react";
import { api } from "../../api";
import { LinkedText, Spinner, failureText, cn } from "../../ui";
import { isHidden, onVisibilityChange } from "../../shell/visibility";
import { t as tr } from "../../i18n/l10n.mjs";

/** How often a live transcript is asked for its next bytes. */
const POLL_MS = 2000;
/** What is kept on screen; older bytes scroll off the top. */
const KEEP_CHARS = 20_000;

export function SessionTranscript({ session, live, className }: { session: string; live: boolean; className?: string }) {
  const [text, setText] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const cursor = useRef(0);
  const box = useRef<HTMLPreElement>(null);

  useEffect(() => {
    let stop = false;
    const ac = new AbortController();
    cursor.current = 0;
    setText("");
    const pull = async () => {
      try {
        const { text: chunk, next_byte } = await api.transcript(session, cursor.current, ac.signal);
        if (stop) return;
        cursor.current = next_byte;
        if (chunk) {
          setText((t) => (t + chunk).slice(-KEEP_CHARS));
          const el = box.current;
          if (el) el.scrollTop = el.scrollHeight;
        }
        setErr(null);
      } catch (e) {
        if (!stop && !ac.signal.aborted) setErr(failureText("work", "session-transcript-failed", e));
      }
    };
    void pull();
    // The tail polls only while visible; on becoming visible it catches up once.
    // The byte cursor is untouched, so nothing refetches from the top.
    const t = live
      ? window.setInterval(() => {
          if (!isHidden()) void pull();
        }, POLL_MS)
      : undefined;
    const off = live
      ? onVisibilityChange(() => {
          if (!isHidden()) void pull();
        })
      : undefined;
    return () => {
      stop = true;
      ac.abort();
      if (t) window.clearInterval(t);
      off?.();
    };
  }, [session, live]);

  if (err) return <p className="text-2xs text-text-dim">{tr("work-session-transcript-no-transcript-available")}</p>;
  if (!text) return <Spinner label={tr("work-session-transcript-reading-transcript")} />;
  return (
    <pre ref={box} className={cn("overflow-auto rounded-control bg-surface-2 p-2 font-mono text-2xs whitespace-pre-wrap", className ?? "max-h-72")}>
      <LinkedText text={text} />
    </pre>
  );
}
