/**
 * The aux pane's artifact occupant (ide/12): one artifact, whole, from
 * `?aux=artifact&auxId=<message>:<ordinal>` — loaded by its message, so the
 * pane opens beside any screen and survives Back and a reload — with the
 * host's own verbs (*Expand*, *Open as a tab* in the IDE) and, folded under
 * it, **In this conversation**: every artifact of the scope grouped by title
 * with its versions, newest first. `?stage=1` opens it over the window.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { errorFields, log } from "../log";
import { setSearch, useRoute, useSearchValue } from "../router";
import type { ArtifactListRow, ArtifactRef, MessageRow } from "../types";
import {
  ArtifactStage,
  ArtifactView,
  Button,
  ErrorNote,
  ICON,
  RelativeTime,
  Spinner,
  Tooltip,
  artifactKey,
  artifactVersions,
  kindWords,
  parseArtifactKey,
  useCollapsed,
  versionLabel,
  versionWords,
} from "../ui";
import { openDoc } from "../views/_workbench/workbenchStore";
import { openArtifactInBrowser } from "./browserDoors";
import { canOpenBrowser } from "./useBrowsers";
import { rootKey, tabId } from "../views/_workbench/workbenchModel.mjs";
import { useArtifactLibraries } from "./artifactSettings";
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";

export function ArtifactPane({ artifactKey: key }: { artifactKey: string | null }) {
  const at = useMemo(() => parseArtifactKey(key), [key]);
  const [message, setMessage] = useState<MessageRow | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [gallery, setGallery] = useState<ArtifactListRow[]>([]);
  const [stage, setStage] = useSearchValue("stage");
  const route = useRoute();
  const ws = useWorkspace();
  const libraries = useArtifactLibraries();
  const [folded, toggleFolded] = useCollapsed("artifact.gallery");

  useEffect(() => {
    if (!at) return;
    const ac = new AbortController();
    setMessage(null);
    setError(null);
    api
      .message(at.message, ac.signal)
      .then((r) => setMessage(r.message))
      .catch((e) => !ac.signal.aborted && setError(e instanceof Error ? e.message : String(e)));
    return () => ac.abort();
  }, [at]);

  const scope = message?.scope_id ?? null;
  // Another conversation's artifacts are never listed under this one: the
  // gallery empties when the conversation changes, and fills from its read.
  useEffect(() => setGallery([]), [scope]);
  useEffect(() => {
    if (!scope) return;
    const ac = new AbortController();
    api
      .artifactsOf(scope, 100, ac.signal)
      .then((r) => setGallery(r.artifacts))
      .catch((e: unknown) => log.debug("view", "this conversation's artifacts did not arrive", { scope, ...errorFields(e) }));
    return () => ac.abort();
  }, [scope, message?.id]);

  const artifact = at ? message?.artifacts?.[at.ordinal] ?? null : null;
  const groups = useMemo(() => artifactVersions(gallery), [gallery]);
  const request = useCallback(async () => {
    if (!artifact) return;
    await api.fetchAttachment(artifact.sha256);
    const r = await api.message(artifact ? at!.message : "");
    setMessage(r.message);
  }, [artifact, at]);

  const inWorkbench = route.name === "workbench";
  const openAsTab = () => {
    if (!inWorkbench || !at || !artifact) return;
    const tab = { kind: "artifact" as const, message: at.message, ordinal: at.ordinal, title: artifact.title };
    openDoc(rootKey(route.scope, route.id), tab);
    setSearch({ doc: tabId(tab), aux: null, auxId: null, stage: null });
  };

  // The stage's neighbours: the conversation's artifacts in order.
  const flat = gallery;
  const here = artifact ? flat.findIndex((r) => r.message_id === at!.message && r.sha256 === artifact.sha256 && r.title === artifact.title) : -1;
  const go = (row: ArtifactListRow | undefined) => {
    if (!row) return;
    const ordinal = ordinalOf(row, message);
    setSearch({ aux: "artifact", auxId: artifactKey(row.message_id, ordinal) });
  };

  if (!at) return <p className="p-3 text-2xs text-text-dim">{t("shell-artifact-pane-nothing-show-address-names-artifact")}</p>;
  if (error) {
    return (
      <div className="p-3">
        <ErrorNote error={error} />
      </div>
    );
  }
  if (!message) {
    return (
      <div className="p-3">
        <Spinner label={t("shell-artifact-pane-loading")} />
      </div>
    );
  }
  if (!artifact) return <p className="p-3 text-2xs text-text-dim">{t("shell-artifact-pane-message-carries-such-artifact")}</p>;

  const author = ws.nameOf(message.author);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <ArtifactView
        artifact={artifact}
        present={artifact.present}
        onRequest={request}
        libraries={libraries}
        onOpenInBrowser={canOpenBrowser() ? (a) => void openArtifactInBrowser(a) : null}
        className="min-h-0 flex-1"
        actions={
          <>
            {inWorkbench && (
              <Tooltip label={t("shell-artifact-pane-open-document-tab-centre")}>
                <Button size="sm" variant="ghost" onClick={openAsTab} aria-label={t("shell-artifact-pane-open-tab")}>
                  <ICON.file size={13} aria-hidden />
                </Button>
              </Tooltip>
            )}
            <Tooltip label={t("shell-artifact-pane-fill-window-esc-come-back")}>
              <Button size="sm" variant="ghost" onClick={() => setStage("1")} aria-label={t("shell-artifact-pane-expand")}>
                <ICON.expand size={13} aria-hidden />
              </Button>
            </Tooltip>
          </>
        }
      />
      <p className="shrink-0 border-t border-border px-3 py-1 text-2xs text-text-dim">
        {author} · <RelativeTime at={message.created_at} />
      </p>
      <section className="shrink-0 border-t border-border">
        <button
          type="button"
          onClick={toggleFolded}
          aria-expanded={!folded}
          className="anim flex w-full items-center gap-1.5 px-3 py-1.5 text-left text-2xs font-semibold text-text hover:bg-surface-2"
        >
          {folded ? <ICON.collapsed size={12} aria-hidden /> : <ICON.expanded size={12} aria-hidden />}
          {t("shell-artifact-pane-in-this-conversation")}
          <span className="tnum font-normal text-text-dim">{gallery.length}</span>
        </button>
        {!folded && (
          <ul className="max-h-56 overflow-auto px-2 pb-2">
            {groups.map((g) => (
              <li key={g.title} className="py-1">
                <GalleryRow row={g.latest} count={g.versions.length} current={artifact} onOpen={() => go(g.latest)} />
                {g.versions.length > 1 && (
                  <ul className="ml-5 border-l border-border pl-2">
                    {g.versions.map((v, i) => (
                      <li key={`${v.message_id}:${v.sha256}`}>
                        <button
                          type="button"
                          onClick={() => go(v)}
                          className={`anim flex w-full items-center gap-2 rounded-control px-1 py-0.5 text-left text-2xs hover:bg-surface-2 ${v.sha256 === artifact.sha256 && v.message_id === at.message ? "text-text" : "text-text-dim"}`}
                        >
                          <span className="tnum w-6">{versionLabel(i, g.versions.length)}</span>
                          <span className="min-w-0 flex-1 truncate">{ws.nameOf(v.author)}</span>
                          <RelativeTime at={v.created_at} className="tnum" />
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
      {stage === "1" && (
        <ArtifactStage
          artifact={artifact}
          present={artifact.present}
          onClose={() => setStage(null)}
          onRequest={request}
          libraries={libraries}
          onPrev={here > 0 ? () => go(flat[here - 1]) : null}
          onNext={here >= 0 && here < flat.length - 1 ? () => go(flat[here + 1]) : null}
        />
      )}
    </div>
  );
}

/** The ordinal a gallery row has on its own message: the loaded message when it is that one, else the first that matches. */
function ordinalOf(row: ArtifactListRow, message: MessageRow | null): number {
  if (message && message.id === row.message_id) {
    const i = (message.artifacts ?? []).findIndex((a) => a.sha256 === row.sha256 && a.title === row.title);
    if (i >= 0) return i;
  }
  return 0;
}

function GalleryRow({ row, count, current, onOpen }: { row: ArtifactListRow; count: number; current: ArtifactRef; onOpen: () => void }) {
  const words = kindWords(row.kind);
  const Glyph = (ICON as Record<string, typeof ICON.file>)[words.glyph] ?? ICON.file;
  const isCurrent = row.title === current.title;
  return (
    <button
      type="button"
      onClick={onOpen}
      className={`anim flex w-full items-center gap-2 rounded-control px-1 py-1 text-left text-2xs hover:bg-surface-2 ${isCurrent ? "text-text" : "text-text-dim"}`}
    >
      <Glyph size={12} aria-hidden className="shrink-0" />
      <span className="min-w-0 flex-1 truncate font-medium">{row.title}</span>
      {count > 1 && (
        <span className="inline-flex items-center gap-1 text-text-dim">
          <ICON.versions size={11} aria-hidden />
          {versionWords(count)}
        </span>
      )}
      {!row.present && <span className="text-text-dim">{t("shell-artifact-pane-here")}</span>}
    </button>
  );
}
