/**
 * One line of the Pulse, and what it could not fit.
 *
 * A row leads with fixed columns — who, the concept's glyph, then what — and
 * they keep their width when empty: an activity list whose left edge moves
 * per row is a list you have to read rather than scan, and scanning is the
 * only thing the screen is for. The sentence and the chevron are separate
 * controls rather than one row that does two things depending on where you
 * hit it: the sentence opens what the row is about, the chevron opens the
 * detail — a verification's evidence, a decision's rationale, a result's
 * output — everything the event carried and none of which fit on a line.
 */

import type { DetailField, Tone } from "../../activity";
import type { PulseSource } from "../../types";
import { Avatar, ICON, LinkedText, RelativeTime, type LucideIcon } from "../../ui";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { t } from "../../i18n/l10n.mjs";

export interface PulseItem {
  key: string;
  seq: number;
  at: number;
  text: string;
  tone: Tone;
  concept: string;
  /** What the row is about — the door the sentence opens, when there is one. */
  source: PulseSource;
  author?: string;
  title?: string;
  icon?: LucideIcon;
  detail?: DetailField[];
}

const TONE_CLASS: Record<Tone, string> = {
  spine: "text-text",
  dim: "text-text-dim",
  fail: "text-danger",
  wait: "text-warn",
  warn: "text-warn",
  danger: "text-danger",
};

export function PulseRow({
  item,
  heading,
  name,
  expanded,
  hasDoor,
  onOpen,
  onToggle,
}: {
  item: PulseItem;
  heading?: string;
  /** The author's name, resolved by the workspace; never the key. */
  name?: string;
  expanded: boolean;
  hasDoor: boolean;
  onOpen: () => void;
  onToggle: () => void;
}) {
  const ws = useWorkspace();
  const Glyph = item.icon;
  const Chevron = expanded ? ICON.expanded : ICON.collapsed;
  return (
    <div className="rounded-control">
      <div className="flex h-[36px] w-full items-center gap-2 px-2 text-xs">
        {item.author ? <Avatar id={item.author} photo={ws.photoOf(item.author)} size={16} /> : <span aria-hidden className="w-4 shrink-0" />}
        <span aria-hidden className="flex w-3.5 shrink-0 justify-center text-text-dim">{Glyph && <Glyph size={13} />}</span>
        <button
          type="button"
          onClick={onOpen}
          disabled={!hasDoor}
          className={`anim flex min-w-0 flex-1 items-baseline gap-1.5 rounded-control px-1 py-1 text-left ${hasDoor ? "hover:bg-surface-2" : "cursor-default"}`}
        >
          {heading && <span className="max-w-[40%] shrink-0 truncate font-medium text-text">{heading}</span>}
          {/* The author's name, not their key. `Avatar` is a colour and two
              letters; a hex string beside it is not a second identifier, it is
              noise the reader has to skip on every row. */}
          {name && <span className="shrink-0 text-text-dim">{name}</span>}
          <span className={`min-w-0 flex-1 truncate ${TONE_CLASS[item.tone]}`}>{item.text}</span>
        </button>
        {item.detail && (
          <button
            type="button"
            onClick={onToggle}
            aria-expanded={expanded}
            aria-label={expanded ? t("pulse-pulse-row-hide-detail") : t("pulse-pulse-row-show-detail")}
            className="anim shrink-0 rounded-control p-1 text-text-dim hover:bg-surface-2 hover:text-text"
          >
            <Chevron size={13} aria-hidden />
          </button>
        )}
        <RelativeTime at={item.at} />
      </div>
      {expanded && item.detail && (
        <dl className="mr-2 mb-1 ml-[46px] space-y-1.5 rounded-control border border-border bg-surface px-3 py-2">
          {item.detail.map((f) => (
            <div key={f.label}>
              <dt className="text-2xs font-medium tracking-wide text-text-dim uppercase">{f.label}</dt>
              {/* `pre-wrap`, because evidence and a rejected result's output
                  are the two things here that were written with newlines in
                  them and mean less without. */}
              <LinkedText as="dd" className="text-2xs whitespace-pre-wrap break-words text-text" text={f.value} />
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}
