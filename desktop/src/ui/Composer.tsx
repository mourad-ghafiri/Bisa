/**
 * The message composer: Enter (or ⌘Enter) sends, Shift+Enter breaks a line,
 * `@` opens a picker over members, agents, the channel itself — and, where
 * the caller has files to offer, the root's paths — and an unsent draft
 * survives a navigation away (per scope, in localStorage): losing a
 * half-typed message because you clicked a goal is the kind of small betrayal
 * that stops people using a tool.
 *
 * # `@` for files
 *
 * A picked person becomes a **mention** the wire resolves. A picked file
 * becomes words in the text and a **context chip** through `files.onMention`
 * — never a mention, which the store would refuse. `fileMentionModel.mjs`
 * decides when the run reads as a path and keeps the chips in step with the
 * text: edit the token out and `files.onUnmention` fires.
 *
 * # What is disabled when
 *
 * `disabled` closes the whole box. `sendDisabled` closes only the send — the
 * caller's context is over its budget, say — so the reader can keep typing
 * and remove a chip rather than lose the sentence they were on.
 */

import { forgetPrefsUnder, readPref, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import type { ArtifactRef, AttachmentRef } from "../types";
import { artifactFromFile } from "./artifact/artifactModel.mjs";
import { Button } from "./Button";
import { cn } from "./cn";
import { SEND_HANDOVER_GRACE_MS, composerButton, explainOff, handoverElapsed } from "./composerButtonModel.mjs";
import { ICON } from "./icons";
import { Menu, type MenuItem } from "./Menu";
import { PendingFiles } from "./PendingFiles";
import { Tooltip } from "./Tooltip";
import { useDockOverlap } from "./dockClearance";
import { useUploads, type PendingFile } from "./useUploads";
import { usePastedImages } from "./usePastedImages";
import { AgentRow, type AgentCandidate } from "./AgentPicker";
import { clampCursor, cursorAction } from "./agentPickerModel.mjs";
import { fileSuggestions, insertFileMention, syncFileMentions, wantsFiles } from "./fileMentionModel.mjs";
import { insertMention as spliceMention, mentionQuery, mentionsIn, suggestions, type MentionSpan } from "./mentionModel.mjs";
import { t as tr } from "../i18n/l10n.mjs";

/** A held file the composer marked as shared to be looked at (an artifact, ide/12) rather than handed over. */
type ComposerFile = PendingFile & { asArtifact?: boolean };

/**
 * One entry in the composer's directory.
 *
 * It is an {@link AgentCandidate} so the `@`-picker draws the same row as
 * every other agent list in the studio — description, tags, harness, and a
 * mark when the entry cannot answer. A dropdown that showed only names was
 * the one agent list you could not read a role off, and it was the only one
 * you could address from.
 */
export interface Mentionable extends AgentCandidate {
  /**
   * Offer this one in the `@`-picker. Absent means yes.
   *
   * A hidden entry is still in this list, and deliberately: the list is also
   * what turns a typed name into an outgoing mention, so dropping one to stop
   * suggesting it would make typing that name produce no mention at all — a
   * message that posts, wakes nobody, and never says so. See
   * `mentionModel.mjs`.
   */
  suggest?: boolean;
}

/** The files a composer can offer under `@`, and what a pick means. */
export interface ComposerFiles {
  /** Every path under the root the conversation works in; null until indexed. */
  paths: readonly string[] | null;
  /** A path was picked: attach it. */
  onMention: (path: string) => void;
  /** Its token was edited out of the text: let it go. */
  onUnmention: (path: string) => void;
}

/** A session the reader can stop from the box. */
export interface ComposerStop {
  onStop: () => void;
  label?: string;
}

/** What every message being written is kept under, a key a scope. */
const DRAFT_PREFIX = "bisa:draft:";
const draftKey = (scope: string) => `${DRAFT_PREFIX}${scope}`;

/**
 * Discard every message being written, in every conversation — *Forget
 * where I was*, when its one checkbox says so. `spared` names what else is
 * parked under the same prefix and is no message: a note's unsaved text. A
 * composer that is open keeps the words it shows until the window opens
 * again.
 */
export function forgetEveryDraft(spared: readonly string[]): number {
  return forgetPrefsUnder(webStorage(), DRAFT_PREFIX, spared);
}

/** The event that hands a composer a drafted message: `{scope, text}`. */
export const COMPOSER_DRAFT = "bisa:composer-draft";

function basename(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? path : path.slice(at + 1);
}

/**
 * The mounted composers, by scope, so a door elsewhere on the screen can put
 * the caret in one — the Agents chord while the centre is the conversation
 * (ide/09). A registry rather than a prop: the composer sits three
 * components down from the surface that owns the door.
 */
const mounted = new Map<string, () => void>();

/** Put the caret in the composer of a scope, when one is on screen. */
export function focusComposer(scope: string): boolean {
  const focus = mounted.get(scope);
  if (!focus) return false;
  focus();
  return true;
}

export function Composer({
  scope,
  onSend,
  mentionables = [],
  files,
  attachItems,
  stop = null,
  placeholder = tr("ui-composer-write-message"),
  disabled = false,
  sendDisabled = false,
  leading = null,
}: {
  scope: string;
  /** What was typed, whom it addresses, the files handed over, and the files shared to be looked at (ide/12). */
  onSend: (text: string, mentions: string[], attachments: AttachmentRef[], artifacts: ArtifactRef[]) => Promise<void> | void;
  mentionables?: Mentionable[];
  /** Offer the root's files under `@`. */
  files?: ComposerFiles;
  /** Ways to attach context, on the attach menu above the OS file picker. */
  attachItems?: MenuItem[];
  /** A live session: the trailing slot is *Stop* instead of *Send* (`composerButtonModel`). */
  stop?: ComposerStop | null;
  placeholder?: string;
  disabled?: boolean;
  /** Only sending is off; typing stays. */
  sendDisabled?: boolean;
  /**
   * A caller's own control, drawn in the foot before the attach button —
   * a conversation's mode picker, say. The kit knows nothing about what it
   * is; it only reserves the seat.
   */
  leading?: ReactNode;
}) {
  const listId = useId();
  const [text, setText] = useState("");
  /**
   * The handover: true from the press of *Send* until the roster names a
   * stoppable session (`stop`) or the grace lapses (`SEND_HANDOVER_GRACE_MS`)
   * — the slot stays Stop-shaped meanwhile, so it never flickers back to
   * *Send* between the POST and the first roster frame.
   */
  const [handover, setHandover] = useState<{ posting: true } | { settledAt: number } | null>(null);
  const busy = handover !== null;
  useEffect(() => {
    if (!handover) return;
    if (stop) {
      setHandover(null);
      return;
    }
    if (!("settledAt" in handover)) return;
    const wait = Math.max(0, SEND_HANDOVER_GRACE_MS - (Date.now() - handover.settledAt));
    const timer = window.setTimeout(() => {
      if (handoverElapsed(handover.settledAt, Date.now())) setHandover(null);
    }, wait);
    return () => window.clearTimeout(timer);
  }, [handover, stop]);
  /** Where the open `@`-run sits in the body, or null when there is none. */
  const [span, setSpan] = useState<MentionSpan | null>(null);
  /** The files picked into this text, so an edited-out token can drop its chip. */
  const [picked, setPicked] = useState<string[]>([]);
  /**
   * Files chosen for this message, uploaded as they are chosen
   * (`useUploads`). Not in the draft: text survives a reload and pending
   * files do not — and the strip says so rather than implying otherwise.
   */
  const uploads = useUploads();
  const pending = uploads.pending as ComposerFile[];
  const fileInput = useRef<HTMLInputElement>(null);
  const [cursor, setCursor] = useState(0);
  const ref = useRef<HTMLTextAreaElement>(null);
  /** Where to put the caret after a splice, applied once the value is painted. */
  const pendingCaret = useRef<number | null>(null);

  useEffect(() => {
    mounted.set(scope, () => ref.current?.focus());
    return () => {
      mounted.delete(scope);
    };
  }, [scope]);

  // Load this scope's draft when the scope changes.
  useEffect(() => {
    setText(readPref(webStorage(), draftKey(scope), (raw) => raw, ""));
    setSpan(null);
    setPicked([]);
  }, [scope]);

  useEffect(() => {
    // The draft is a convenience, not state: an empty one is forgotten, and
    // a storage that refuses costs nothing (`writePref` never throws).
    writePref(webStorage(), draftKey(scope), text || null);
  }, [scope, text]);

  // A draft handed over from elsewhere — a conflict's question for an agent
  // — lands in the box of the scope it names, under what is already typed.
  useEffect(() => {
    const onDraft = (e: Event) => {
      const d = (e as CustomEvent<{ scope: string; text: string }>).detail;
      if (!d || d.scope !== scope || !d.text) return;
      setText((t) => (t.trim() ? `${t.replace(/\s+$/, "")}\n\n${d.text}` : d.text));
      ref.current?.focus();
    };
    window.addEventListener(COMPOSER_DRAFT, onDraft);
    return () => window.removeEventListener(COMPOSER_DRAFT, onDraft);
  }, [scope]);

  // React owns the value, so the caret has to be restored after the render
  // that carries the spliced text — setting it inside the handler would put it
  // where the *old* value ended and drop the reader at the end of the line.
  useEffect(() => {
    const at = pendingCaret.current;
    if (at === null) return;
    pendingCaret.current = null;
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(at, at);
  }, [text]);

  // What the picker offers: the suggestable entries only, ranked by the same
  // predicate every other agent list in the app uses.
  const matches = useMemo(() => (span === null ? [] : suggestions(mentionables, span.query)), [mentionables, span]);
  // …and, under them, the files the run reads as a path.
  const fileMatches = useMemo(
    () => (span !== null && files?.paths && wantsFiles(span.query, matches.length) ? fileSuggestions(span.query, files.paths) : []),
    [files?.paths, matches.length, span],
  );

  // What the message addresses: every known entry, hidden ones included. A
  // name a reader typed has to reach whoever it names whether or not the
  // picker would have offered it.
  const mentions = useMemo(() => mentionsIn(mentionables, text), [mentionables, text]);

  const total = matches.length + fileMatches.length;
  const open = span !== null && total > 0;
  const at = clampCursor(cursor, total);
  const rowId = (i: number) => `${listId}-mention-${i}`;

  /** Recompute the `@`-run from the caret after any edit or caret move. */
  const track = (el: HTMLTextAreaElement) => {
    const caret = el.selectionStart ?? el.value.length;
    setSpan(mentionQuery(el.value, caret, mentionables));
  };

  const insert = (m: Mentionable) => {
    if (span === null) return;
    const next = spliceMention(text, span, m.name);
    setText(next.text);
    pendingCaret.current = next.caret;
    setSpan(null);
    setCursor(0);
  };

  const insertFile = (path: string) => {
    if (span === null || !files) return;
    const next = insertFileMention(text, span, path);
    setText(next.text);
    pendingCaret.current = next.caret;
    setSpan(null);
    setCursor(0);
    if (!picked.includes(path)) setPicked([...picked, path]);
    files.onMention(path);
  };

  /** The row at a combined picker index: a person first, then a file. */
  const pick = (i: number) => {
    if (i < matches.length) {
      const m = matches[i];
      if (m) insert(m);
    } else {
      const p = fileMatches[i - matches.length];
      if (p) insertFile(p);
    }
  };

  /** The text changed by typing or pasting: keep the file chips in step. */
  const edit = (el: HTMLTextAreaElement) => {
    const next = el.value;
    setText(next);
    track(el);
    setCursor(0);
    if (files && picked.length > 0) {
      const { kept, dropped } = syncFileMentions(next, picked);
      if (dropped.length > 0) {
        for (const p of dropped) files.onUnmention(p);
        setPicked(kept);
      }
    }
  };

  const take = uploads.take;
  // A pasted picture is named before it is taken; a pasted file is taken as it is.
  const pasted = usePastedImages(take);
  const uploaded = pending.filter((p) => p.file && !p.asArtifact).map((p) => p.file as AttachmentRef);
  const shared = pending.filter((p) => p.file && p.asArtifact).map((p) => artifactFromFile(p.file as AttachmentRef));
  const uploading = uploads.uploading;
  // A picture with no caption is a message. Text-only was the old rule and it
  // is the reason an attachment-only message could not be sent at all.
  const sendable = (text.trim().length > 0 || uploaded.length > 0 || shared.length > 0) && !uploading && !sendDisabled;

  const send = async () => {
    if (!sendable || busy || disabled) return;
    setHandover({ posting: true });
    try {
      await onSend(text.trim(), mentions, uploaded, shared);
      setText("");
      uploads.clear();
      setSpan(null);
      setPicked([]);
      // Posted: the effect above hands the slot to *Stop*, or back to *Send* when the grace lapses.
      setHandover({ settledAt: Date.now() });
    } catch (e) {
      setHandover(null);
      throw e;
    }
  };
  // The model's word for the slot, with the reason a *Send* that is off is off.
  const button = explainOff(composerButton({ busy, stop, sendable, disabled }), sendDisabled ? "over-budget" : uploading ? "uploading" : "empty");

  // The one attach door: the caller's ways of attaching context, then files
  // from disk. With nothing but disk, the paperclip is a plain button.
  const attachMenu: MenuItem[] = attachItems
    ? [...attachItems, { label: tr("ui-composer-files-from-disk"), icon: ICON.attach, separatorBefore: attachItems.length > 0, immediate: true, onSelect: () => fileInput.current?.click() }]
    : [];
  const attachButton = "anim h-7 shrink-0 rounded-control px-1.5 text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-40";
  const root = useRef<HTMLDivElement>(null);
  const dockRoom = useDockOverlap(root);

  return (
    // The room a floating dock over this row takes (`dockClearance.ts`), so Send is never under it.
    <div ref={root} className="relative" style={dockRoom > 0 ? { marginRight: dockRoom } : undefined}>
      {open && (
        <div id={listId} role="listbox" aria-label={tr("ui-composer-mention")} className="absolute bottom-full left-0 z-30 mb-1 max-h-72 w-80 max-w-full overflow-y-auto rounded-control border border-border bg-surface p-1 shadow-lg">
          {matches.length > 0 && fileMatches.length > 0 && <div className="px-2 pb-1 pt-1.5 text-2xs font-semibold text-text-dim">{tr("ui-composer-people-agents")}</div>}
          {matches.map((m, i) => (
            <AgentRow key={m.id} id={rowId(i)} candidate={m} compact active={i === at} onHover={() => setCursor(i)} onSelect={() => insert(m)} />
          ))}
          {fileMatches.length > 0 && <div className="px-2 pb-1 pt-1.5 text-2xs font-semibold text-text-dim">{tr("ui-composer-files-attached-context")}</div>}
          {fileMatches.map((p, j) => {
            const i = matches.length + j;
            return (
              <button
                key={p}
                id={rowId(i)}
                type="button"
                role="option"
                aria-selected={i === at}
                tabIndex={-1}
                onMouseEnter={() => setCursor(i)}
                // The dropdown sits over a textarea that must keep its caret.
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => insertFile(p)}
                className={cn("anim flex w-full items-center gap-2 rounded-control px-2 py-1 text-left text-2xs", i === at ? "bg-selected text-text" : "text-text hover:bg-surface-2")}
              >
                <ICON.file size={11} aria-hidden className="shrink-0 text-text-dim" />
                <span className="shrink-0 font-mono">{basename(p)}</span>
                <span className="min-w-0 flex-1 truncate font-mono text-text-dim">{p}</span>
              </button>
            );
          })}
        </div>
      )}
      {/*
        Chosen files, above the box; each row its own state. A ready row
        carries the one choice that is the composer's: handed over as a
        file, or shared to be looked at (an artifact, ide/12).
      */}
      {pasted.dialog}
      <PendingFiles
        files={pending}
        onRemove={uploads.remove}
        className="mb-1"
        tone={(f) => ((f as ComposerFile).asArtifact ? "accent" : "plain")}
        trailing={(f) => {
          const asArtifact = !!(f as ComposerFile).asArtifact;
          return (
            <Tooltip label={asArtifact ? tr("ui-composer-shared-looked-rendered-where-read-click") : tr("ui-composer-handed-over-file-click-share-artifact")}>
              <button
                type="button"
                aria-pressed={asArtifact}
                onClick={() => uploads.patch(f.key, { asArtifact: !asArtifact })}
                className="anim shrink-0 rounded px-1 text-2xs font-medium capitalize hover:text-text"
              >
                {asArtifact ? tr("ui-composer-artifact") : tr("ui-composer-file")}
              </button>
            </Tooltip>
          );
        }}
      />
      <div
        className="flex items-end gap-2 rounded-card border border-border bg-surface p-2"
        // The OS's own file drop — a browser event, not the app's drag world.
        onDragOver={(e) => {
          // Only claim the drop when there is actually a file in it, so
          // dragging selected text inside the box still behaves normally.
          if (e.dataTransfer.types.includes("Files")) e.preventDefault();
        }}
        onDrop={(e) => {
          if (!e.dataTransfer.files.length) return;
          e.preventDefault();
          take(e.dataTransfer.files);
        }}
      >
        <input
          ref={fileInput}
          type="file"
          multiple
          hidden
          onChange={(e) => {
            take(e.target.files);
            // Cleared so choosing the same file twice in a row fires again.
            e.target.value = "";
          }}
        />
        {leading}
        {attachMenu.length > 0 ? (
          <Menu
            label={tr("ui-composer-attach")}
            align="start"
            items={attachMenu}
            trigger={
              <span aria-label={tr("ui-composer-attach")} title={tr("ui-composer-attach-file-selection-terminal-s-last")} className={cn(attachButton, "inline-flex items-center", disabled && "pointer-events-none opacity-40")}>
                <ICON.attach size={14} aria-hidden />
              </span>
            }
          />
        ) : (
          <Tooltip label={tr("ui-composer-attach-files")}>
            <button type="button" aria-label={tr("ui-composer-attach-files")} disabled={disabled} onClick={() => fileInput.current?.click()} className={attachButton}>
              <ICON.attach size={14} aria-hidden />
            </button>
          </Tooltip>
        )}
        <textarea
          ref={ref}
          rows={1}
          value={text}
          disabled={disabled}
          placeholder={placeholder}
          aria-label={tr("ui-composer-message")}
          // A textarea is natively `role="textbox"`, which supports both of
          // these; a `role="combobox"` override would rename the message box
          // itself for a dropdown that is open a few seconds a day.
          aria-autocomplete={open ? "list" : undefined}
          aria-controls={open ? listId : undefined}
          aria-activedescendant={open ? rowId(at) : undefined}
          aria-keyshortcuts="Enter Meta+Enter"
          onChange={(e) => edit(e.target)}
          // Clicking or arrowing out of an `@`-run has to close the picker
          // too: it used to survive until the next keystroke, so Enter sent
          // the message *and* inserted a mention somewhere behind the caret.
          onSelect={(e) => track(e.currentTarget)}
          onBlur={() => setSpan(null)}
          // A screenshot is the commonest attachment there is, and the only
          // way to get one here without saving it to disk first — named as
          // it arrives, since the engine's `image.png` says nothing.
          onPaste={pasted.onPaste}
          onKeyDown={(e) => {
            // Mid-composition (an IME assembling a character) Enter commits
            // the character, not the message.
            if (e.nativeEvent.isComposing) return;
            if (open) {
              const action = cursorAction(total, at, e.key);
              if (action) {
                e.preventDefault();
                if (action.type === "move") setCursor(action.index);
                else if (action.type === "dismiss") setSpan(null);
                else pick(action.index);
                return;
              }
            }
            // Enter sends; ⌘Enter (Ctrl+Enter elsewhere) sends too, even with
            // Shift held; Shift+Enter alone is a new line.
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey || !e.shiftKey)) {
              e.preventDefault();
              void send();
            }
          }}
          className="max-h-40 min-h-[1.75rem] flex-1 resize-none bg-transparent text-xs text-text placeholder:text-text-dim focus:outline-none"
        />
        {/* One slot: Send, or Stop while a turn works — never both (`composerButtonModel`). Enter still sends: a follow-up into a running turn is a supported act. */}
        <Button
          size="sm"
          variant={button.kind === "send" ? "primary" : "danger"}
          disabled={!button.enabled}
          title={button.hint}
          aria-label={button.label}
          onClick={button.kind === "stop" && stop ? stop.onStop : () => void send()}
          className="text-2xs"
        >
          {button.kind !== "send" && <ICON.stop size={12} aria-hidden />}
          {button.label}
        </Button>
      </div>
    </div>
  );
}
