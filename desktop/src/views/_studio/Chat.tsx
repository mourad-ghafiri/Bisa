/**
 * The conversation. One component for channels, DMs, goal threads and
 * conversation — because they *are* the same act, and giving each its
 * own treatment is what made the app feel like four half-built screens.
 *
 * Identity comes from the shell's workspace store, so an author is a name and
 * a face; the old goal thread printed `a1b2c3d4…` at people. Agents are
 * marked, and when one is mid-turn the conversation says so instead of going
 * quiet — an agent thinking for forty seconds with no sign is indistinguishable
 * from an agent that is never going to answer.
 *
 * Three things the timeline promises, each of which costs something if it
 * breaks:
 *
 * - **A message's actions are the same verbs in the same order wherever you
 *   reach them.** The ⋮ menu and the right-click menu are built from one
 *   list (`messageVerbsModel.messageVerbs`: *Reply · Copy · Copy link*, and
 *   *Retract* on your own), so they cannot drift apart, and right-click is
 *   the gesture people arrive at a chat window already knowing; React, Reply
 *   and Copy stand on the hover bar too. *Copy* is the text as its author
 *   wrote it — no name, no time — through the kit's one clipboard door.
 * - **Nothing moves when you point at a row.** The action bar is absolutely
 *   positioned and revealed by opacity (`row-actions`, which also reveals on
 *   `:focus-within`), so the timeline you are reading does not reflow under
 *   the pointer and a keyboard user can still reach every verb.
 * - **The thread reads itself as it is shown** (`useReadAsShown` over
 *   `readModel.mjs`): wherever this component is mounted — the IDE's Agent
 *   pane, a goal's or a workflow's conversation, a channel, a direct message,
 *   the Inbox's own pane, a hosted channel — the scope is marked read a beat
 *   after the person can see its newest message: the window in front, the
 *   thread at its bottom, the first page landed. No route knows about
 *   reading; a reply that lands while you look is read as it lands, and one
 *   that lands while you are scrolled up waits until you come down.
 * - **The thread comes back where it was being read** (`useThreadPlace` over
 *   `threadPlaceModel.mjs`): a thread left above its bottom is kept as the
 *   message under the viewport's top edge and the offset inside it, and a
 *   mount puts it back — paging older messages in when the message is
 *   above the page, four pages at most — until the person's first wheel,
 *   click or key. A thread left at its bottom keeps nothing and opens at its
 *   bottom. Reading is untouched by this: a thread put back above new
 *   replies is not at its bottom, so it stays unread until the reader comes
 *   down — *Jump to newest* is the door.
 * - **The unread marker is captured once when you arrive and then stays put.**
 *   The thread reads itself a beat after it is shown, so a marker recomputed
 *   from the live count would vanish before you read the messages it was
 *   pointing at, and a refetch would move it.
 *
 * A rostered agent still does not answer unless it is addressed. Nothing here
 * — not the roster, not the address tray's suggestions — subscribes an agent
 * to a room; the tray is the list of who *this message* will reach.
 *
 * **A reply streams** (13 — Conversations §The reply streams). While an
 * agent's turn runs, its words, its thinking and the tool it runs arrive as
 * `agent_streamed` frames and stand at the foot of the timeline as a live
 * row (`LiveTurnRow`, reading its own turn from `liveTurnsStore` so a frame
 * re-renders the row and not the timeline) — the words paced to the
 * engine's cadence (`useStreamPacer`) and parsed block by block
 * (`StreamedMarkdown`) with a caret at the end, the thinking above them, a
 * dim *running Read …* line while a tool works. When the reply lands
 * (`agent_replied` names its message) the row stands, frozen, until that
 * message is in the page (`retireLandedTurns`), and the message takes its
 * place with its thinking above its words (`ThinkingBlock`). The footer's
 * control sets how every thinking shows — *auto · shown · hidden*
 * (`ThinkingPicker`, `thinkingStore`, remembered); one block can still be
 * opened or closed on its own.
 *
 * The timeline is deliberately *not* an `AnimatedList`, unlike the Inbox's.
 * Paging backwards prepends sixty rows at once, and a layout animation on
 * sixty rows reads as the app stuttering rather than as anything having
 * happened — and it would fight the scroll anchor below, whose whole job is
 * to make that prepend invisible.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import {
  ArtifactCard,
  artifactKey,
  Avatar,
  Button,
  Composer,
  ConfirmDialog,
  ContextMenu,
  EmptyState,
  ErrorNote,
  ICON,
  Markdown,
  Menu,
  Popover,
  RelativeTime,
  DayDivider,
  dayKey,
  Separator,
  Skeleton,
  StreamCaret,
  ThinkingBlock,
  Tooltip,
  WorkingDot,
  useToast,
  type LucideIcon,
  type ComposerFiles,
  type ComposerStop,
  type MenuItem,
  StreamedMarkdown,
  copyText,
  harnessMark,
} from "../../ui";
import { messageVerbs, verbWords } from "./messageVerbsModel.mjs";
import { TIMELINE_END } from "./turnChangesModel.mjs";
import { ChangedFilesBar } from "./ChangedFilesBar";
import type { ChangedFilesBarProps } from "./ChangedFilesBar";

/** The harness's own mark on the agent chip — which harness answered, at a glance. */
function HarnessGlyph({ harness }: { harness: string | null | undefined }) {
  const Glyph = harnessMark(harness);
  return <Glyph size={9} aria-hidden />;
}
import { api } from "../../api";
import type {
  AgentDef,
  ChannelDef,
  Directory,
  MessageAttachment,
  MessageRow,
} from "../../types";
import { planSummary } from "../../types";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { useArtifactLibraries } from "../../shell/artifactSettings";
import { hostedUnread } from "../../shell/hostedModel.mjs";
import { useVisible } from "../../shell/visibility";
import { atBottomOf, firstUnreadId as firstUnreadOf, shownToReader } from "./readModel.mjs";
import { useReadAsShown } from "./useReadAsShown";
import { chatKey } from "./chatScopeModel.mjs";
import { placeFrom, restoreStep } from "./threadPlaceModel.mjs";
import type { ThreadPlace, ThreadRow } from "./threadPlaceModel.mjs";
import { useThreadPlace } from "./useThreadPlace";
import { RESTORE_GRACE_MS } from "../../ui/keptScrollModel.mjs";
import { setSearch } from "../../router";
import { AddressTray, useAddressed } from "./AddressTray";
import { reachableIn } from "./addressModel.mjs";
import { ContextChipRow, ContextTray } from "./ContextChips";
import type { FrameChip } from "../_workbench/contextChips.mjs";
import { fitsBudget } from "../_workbench/contextChips.mjs";
import type { ContextRef } from "../../types";
import { useParticipants } from "./participants";
import { hostedMentionables, hostedNameOf, hostedPhotoOf } from "../../shell/hostedModel.mjs";
import {
  describe as describeFile,
  glyphFor,
  isRenderableImage,
} from "./attachmentModel.mjs";
import { QUICK_EMOJI, groupReactions, type ScopeKind } from "./scope";
import { useScopeMessages } from "./useScopeMessages";
import { thinkingOpen } from "./liveTurnModel.mjs";
import { contentOf, isCompact, threads } from "./timelineModel.mjs";
import { primeLiveTurns, retireLandedTurns, useLiveTurn, useLiveTurnAgents } from "./liveTurnsStore";
import { setThinkingMode, useThinkingMode } from "./thinkingStore";
import { ThinkingPicker } from "./ThinkingPicker";
import { useStreamPacer } from "./useStreamPacer";
import { t as tr } from "../../i18n/l10n.mjs";

