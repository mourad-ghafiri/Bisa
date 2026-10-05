/**
 * One note: a textarea, a live preview, and a toolbar that marks the
 * selection.
 *
 * Hand-rolled, following `ui/FileView.tsx`'s precedent — a `SegmentedControl`
 * switching between the source and the rendered document, over the same
 * `Markdown` component every other markdown in the app goes through. The only
 * piece with real logic is the selection splice, and it lives in
 * `notesModel.mjs` where it has tests.
 *
 * # The buffer is local, and nothing takes it away
 *
 * **This is the rule the first version broke, and breaking it made the panel
 * unusable.** It re-seeded the textarea from every incoming copy of the note,
 * including the echo of its own save, so text typed since that save was
 * replaced by an older version of itself — and the difference that produced
 * was saved, which produced another echo. Typing was a fight.
 *
 * So: props seed this component when the **note id** changes and at no other
 * time. A server copy arriving while you have unsaved text goes through
 * `adoptIncoming`, which takes it only when there is nothing of yours to lose
 * and otherwise says so and leaves your text alone. `ui/Composer.tsx` holds
 * its draft the same way and for the same reason.
 *
 * # The debounce is a debounce
 *
 * The timer and the latest text live in refs, and `save` is in no dependency
 * array — the previous version had `useEffect(() => () => void save(), [save])`
 * meaning to flush on unmount, but `save` changed identity on every keystroke,
 * so React ran the cleanup — and the save — on every keystroke instead. One
 * PATCH is in flight at a time; a save that finishes while you kept typing
 * schedules exactly one more.
 *
 * Text is also parked in `localStorage` as you type, so the window closing
 * between saves costs nothing.
 *
 * # Find and replace is the kit's bar
 *
 * `⌘F` and `⌘R` here — on the editor's own keydown, because the overlay is
 * portalled outside the workbench and the app's document chord never reaches
 * it — open the one `FindBar`, over the one `findModel`. In Write and Split
 * the matches are `matchesOf(body)`, stepping selects the match in the
 * textarea, and a replacement goes through `setBody` so the debounce, the
 * draft and the save see it like typing. In Read the bar searches the
 * rendering through `useDomFind`, as every rendered document does, and has
 * nothing to replace into.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { ApiError, api } from "../api";
import type { NoteRow } from "../types";
import { Button, ConfirmDialog, FindBar, Markdown, SegmentedControl, Tooltip, emptyFind, matchesOf, replaceAll, replaceOne, stepIndex, useDockOverlap, useDomFind } from "../ui";
import type { Find } from "../ui";
import { cn } from "../ui/cn";
import { ICON } from "../ui/icons";
import {
  adoptIncoming,
  draftKey,
  matchAt,
  wrapSelection,
  type MarkKind,
  type NoteDraft,
  type NoteView,
  noteGoneWords,
  noteRefusal,
  noteStatusWords,
  draftAction,
  parseDraft,
  restoredDraft,
} from "./notesModel.mjs";
import { forgetPref, readPref, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { MaximizeToggle } from "../shell/MaximizeToggle";
import { isSaveChord, useDocumentHold } from "../shell/documentGuard";
import { ConversationDrawer } from "../views/_studio/ConversationDrawer";
import { notesGuard, setNotesAskOpen, setNotesOpen, toggleNotesMaximized, useNotesOverlay } from "./notesStore";
import { t } from "../i18n/l10n.mjs";

/** The model owns the vocabulary; this is the same three, named locally. */
type View = NoteView;

const VIEWS = [
  { id: "write" as const, label: t("notes-note-editor-write") },
  { id: "split" as const, label: t("notes-note-editor-split") },
  { id: "read" as const, label: t("notes-note-editor-read") },
];

