/**
 * The pet: an animated companion that says what the agents are doing.
 *
 * It reads the workspace the app already holds — nothing is fetched for it
 * except the list of installed pets — and turns four facts into one character
 * you can see from across the room:
 *
 * ```text
 * standing, highest first        plays once, then falls back
 *   waiting   something owes you   waving   the workspace came online
 *   review    a gate is open       jumping  a gate approved, work accepted
 *   running   an agent mid-turn    failed   a result rejected, a check failed
 *   idle      nothing
 * ```
 *
 * Which state that is lives in `petModel.mjs` with tests, because a pet
 * claiming nothing is happening while three agents work is a wrong fact rather
 * than a wrong pixel.
 *
 * # Where it sits, and why not beside the notes dock
 *
 * `z-30`: under the notes panel, which is a thing you are *using*, and well
 * under the `z-50` tier every dialog and menu is portalled to. Its default
 * position is clear of the notes dock, because two draggable things landing on
 * the same pixel on first run read as one broken thing. It is placed as the
 * notes dock is (`ui/Dock`): anchored to the edges it is nearest, so it stays
 * where it was put when the window is resized or maximized.
 *
 * Mounted in `App.tsx` outside `<Screen>`, `Suspense` and the error boundary —
 * a companion that vanished when a screen threw would be reporting on the one
 * moment worth reporting on. The footer's Pet read-out (`shell/PetStat.tsx`,
 * `shell/PetOverlay.tsx`) is the panel *about* it — show, size, which one.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { navigate, useRoute } from "../router";
import { followWords, followedSession } from "../shell/followedSessionModel.mjs";
import { workSummary } from "../shell/workSummaryModel.mjs";
import { openFollowed, useFollowedSession } from "../shell/followedSessionStore";
import { useBrowserClear } from "../shell/browserClear";
import { onSessionTransition } from "../shell/sessionsStore";
import { useSettledSessions } from "../shell/useSettledSessions";
import { useWorkspace } from "../shell/useWorkspaceData";
import { counts as sessionCounts } from "../ui/sessionState.mjs";
import { dockBox, dockStyle, useDockDrag, useDockViewport } from "../ui/Dock";
import { cn } from "../ui/cn";
import { PetSprite } from "./PetSprite";
import { movePet, refreshPets, usePet } from "./petStore";
import {
  dragState,
  frameDuration,
  petHeight,
  petStateOfSession,
  routeFor,
  spriteBox,
  standingState,
  stateWords,
  transientFor,
  transientForTransition,
  type PetState,
  type SpriteBox,
} from "./petModel.mjs";
import { t } from "../i18n/l10n.mjs";

/** How long a once-through state holds the stage before handing back. */
const TRANSIENT_MS = frameDuration("jumping") * 8;