/**
 * Whether one thinking block is open: the control's mode decides — *auto*
 * open only while the thinking is live and the words have not begun, *shown*
 * open, *hidden* folded — and this block's own press wins until the mode
 * changes again (`thinkingOpen`).
 */
function useThinkingOpen({ live, writing }: { live: boolean; writing: boolean }): [boolean, () => void] {
  const mode = useThinkingMode();
  const [own, setOwn] = useState<boolean | null>(null);
  // The control speaks for every block again once it is changed.
  useEffect(() => setOwn(null), [mode]);
  const open = thinkingOpen({ mode, own, live, writing });
  return [open, () => setOwn(!open)];
}

/** The thinking a landed reply carries, folded above its words. */
function MessageThinking({ text }: { text: string }) {
  const [open, toggle] = useThinkingOpen({ live: false, writing: true });
  return <ThinkingBlock text={text} open={open} onToggle={toggle} />;
}

/**
 * A turn in flight at the foot of the timeline: the agent, the working
 * dot, its thinking so far above its words so far — each paced to the
 * engine's cadence and parsed block by block, the caret at the end of
 * whichever is still being written — and the tool it runs, one dim line.
 * Reads its own turn, so a frame re-renders this row alone. Once the reply
 * landed it stands frozen until its message is drawn; then it goes.
 */
