/**
 * A drawing on the canvas (19 — Drawings): a header bar — back, the title,
 * the status, *Save*, the Ask drawer, maximize, delete and the panel's × —
 * over the canvas,
 * with the drawer beside it when it is open. *Save* saves now (⌘S too);
 * leaving with unsaved strokes asks through the panel's leave guard
 * (`shell/documentGuard.ts`), and *Delete* asks first.
 *
 * # Saving
 *
 * Every stroke the canvas reports is compared with what the store last
 * confirmed (`liveScene.lastSaved`): a hover changes nothing, a stroke bumps
 * an element's version. A change arms the timer; the timer saves the scene
 * — the drawn elements and the two facts of the app state — against the
 * hash the store last gave, one PATCH in flight at a time
 * (`autosaveModel.mjs`). A 409 means somebody drew meanwhile: saving stops,
 * the bar says so, and *Take theirs* adopts what is there — a drawing has no
 * textual merge, so the strokes since are the price, said in words.
 *
 * A `drawing_changed` frame for this drawing whose hash is not the one last
 * saved means somebody else drew: a clean canvas reloads; a dirty one is a
 * conflict, resolved the same way. The frames reach the editor through the
 * overlay (`heard`), the one subscriber, so a frame between the detail's
 * read and this mount is judged on the mount. The bridge saves through the
 * same `liveScene` record when it draws on this canvas, so an agent's stroke
 * landing here is never mistaken for somebody else's write; a save it made
 * offscreen moves no record of ours and arrives as that frame.
 *
 * The canvas is **live from its first change**: Excalidraw hands its API
 * over before the scene loads and reports nothing while loading, so the
 * first `onChange` is the load, and the record is made from it — never from
 * the empty canvas the API arrived on, which read every drawing as moved. A
 * reload asked before the canvas was there is owed to that first change.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, api } from "../api";
import { errorFields, log } from "../log";
import type { DrawingDetail } from "../types";
import { Button, ConfirmDialog, Tooltip } from "../ui";
import { cn } from "../ui/cn";
import { ICON } from "../ui/icons";
import { loadExcalidraw } from "../ui/excalidraw";
import type { AppState, ExcalidrawImperativeAPI, OrderedExcalidrawElement } from "../ui/excalidraw";
import { abandoned, adopted, atStore, changed, frameHeard, heardAfterSave, opened, reloadDecision, reloadOwed, saveAsked, saveConflicted, saveFailed, saveLanded } from "./autosaveModel.mjs";
import type { Autosave } from "./autosaveModel.mjs";
import { canvasAppState, drawStatusWords, drawnElements, persistedAppState, sceneMoved } from "./drawModel.mjs";
import { ExcalidrawCanvas } from "./ExcalidrawCanvas";
import { ConversationDrawer } from "../views/_studio/ConversationDrawer";
import { lastSaved, noteSaved, registerLiveScene, unregisterLiveScene } from "./liveScene";
import { MaximizeToggle } from "../shell/MaximizeToggle";
import { isSaveChord, useDocumentHold } from "../shell/documentGuard";
import { drawGuard, setDrawAskOpen, setDrawOpen, toggleDrawMaximized, useDrawOverlay } from "./drawStore";
import { t } from "../i18n/l10n.mjs";

type ExcalidrawModule = typeof import("@excalidraw/excalidraw");

export function DrawEditor({
  detail,
  scopeName,
  onSaved,
  onBack,
  onDeleted,
  onApi,
  heard,
}: {
  detail: DrawingDetail;
  /** What the drawing is about, in the words the list's chip uses. */
  scopeName: string;
  onSaved: (next: DrawingDetail) => void;
  onBack: () => void;
  onDeleted: () => void;
  /** The canvas's API as it mounts, for the panel's Escape rule. */
  onApi: (api: ExcalidrawImperativeAPI | null) => void;
  /**
   * The latest `drawing_changed` the overlay heard for this drawing — its
   * hash — judged here (`reloadDecision`): the overlay is the one subscriber,
   * so a frame heard before this editor mounted is judged on its mount.
   */
  heard: { drawing: string; hash: string } | null;
}) {
  const { saveDelay, maximized, askOpen } = useDrawOverlay();
  // Read at arm time, through a ref: `save` is made once and re-arms
  // through the `arm` it closed over, which must still take the delay as
  // the preference stands now, not as it stood when `save` was made.
  const delay = useRef(saveDelay);
  delay.current = saveDelay;
  const [mod, setMod] = useState<ExcalidrawModule | null>(null);
  const [title, setTitle] = useState(detail.title);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [conflict, setConflict] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const canvas = useRef<ExcalidrawImperativeAPI | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** The save in the air, for whoever must wait for it. */
  const running = useRef<Promise<boolean> | null>(null);
  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;
  /** *Take theirs*, for the save that finds somebody else's scene once it answered — set below, where it is defined. */
  const adoptRef = useRef<() => Promise<void>>(() => Promise.resolve());
  const live = useRef<{ id: string; auto: Autosave; lastAppState: ReturnType<typeof persistedAppState> }>({
    id: detail.id,
    auto: opened(detail.hash),
    lastAppState: detail.scene.app_state ? persistedAppState(canvasAppState(detail.scene.app_state)) : persistedAppState(null),
  });

  useEffect(() => {
    void loadExcalidraw()
      .then(setMod)
      .catch((e: unknown) => {
        log.warn("draw", "the canvas could not be loaded", errorFields(e));
        setError(t("draw-editor-could-not-load-canvas"));
      });
  }, []);

  /** The scene the store last confirmed through this canvas, so its next change is measured against it. */
  const confirm = useCallback((id: string, hash: string, elements: readonly OrderedExcalidrawElement[]) => {
    const api_ = canvas.current;
    if (api_) noteSaved(id, { hash, elements }, api_);
  }, []);

  const save = useCallback(async (): Promise<boolean> => {
    const s = live.current;
    const api_ = canvas.current;
    if (!api_) return false;
    const asked = saveAsked(s.auto);
    s.auto = asked.next;
    if (!asked.run) return running.current ?? (!s.auto.dirty && !s.auto.conflicted);
    setSaving(true);
    const elements = drawnElements(api_.getSceneElements());
    const app_state = persistedAppState(api_.getAppState());
    const run = (async () => {
      try {
        const { drawing } = await api.patchDrawing(s.id, { scene: { elements, app_state }, base_hash: atStore(s.auto, lastSaved(s.id, api_)?.hash ?? null).savedHash });
        s.auto = saveLanded(s.auto, drawing.hash);
        s.lastAppState = app_state;
        confirm(s.id, drawing.hash, elements);
        setConflict(null);
        setError(null);
        onSavedRef.current(drawing);
      } catch (e) {
        if (e instanceof ApiError && e.status === 409) {
          s.auto = saveConflicted(s.auto);
          setConflict(t("draw-editor-somebody-drew-meanwhile"));
        } else {
          s.auto = saveFailed(s.auto);
          setError(e instanceof ApiError ? e.message : t("draw-editor-could-not-save"));
        }
      } finally {
        running.current = null;
        setSaving(false);
        // A frame heard while this save was in the air is judged now that it
        // answered: the save's own echo is nothing — and so is the bridge's,
        // which saves through this canvas's record — any other hash is
        // somebody else's scene (`heardAfterSave`).
        const late = heardAfterSave(atStore(live.current.auto, lastSaved(live.current.id, api_)?.hash ?? null));
        live.current.auto = late.next;
        if (late.decision === "reload") void adoptRef.current();
        else if (late.decision === "conflict") {
          live.current.auto = saveConflicted(live.current.auto);
          setConflict(t("draw-editor-somebody-drew-meanwhile"));
        }
        setDirty(live.current.auto.dirty);
        if (live.current.auto.dirty && !live.current.auto.conflicted) arm();
      }
      return !live.current.auto.dirty && !live.current.auto.conflicted;
    })();
    running.current = run;
    return run;
    // `arm` is defined below and depends on `save` in turn; the re-arm reads
    // it through the closure at run time, when it exists.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [confirm]);

  /** Save now and say whether the canvas is clean afterwards — the hold's save, and the Save button's. */
  const saveNow = useCallback(async (): Promise<boolean> => {
    for (let round = 0; round < 3; round++) {
      if (running.current) await running.current;
      else if (!(await save())) return false;
      const a = live.current.auto;
      if (a.conflicted) return false;
      if (!a.dirty && !a.inFlight && !running.current) return true;
    }
    return false;
  }, [save]);

  /** Let go: nothing drawn since the last save is saved. */
  const discard = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    live.current.auto = abandoned(live.current.auto);
    setDirty(false);
  }, []);

  const arm = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void save(), delay.current);
  }, [save]);

  /** Every change the canvas reports: the first is the load; then a stroke arms a save and a hover does not. */
  const onChange = useCallback(
    (elements: readonly OrderedExcalidrawElement[], appState: AppState) => {
      const s = live.current;
      const api_ = canvas.current;
      if (!api_) return;
      const app = persistedAppState(appState);
      const saved = lastSaved(s.id, api_);
      // The first change the canvas reports is the load — Excalidraw says
      // nothing while it loads — so the record is made from it, at the hash
      // the drawing was read at: nothing is dirty, and a reload owed while
      // the canvas was not there is done now.
      if (!saved) {
        registerLiveScene(s.id, api_, { hash: s.auto.savedHash, elements });
        s.lastAppState = app;
        if (s.auto.owed) void adoptRef.current();
        return;
      }
      const movedApp = app.view_background_color !== s.lastAppState.view_background_color || app.grid !== s.lastAppState.grid;
      if (!movedApp && !sceneMoved(elements, saved.elements)) return;
      s.auto = changed(s.auto);
      if (!dirty) setDirty(true);
      arm();
    },
    [arm, dirty],
  );

  /** Take what is there: the store's scene replaces the canvas's, and saving resumes. */
  const adopt = useCallback(async () => {
    const api_ = canvas.current;
    const m = mod;
    if (!api_ || !m || !lastSaved(live.current.id, api_)) {
      // Not loaded yet: the load would wipe whatever is put on the canvas
      // now, so the reload is owed to the first change the canvas reports.
      live.current.auto = reloadOwed(live.current.auto);
      return;
    }
    try {
      const { drawing } = await api.drawing(live.current.id);
      const elements = m.restoreElements(drawing.scene.elements as Parameters<typeof m.restoreElements>[0], null, { repairBindings: true });
      api_.updateScene({ elements, appState: canvasAppState(drawing.scene.app_state), captureUpdate: m.CaptureUpdateAction.NEVER });
      live.current.auto = adopted(live.current.auto, drawing.hash);
      live.current.lastAppState = persistedAppState(canvasAppState(drawing.scene.app_state));
      confirm(drawing.id, drawing.hash, api_.getSceneElements());
      setTitle(drawing.title);
      setConflict(null);
      setError(null);
      setDirty(false);
      onSavedRef.current(drawing);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("draw-editor-could-not-re-read"));
    }
  }, [confirm, mod]);

  adoptRef.current = adopt;

  // Somebody else drew: reload when clean, conflict when not; our own save's
  // echo is nothing — the bridge's through this canvas too — and while a save
  // is in the air a frame may be that echo outrunning the answer, so it waits
  // for it. The frames come down from the overlay, the one subscriber.
  const judge = useCallback((hash: string) => {
    const api_ = canvas.current;
    const decision = reloadDecision(atStore(live.current.auto, lastSaved(live.current.id, api_ ?? undefined)?.hash ?? null), hash);
    if (decision === "reload") void adoptRef.current();
    else if (decision === "wait") live.current.auto = frameHeard(live.current.auto, hash);
    else if (decision === "conflict") {
      live.current.auto = saveConflicted(live.current.auto);
      setConflict(t("draw-editor-somebody-drew-meanwhile"));
    }
  }, []);
  useEffect(() => {
    if (heard && heard.drawing === live.current.id) judge(heard.hash);
  }, [heard, judge]);

  // Flush once on unmount, and forget the live scene once the flush has
  // landed — the save reads and writes that record, so it must outlive it —
  // this canvas's record alone: a canvas that took the drawing meanwhile
  // keeps its own.
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
      const id = live.current.id;
      const api_ = canvas.current;
      const flush = live.current.auto.dirty && !live.current.auto.conflicted ? save() : Promise.resolve(false);
      void flush.finally(() => {
        if (api_) unregisterLiveScene(id, api_);
      });
      onApi(null);
    },
    // Unmount only: following `save` or `onApi` would flush and hand the
    // canvas back while the editor is still open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  // The API arrives before the scene loads: held, and registered live by the
  // first change the canvas reports (`onChange`), never from an empty canvas.
  const takeApi = useCallback(
    (api_: ExcalidrawImperativeAPI) => {
      canvas.current = api_;
      onApi(api_);
    },
    [onApi],
  );

  const renameIfMoved = async () => {
    const next = title.trim();
    if (next === "" || next === detail.title) {
      setTitle(detail.title);
      return;
    }
    try {
      const { drawing } = await api.patchDrawing(detail.id, { title: next });
      onSavedRef.current(drawing);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("draw-editor-could-not-rename"));
    }
  };

  const remove = async () => {
    setDeleting(true);
    // Nothing is saved at a drawing that is going away.
    const before = live.current.auto;
    discard();
    try {
      await api.deleteDrawing(detail.id);
      onDeleted();
    } catch (e) {
      live.current.auto = before;
      setDirty(before.dirty);
      setError(e instanceof ApiError ? e.message : t("draw-editor-could-not-delete"));
      setDeleting(false);
    }
  };

  const status = drawStatusWords({ conflict, error, saving, dirty });
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  // The hold: what the leave guard and the quit question read.
  useDocumentHold(drawGuard, "drawing", detail.id, { dirty: () => dirtyRef.current, save: saveNow, title: () => title, discard });

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-draw-editor
      onKeyDownCapture={(e) => {
        // The keymap's `save` — ⌘S by default, a rebinding followed — for this drawing.
        if (isSaveChord(e)) {
          e.preventDefault();
          void saveNow();
        }
      }}
    >
      <header className="flex shrink-0 items-center gap-1 border-b border-hairline px-2 py-1.5">
        <Tooltip label={t("draw-editor-back-to-list")}>
          <button type="button" aria-label={t("draw-editor-back-to-list")} onClick={onBack} className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
            <ICON.back size={14} aria-hidden />
          </button>
        </Tooltip>
        <input
          value={title}
          aria-label={t("draw-editor-title")}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => void renameIfMoved()}
          onKeyDown={(e) => {
            if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          }}
          className="min-w-0 flex-1 bg-transparent text-xs font-medium text-text focus:outline-none"
        />
        <span className="max-w-[8rem] shrink-0 truncate text-2xs text-text-dim" title={scopeName}>
          {scopeName}
        </span>
        <span aria-live="polite" className={cn("tnum shrink-0 text-2xs", conflict || error ? "text-danger" : "text-text-dim")}>
          {status}
        </span>
        <Button size="sm" disabled={!dirty || saving || conflict !== null} onClick={() => void saveNow()}>
          {t("draw-editor-save")}
        </Button>
        <Tooltip label={askOpen ? t("draw-editor-hide-ask") : t("draw-editor-ask-agent")}>
          <button
            type="button"
            aria-label={t("draw-editor-ask-agent")}
            aria-pressed={askOpen}
            onClick={() => setDrawAskOpen(!askOpen)}
            className={cn("anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control", askOpen ? "bg-selected text-text" : "text-text-dim hover:bg-surface-2 hover:text-text")}
          >
            <ICON.agent size={14} aria-hidden />
          </button>
        </Tooltip>
        <MaximizeToggle maximized={maximized} onToggle={toggleDrawMaximized} />
        <Tooltip label={t("draw-editor-delete")}>
          <button type="button" aria-label={t("draw-editor-delete")} disabled={deleting} onClick={() => setConfirmingDelete(true)} className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-danger disabled:opacity-45">
            <ICON.delete size={14} aria-hidden />
          </button>
        </Tooltip>
        {/* The panel's ×, last as in the list: it closes the panel and keeps this drawing the open one. */}
        <Tooltip label={t("draw-overlay-close")}>
          <button type="button" aria-label={t("draw-overlay-close")} onClick={() => setDrawOpen(false)} className="anim flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
            <ICON.close size={13} aria-hidden />
          </button>
        </Tooltip>
      </header>
      <ConfirmDialog
        open={confirmingDelete}
        onClose={() => setConfirmingDelete(false)}
        onConfirm={() => {
          setConfirmingDelete(false);
          void remove();
        }}
        title={t("draw-editor-delete-title", { title: title.trim() || t("shell-leave-guard-this-drawing") })}
        body={t("draw-editor-delete-body")}
        confirmLabel={t("draw-editor-delete-confirm")}
        danger
      />
      {conflict && (
        <div className="flex shrink-0 items-center gap-2 border-b border-hairline bg-danger-soft py-1 pl-3 pr-2 text-2xs text-danger">
          <span className="min-w-0 flex-1">{t("draw-editor-strokes-since-are-lost")}</span>
          <Button size="sm" variant="ghost" onClick={() => void adopt()}>
            {t("draw-editor-take-theirs")}
          </Button>
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 min-w-0 flex-1">
          {mod ? <ExcalidrawCanvas mod={mod} detail={detail} onApi={takeApi} onChange={onChange} /> : <p className="p-4 text-2xs text-text-dim">{error ?? t("draw-editor-loading-canvas")}</p>}
        </div>
        {askOpen && (
          <ConversationDrawer
            origin={{ kind: "drawing", id: detail.id }}
            icon={<ICON.draw size={14} aria-hidden />}
            subject={detail.title}
            widthKey="bisa.draw.askwidth"
            label={t("draw-editor-ask-width")}
            hint={t("draw-editor-ask-hint")}
          />
        )}
      </div>
    </div>
  );
}