export function PetCompanion() {
  const ws = useWorkspace();
  const { active, dock, pets, size } = usePet();
  const [transient, setTransient] = useState<PetState | null>(null);
  const clearAt = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Read once into the shared store. The settings panel refreshes the same
  // list when it installs or removes one, which is what makes a pet appear the
  // moment it is imported rather than on the next launch.
  useEffect(() => {
    const ctrl = new AbortController();
    void refreshPets(ctrl.signal);
    return () => ctrl.abort();
  }, []);
  // A node that was not there at boot answered no list: read once it is back.
  useReloadOnReconnect(() => void refreshPets());

  const play = useCallback((next: PetState) => {
    setTransient(next);
    if (clearAt.current) clearTimeout(clearAt.current);
    clearAt.current = setTimeout(() => setTransient(null), TRANSIENT_MS);
  }, []);

  // A wave when the workspace comes up — the one animation that is not about
  // the work, and the only greeting a companion gets to make.
  const greeted = useRef(false);
  useEffect(() => {
    if (!ws.ready || greeted.current) return;
    greeted.current = true;
    play("waving");
  }, [play, ws.ready]);

  // In the Project IDE the pet stands for one harness: the session the
  // workstream in view is about (`followedSessionModel` — the one chosen,
  // else the loudest live). Everywhere else it stands for the workspace.
  const route = useRoute();
  const wid = route.name === "workbench" && route.scope === "workstream" ? route.id : null;
  // The roster as the tabs say it: a harness whose tab exited is settled
  // here as on the rail, so the pet never keeps working for a closed shell.
  const sessions = useSettledSessions();
  const chosenId = useFollowedSession(wid);
  const followed = useMemo(() => followedSession(chosenId, sessions, wid), [chosenId, sessions, wid]);
  const following = useRef(followed);
  following.current = followed;

  // While following, a run finishing three goals away is not this pet's
  // news: the workspace's events are left alone and only the followed
  // session's own edges play.
  useEngineEvents((e) => {
    if (following.current) return;
    const next = transientFor(e.payload);
    if (next) play(next);
  });
  const followedId = followed?.id ?? null;
  useEffect(() => {
    if (!followedId) return;
    return onSessionTransition((prev, next) => {
      if (next.id !== followedId) return;
      const once = transientForTransition(prev, next.state);
      if (once) play(once);
    });
  }, [followedId, play]);

  // What the workspace is doing (`workSummaryModel` — the rule an addon's
  // summary reads too): what waits, what is open for review, what is worked
  // on, and the conversation an agent is working in, for the click to land on.
  // Keyed on the three facts `workSummary` reads, not the `ws` object a
  // workspace fact the pet never shows would remake.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const summary = useMemo(() => workSummary(ws, sessionCounts(sessions)), [ws.inbox, ws.waiting, ws.working, sessions]);
  const busyScope = summary.busyScope;
  const standing: PetState = followed ? petStateOfSession(followed.state) : standingState(summary);

  // How big this pet actually is, measured from its own sheet. The reference
  // size stands in until it has loaded, so the first paint is already close.
  const tall = petHeight(size);
  const [sprite, setSprite] = useState<SpriteBox>(() => spriteBox(0, 0, tall));

  // The pet is not square and is nothing like 40px, so the window it is
  // placed in is told its real extent, or the clamp will happily park it half
  // off the screen. The paint is the stored placement projected into this
  // window; the drag starts from it, so a clamped pet does not jump.
  const viewport = useDockViewport(sprite);
  const box = dockBox(dock, viewport);
  const drag = useDockDrag(box, viewport, movePet);
  // Facing the way it is dragged beats anything it was saying: you are holding
  // it, and where it is going is the only thing it can usefully report.
  const facing = drag.dragging ? dragState(drag.dx) : null;
  const state: PetState = facing ?? transient ?? standing;

  // Nothing installed, or the chosen one was removed elsewhere: no pet, and no
  // placeholder standing in for one.
  const chosen = active && pets.some((p) => p.id === active) ? active : null;
  // Over a browser tab the layer leaves a hole for the pet (ide/18) — asked before the early return, as hooks are.
  const body = useRef<HTMLButtonElement>(null);
  useBrowserClear("pet", body, Boolean(chosen));
  if (!chosen) return null;

  const label = pets.find((p) => p.id === chosen)?.displayName ?? t("pet-pet-companion-pet");
  const follow = followed && wid ? { workstream: wid } : null;
  const where = routeFor(state, busyScope, follow);
  // The state is the useful half of the label: which pet it is does not
  // change, and what it is doing is the reason to look at it — and, while
  // it follows a harness, whose doing it is.
  const saying = followed ? `${stateWords(state)} · ${followWords(followed)}` : stateWords(state);

  return createPortal(
    <button
      ref={body}
      type="button"
      aria-label={`${label} — ${saying}`}
      title={`${label} — ${saying}`}
      // Sized to the pet, so all of the pet is the grab handle.
      style={dockStyle(box, sprite)}
      onPointerDown={drag.onPointerDown}
      onClick={() => {
        // Finishing a drag must not also navigate.
        if (!drag.wasClick()) return;
        if (!where) return;
        navigate(where);
        // A following pet is a door to its harness: the workstream's Agent panel.
        if (follow) openFollowed(follow.workstream);
      }}
      className={cn(
        "fixed z-30 block touch-none select-none overflow-visible border-0 bg-transparent p-0",
        drag.dragging
          ? "cursor-grabbing"
          : where
            ? "cursor-pointer"
            : "cursor-grab",
      )}
    >
      <PetSprite
        pet={chosen}
        def={pets.find((p) => p.id === chosen)}
        state={state}
        height={tall}
        onSize={setSprite}
        className="pointer-events-none block"
      />
    </button>,
    document.body,
  );
}