/** The toolbar, in the order a hand reaches for them. */
const TOOLS = [
  { kind: "bold", icon: ICON.bold, label: t("notes-note-editor-bold") },
  { kind: "italic", icon: ICON.italic, label: t("notes-note-editor-italic") },
  { kind: "heading", icon: ICON.heading, label: t("notes-note-editor-heading") },
  { kind: "bullet", icon: ICON.bullet, label: t("notes-note-editor-list") },
  { kind: "quote", icon: ICON.quote, label: t("notes-note-editor-quote") },
  { kind: "code", icon: ICON.code, label: t("notes-note-editor-code") },
  { kind: "link", icon: ICON.link, label: t("notes-note-editor-link") },
] satisfies { kind: MarkKind; icon: (typeof ICON)["bold"]; label: string }[];

/**
 * Bring a textarea's line into view without taking the focus — the find bar
 * keeps it while you step. A textarea only scrolls to its selection when it
 * is focused, so the line's top is computed from the line height instead;
 * a wrapped long line lands a little short, which is still on screen.
 */
function revealLine(el: HTMLTextAreaElement, at: number) {
  const line = el.value.slice(0, at).split("\n").length - 1;
  const lineHeight = parseFloat(getComputedStyle(el).lineHeight) || 18;
  const top = line * lineHeight;
  const visible = top >= el.scrollTop && top <= el.scrollTop + el.clientHeight - lineHeight;
  if (!visible) el.scrollTop = Math.max(0, top - el.clientHeight / 2);
}

/** The draft parked for a note — its text and the hash it was typed against — or none. */
function readDraft(id: string): NoteDraft | null {
  return readPref(webStorage(), draftKey(id), parseDraft, null);
}

/** Park a draft with the hash of the note it is typed against, so a restore later knows whether the note moved since. */
function parkDraft(id: string, body: string, base_hash: string): void {
  writePref(webStorage(), draftKey(id), { body, base_hash });
}