function LiveTurnRow({ scope, agentId, agent, name }: { scope: string; agentId: string; agent: AgentDef | undefined; name: string }) {
  const turn = useLiveTurn(scope, agentId);
  const landed = !!turn?.landed;
  const writing = (turn?.text.length ?? 0) > 0;
  const text = useStreamPacer(turn?.text ?? "", landed);
  // The thinking is shown whole once the words begin: the reader's eye is on the words now.
  const thinking = useStreamPacer(turn?.thinking ?? "", landed || writing);
  const [open, toggle] = useThinkingOpen({ live: !landed, writing });
  if (!turn) return null;
  const live = !landed;
  return (
    <div role="status" aria-label={landed ? tr("studio-chat-replied", { name }) : tr("studio-chat-writing-2", { name })} className="border-l-2 border-l-transparent px-3 py-1">
      <div className="flex gap-2">
        <div className="w-6 shrink-0 pt-0.5">
          <Avatar id={agent?.pubkey ?? agentId} name={name} photo={agent?.photo} size={24} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-1.5">
            <span className="text-xs font-semibold">{name}</span>
            {agent && (
              <Tooltip label={tr("studio-chat-runs", { agent: agent.name, harness: agent.harness, models: planSummary(agent.models) })}>
                <span className="inline-flex items-center gap-0.5 rounded-full border border-border px-1 text-3xs leading-tight font-medium tracking-wide text-text-dim uppercase">
                  <HarnessGlyph harness={agent.harness} />{tr("studio-chat-agent")}</span>
              </Tooltip>
            )}
            {live && <WorkingDot title={turn.working ? tr("studio-chat-dot-working") : tr("studio-chat-dot-writing")} />}
          </div>
          {turn.thinking && <ThinkingBlock text={thinking} open={open} onToggle={toggle} live={live && !writing} since={turn.since} />}
          {writing && (
            <div className="text-xs text-text">
              <StreamedMarkdown text={text} />
              {live && <StreamCaret />}
            </div>
          )}
          {live && turn.working && (
            <p className="mt-0.5 truncate text-2xs text-text-dim italic" title={turn.working}>{tr("studio-chat-running", { working: turn.working })}</p>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * What an empty conversation is empty *of*. Keyed by scope kind rather than
 * fixed to the speech bubble: that glyph means "direct message" everywhere
 * else in the app, and an empty channel wearing it would teach the reader that
 * a channel is a DM.
 */
const SCOPE_ICON: Record<ScopeKind, LucideIcon> = {
  goal: ICON.goal,
  channel: ICON.channel,
  dm: ICON.dm,
  conversation: ICON.dm,
};


/**
 * Where you had read up to when you opened this.
 *
 * `decorative={false}` on the rules: this is the one line in the timeline
 * that is not there to look tidy — it is the only thing saying that what
 * follows is new — so it should be announced rather than skipped.
 */
function UnreadDivider() {
  return (
    <div className="flex items-center gap-2 px-3 py-1">
      <Separator decorative={false} className="flex-1 bg-accent/40" />
      <span className="text-2xs font-semibold tracking-wide text-accent-ink uppercase">{tr("studio-chat-new")}</span>
      <Separator decorative={false} className="flex-1 bg-accent/40" />
    </div>
  );
}

/** The shape of a conversation that has not arrived, at the height it will be. */
function TimelineSkeleton() {
  return (
    <div className="flex flex-col gap-3 p-3" aria-busy>
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="flex gap-2">
          <Skeleton className="h-6 w-6 shrink-0 rounded-full" />
          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <Skeleton className="h-3 w-28" />
            <Skeleton className={i % 2 === 0 ? "h-3 w-3/4" : "h-3 w-1/2"} />
          </div>
        </div>
      ))}
    </div>
  );
}

/** The glyph for a media type, resolved through the kit's one icon map. */
function fileGlyph(mime: string) {
  const name = glyphFor(mime);
  const map = ICON as Record<string, typeof ICON.file>;
  return map[name] ?? ICON.file;
}

/**
 * One file on a message.
 *
 * Three states, and the third is the interesting one. Present and renderable →
 * the picture. Present and not → a chip that downloads. **Absent** → a chip
 * that offers to fetch it, because a file's bytes do not travel with its
 * message: the descriptor syncs and the bytes come from whoever has them. That
 * is a state, not a failure, and the chip says so rather than showing a broken
 * image with no explanation.
 */
function AttachmentChip({ file }: { file: MessageAttachment }) {
  const toast = useToast();
  const [asking, setAsking] = useState(false);
  const [present, setPresent] = useState(file.present);
  const Glyph = fileGlyph(file.mime);

  const request = async () => {
    setAsking(true);
    try {
      await api.fetchAttachment(file.sha256);
      setPresent(true);
    } catch (e) {
      // A peer that is offline is the ordinary reason, and it is worth saying
      // out loud rather than leaving the chip looking inert.
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setAsking(false);
    }
  };

  if (isRenderableImage(file, present)) {
    return (
      <a
        href={api.attachmentUrl(file.sha256)}
        download={file.name}
        title={`${file.name} · ${describeFile(file, present)}`}
        className="anim block max-w-xs overflow-hidden rounded-control border border-border"
      >
        <img
          src={api.attachmentUrl(file.sha256, { image: true })}
          alt={file.name}
          className="max-h-64 w-auto object-contain"
        />
      </a>
    );
  }

  const body = (
    <>
      <Glyph size={12} aria-hidden className="shrink-0 text-text-dim" />
      <span className="min-w-0 truncate">{file.name}</span>
      <span className="tnum shrink-0 text-text-dim">{describeFile(file, present)}</span>
    </>
  );

  if (!present) {
    return (
      <button
        type="button"
        disabled={asking}
        onClick={() => void request()}
        title={tr("studio-chat-ask-peer", { file: file.name })}
        className="anim inline-flex max-w-72 items-center gap-1.5 rounded-full border border-dashed border-border px-2 py-1 text-2xs text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-50"
      >
        {body}
        <span className="shrink-0 text-accent-ink">{asking ? tr("studio-chat-asking") : tr("studio-chat-request")}</span>
      </button>
    );
  }

  return (
    <a
      href={api.attachmentUrl(file.sha256)}
      download={file.name}
      title={file.name}
      className="anim inline-flex max-w-72 items-center gap-1.5 rounded-full border border-border bg-surface px-2 py-1 text-2xs hover:bg-surface-2"
    >
      {body}
    </a>
  );
}

function Row({
  message,
  reactions,
  me,
  name,
  agent,
  compact,
  onReply,
  onReact,
  onUnreact,
  onRetract,
  onArrived,
  onCopy,
  onCopyLink,
  nameOf,
  photo,
  openArtifact,
  libraries,
  extraActions,
}: {
  message: MessageRow;
  reactions: ReturnType<typeof groupReactions>;
  me: string;
  name: string;
  /** The author's face — a member's row, here or on the host — when they set one. */
  photo: { sha256: string } | null;
  agent?: AgentDef;
  compact: boolean;
  onReply: () => void;
  onReact: (emoji: string) => void;
  onUnreact: (reactionId: string) => void;
  onRetract: () => void;
  /** An artifact's bytes arrived: read the message again where it is drawn. */
  onArrived: (messageId: string) => Promise<void>;
  /** The message's text, as written, to the clipboard. */
  onCopy: () => void;
  onCopyLink: () => void;
  /** Resolves a reaction author's pubkey to a name — the same map the author
   *  line uses, so an agent's mark reads as that agent. */
  nameOf: (pubkey: string) => string;
  /** Open one of the message's artifacts beside the conversation (ide/12). */
  openArtifact: (key: string, expand: boolean) => void;
  /** Whether a page artifact may load libraries from the CDNs. */
  libraries: boolean;
  /** A caller's own actions beside the fixed set — *Restore to before this message* (ide/09), when this message woke a turn. */
  extraActions?: MenuItem[];
}) {
  const [picking, setPicking] = useState(false);
  const mine = message.author === me;

  if (message.retracted) {
    return (
      <div className="border-l-2 border-l-transparent px-3 py-1 text-2xs text-text-dim italic">{tr("studio-chat-retracted-message", { name })}</div>
    );
  }

  // One list, two surfaces. A verb that exists in the ⋮ menu and not in the
  // right-click menu is a verb half the readers never find — so the list is
  // the model's (`messageVerbs`), and this only binds each verb to its
  // handler and glyph.
  //
  // Every verb here has a glyph, which is the condition for using any of
  // them: a menu where two of three items carry a symbol starts its labels in
  // two places, and aligned words beat two-thirds of an icon set.
  const handler = { reply: onReply, copy: onCopy, "copy-link": onCopyLink, retract: onRetract } as const;
  const glyph = { reply: ICON.reply, copy: ICON.copy, link: ICON.link, delete: ICON.delete } as const;
  const items: MenuItem[] = [
    ...messageVerbs({ mine, retracted: message.retracted }).map((id) => {
      const words = verbWords(id);
      return { label: words.label, onSelect: handler[id], icon: glyph[words.icon], danger: words.danger, separatorBefore: words.apart };
    }),
    ...(extraActions ?? []),
  ];

  return (
    <ContextMenu items={items} className="block">
      <div
        className={`group relative border-l-2 px-3 py-1 hover:bg-surface-2/60 ${
          // Your own messages carry an accent rail rather than being pushed to
          // the other side of the pane: alignment is what makes a timeline
          // scannable, and a right-aligned run of your own turns costs that to
          // say something the name already says. The rail is reserved as a
          // transparent border on every row, so it changes colour rather than
          // width and nothing shifts.
          mine ? "border-l-accent/40" : "border-l-transparent"
        }`}
      >
        <div className="flex gap-2">
          <div className="w-6 shrink-0 pt-0.5">
            {!compact && (
              <Avatar id={message.author} name={name} photo={agent?.photo ?? photo} size={24} />
            )}
          </div>
          <div className="min-w-0 flex-1">
            {!compact && (
              <div className="flex items-baseline gap-1.5">
                <span className="text-xs font-semibold">{mine ? tr("studio-chat-you") : name}</span>
                {agent && (
                  // What an agent runs on decides how it answers and what it
                  // costs, and it is the first thing anyone asks when a reply
                  // reads oddly — but it is four facts, and four facts in a
                  // message header is a header nobody reads.
                  <Tooltip label={tr("studio-chat-runs", { agent: agent.name, harness: agent.harness, models: planSummary(agent.models) })}>
                    <span className="inline-flex items-center gap-0.5 rounded-full border border-border px-1 text-3xs leading-tight font-medium tracking-wide text-text-dim uppercase">
                      <HarnessGlyph harness={agent.harness} />{tr("studio-chat-agent")}</span>
                  </Tooltip>
                )}
                <RelativeTime at={message.created_at} className="text-2xs text-text-dim tnum" />
              </div>
            )}
            {message.thinking && <MessageThinking text={message.thinking} />}
            <div className="text-xs text-text">
              {/* A post the platform authored — the Workflow Agent's note, the reply-cut note — is said here in this language from its `said` message; every other post is its content (17, `contentOf`). */}
              <Markdown text={contentOf(message)} className="prose-i" />
            </div>
            {/*
              What the author made to be looked at (ide/12), first: a card
              per artifact, live where it can be. Then the files, between
              what was said and how people reacted to it. Images inline
              because that is the whole point of sending one; everything else
              a chip, because a PDF handed over as a file has nothing to show.
            */}
            {(message.artifacts ?? []).length > 0 && (
              <div className="mt-1.5 flex flex-col gap-1.5">
                {(message.artifacts ?? []).map((a, i) => (
                  <ArtifactCard
                    key={`${a.sha256}:${i}`}
                    artifact={a}
                    present={a.present}
                    libraries={libraries}
                    onOpen={({ expand }) => openArtifact(artifactKey(message.id, i), expand)}
                    onRequest={async () => {
                      // The bytes arrived: the message is read again so the
                      // card draws *present*, as the aux pane does.
                      await api.fetchAttachment(a.sha256);
                      await onArrived(message.id);
                    }}
                  />
                ))}
              </div>
            )}
            {(message.attachments ?? []).length > 0 && (
              <div className="mt-1.5 flex flex-wrap gap-1.5">
                {(message.attachments ?? []).map((a) => (
                  <AttachmentChip key={`${a.sha256}:${a.name}`} file={a} />
                ))}
              </div>
            )}
            <ContextChipRow refs={message.context} />
            {reactions.length > 0 && (
              <div className="mt-1 flex flex-wrap gap-1">
                {reactions.map((r) => {
                  // Who, not how many. An agent marks the message it has picked
                  // up, and that mark's whole content is *which* agent — an
                  // anonymous count says something is happening and not what.
                  const who = r.authors.map(nameOf).join(", ");
                  return (
                    <Tooltip key={r.emoji} label={`${r.emoji} ${who}`}>
                      <button
                        type="button"
                        aria-pressed={!!r.mine}
                        aria-label={`${r.emoji} — ${who}`}
                        onClick={() => (r.mine ? onUnreact(r.mine) : onReact(r.emoji))}
                        className={`anim inline-flex h-5 items-center gap-1 rounded-full border px-1.5 text-2xs ${
                          r.mine
                            ? "border-accent/40 bg-accent-soft text-accent-ink"
                            : "border-border bg-surface hover:bg-surface-2"
                        }`}
                      >
                        <span aria-hidden>{r.emoji}</span>
                        <span aria-hidden className="tnum">
                          {r.count}
                        </span>
                      </button>
                    </Tooltip>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        {/* `row-actions` is opacity-only and reveals on hover *and* focus, so
            the bar is reachable by tab without the timeline reflowing. The
            emoji picker portals out of this subtree, which takes `:hover` and
            `:focus-within` with it — so while it is open the bar is pinned
            visible, or the row it belongs to would fade out from under it. */}
        <div
          style={picking ? { opacity: 1 } : undefined}
          className="row-actions anim absolute top-0.5 right-2 flex items-center gap-0.5 rounded-control border border-border bg-surface px-1 py-0.5 shadow-sm"
        >
          <Popover
            open={picking}
            onOpenChange={setPicking}
            side="top"
            align="end"
            className="p-1"
            trigger={
              // A sticker, not a face: the picker below offers arbitrary
              // emoji, and a smiley would promise only the happy ones.
              <span className="anim flex h-5 w-5 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
                <ICON.react size={13} aria-hidden />
              </span>
            }
            label={tr("studio-chat-react-message")}
          >
            <div className="flex items-center gap-0.5">
              {QUICK_EMOJI.map((e) => (
                <button
                  key={e}
                  type="button"
                  aria-label={tr("studio-chat-react", { e })}
                  className="anim rounded-control px-1.5 py-0.5 text-sm hover:bg-surface-2"
                  onClick={() => {
                    onReact(e);
                    setPicking(false);
                  }}
                >
                  {e}
                </button>
              ))}
            </div>
          </Popover>
          {/* The bar is glyph-only, so every control has to name itself. The
              picker and the ⋮ menu take a `label` from their own component;
              this is a plain button, so the name comes from a `Tooltip` —
              which Radix gives to assistive technology as the accessible
              name, not as a box only a sighted mouse user ever sees. */}
          <Tooltip label={tr("studio-chat-reply")}>
            <button
              type="button"
              aria-label={tr("studio-chat-reply")}
              onClick={onReply}
              className="anim flex h-5 w-5 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
            >
              <ICON.reply size={13} aria-hidden />
            </button>
          </Tooltip>
          <Tooltip label={tr("studio-chat-copy")}>
            <button
              type="button"
              aria-label={tr("studio-chat-copy")}
              onClick={onCopy}
              className="anim flex h-5 w-5 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
            >
              <ICON.copy size={13} aria-hidden />
            </button>
          </Tooltip>
          <Menu
            label={tr("studio-chat-more-actions")}
            items={items}
            trigger={
              <span className="anim flex h-5 w-5 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
                <ICON.more size={13} aria-hidden />
              </span>
            }
          />
        </div>
      </div>
    </ContextMenu>
  );
}

/** What says the person has taken over: any of these ends a restore, as the kit's `useKeptScroll` has it. */
const HANDS = ["wheel", "pointerdown", "keydown", "touchstart"] as const;

/** The message rows a viewport draws, measured from its content's top — what `placeFrom` reads. */
function rowsOf(el: HTMLElement): ThreadRow[] {
  const origin = el.getBoundingClientRect().top - el.scrollTop;
  return Array.from(el.querySelectorAll<HTMLElement>("[data-message]"), (row) => {
    const box = row.getBoundingClientRect();
    return { id: row.dataset.message ?? "", top: box.top - origin, height: box.height };
  });
}

/** Put a viewport on a kept place; nothing moves while the message is not drawn. */
function scrollToPlace(el: HTMLElement, at: ThreadPlace): void {
  const row = rowsOf(el).find((r) => r.id === at.message);
  if (row) el.scrollTop = Math.max(0, row.top + at.offset);
}

/** An artifact opens beside the conversation — the aux pane, by URL — and, on ⌘-click, over it. */
function openArtifact(key: string, expand: boolean): void {
  setSearch({ aux: "artifact", auxId: key, stage: expand ? "1" : null });
}

export function Chat({
  scope,
  kind,
  channel,
  pinned,
  placeholder,
  emptyTitle,
  emptyHint,
  emptyAction,
  context,
  onRemoveContext,
  onClearContext,
  frame,
  files,
  attachItems,
  stop,
  activity,
  changedFiles = null,
  addressee,
  hosted,
  afterMessage,
  restore,
  composerLeading,
  onComposerKeyDownCapture,
}: {
  scope: string;
  kind: ScopeKind;
  /** The channel behind a chat or DM, when there is one — seeds the tray. */
  channel?: ChannelDef | null;
  /** Cards that ride above the timeline: a gate, a question. */
  pinned?: ReactNode;
  placeholder?: string;
  emptyTitle?: string;
  emptyHint?: string;
  emptyAction?: ReactNode;
  /** Chips attached to the next message (ide/09); removable in the tray. */
  context?: readonly ContextRef[];
  onRemoveContext?: (index: number) => void;
  onClearContext?: () => void;
  /** The leading, non-removable chips that state the placement. */
  frame?: FrameChip[];
  /** The root's files under `@`, ways to attach context, and a session to stop (ide/09). */
  files?: ComposerFiles;
  attachItems?: MenuItem[];
  stop?: ComposerStop | null;
  /**
   * What is happening here, in the caller's words. The one Stop is the
   * composer's trailing slot — *Send* becomes *Stop* while a turn works
   * (`stop`, `composerButtonModel`): this line only says.
   *
   * A caller that knows its sessions (the IDE reads the roster) passes this
   * and the *is writing…* line is not drawn: two sources for one fact is how
   * the rail and the timeline came to disagree.
   */
  activity?: { words: string | null } | null;
  /**
   * What the agents changed in this conversation (ide/20): the ledger's
   * view and the verbs on it, drawn as `ChangedFilesBar` above the composer.
   * Git's working tree is never drawn here — the Git panel's alone.
   */
  changedFiles?: ChangedFilesBarProps | null;
  /** Who an unaddressed message reaches, drawn as the address tray's first chip. */
  addressee?: ReactNode;
  /**
   * A scope on a workspace this node is a guest of (14-collaboration): the
   * host's pubkey and its directory. The messages come and go through the
   * host; the people are the host's, never this workspace's; and the agents
   * are the host's to wake — none is offered here.
   */
  hosted?: { host: string; directory: Directory[] } | null;
  /**
   * A caller's own render, after each landed message — a turn's changes
   * card, under the reply that made them (ide/09 §Modes and the change
   * ledger). Given the message id; `null` draws nothing for that message.
   */
  afterMessage?: (id: string) => ReactNode;
  /**
   * *Restore to before this message* (ide/09): offered on the message that
   * woke a turn with something to restore. `isPrompt` names which messages
   * qualify; `onRestore` writes files, so the caller confirms first.
   */
  restore?: { isPrompt: (id: string) => boolean; onRestore: (id: string) => void };
  /** A caller's own control in the composer's foot — a conversation's mode picker (ide/09). */
  composerLeading?: ReactNode;
  /** Capture-phase keydown on the composer's box, before it handles the key itself — the mode picker's `Shift+Tab`. */
  onComposerKeyDownCapture?: (e: ReactKeyboardEvent) => void;
}) {
  const toast = useToast();
  const ws = useWorkspace();
  const libraries = useArtifactLibraries();
  const conv = useScopeMessages(scope, kind, hosted ? { host: hosted.host } : null);
  const attached = context ?? [];
  const overBudget = !fitsBudget([...attached]);
  const local = useParticipants(hosted ? null : channel, kind);
  const hostedMentions = useMemo(() => (hosted ? hostedMentionables(hosted.directory, ws.me) : []), [hosted, ws.me]);
  const { mentionables, agents, addressCandidates, inAudience } = hosted
    ? { mentionables: hostedMentions, agents: [], addressCandidates: [], inAudience: [] }
    : local;
  const nameOf = hosted ? (p: string) => hostedNameOf(hosted.directory, p) : ws.nameOf;
  const photoOf = hosted ? (p: string) => hostedPhotoOf(hosted.directory, p) : ws.photoOf;
  // A remembered addressee a conversation of this kind cannot reach — the
  // Workflow Agent on a checkout — is neither drawn nor sent.
  const unreachable = useMemo(() => ws.agents.filter((a) => !reachableIn(a, kind)).map((a) => a.pubkey), [ws.agents, kind]);
  const addressed = useAddressed(scope, inAudience, unreachable);
  const [replyTo, setReplyTo] = useState<MessageRow | null>(null);
  /** The chat's root: where the person's first wheel, click or key is heard. */
  const root = useRef<HTMLDivElement>(null);
  const viewport = useRef<HTMLDivElement>(null);
  /** The timeline's content inside the viewport — what grows as a reply streams. */
  const content = useRef<HTMLDivElement>(null);
  /** The cards pinned above the timeline: an ask that took the focus wins over a kept place. */
  const pinnedBox = useRef<HTMLDivElement>(null);
  // Where this thread was being read, under the chat's own key — one place
  // whichever screen shows the thread. Read once, here; kept on scroll.
  const place = useThreadPlace(chatKey(kind, scope));
  /** The place being put back; `null` once the restore has ended — reached and left alone, given up, or taken over by the person. */
  const restoring = useRef<ThreadPlace | null>(place.kept);
  /** How many older pages the restore has read to reach the kept message, and the page it last asked from — one ask a page. */
  const pagesRead = useRef(0);
  const askedFrom = useRef<readonly MessageRow[] | null>(null);
  /** The restore's grace, and the frame a scroll waits on to be kept. */
  const grace = useRef(0);
  const keeping = useRef(0);
  /** Whether the reader stands at the thread's bottom — following the newest, and seeing it. A thread with a place kept does not start there. */
  const [atBottom, setAtBottom] = useState(() => place.kept === null);
  /** Scroll height captured just before a page of older messages lands. */
  const beforePrepend = useRef<number | null>(null);

  /**
   * How much was unread when this conversation was opened.
   *
   * Captured on entry because the thread reads itself a beat after it is
   * shown: reading the live count instead would put the marker at zero
   * before the messages it points at were read. A workspace store that has
   * not loaded yet reports nothing, so the marker is simply absent that once
   * — a missing divider is a smaller lie than one drawn in the wrong place.
   */
  const entryUnread = useRef<{ scope: string; count: number }>({ scope: "", count: 0 });
  if (entryUnread.current.scope !== scope) {
    entryUnread.current = { scope, count: ws.unread[scope] ?? 0 };
  }
  // The thread reads itself as it is shown: the newest message on screen — the
  // window visible, the reader at the bottom, the first page landed — for a
  // beat marks the scope read, here and in every surface that mounts this.
  const visible = useVisible();
  useReadAsShown({
    scope,
    host: hosted?.host ?? null,
    unreadCount: hosted ? hostedUnread(ws.hosted, hosted.host, scope) : (ws.unread[scope] ?? 0),
    shown: shownToReader({ visible, atBottom, loaded: !conv.loading }),
  });

  const grouped = useMemo(() => threads(conv.messages), [conv.messages]);
  const workingHere = ws.working[scope] ?? [];
  // The agents with a turn in flight here — words, thinking or a tool to
  // show. An agent working with nothing said yet is the footer's line, not
  // a row. A stable array: a frame moves one row, never this list.
  const liveAgents = useLiveTurnAgents(scope);
  const liveSet = useMemo(() => new Set(liveAgents), [liveAgents]);
  const workingNames = workingHere
    .filter((id) => !liveSet.has(id))
    .map((id) => ws.agents.find((a) => a.id === id)?.name ?? id)
    .filter(Boolean);
  // A reader that joins mid-turn is given the node's whole turn once; the
  // frames from here on append to it. Only a conversation has the route.
  useEffect(() => {
    if (kind !== "conversation" || hosted) return;
    const ctl = new AbortController();
    void primeLiveTurns(scope, ctl.signal);
    return () => ctl.abort();
  }, [scope, kind, hosted]);
  const thinkingMode = useThinkingMode();
  /** Whether there is any thinking to show or hide — the control stands only where it means something. */
  const anyThinking = liveAgents.length > 0 || conv.messages.some((m) => m.thinking);
  // A settled live row gives way the moment its message is in the page —
  // the same words, drawn once, never a blink between the two.
  useEffect(() => {
    retireLandedTurns(conv.messages.map((m) => m.id));
  }, [conv.messages]);

  /**
   * The first message you had not seen, as an id rather than an index, so a
   * refetch that lengthens the list leaves the marker on the same message.
   */
  const firstUnreadId = useMemo(() => firstUnreadOf(conv.messages, entryUnread.current.count), [conv.messages]);

  useEffect(() => setReplyTo(null), [scope]);

  // A page of older messages lands above the reader. Adding back exactly the
  // height it took keeps the message they were looking at under their eye —
  // without this, "Load older" throws you a screen and a half backwards.
  useLayoutEffect(() => {
    const el = viewport.current;
    if (!el || beforePrepend.current === null) return;
    el.scrollTop += el.scrollHeight - beforePrepend.current;
    beforePrepend.current = null;
  }, [conv.messages.length]);

  /** The restore is over: where it left the thread is where the reader stands. */
  const endRestore = useCallback(() => {
    if (!restoring.current) return;
    restoring.current = null;
    window.clearTimeout(grace.current);
    grace.current = 0;
    const el = viewport.current;
    if (el) setAtBottom(atBottomOf(el.scrollHeight, el.scrollTop, el.clientHeight));
  }, []);

  // A restore must never fight the hand on the wheel: the person's first
  // wheel, click or key anywhere in the chat ends it, as the grace does.
  useEffect(() => {
    const el = root.current;
    if (!el || !restoring.current) return;
    for (const hand of HANDS) el.addEventListener(hand, endRestore, true);
    return () => {
      for (const hand of HANDS) el.removeEventListener(hand, endRestore, true);
      window.clearTimeout(grace.current);
      grace.current = 0;
    };
  }, [endRestore]);

  // Nothing is kept on unmount — a detached viewport reads zero, which would
  // erase the place it meant to keep: the scroll still waiting for its frame
  // is dropped, and the place stands as the last frame kept it.
  useEffect(
    () => () => {
      cancelAnimationFrame(keeping.current);
      keeping.current = 0;
    },
    [],
  );

  // The thread is put back where it was being read (`restoreStep`), asked
  // again whenever the page changes: the first page may still be out, and
  // the message may be above it.
  const loadOlder = conv.loadOlder;
  useLayoutEffect(() => {
    const kept = restoring.current;
    const el = viewport.current;
    if (!kept || !el) return;
    const ids = conv.messages.map((m) => m.id);
    // A read that failed is no answer about a message it did not bring: the place waits for the retry.
    const out = conv.loading || (conv.error !== null && !ids.includes(kept.message));
    const step = restoreStep({ kept, loading: out, ids, hasOlder: conv.hasOlder, pagesLoaded: pagesRead.current });
    if (step.do === "wait") return;
    if (step.do === "scroll") {
      // An ask that took the focus is where the person is wanted: it wins,
      // and the place stays kept for the next visit.
      if (pinnedBox.current?.contains(document.activeElement)) {
        endRestore();
        return;
      }
      scrollToPlace(el, kept);
      // What is above may still grow — a picture, a card: the place is put
      // back as it does (the content's observer, below) until the grace ends.
      if (!grace.current) grace.current = window.setTimeout(endRestore, RESTORE_GRACE_MS);
      return;
    }
    if (step.do === "older") {
      if (conv.loadingOlder || askedFrom.current === conv.messages) return;
      askedFrom.current = conv.messages;
      pagesRead.current += 1;
      beforePrepend.current = el.scrollHeight;
      void loadOlder();
      return;
    }
    // It cannot be reached: the thread opens at its bottom, as one never read does.
    if (step.forget) place.forget();
    restoring.current = null;
    setAtBottom(true);
  }, [conv.messages, conv.loading, conv.error, conv.hasOlder, conv.loadingOlder, loadOlder, place, endRestore]);

  // Follow the conversation unless the reader has scrolled up to read back.
  //
  // The viewport's own `scrollTop`, never `scrollIntoView`: that scrolls
  // *every* scrollable ancestor, and in the IDE the right panel is one — so
  // following a reply used to drag the whole column. The footer's height is a
  // dependency because a line appearing there shortens the viewport.
  useEffect(() => {
    const el = viewport.current;
    if (el && atBottom) el.scrollTop = el.scrollHeight;
  }, [atBottom, conv.messages.length, workingNames.length, activity?.words, changedFiles?.view.pending]);
  // A reply growing at the foot is followed by its size, not by a render:
  // the content's `ResizeObserver` fires as the live row grows, so the
  // timeline itself never re-renders for a frame it does not draw.
  const following = useRef(atBottom);
  following.current = atBottom;
  useEffect(() => {
    const el = viewport.current;
    const inner = content.current;
    if (!el || !inner || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      // While a kept place is being put back, what grows above it must not carry it away.
      if (restoring.current) scrollToPlace(el, restoring.current);
      else if (following.current) el.scrollTop = el.scrollHeight;
    });
    ro.observe(inner);
    return () => ro.disconnect();
  }, []);

  const guard = (p: Promise<unknown>, what: string) => {
    void p.catch((e: unknown) => toast.error(e instanceof Error ? e.message : tr("studio-chat-could-not", { what })));
  };

  const copyLink = (id: string) => {
    const url = `${window.location.origin}${window.location.pathname}#${window.location.hash.replace(/^#/, "").split("?")[0]}?item=${id}`;
    void copyText(url).then((ok) => (ok ? toast.ok(tr("studio-chat-link-copied")) : toast.error(tr("studio-chat-clipboard-refused"))));
  };
  /** The message's text as its author wrote it — the Markdown source, nothing around it. */
  const copyMessage = (m: MessageRow) => {
    void copyText(m.content).then((ok) => (ok ? toast.ok(tr("studio-chat-message-copied")) : toast.error(tr("studio-chat-clipboard-refused"))));
  };

  /** *Jump to newest*: down to the thread's bottom, where it follows the newest again — and keeps no place. */
  const jumpToNewest = () => {
    const el = viewport.current;
    if (!el) return;
    endRestore();
    el.scrollTop = el.scrollHeight;
    setAtBottom(true);
    place.keep(null);
  };

  /** The message a *Restore to before this message* confirmation is pending on. */
  const [restoreConfirm, setRestoreConfirm] = useState<string | null>(null);

  const renderMessage = (m: MessageRow, previous: MessageRow | undefined, indented: boolean) => {
    const compact = isCompact(m, previous, firstUnreadId);
    const canRestore = !!restore && restore.isPrompt(m.id);
    const extraActions: MenuItem[] | undefined = canRestore
      ? [{ label: tr("studio-chat-restore-before-message"), icon: ICON.history, danger: true, separatorBefore: true, onSelect: () => setRestoreConfirm(m.id) }]
      : undefined;
    return (
      <div key={m.id} data-message={m.id} className={indented ? "ml-8 border-l border-border pl-2" : undefined}>
        <Row
          message={m}
          reactions={groupReactions(conv.reactions, m.id, ws.me)}
          me={ws.me}
          name={nameOf(m.author)}
          nameOf={nameOf}
          photo={photoOf(m.author)}
          agent={hosted ? undefined : ws.agentByPubkey(m.author)}
          compact={compact && !indented}
          onReply={() => setReplyTo(m)}
          onReact={(emoji) => guard(conv.react(m.id, emoji), "react")}
          onUnreact={(id) => guard(conv.unreact(id), tr("studio-chat-remove-reaction"))}
          onRetract={() => guard(conv.retract(m.id), "retract")}
          onArrived={conv.refreshMessage}
          onCopy={() => copyMessage(m)}
          onCopyLink={() => copyLink(m.id)}
          openArtifact={openArtifact}
          libraries={libraries}
          extraActions={extraActions}
        />
        {afterMessage?.(m.id)}
      </div>
    );
  };

  return (
    <div ref={root} className="flex h-full min-h-0 flex-col">
      <div
        ref={viewport}
        onScroll={(e) => {
          // A scroll the restore made is not the reader's: nothing is read from it, and nothing kept.
          if (restoring.current) return;
          const el = e.currentTarget;
          const next = atBottomOf(el.scrollHeight, el.scrollTop, el.clientHeight);
          setAtBottom((v) => (v === next ? v : next));
          // Where the thread is being read, one write a frame: the message
          // under the top edge and the offset inside it; at the bottom, nothing.
          if (keeping.current) return;
          keeping.current = requestAnimationFrame(() => {
            keeping.current = 0;
            if (!el.isConnected || restoring.current) return;
            place.keep(atBottomOf(el.scrollHeight, el.scrollTop, el.clientHeight) ? null : placeFrom(rowsOf(el), el.scrollTop));
          });
        }}
        className="min-h-0 flex-1 overflow-y-auto py-2"
      >
        <div ref={content}>
        {pinned && <div ref={pinnedBox} className="flex flex-col gap-2 px-3 pb-2">{pinned}</div>}

        {conv.error && (
          <div className="px-3 pb-2">
            <ErrorNote error={conv.error} retry={() => void conv.reload()} />
          </div>
        )}

        {conv.loading ? (
          <TimelineSkeleton />
        ) : conv.messages.length === 0 ? (
          <div className="p-4">
            <EmptyState
              icon={SCOPE_ICON[kind]}
              title={emptyTitle ?? tr("studio-chat-no-messages-yet")}
              hint={
                emptyHint ??
                (agents.length
                  ? tr("studio-chat-say-something-address-agent-below-will")
                  : kind === "channel"
                    ? // Nothing to pick is not the same as nobody to ask. Since
                      // M13 an unaddressed message in a standing channel reaches
                      // the core agent, so the honest instruction here is to
                      // send it rather than to go and install an agent first.
                      tr("studio-chat-say-something-message-addresses-nobody-reaches")
                    : tr("studio-chat-say-something-where-work-gets-talked"))
              }
              action={emptyAction ?? null}
            />
          </div>
        ) : (
          <>
            {conv.hasOlder && (
              <div className="px-3 pb-2">
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={conv.loadingOlder}
                  onClick={() => {
                    beforePrepend.current = viewport.current?.scrollHeight ?? null;
                    void conv.loadOlder();
                  }}
                >
                  {conv.loadingOlder ? tr("studio-chat-loading") : tr("studio-chat-load-older")}
                </Button>
              </div>
            )}
            {/* Flattened rather than one wrapper per thread: a sticky day
                divider only sticks within its own scrolling ancestor's
                children, so a wrapper would pin it to that thread's few
                pixels instead of to the day it names. */}
            {grouped.flatMap((t, i) => {
              const previousRoot = grouped[i - 1]?.root;
              const newDay =
                !previousRoot || dayKey(previousRoot.created_at) !== dayKey(t.root.created_at);
              const nodes: ReactNode[] = [];
              if (newDay) nodes.push(<DayDivider key={`day:${t.root.id}`} at={t.root.created_at} sticky />);
              if (t.root.id === firstUnreadId) {
                nodes.push(<UnreadDivider key={`unread:${t.root.id}`} />);
              }
              nodes.push(renderMessage(t.root, newDay ? undefined : previousRoot, false));
              for (let ri = 0; ri < t.replies.length; ri++) {
                nodes.push(renderMessage(t.replies[ri]!, t.replies[ri - 1], true));
              }
              return nodes;
            })}
          </>
        )}
        {/* Where a card whose reply and prompt are both off this page draws,
            and where the plan banner draws (ide/09). */}
        {!conv.loading && conv.messages.length > 0 && afterMessage?.(TIMELINE_END)}
        {/* The turns in flight, after everything that landed: the reply as it
            is written, in the place its message will take. */}
        {liveAgents.map((id) => {
          const def = hosted ? undefined : ws.agents.find((a) => a.id === id);
          return <LiveTurnRow key={`live:${id}`} scope={scope} agentId={id} agent={def} name={def?.name ?? id} />;
        })}
        </div>
      </div>

      <div className="relative border-t border-border p-2">
        {/* Away from the bottom with the thread below: the door back down. It
            floats over the timeline's foot, so nothing reflows when it shows. */}
        {!atBottom && !conv.loading && conv.messages.length > 0 && (
          <div className="pointer-events-none absolute inset-x-0 -top-9 flex justify-center">
            <Button size="sm" className="pointer-events-auto rounded-full shadow-sm" onClick={jumpToNewest}>
              <ICON.down size={12} aria-hidden />
              {tr("studio-chat-jump-newest")}
            </Button>
          </div>
        )}
        {/* What is happening here, and the thinking toggle at its right. The
            caller's line when it has one — the IDE reads the roster, which
            knows the tool and the sub-agents — else the conversation stream's
            own tr("studio-chat-writing") for an agent whose words have not begun (one
            that has begun is a live row above). The thinking control stands
            only where there is thinking to show or hide. */}
        {(activity ? !!activity.words : workingNames.length > 0) || anyThinking ? (
          <div role="status" className="mb-1 flex items-center gap-1.5 px-1 text-2xs text-text-dim">
            {activity ? (
              activity.words && (
                <>
                  <WorkingDot />
                  <span className="min-w-0 flex-1 truncate">{activity.words}</span>
                </>
              )
            ) : (
              workingNames.length > 0 && (
                // `WorkingDot` rather than a hand-rolled ping: it is the same mark
                // the sidebar and the top chrome use for the same fact, and it drops
                // to a static dot under reduced motion, which the local copy did not.
                <>
                  <WorkingDot />
                  <span className="min-w-0 flex-1 truncate">{tr("studio-chat-writing-3", { names: workingNames.join(", "), n: workingNames.length })}</span>
                </>
              )
            )}
            {anyThinking && (
              <span className="ml-auto shrink-0">
                <ThinkingPicker mode={thinkingMode} onChange={setThinkingMode} />
              </span>
            )}
          </div>
        ) : null}

        {/* What the agents changed in this conversation — the ledger's bar (ide/20); nothing of git's tree is here. */}
        {changedFiles && <ChangedFilesBar {...changedFiles} />}

        {replyTo && (
          <div className="mb-1 flex items-center gap-2 rounded-control bg-surface-2 px-2 py-1 text-2xs text-text-dim">
            <span className="truncate">{tr("studio-chat-replying-to", { name: nameOf(replyTo.author), words: contentOf(replyTo).slice(0, 60) })}</span>
            <Tooltip label={tr("studio-chat-stop-replying")}>
              <button
                type="button"
                aria-label={tr("studio-chat-stop-replying")}
                className="anim ml-auto shrink-0 hover:text-text"
                onClick={() => setReplyTo(null)}
              >
                <ICON.close size={12} aria-hidden />
              </button>
            </Tooltip>
          </div>
        )}

        {/* Rendered whenever there is something to say — an offer to address
            somebody, or somebody already addressed. Gating it on the offer
            alone is how a remembered addressee ended up on the wire with no
            chip anywhere on screen. */}
        <AddressTray
          addressed={addressed}
          candidates={addressCandidates}
          known={ws.agents}
          working={workingHere}
          lead={addressee}
        />

        <ContextTray
          frame={frame ?? []}
          refs={attached}
          onRemove={(i) => onRemoveContext?.(i)}
          onClear={() => onClearContext?.()}
          overBudget={overBudget}
        />

        <div onKeyDownCapture={onComposerKeyDownCapture}>
          <Composer
            scope={scope}
            mentionables={mentionables}
            placeholder={placeholder}
            files={files}
            attachItems={attachItems}
            stop={stop}
            leading={composerLeading}
            // Over the budget only the send closes: the reader removes a chip
            // rather than losing the sentence they were on.
            sendDisabled={overBudget}
            onSend={async (text, mentions, attachments, artifacts) => {
              try {
                // Everyone addressed in the tray is mentioned, whether or not
                // the text names them — that is what the tray promises.
                const all = [...new Set([...mentions, ...addressed.pubkeys])];
                await conv.post(text, all, attachments, replyTo?.id, [...attached], artifacts);
                onClearContext?.();
                setReplyTo(null);
              } catch (e) {
                toast.error(e instanceof Error ? e.message : tr("studio-chat-could-not-send"));
                throw e;
              }
            }}
          />
        </div>
        {restore && (
          <ConfirmDialog
            open={restoreConfirm !== null}
            onClose={() => setRestoreConfirm(null)}
            onConfirm={() => {
              const id = restoreConfirm;
              setRestoreConfirm(null);
              if (id) restore.onRestore(id);
            }}
            title={tr("studio-chat-restore-before-message")}
            body={tr("studio-chat-every-file-turn-any-later-turn")}
            confirmLabel={tr("studio-chat-restore")}
            danger
          />
        )}
      </div>
    </div>
  );
}
