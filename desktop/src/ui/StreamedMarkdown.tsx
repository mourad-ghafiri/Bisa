/**
 * Markdown for a reply still being written (13 — Conversations §The reply
 * streams): the text cut into the blocks that are done and the one still
 * arriving (`streamBlocksModel.settledBlocks`). Each settled block is one
 * memoised `Markdown` — parsed when it settles, never again, a diagram
 * included — and the tail alone is re-parsed as its words land. A settled
 * block only ever gains a successor, so its index is a stable key.
 */

import { memo, useMemo } from "react";
import { settledBlocks } from "../views/_studio/streamBlocksModel.mjs";
import { Markdown } from "./Markdown";
import { cn } from "./cn";

const Block = memo(function Block({ text }: { text: string }) {
  return <Markdown text={text} />;
});

export function StreamedMarkdown({ text, className }: { text: string; className?: string }) {
  const { settled, tail } = useMemo(() => settledBlocks(text), [text]);
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {settled.map((block, i) => (
        <Block key={i} text={block} />
      ))}
      {tail.trim() !== "" && <Markdown text={tail} />}
    </div>
  );
}
