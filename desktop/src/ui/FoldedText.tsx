/**
 * Long text folded to its first sentence, with one control to show the rest
 * (rendered as markdown) and fold it back. The words and the cut are
 * `foldModel.mjs`'s; this draws. A text that fits in one sentence has no
 * control at all — a chevron with nothing behind it is a lie.
 *
 * Remembered per `storeKey` when one is given (`collapsedStore`), so a thread
 * a person opened stays open across a refresh; local otherwise.
 */

import { useState } from "react";
import { cn } from "./cn";
import { ICON } from "./icons";
import { Markdown } from "./Markdown";
import { Tooltip } from "./Tooltip";
import { useCollapsed } from "./collapsedStore";
import { firstSentence, foldLabel, needsFold } from "./foldModel.mjs";

function Fold({ text, className, open, toggle }: { text: string; className?: string; open: boolean; toggle: () => void }) {
  const foldable = needsFold(text);
  return (
    <div className={cn("flex min-w-0 items-start gap-1", className)}>
      {open && foldable ? (
        <Markdown text={text} className="min-w-0 flex-1" />
      ) : (
        <span className="min-w-0 flex-1 text-text" title={foldable ? undefined : text}>
          {firstSentence(text)}
        </span>
      )}
      {foldable && (
        <Tooltip label={foldLabel(open)}>
          <button
            type="button"
            aria-expanded={open}
            aria-label={foldLabel(open)}
            onClick={toggle}
            className="anim mt-px flex h-4 w-4 shrink-0 items-center justify-center rounded text-text-dim hover:bg-surface-2 hover:text-text"
          >
            {open ? <ICON.expanded size={11} aria-hidden /> : <ICON.collapsed size={11} aria-hidden />}
          </button>
        </Tooltip>
      )}
    </div>
  );
}

function StoredFold({ text, className, storeKey }: { text: string; className?: string; storeKey: string }) {
  const [collapsed, toggle] = useCollapsed(storeKey, true);
  return <Fold text={text} className={className} open={!collapsed} toggle={toggle} />;
}

function LocalFold({ text, className }: { text: string; className?: string }) {
  const [open, setOpen] = useState(false);
  return <Fold text={text} className={className} open={open} toggle={() => setOpen((o) => !o)} />;
}

export function FoldedText({ text, className, storeKey }: { text: string; className?: string; storeKey?: string }) {
  return storeKey ? <StoredFold text={text} className={className} storeKey={storeKey} /> : <LocalFold text={text} className={className} />;
}
