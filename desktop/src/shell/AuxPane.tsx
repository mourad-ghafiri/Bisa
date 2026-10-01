/**
 * The right-hand pane: one slot, seven possible occupants.
 *
 * Which occupant is showing lives in the URL (`?aux=thread&auxId=…`), so Back
 * closes the pane instead of leaving the screen, and a reload puts it back.
 * The width is shared by all seven, so switching from a thread to a profile
 * doesn't make the layout jump — and its bounds are the room's, not a
 * number of pixels (`auxPaneModel.auxBounds`): the edge drags to seven
 * tenths of the column beside the sidebar, which `App.tsx` measures and
 * hands in as `available`. The width a person chose is kept as chosen; what
 * is drawn is that width held within the room (`shownWidth`), so a window
 * made narrower narrows the pane and one made wider gives the choice back.
 * The IDE's own right pane keeps its own numbers: in Project Mode the
 * browser is a centre tab, not this pane.
 *
 * The shell owns the frame; views fill it through `AuxPortal` — except the
 * artifact occupant (ide/12), the transcript occupant (ide/09) and the
 * browser occupant (ide/18), which the shell draws itself from the URL
 * alone, so an artifact opens beside any screen a message shows on, a
 * transcript beside any screen a session is named on, and the browser
 * beside any screen at all — ⌘⇧L, the footer, a Browser button.
 *
 * The slot a view fills is **published, never looked up** (`auxSlot.ts`):
 * the pane hands its element over through a callback ref as it mounts and
 * withdraws it as it goes, and the portal follows. A lookup by id captured
 * once kept portalling into the element of a pane that had closed, so the
 * pane opened again empty.
 */

import { createPortal } from "react-dom";
import { useCallback, useEffect, useRef, type ReactNode } from "react";
import { setSearch, useAddress, useSearchParams } from "../router";
import { splitHash } from "../routeModel.mjs";
import { ResizeHandle, useStoredSize } from "../ui";
import { ArtifactPane } from "./ArtifactPane";
import { AUX_DEFAULT_WIDTH, AUX_MIN_WIDTH, AUX_WIDTH_KEY, auxBounds, shownWidth, toggledAux } from "./auxPaneModel.mjs";
import { publishAuxSlot, useAuxSlot } from "./auxSlot";
import { BrowserPane } from "./BrowserPane";
import { toggleBrowserPane } from "./browserDoors";
import { OPEN_BROWSER, onDoor } from "./shortcuts";
import { TranscriptPane } from "./TranscriptPane";
import { useViewScroll } from "./useViewScroll";
import { t } from "../i18n/l10n.mjs";

export type AuxOccupant = "thread" | "session" | "profile" | "inspector" | "artifact" | "transcript" | "browser";

const OCCUPANTS: AuxOccupant[] = ["thread", "session", "profile", "inspector", "artifact", "transcript", "browser"];

interface AuxState {
  kind: AuxOccupant | null;
  id: string | null;
  open: (kind: AuxOccupant, id?: string) => void;
  close: () => void;
  toggle: (kind: AuxOccupant, id?: string) => void;
}

export function useAux(): AuxState {
  const params = useSearchParams();
  const raw = params.get("aux");
  const kind = OCCUPANTS.includes(raw as AuxOccupant) ? (raw as AuxOccupant) : null;
  const id = params.get("auxId");

  const open = useCallback(
    (next: AuxOccupant, nextId?: string) => setSearch({ aux: next, auxId: nextId ?? null }),
    [],
  );
  const close = useCallback(() => setSearch({ aux: null, auxId: null }), []);
  // The same occupant with no tab in mind, or with the tab it shows, closes
  // — so ⌘⇧L and a screen's button close a pane opened on a tab at once.
  const toggle = useCallback(
    (next: AuxOccupant, nextId?: string) => {
      const target = toggledAux({ kind, id }, next, nextId);
      if (target === null) close();
      else open(next, target.auxId ?? undefined);
    },
    [close, id, kind, open],
  );

  return { kind, id, open, close, toggle };
}

/**
 * Fill the pane from a view. Unmounting removes the content but leaves the
 * pane open, so a view that is still loading shows the frame, not a flicker.
 * The slot is the pane's published element, so a pane that closed and opened
 * again is followed to its new one.
 */
export function AuxPortal({ children }: { children: ReactNode }) {
  const slot = useAuxSlot();
  if (!slot) return null;
  return createPortal(children, slot);
}

const TITLES: Record<AuxOccupant, string> = {
  thread: t("shell-aux-pane-conversations"),
  session: t("shell-aux-pane-agent-session"),
  profile: t("shell-aux-pane-profile"),
  inspector: t("shell-aux-pane-details"),
  artifact: t("shell-aux-pane-artifact"),
  transcript: t("shell-aux-pane-transcript"),
  browser: t("shell-browser-pane-browser"),
};

export function AuxPane({ singleColumn, available }: { singleColumn: boolean; available: number }) {
  const { kind, id, close, toggle } = useAux();
  // The floor is read in with the store; the ceiling is the room's, known
  // only once the column is measured — so it is applied to what is drawn.
  const [stored, setWidth] = useStoredSize(AUX_WIDTH_KEY, AUX_DEFAULT_WIDTH, { min: AUX_MIN_WIDTH, max: Number.POSITIVE_INFINITY });
  const bounds = auxBounds(available);
  const width = shownWidth(stored, bounds);
  // ⌘⇧L and the palette: the Browser occupant, shown or hidden — and, with
  // no tab in sight, a tab opened for it: the person's act, at this door.
  useEffect(() => onDoor(OPEN_BROWSER, () => toggleBrowserPane({ kind, toggle })), [kind, toggle]);
  // The pane's own scroll is the place's it stands beside: kept with the
  // screen's, under the occupant's name, so a goal's details come back
  // where they were read.
  const pane = useRef<HTMLElement>(null);
  const place = splitHash(useAddress()).path;
  useViewScroll(pane, `${place}#${kind ?? ""}`, place);

  if (!kind) return null;

  return (
    <>
      {!singleColumn && (
        <ResizeHandle
          side="left"
          size={width}
          min={bounds.min}
          max={bounds.max}
          defaultSize={AUX_DEFAULT_WIDTH}
          onSize={setWidth}
          label={t("shell-aux-pane-resize-details-pane")}
        />
      )}
      <aside
        ref={pane}
        data-pane
        aria-label={TITLES[kind]}
        className="flex min-w-0 shrink-0 flex-col border-l border-border bg-surface"
        style={singleColumn ? { flex: "1 1 auto" } : { width }}
      >
        <header className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
          <h2 className="flex-1 truncate text-xs font-semibold">{TITLES[kind]}</h2>
          <button
            type="button"
            onClick={close}
            title={t("shell-aux-pane-close-esc")}
            aria-label={t("shell-aux-pane-close-details-pane")}
            className="anim rounded-control px-1.5 py-0.5 text-xs text-text-dim hover:bg-surface-2 hover:text-text"
          >
            ✕
          </button>
        </header>
        {kind === "artifact" ? (
          <ArtifactPane artifactKey={id} />
        ) : kind === "transcript" ? (
          <TranscriptPane session={id} />
        ) : kind === "browser" ? (
          <BrowserPane tab={id} />
        ) : (
          <div ref={publishAuxSlot} data-scroll-keep={`pane:${kind}:${id ?? ""}`} className="min-h-0 flex-1 overflow-auto" />
        )}
      </aside>
    </>
  );
}
