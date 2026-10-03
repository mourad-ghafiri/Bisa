/**
 * The Details pane's Browser occupant (ide/18): every open tab in a strip,
 * the active one's bar — the same bar the IDE draws, the wand on it — and
 * under it the box its native webview is drawn over, beside any screen, so
 * a page an agent opened from a channel, a goal or a workflow is seen where
 * the conversation is. `?auxId=<key>` focuses that tab while it is in
 * sight. **The pane opens no tab of its own**: a tab is born from an act —
 * a person's door, an agent's request, a page's window — never from a pane
 * that merely mounted (an address the place memory brought back, an addon's
 * navigation), so with no tab in sight it says so and offers *New tab*
 * (ide/18 §The footer's count). A tab kept out of sight (ide/18 §Headless
 * tabs) is in the strip, dim with the hidden glyph, and a click shows it —
 * a click, never an address. A tab the IDE's centre shows at the same time
 * is drawn there, the bigger box, and the pane says so.
 *
 * A page is annotated here only while the pane stands beside the Project
 * IDE (`useBrowserAnnotation`'s `offered`): beside any other screen the bar
 * draws no wand and the page is only shown. In the IDE the note box is the
 * page's own, drawn over the element by the inspector, and the tray sits
 * under the page — the checkout's tray for a tab at home in a workstream
 * (`CheckoutAnnotationTray`: Send, the file chips), the screen's for any
 * other (`PaneAnnotationTray`: the chips into the composer beside it). The body
 * carries `data-browser-doc`, so the browser chords are live in it.
 */

import { useEffect } from "react";
import { Button, EmptyState, ICON, StripControlButton, TabStrip, Tooltip } from "../ui";
import { CheckoutAnnotationTray } from "../views/_workbench/AnnotationTray";
import { PaneAnnotationTray } from "../views/_workbench/PaneAnnotationTray";
import { useBrowserAnnotation } from "../views/_workbench/useBrowserAnnotation";
import { BrowserBar } from "./BrowserBar";
import { screenHome } from "./browserDoors";
import { useBrowserPlaces } from "./browserPlaces";
import { whereWords } from "./browserPlacesModel.mjs";
import { BLANK_URL, PERSON, browserLabel, browserTitle, seenSessions, visibilityWords } from "./browsersModel.mjs";
import type { BrowserSession } from "./browsersModel.mjs";
import { LayerSlot } from "./LayerSlot";
import { useLayerSlot } from "./layerSlots";
import { closeBrowserTab, focusBrowserTab, openBrowserIn, revealBrowserTab, useBrowsers, whyNotBrowser } from "./useBrowsers";
import { t } from "../i18n/l10n.mjs";

export function BrowserPane({ tab }: { tab: string | null }) {
  const { sessions, active } = useBrowsers();
  const places = useBrowserPlaces();
  const why = whyNotBrowser();

  // The address names a tab in sight: it comes to the front, once per name.
  // One kept out of sight stays there — an address can be a memory, and
  // showing an agent's page is a click's to do, never a memory's.
  const seen = seenSessions(sessions);
  useEffect(() => {
    if (tab && seen.some((s) => s.key === tab)) focusBrowserTab(tab);
    // Once per name: following `seen` would bring the named tab back to the
    // front every time the person focused another.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);
  /** *New tab*, the person's: at home beside this screen. */
  const newTab = () => void openBrowserIn({ home: screenHome(), by: PERSON });

  const shown = seen.find((s) => s.key === active) ?? seen[0] ?? null;
  if (why) return <p className="p-4 text-2xs text-text-dim">{why}</p>;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* The kit's strip: one tab stop, ← → Home End between tabs, Delete
          closes the focused one, the × shows on hover and on focus. A tab
          kept out of sight wears the hidden glyph, dimmed. */}
      <TabStrip
        size="sm"
        label={t("shell-browser-pane-browser-tabs")}
        className="h-row shrink-0 border-b border-hairline px-1.5"
        tabs={sessions.map((s) => ({
          id: s.key,
          label: browserLabel(s),
          title: t("shell-browser-pane-words", { s: browserTitle(s), places: whereWords(s, places), s2: visibilityWords(s), flag: (s.headless) ? "yes" : "no" }),
          icon: s.headless ? ICON.hidden : undefined,
          dimmed: s.headless,
        }))}
        active={shown?.key ?? null}
        onSelect={revealBrowserTab}
        onClose={closeBrowserTab}
        menuFor={(key) => {
          const s = sessions.find((x) => x.key === key);
          return s ? [{ label: t("shell-browser-pane-close-tab", { label: browserLabel(s) }), icon: ICON.close, onSelect: () => closeBrowserTab(key) }] : [];
        }}
        trailing={
          <Tooltip label={t("shell-browser-overlay-new-tab")}>
            <StripControlButton label={t("shell-browser-overlay-new-tab")} onClick={newTab}>
              <ICON.add size={13} aria-hidden />
            </StripControlButton>
          </Tooltip>
        }
      />
      {shown ? (
        <PaneTab key={shown.key} session={shown} />
      ) : (
        // Nothing in sight to look at: the pane says so and offers the one act that opens a tab here.
        <EmptyState
          icon={ICON.page}
          title={t("shell-browser-pane-no-tab-here-yet")}
          hint={t("shell-browser-pane-tab-opens-from-here-link-agent")}
          action={
            <Button variant="primary" size="sm" onClick={newTab}>
              <ICON.add size={12} aria-hidden />{t("shell-browser-overlay-new-tab")}</Button>
          }
        />
      )}
    </div>
  );
}

/** The active tab's body: the bar, the slot, the note box and the tray — one hook's facts per tab. */
function PaneTab({ session }: { session: BrowserSession }) {
  const center = useLayerSlot("center");
  const a = useBrowserAnnotation(session);
  const inCentre = center.layer === "browser" && center.key === session.key;
  const blank = session.url === BLANK_URL;
  const traying = a.annotatable && (a.draft.annotations.length > 0 || a.inspecting);
  return (
    <div data-browser-doc tabIndex={-1} className="flex min-h-0 flex-1 flex-col outline-none">
      <BrowserBar session={session} server={a.server} inIde={false} wand={a.offered ? { enabled: a.annotatable, whyNot: a.whyNot, inspecting: a.inspecting, count: a.draft.annotations.length, onToggle: () => a.setInspecting(!a.inspecting) } : null} />
      <div className="relative min-h-0 flex-1">
        <LayerSlot host="aux" layer="browser" tabKey={session.key} label={t("shell-browser-pane-browser")} />
        {inCentre && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-4 text-center text-2xs text-text-dim">{t("shell-browser-pane-tab-showing-project-ide-s-centre")}</div>
        )}
        {!inCentre && blank && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-4 text-center text-2xs text-text-dim">{t("shell-browser-pane-type-address-above-page-project-serves")}</div>
        )}
      </div>
      {traying && a.home.kind === "checkout" && <CheckoutAnnotationTray wid={a.home.wid} page={a.page} draft={a.draft} lost={a.lost} onDraft={a.setDraft} onDone={a.done} />}
      {traying && a.home.kind === "screen" && <PaneAnnotationTray page={a.page} draft={a.draft} lost={a.lost} onDraft={a.setDraft} onDone={a.done} />}
    </div>
  );
}
