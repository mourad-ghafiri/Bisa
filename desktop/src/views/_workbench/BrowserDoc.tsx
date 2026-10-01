/**
 * A browser tab's body in the Project IDE (ide/18): the shared bar
 * (`shell/BrowserBar.tsx`, the wand on it) over the slot the tab's native
 * webview is drawn on (`shell/BrowserPanel.tsx`), and under it the
 * annotation tray when the wand is on or annotations wait. The note box is
 * the page's own — the inspector draws it over the clicked element, as it
 * does in a rendered file's frame — so nothing here has to paint above the
 * native view.
 *
 * What can be annotated, which page the elements are on and the draft are
 * `useBrowserAnnotation`'s, shared with the Browser pane; the tray is the
 * checkout's (`AnnotationTray`: Send to an agent, file chips when the page
 * is a file of it) for a tab at home in a workstream, and the screen's
 * (`PaneAnnotationTray`: Attach to the message) for a goal's or a work
 * item's tab. The body carries `data-browser-doc`, so the browser chords
 * are live while it has the focus (ide/15).
 */

import { BrowserBar } from "../../shell/BrowserBar";
import type { BrowserSession } from "../../shell/browsersModel.mjs";
import { LayerSlot } from "../../shell/LayerSlot";
import { AnnotationTray } from "./AnnotationTray";
import { PaneAnnotationTray } from "./PaneAnnotationTray";
import { useBrowserAnnotation } from "./useBrowserAnnotation";
import { t } from "../../i18n/l10n.mjs";

export function BrowserDoc({ session, project }: { session: BrowserSession; project: string | null }) {
  const a = useBrowserAnnotation(session);
  const blank = session.url === "about:blank";
  const traying = a.annotatable && (a.draft.annotations.length > 0 || a.inspecting);

  return (
    <div data-browser-doc tabIndex={-1} className="flex h-full min-h-0 flex-col outline-none">
      <BrowserBar session={session} server={a.server} inIde wand={{ enabled: a.annotatable, whyNot: a.whyNot, inspecting: a.inspecting, count: a.draft.annotations.length, onToggle: () => a.setInspecting(!a.inspecting) }} />
      <div className="relative min-h-0 flex-1">
        <LayerSlot layer="browser" label={t("workbench-browser-doc-browser")} tabKey={session.key} />
        {blank && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-2xs text-text-dim">{t("workbench-browser-doc-type-address-above-page-project-serves")}</div>
        )}
      </div>
      {traying && a.home.kind === "checkout" && <AnnotationTray wid={a.home.wid} pid={project} page={a.page} draft={a.draft} lost={a.lost} onDraft={a.setDraft} onDone={a.done} />}
      {traying && a.home.kind === "screen" && <PaneAnnotationTray page={a.page} draft={a.draft} lost={a.lost} onDraft={a.setDraft} onDone={a.done} />}
    </div>
  );
}