export function NoteEditor({
  note,
  scopeName,
  onSaved,
  onBack,
  onDeleted,
}: {
  note: NoteRow;
  /** What the note is about, in the words the list's chip uses. */
  scopeName: string;
  onSaved: (next: NoteRow) => void;
  onBack: () => void;
  onDeleted: () => void;
}) {
  const { defaultView, askOpen, saveDelay, maximized } = useNotesOverlay();
  // Seeded from the preference, then session-only: switching view here is a
  // glance at the rendered copy, not a decision about how every note opens.
  // Writing it back would make an idle click on Read permanent.
  const [view, setView] = useState<View>(defaultView);
  // Seeded once per note id. A draft left by a window that closed mid-sentence
  // is restored — it is what had not been sent yet — but it was typed against
  // the note as it then stood: typed against this very text, it may save once
  // typed into; typed against another, it is a conflict for the person to
  // settle, never a silent save over what an agent wrote since
  // (`restoredDraft`).
  const seeded = useRef<ReturnType<typeof restoredDraft> | null>(null);
  if (seeded.current === null) seeded.current = restoredDraft(readDraft(note.id), note);
  const [title, setTitle] = useState(note.title);
  const [body, setBody] = useState(seeded.current.body);
  const [saving, setSaving] = useState(false);
  const [conflict, setConflict] = useState<string | null>(seeded.current.conflict ? t("notes-note-editor-draft-from-before") : null);
  const [error, setError] = useState<string | null>(null);
  /** The body before an agent's rewrite was taken, while the person may still want it back — session memory, gone on typing or leaving. */
  const [restorable, setRestorable] = useState<string | null>(null);
  /** The note was deleted under the editor (the node's 404): said in the node's words; nothing more is saved, the text stays to copy. */
  const [gone, setGone] = useState<string | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);
  const preview = useRef<HTMLDivElement>(null);
  // The editor's foot holds Delete at its right end, where a floating dock may stand: the button keeps clear of it, the dock never moves.
  const foot = useRef<HTMLElement>(null);
  const dockRoom = useDockOverlap(foot);
  /** Where to put the caret after a splice, applied once the value is painted. */
  const pendingRange = useRef<{ from: number; to: number } | null>(null);
  /** The find bar: `null` while closed. */
  const [find, setFind] = useState<Find | null>(null);
  const [findIndex, setFindIndex] = useState(-1);
  const [replacing, setReplacing] = useState(false);
  const [findFocus, setFindFocus] = useState<{ field: "find" | "replace"; nonce: number }>({ field: "find", nonce: 0 });

  /**
   * Everything the save path reads, so it can be a stable callback.
   *
   * A `useCallback` over `[title, body]` would change identity on every
   * keystroke, and anything holding it in a dependency array would then fire
   * on every keystroke. That is precisely the bug this rewrite exists to
   * remove, so the values go in a ref and the callback has no dependencies at
   * all.
   */
  const live = useRef({
    id: note.id,
    title: note.title,
    body: note.body,
    /** What the server has confirmed — the baseline "dirty" is measured from. */
    confirmed: { title: note.title, body: note.body },
    hash: note.hash,
    inFlight: false,
    /** Typed again while a save was in the air. */
    again: false,
    conflicted: seeded.current.conflict,
    /** The person typed since this note opened: a restored draft is not saved until they do. */
    typed: false,
    /** Let go of — *Don't save*, or the note is being deleted: nothing more is saved. */
    abandoned: false,
    /** The save in the air, for whoever must wait for it. */
    pending: null as Promise<boolean> | null,
  });
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;

  live.current.title = title;
  live.current.body = body;

  const save = useCallback(async (): Promise<boolean> => {
    const s = live.current;
    if (s.conflicted || s.abandoned) return false;
    if (s.title === s.confirmed.title && s.body === s.confirmed.body) return true;
    if (s.inFlight) {
      // Typed while a save was in the air: one more save once it lands.
      s.again = true;
      return s.pending ?? false;
    }
    s.inFlight = true;
    setSaving(true);
    const sending = { id: s.id, title: s.title, body: s.body };
    const run = (async () => {
      try {
        const { note: next } = await api.patchNote(sending.id, { title: sending.title, body: sending.body, base_hash: s.hash });
        // The list's row, the search index and every other reader see the
        // saved text through the owner's callback, never through the bus.
        onSavedRef.current(next);
        forgetPref(webStorage(), draftKey(sending.id));
        // The panel moved on to another note while this save was in the air:
        // the answer is the note's that was left — its hash and its text are
        // not the baseline of the one on screen, which a write here would turn
        // into a conflict nobody caused.
        if (s.id !== sending.id) return false;
        s.hash = next.hash;
        s.confirmed = { title: next.title, body: next.body };
        // A conflict flagged while this save was in the air is settled by it
        // landing — the flag goes with the banner, or saving would stop for
        // good with nothing on screen to say so.
        s.conflicted = false;
        setConflict(null);
        setError(null);
      } catch (e) {
        // A refusal about the note that was left says nothing of the one on screen.
        if (s.id !== sending.id) return false;
        const refusal = e instanceof ApiError ? noteRefusal(e.status) : "failed";
        if (refusal === "conflict") {
          s.conflicted = true;
          setConflict(t("notes-note-editor-agent-wrote-note-while-editing"));
        } else if (refusal === "gone") {
          // Deleted under the editor: a save can never land, so none is tried again; what was typed stays on screen.
          s.abandoned = true;
          setGone(noteGoneWords(e instanceof Error ? e.message : String(e)));
        } else {
          setError(e instanceof ApiError ? e.message : t("notes-note-editor-could-not-save"));
        }
      } finally {
        s.inFlight = false;
        s.pending = null;
        setSaving(false);
        if (s.again && !s.conflicted && !s.abandoned) {
          s.again = false;
          void save();
        }
      }
      return !s.conflicted && s.title === s.confirmed.title && s.body === s.confirmed.body;
    })();
    s.pending = run;
    return run;
  }, []);

  /** Save now and say whether the note is clean afterwards — the hold's save, and the Save button's. */
  const saveNow = useCallback(async (): Promise<boolean> => {
    const s = live.current;
    for (let round = 0; round < 3; round++) {
      if (s.pending) await s.pending;
      else if (!(await save())) return false;
      if (s.conflicted || s.abandoned) return false;
      if (!s.inFlight && s.title === s.confirmed.title && s.body === s.confirmed.body) return true;
    }
    return false;
  }, [save]);

  /** Let go: nothing typed since the last save is kept or saved. */
  const discard = useCallback(() => {
    const s = live.current;
    s.abandoned = true;
    if (timer.current) clearTimeout(timer.current);
    forgetPref(webStorage(), draftKey(s.id));
  }, []);

  // Park the text and arm the timer. Both keyed on the text itself, so this is
  // the one effect that runs per keystroke — and all it does is set a timeout.
  useEffect(() => {
    const s = live.current;
    // The panel moved on to another note: the effect below re-seeds for it,
    // and this one runs again for the right note — never parking the note
    // that was left under the new one's key.
    if (s.id !== note.id) return;
    if (draftAction(body, s.confirmed.body) === "forget") forgetPref(webStorage(), draftKey(s.id));
    else parkDraft(s.id, body, s.hash);
    // A conflict pauses saving; a restored draft is not saved until typed into.
    if (conflict || s.abandoned || !s.typed) return;
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void save(), saveDelay);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [body, title, conflict, note.id, save, saveDelay]);

  // Flush once, on unmount — unless the note was let go of. `save` never
  // changes identity, so this runs once.
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
      if (!live.current.abandoned) void save();
    },
    [save],
  );

  /**
   * A different note in this panel. **Keyed on the id alone** — the previous
   * version also watched the content and the hash, so its own save's echo
   * re-entered here and overwrote the textarea.
   */
  useEffect(() => {
    const s = live.current;
    if (s.id === note.id) return;
    // Whatever was typed in the note being left must go before its state does
    // — unless it was let go of.
    if (!s.abandoned) void save();
    const restored = restoredDraft(readDraft(note.id), { body: note.body, hash: note.hash });
    s.id = note.id;
    s.confirmed = { title: note.title, body: note.body };
    s.hash = note.hash;
    s.conflicted = restored.conflict;
    s.typed = false;
    s.abandoned = false;
    s.again = false;
    setTitle(note.title);
    setBody(restored.body);
    setConflict(restored.conflict ? t("notes-note-editor-draft-from-before") : null);
    setRestorable(null);
    setError(null);
  }, [note.id, note.title, note.body, note.hash, save]);

  /**
   * The same note, changed by somebody else — an agent appending, or
   * rewriting it when asked. `adoptIncoming` decides, and its whole job is
   * that it never replaces text you have not saved: an append lands under
   * your typing, live; a rewrite over a clean buffer is taken with the way
   * back kept (*Restore my version*); a rewrite over your typing is a conflict
   * you settle. A title that moved alone — the hash is the body's — is taken
   * when yours was not touched.
   */
  useEffect(() => {
    const s = live.current;
    if (s.id !== note.id || (note.hash === s.hash && note.title === s.confirmed.title)) return;
    if (s.conflicted) return;
    const { body: next, outcome } = adoptIncoming(s.body, s.confirmed.body, note.body);
    if (outcome === "conflict") {
      s.conflicted = true;
      setConflict(t("notes-note-editor-agent-wrote-note-while-editing"));
      return;
    }
    if (outcome === "taken") setRestorable(s.confirmed.body);
    if (s.title === s.confirmed.title) setTitle(note.title);
    s.hash = note.hash;
    s.confirmed = { title: note.title, body: note.body };
    if (next !== s.body) setBody(next);
  }, [note.id, note.hash, note.title, note.body]);

  // React owns the value, so the caret has to be restored after the render
  // that carries the spliced text — setting it inside the handler would put it
  // where the *old* value ended.
  useEffect(() => {
    const range = pendingRange.current;
    if (!range) return;
    pendingRange.current = null;
    const el = area.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(range.from, range.to);
  }, [body]);

  const mark = (kind: MarkKind) => {
    const el = area.current;
    if (!el) return;
    const next = wrapSelection(body, el.selectionStart, el.selectionEnd, kind);
    pendingRange.current = { from: next.from, to: next.to };
    typed();
    setBody(next.text);
  };

  /** The person typed: a restored draft may save now, and a rewrite taken a moment ago is theirs to keep. */
  const typed = () => {
    live.current.typed = true;
    if (restorable !== null) setRestorable(null);
  };

  /** Take theirs: the store's copy replaces the buffer — a draft typed against another text goes with it — and saving resumes at its hash. */
  const takeTheirs = async () => {
    try {
      const { note: current } = await api.note(note.id);
      const s = live.current;
      s.hash = current.hash;
      s.confirmed = { title: current.title, body: current.body };
      s.conflicted = false;
      s.typed = false;
      forgetPref(webStorage(), draftKey(s.id));
      setTitle(current.title);
      setBody(current.body);
      setConflict(null);
    } catch (e) {
      if (e instanceof ApiError && noteRefusal(e.status) === "gone") {
        live.current.abandoned = true;
        setGone(noteGoneWords(e.message));
      } else setError(e instanceof ApiError ? e.message : t("notes-note-editor-could-not-re-read"));
    }
  };

  /** Keep mine: the buffer stands and saves over what is there, at the hash the list holds — the person's decision, said. */
  const keepMine = () => {
    const s = live.current;
    s.hash = note.hash;
    s.confirmed = { title: note.title, body: note.body };
    s.conflicted = false;
    s.typed = true;
    setConflict(null);
  };

  /** The body from before an agent's rewrite, back in the buffer — dirty, so the next save makes it the note again. */
  const restoreMine = () => {
    const mine = restorable;
    if (mine === null) return;
    live.current.typed = true;
    setRestorable(null);
    setBody(mine);
  };

  // The matches: over the text in Write and Split, over the rendering in Read.
  const reading = view === "read";
  const matches = useMemo(() => (find && !reading ? matchesOf(body, find) : []), [body, find, reading]);
  const domFound = useDomFind(preview, reading ? find : null, findIndex, body.length);
  const findCount = find === null ? null : reading ? domFound.count : matches.length;
  useEffect(() => {
    setFindIndex((i) => (findCount && findCount > 0 ? (i >= 0 && i < findCount ? i : 0) : -1));
  }, [findCount, find?.query, find?.regex, find?.caseSensitive]);
  // Stepping selects the match in the textarea without taking the focus from
  // the bar, so Enter keeps stepping; the line is brought into view by hand.
  useEffect(() => {
    const el = area.current;
    const hit = matchAt(matches, findIndex);
    if (!el || !hit) return;
    el.setSelectionRange(hit.start, hit.end);
    revealLine(el, hit.start);
  }, [matches, findIndex]);

  const openFind = (replace: boolean) => {
    setReplacing(replace && !reading);
    setFind((f) => f ?? emptyFind());
    setFindFocus((r) => ({ field: replace && !reading ? "replace" : "find", nonce: r.nonce + 1 }));
  };
  const closeFind = () => {
    setFind(null);
    setReplacing(false);
    setFindIndex(-1);
    area.current?.focus();
  };
  const replaceInBody = (all: boolean) => {
    if (!find || reading) return;
    const next = all ? replaceAll(body, find).text : replaceOne(body, find, findIndex).text;
    if (next !== body) {
      typed();
      setBody(next);
    }
  };
  // ⌘F and ⌘R, on the editor's own keydown: the overlay sits outside the
  // workbench, so the app's document chord never reaches it.
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key !== "f" && key !== "r") return;
    e.preventDefault();
    e.stopPropagation();
    openFind(key === "r");
  };

  const dirty =
    title !== live.current.confirmed.title || body !== live.current.confirmed.body;
  const status = useMemo(() => noteStatusWords({ gone, conflict, error, saving, dirty }), [conflict, dirty, error, gone, saving]);
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  // The hold: what the leave guard and the quit question read.
  // A note that is gone holds nobody back: there is nothing a save could write to, so the leave guard has no question to ask.
  useDocumentHold(notesGuard, "note", note.id, { dirty: () => dirtyRef.current && !live.current.abandoned, save: saveNow, title: () => live.current.title, discard });

  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const remove = async () => {
    // Nothing is saved at a note that is going away.
    discard();
    try {
      await api.deleteNote(note.id);
      onDeleted();
    } catch (e) {
      // A note already gone is a delete that happened: the editor leaves, as it would have.
      if (e instanceof ApiError && noteRefusal(e.status) === "gone") {
        onDeleted();
        return;
      }
      live.current.abandoned = false;
      setError(e instanceof ApiError ? e.message : t("notes-note-editor-could-not-delete"));
    }
  };

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-note-editor
      onKeyDown={onKeyDown}
      onKeyDownCapture={(e) => {
        // The keymap's `save` — ⌘S by default, a rebinding followed — for this note.
        if (isSaveChord(e)) {
          e.preventDefault();
          void saveNow();
        }
      }}
    >
      <header className="flex shrink-0 items-center gap-1 border-b border-hairline px-2 py-1.5">
        <Tooltip label={t("notes-note-editor-back-list")}>
          <button
            type="button"
            aria-label={t("notes-note-editor-back-list")}
            onClick={onBack}
            className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
          >
            <ICON.back size={14} aria-hidden />
          </button>
        </Tooltip>
        <input
          value={title}
          aria-label={t("notes-note-editor-note-title")}
          onChange={(e) => {
            typed();
            setTitle(e.target.value);
          }}
          className="min-w-0 flex-1 bg-transparent text-xs font-medium text-text focus:outline-none"
        />
        <span className="max-w-[8rem] shrink-0 truncate text-2xs text-text-dim" title={scopeName}>
          {scopeName}
        </span>
        <span
          aria-live="polite"
          className={cn(
            "tnum shrink-0 text-2xs",
            conflict || error ? "text-danger" : "text-text-dim",
          )}
        >
          {status}
        </span>
        <Button size="sm" disabled={!dirty || saving || conflict !== null} onClick={() => void saveNow()}>
          {t("notes-note-editor-save")}
        </Button>
        <Tooltip label={askOpen ? t("notes-note-editor-hide-ask") : t("notes-note-editor-ask-agent")}>
          <button
            type="button"
            aria-label={t("notes-note-editor-ask-agent")}
            aria-pressed={askOpen}
            onClick={() => setNotesAskOpen(!askOpen)}
            className={cn("anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control", askOpen ? "bg-selected text-text" : "text-text-dim hover:bg-surface-2 hover:text-text")}
          >
            <ICON.agent size={14} aria-hidden />
          </button>
        </Tooltip>
        <MaximizeToggle maximized={maximized} onToggle={toggleNotesMaximized} />
        {/* The panel's ×, in the same place as in the list: it closes the panel and keeps this note the open one. */}
        <Tooltip label={t("notes-note-overlay-close-notes")}>
          <button
            type="button"
            aria-label={t("notes-note-overlay-close-notes")}
            onClick={() => setNotesOpen(false)}
            className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
          >
            <ICON.close size={13} aria-hidden />
          </button>
        </Tooltip>
      </header>

      {/* The document's column — toolbar, find, body, foot — and, beside it, the conversation drawer. */}
      <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-0.5 border-b border-hairline px-2 py-1">
        {TOOLS.map((tool) => {
          const Glyph = tool.icon;
          return (
            <Tooltip key={tool.kind} label={tool.label}>
              <button
                type="button"
                aria-label={tool.label}
                disabled={view === "read"}
                onClick={() => mark(tool.kind)}
                className="anim flex h-6 w-6 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45"
              >
                <Glyph size={13} aria-hidden />
              </button>
            </Tooltip>
          );
        })}
        <Tooltip label={find ? t("notes-note-editor-close-find") : t("notes-note-editor-find-replace")}>
          <button
            type="button"
            aria-label={t("notes-note-editor-find-replace")}
            aria-pressed={find !== null}
            onClick={() => (find ? closeFind() : openFind(!reading))}
            className={cn(
              "anim ml-1 flex h-6 w-6 items-center justify-center rounded-control",
              find ? "bg-selected text-text" : "text-text-dim hover:bg-surface-2 hover:text-text",
            )}
          >
            <ICON.search size={13} aria-hidden />
          </button>
        </Tooltip>
        <div className="flex-1" />
        <SegmentedControl options={VIEWS} value={view} onChange={setView} label={t("notes-note-editor-view")} size="sm" />
      </div>

      {find && (
        <div className="shrink-0 border-b border-hairline px-2 py-1">
          <FindBar
            find={find}
            onChange={setFind}
            index={findIndex}
            count={findCount}
            onStep={(dir) => setFindIndex((i) => stepIndex(i, findCount ?? 0, dir))}
            onClose={closeFind}
            replace={replacing && !reading ? { onReplace: () => replaceInBody(false), onReplaceAll: () => replaceInBody(true) } : undefined}
            note={reading ? t("notes-note-editor-find-works-over-rendering-write-split") : undefined}
            focus={findFocus}
            label={t("notes-note-editor-find-note")}
            className="border-0 bg-transparent px-0 py-0 shadow-none"
          />
        </div>
      )}

      {conflict && (
        <div className="flex shrink-0 items-center gap-2 border-b border-hairline bg-danger-soft py-1 pl-3 pr-2 text-2xs text-danger">
          <span className="min-w-0 flex-1">{t("notes-note-editor-saving-paused")}</span>
          <Button size="sm" variant="ghost" onClick={() => void takeTheirs()}>
            {t("notes-note-editor-take-theirs")}
          </Button>
          <Button size="sm" variant="ghost" onClick={keepMine}>
            {t("notes-note-editor-keep-mine")}
          </Button>
        </div>
      )}
      {restorable !== null && !conflict && (
        <div className="flex shrink-0 items-center gap-2 border-b border-hairline bg-surface-2 py-1 pl-3 pr-2 text-2xs text-text-dim">
          <span className="min-w-0 flex-1">{t("notes-note-editor-agent-rewrote-note")}</span>
          <Button size="sm" variant="ghost" onClick={restoreMine}>
            {t("notes-note-editor-restore-my-version")}
          </Button>
        </div>
      )}

      <div className="flex min-h-0 flex-1">
        {view !== "read" && (
          // A raw textarea rather than the kit's `TextArea`, for the reason
          // `Composer` uses one: the toolbar splices at the caret, so this
          // needs a ref, and the kit's wrapper takes none.
          <textarea
            ref={area}
            value={body}
            aria-label={t("notes-note-editor-note-body")}
            spellCheck
            placeholder={t("notes-note-editor-write-anything-nobody-else-sees")}
            onChange={(e) => {
              typed();
              setBody(e.target.value);
            }}
            className={cn(
              "min-h-0 flex-1 resize-none bg-transparent p-3 font-mono text-xs leading-relaxed text-text placeholder:text-text-dim focus:outline-none",
              view === "split" && "border-r border-hairline",
            )}
          />
        )}
        {view !== "write" && (
          <div ref={preview} className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
            <Markdown text={body} />
          </div>
        )}
      </div>

      <footer ref={foot} style={dockRoom > 0 ? { paddingRight: dockRoom } : undefined} className="flex shrink-0 items-center justify-between border-t border-hairline py-1 pl-3 pr-2">
        <span className="tnum text-2xs text-text-dim">
          {t("notes-note-editor-characters", { n: body.length.toLocaleString() })}
        </span>
        <Button size="sm" variant="danger" onClick={() => setConfirmingDelete(true)}>{t("notes-note-editor-delete")}</Button>
      </footer>
      <ConfirmDialog
        open={confirmingDelete}
        onClose={() => setConfirmingDelete(false)}
        onConfirm={() => {
          setConfirmingDelete(false);
          void remove();
        }}
        title={t("notes-note-editor-delete-title", { title: title.trim() || t("shell-leave-guard-this-note") })}
        body={t("notes-note-editor-delete-body")}
        confirmLabel={t("notes-note-editor-delete-confirm")}
        danger
      />
      </div>
      {askOpen && (
        <ConversationDrawer
          origin={{ kind: "note", id: note.id }}
          icon={<ICON.note size={14} aria-hidden />}
          subject={title}
          widthKey="bisa.notes.askwidth"
          label={t("notes-note-editor-ask-width")}
          hint={t("notes-note-editor-ask-hint")}
        />
      )}
      </div>
    </div>
  );
}
