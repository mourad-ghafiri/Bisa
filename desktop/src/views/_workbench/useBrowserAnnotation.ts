/**
 * Annotating a browser tab's page for an agent (ide/18), the half both
 * hosts share — the IDE's `BrowserDoc` and the Details pane's `BrowserPane`:
 * whether the page can be annotated at all and why not, which page the
 * elements are on (the file the node serves it from, else the URL), the
 * session's draft keyed by that page, the wand's state and the notes the
 * page's own box hands back (`browserInspectorStore`), and the two words
 * the page is driven with — the mode and the badges with their notes —
 * said once a load has finished, never into a document that is leaving.
 * The hosts draw the bar and the tray; the page draws the box; this
 * decides and drives.
 *
 * Whether a page can be annotated is `serversModel.annotatable`'s one rule:
 * any page the tab shows, never an artifact's own — and only in the
 * Project IDE (`offered`): the Details pane beside any other screen shows a
 * page and never annotates it, and a wand left on, or badges left drawn,
 * are taken back from the page when the person leaves the IDE. Where the chips go is
 * `annotationHome`'s: the checkout's conversation for a tab at home in a
 * workstream, else the conversation on screen.
 */

import { useEffect, useMemo } from "react";
import { api } from "../../api";
import { driveBrowserView } from "../../browser/session";
import { errorFields, log } from "../../log";
import { HELD } from "../../shell/browserChromeModel.mjs";
import { browserAnnotationsDone, listenBrowserNotes, setBrowserInspecting, useTabInspector } from "../../shell/browserInspectorStore";
import { draftScopeOf } from "../../shell/browsersModel.mjs";
import type { BrowserSession } from "../../shell/browsersModel.mjs";
import { useServing } from "../../shell/useServing";
import type { ServedFolder } from "../../types";
import { useInspectorTheme } from "../../ui/artifact/inspectorTokens";
import { inspectMessage, marksMessage, themeMessage } from "../../ui/artifact/pageInspector.mjs";
import { useRoute } from "../../router";
import { useSessionDraft } from "../_work/gitPanelStore";
import { useAsync } from "../_work/useAsync";
import { EMPTY_DRAFT, addAnnotation, annotationsKey, marksOf } from "./annotationModel.mjs";
import type { AnnotationDraft } from "./annotationModel.mjs";
import { filePage, urlPage } from "./contextChips.mjs";
import type { PageRef } from "./contextChips.mjs";
import { annotatable as canAnnotate, annotationHome, serverAt } from "./serversModel.mjs";
import type { AnnotationHome } from "./serversModel.mjs";

export interface BrowserAnnotation {
  /** Annotating is offered here at all: the Project IDE alone. Elsewhere the host draws no wand and no tray. */
  readonly offered: boolean;
  /** The page can be annotated; when not, `whyNot` says why in the wand's words. */
  readonly annotatable: boolean;
  readonly whyNot: string | null;
  /** The server the node serves the page from, for the bar's *served* chip. */
  readonly server: ServedFolder | null;
  /** Where the chips go. */
  readonly home: AnnotationHome;
  /** The page the elements are on: a file of the checkout, else the URL. */
  readonly page: PageRef;
  readonly inspecting: boolean;
  readonly lost: readonly number[];
  readonly draft: AnnotationDraft;
  readonly setDraft: (next: AnnotationDraft | ((prev: AnnotationDraft) => AnnotationDraft)) => void;
  readonly setInspecting: (on: boolean) => void;
  /** The annotations left the tray — sent, attached or cleared. */
  readonly done: () => void;
}

export function useBrowserAnnotation(session: BrowserSession): BrowserAnnotation {
  const key = session.key;
  const home = annotationHome(session);
  const wid = home.kind === "checkout" ? home.wid : null;
  const { inspecting, lost } = useTabInspector(key);
  // Annotating a page for an agent is the Project IDE's, wherever else the tab is seen.
  const offered = useRoute().name === "workbench";

  // Which page the elements are on, and whether it can be annotated at all,
  // once the servers are read — an artifact's page is an agent's own.
  const { all, read } = useServing(wid);
  const server = useMemo(() => serverAt(all, session.url), [all, session.url]);
  const annotatable = offered && read && canAnnotate(session, all);
  const whyNot = annotatable || !offered ? null : HELD.artifact;
  const resolved = useAsync(
    (s) => {
      if (!wid || !server || server.owner.kind !== "workstream") return Promise.resolve(null);
      let path = "/";
      try {
        path = new URL(session.url).pathname;
      } catch {
        return Promise.resolve(null);
      }
      return api.resolveServed(wid, server.id, path, s).then((r) => r.path);
    },
    [wid, server?.id, session.url],
  );
  const page: PageRef = useMemo(() => (resolved.data ? filePage(resolved.data) : urlPage(session.url)), [resolved.data, session.url]);
  const [draft, setDraft] = useSessionDraft<AnnotationDraft>(annotationsKey(draftScopeOf(session), page), EMPTY_DRAFT);
  const marks = useMemo(() => marksOf(draft), [draft]);

  // The page's inspector wears the app's theme and follows the wand and the
  // badges — each said once a load has finished, into the document that is
  // there, never the one leaving. The theme first, and again after every
  // load: the tab's script was dressed as the tab opened, and a document
  // loaded after a switch would otherwise wear the old one.
  const theme = useInspectorTheme();
  useEffect(() => {
    if (!annotatable || session.loading) return;
    void driveBrowserView(key, themeMessage(theme)).catch((e: unknown) => log.debug("browser", "the page did not take the theme", { key, ...errorFields(e) }));
  }, [key, theme, annotatable, session.url, session.loading]);
  const mode = annotatable && inspecting ? "picking" : "off";
  useEffect(() => {
    if (!annotatable || session.loading) return;
    void driveBrowserView(key, inspectMessage(mode)).catch((e: unknown) =>
      log.debug("browser", "the page did not take the inspector message", { key, mode, ...errorFields(e) }),
    );
  }, [key, mode, annotatable, session.url, session.loading]);
  useEffect(() => {
    if (!annotatable || session.loading) return;
    void driveBrowserView(key, marksMessage(marks)).catch((e: unknown) => log.debug("browser", "the page did not take the marks", { key, ...errorFields(e) }));
  }, [key, marks, annotatable, session.url, session.loading]);

  // Off the IDE's screen, or on a page nobody may edit: a wand left on is
  // turned off, in the app and in the page, and the page's badges go — they
  // come back with the draft when the person is in the IDE again.
  useEffect(() => {
    if (annotatable || session.loading) return;
    if (inspecting) setBrowserInspecting(key, false);
    void driveBrowserView(key, inspectMessage("off")).catch((e: unknown) => log.debug("browser", "the page did not take the inspector message", { key, mode: "off", ...errorFields(e) }));
    void driveBrowserView(key, marksMessage([])).catch((e: unknown) => log.debug("browser", "the page did not take the marks", { key, ...errorFields(e) }));
  }, [key, annotatable, inspecting, session.url, session.loading]);

  // The notes typed in the page's own box, each with the element it is about.
  useEffect(() => listenBrowserNotes(key, (m) => setDraft((d) => addAnnotation(d, m, m.note))), [key, setDraft]);

  return {
    offered,
    annotatable,
    whyNot,
    server,
    home,
    page,
    inspecting,
    lost,
    draft,
    setDraft,
    setInspecting: (on) => setBrowserInspecting(key, on),
    done: () => {
      setDraft(EMPTY_DRAFT);
      browserAnnotationsDone(key);
    },
  };
}
