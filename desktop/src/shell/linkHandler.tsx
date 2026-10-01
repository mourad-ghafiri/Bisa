/**
 * The door every path and link in the desktop opens through (ide/17).
 *
 * The kit defines the contract (`ui/linkContext.ts`) and every text surface
 * — rendered markdown, plain text, a terminal, an external anchor — hands
 * a click here with where it was and which roots the surface knows. This
 * provider, mounted once in `App`, answers with one of two things at the
 * pointer:
 *
 * - **A path** — a small card of verbs. *Open src/a.rs:42* in the IDE (the
 *   line parked with `requestLine` so it survives the navigation), *Open in
 *   <root>* per candidate when several checkouts hold the file, *Reveal in
 *   Finder* (the platform's word from `revealLabel`), *Copy the path*. ⌘-click
 *   skips the card when the door is plain.
 * - **A URL** — the host in bold, the URL beneath, *Open in Bisa's browser* (ide/18), *Open in the machine's browser* and
 *   *Copy the URL*. The browser never opens on a bare click, and the window
 *   never navigates: a URL an agent wrote is untrusted text.
 *
 * Roots: the surface's, or — for a channel, the Pulse, the Inbox — every
 * checkout on disk, the one on screen first. An absolute path is matched to
 * a root here, on the desktop, and named to the node as `(scope, id,
 * relative)`; the webview never sends an absolute path to the node. Reveal
 * and *Open in the IDE* are the two acts on an absolute path outside every
 * root — the opener, and the shell reading it as a loose file (ide/03).
 */

import { useCallback, useMemo, useState, type ReactNode } from "react";
import { openExternal, revealPath } from "../api";
import { openUrlInBrowser } from "./browserDoors";
import { canOpenBrowser } from "./useBrowsers";
import { navigate, useRoute } from "../router";
import { LinkCard, LinkHandlerContext, copyText, resolveLink, useToast } from "../ui";
import type { DocResolution, LinkCardVerb, LinkHandler, LinkPointer, LinkRoot, LinkRootRef } from "../ui";
import { revealLabel } from "../ui/fileTreeMutations.mjs";
import { editorKey, requestLine } from "../views/_workbench/editorRegistry";
import { rootKey } from "../views/_workbench/workbenchModel.mjs";
import { DOCUMENT_MODE } from "../views/_workbench/ideModeModel.mjs";
import { setIdeMode } from "../views/_workbench/ideModeStore";
import { createLatest } from "./latestModel.mjs";
import { copiedWords, defaultRoots, pathCard, rootLabel, urlCard } from "./linkCardModel.mjs";
import type { CardSpec, CardVerb } from "./linkCardModel.mjs";
import { cachedIndex, loadIndex } from "./pathIndexStore";
import { openLooseFiles } from "./shortcuts";
import { useWorkspace } from "./useWorkspaceData";
import { errorFields, log } from "../log";
import { t } from "../i18n/l10n.mjs";

interface Card extends CardSpec {
  at: LinkPointer;
}

/**
 * Open a document in the IDE at a line: the line waits for the editor, the
 * root is put in the mode that shows documents — a path opened from Agent
 * Mode's conversation must not land behind it — then the route changes.
 */
function openResolvedDoc(res: DocResolution): void {
  const tab = `file:${res.path}`;
  const root = rootKey(res.scope, res.id);
  if (res.line !== null) requestLine(editorKey(root, tab), res.line);
  setIdeMode(root, DOCUMENT_MODE);
  navigate({ name: "workbench", scope: res.scope, id: res.id }, res.kind === "dir" ? undefined : { doc: tab });
}

export function LinkProvider({ children }: { children: ReactNode }) {
  const ws = useWorkspace();
  const route = useRoute();
  const toast = useToast();
  const [card, setCard] = useState<Card | null>(null);
  const reveal = useMemo(() => revealLabel(navigator.userAgent), []);
  // The clicks, in order (`latestModel`): a path's roots are described before
  // its card is drawn — an index read may be out — and the card of an earlier
  // click must never land over the card of a later one.
  const clicks = useMemo(() => createLatest(), []);

  const describe = useCallback(
    async (refs: readonly LinkRootRef[]): Promise<LinkRoot[]> =>
      Promise.all(
        refs.map(async (r) => {
          const w = r.scope === "workstream" ? ws.workstreams.find((x) => x.workstream.id === r.id) : undefined;
          // A root whose index cannot be read is still a root: an absolute path under it opens, a relative one finds nothing.
          const paths = cachedIndex(r.scope, r.id) ?? (await loadIndex(r.scope, r.id).catch(() => null));
          return { scope: r.scope, id: r.id, root: w?.path ?? null, label: rootLabel(r, w), paths: paths?.paths ?? [] };
        }),
      ),
    [ws.workstreams],
  );

  /** What each verb of a card does — the verbs themselves are the model's (`linkCardModel`). */
  const act = useCallback(
    (verb: CardVerb): (() => void) => {
      switch (verb.id) {
        case "open":
          return () => openResolvedDoc(verb.doc);
        case "open_loose":
          return () => {
            if (!openLooseFiles([verb.absolute])) toast.info(t("shell-link-handler-open-project-first-file-from-machine"));
          };
        case "reveal":
          return () =>
            void revealPath(verb.absolute).catch((e: unknown) => {
              log.warn("shell", "the file manager did not reveal a path", errorFields(e));
              toast.error(t("shell-link-handler-did-answer", { reveal }));
            });
        case "open_here":
          return () => void openUrlInBrowser(verb.url);
        case "open_machine":
          return () =>
            void openExternal(verb.url).catch((e: unknown) => {
              log.warn("shell", "the machine's browser could not be opened", errorFields(e));
              toast.error(t("shell-setup-gate-browser-did-open"));
            });
        default:
          return () => void copyText(verb.text).then((ok) => (ok ? toast.ok(copiedWords(verb.what, true)) : toast.error(copiedWords(verb.what, false))));
      }
    },
    [reveal, toast],
  );
  const show = useCallback(
    (spec: CardSpec, at: LinkPointer) => setCard({ ...spec, at }),
    [],
  );

  const handler = useMemo<LinkHandler>(
    () => ({
      onLink: (hit, at, roots, opts) => {
        const ticket = clicks.begin();
        if (hit.kind === "url") {
          show(urlCard(hit.url, { embedded: canOpenBrowser() }), at);
          return;
        }
        if (hit.kind === "doc") {
          openResolvedDoc({ kind: "doc", scope: hit.scope, id: hit.id, path: hit.path, line: hit.line, col: null, root: null, label: hit.id, indexed: true });
          return;
        }
        void describe(roots && roots.length > 0 ? roots : defaultRoots(route, ws.workstreams)).then((known) => {
          // A later click took the pointer: this one's answer is nobody's.
          if (!clicks.lands(ticket)) return;
          const res = resolveLink(hit, known);
          if (opts?.direct && res.kind === "doc") openResolvedDoc(res);
          else show(pathCard(hit, res, { reveal }), at);
        });
      },
    }),
    [clicks, describe, reveal, route, show, ws.workstreams],
  );

  const close = useCallback(() => setCard(null), []);
  const verbs: LinkCardVerb[] = useMemo(() => (card ? card.verbs.map((v) => ({ label: v.label, primary: v.primary, onSelect: act(v) })) : []), [card, act]);

  return (
    <LinkHandlerContext.Provider value={handler}>
      {children}
      {card && <LinkCard at={card.at} title={card.title} subtitle={card.subtitle} verbs={verbs} onClose={close} />}
    </LinkHandlerContext.Provider>
  );
}
