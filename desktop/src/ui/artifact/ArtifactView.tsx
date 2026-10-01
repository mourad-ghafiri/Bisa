/**
 * One artifact, whole (ide/12): the toolbar — its title, its kind, its size,
 * the verbs — and, under it, the renderer its kind picks. The viewer the aux
 * pane, an IDE tab and the stage all draw; the card under a message draws a
 * smaller thing.
 *
 * The verbs are the same everywhere: **Save as…** (the save dialog, then the
 * shell copies the named copy there), **Reveal in Finder** (the platform's
 * word), **Open with the default app**, *Copy* for a text kind, *Open in the
 * IDE* when the artifact was written inside a checkout, *View as page* for a
 * figure, *Source* for a page. The named copy is asked of the node once per
 * hash and remembered.
 */

import { useCallback, useMemo, useState, type ReactNode } from "react";
import { api, openArtifactFile, revealPath, saveArtifactCopy, inDesktopShell } from "../../api";
import type { ArtifactRef } from "../../types";
import { Button } from "../Button";
import { Spinner } from "../Card";
import { Chip } from "../Chip";
import { copyText } from "../clipboard";
import { cn } from "../cn";
import { revealLabel } from "../fileTreeMutations.mjs";
import { ICON } from "../icons";
import { useLinkHandler } from "../linkContext";
import { Menu } from "../Menu";
import { useToast } from "../Toast";
import { Tooltip } from "../Tooltip";
import { bytesWords, isTextKind, kindWords } from "./artifactModel.mjs";
import { loadArtifactBytes, textOf, useArtifactBytes } from "./artifactBytes";
import { KindView } from "./KindView";
import { TextView } from "./TextView";
import { t } from "../../i18n/l10n.mjs";

const named = new Map<string, Promise<string>>();

/** The blob under its name, asked once per hash. */
function namedCopy(artifact: ArtifactRef): Promise<string> {
  const key = `${artifact.sha256}:${artifact.name}`;
  let p = named.get(key);
  if (!p) {
    p = api.attachmentFile(artifact.sha256, artifact.name).then((r) => r.path);
    named.set(key, p);
  }
  return p;
}

export function ArtifactView({
  artifact,
  present,
  onRequest,
  libraries = true,
  actions,
  className,
  fill = true,
  onOpenInBrowser,
}: {
  artifact: ArtifactRef;
  present: boolean;
  /** Ask a peer for the bytes; the existing fetch door. */
  onRequest?: () => Promise<void>;
  /** The host's door to the embedded browser (ide/18): a page or a figure opened with an origin of its own. Absent where there is none. */
  onOpenInBrowser?: ((artifact: ArtifactRef) => void) | null;
  /** Whether a page may load libraries from the CDNs (`artifacts.html.libraries`). */
  libraries?: boolean;
  /** The host's own verbs — *Open as a tab*, *Expand*, the versions — drawn beside the artifact's. */
  actions?: ReactNode;
  className?: string;
  /** Whether the body takes the whole height (the pane) or its content's (a card). */
  fill?: boolean;
}) {
  const toast = useToast();
  const link = useLinkHandler();
  const bytes = useArtifactBytes(artifact.sha256, artifact.mime, present);
  const [asSource, setAsSource] = useState(false);
  const [facts, setFacts] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const words = kindWords(artifact.kind);
  const Glyph = (ICON as Record<string, typeof ICON.file>)[words.glyph] ?? ICON.file;
  const shell = inDesktopShell();
  const reveal = useMemo(() => revealLabel(navigator.userAgent), []);
  const text = useMemo(() => (bytes.state === "ready" && isTextKind(artifact.kind) ? textOf(bytes.bytes) : null), [bytes, artifact.kind]);
  const onFacts = useCallback((w: string) => setFacts(w), []);

  const fail = (e: unknown) => toast.error(e instanceof Error ? e.message : String(e));
  const save = async () => {
    try {
      const from = shell ? await namedCopy(artifact) : "";
      const path = await saveArtifactCopy(from, artifact.name, () => loadArtifactBytes(artifact.sha256, artifact.mime).then((l) => l.bytes), artifact.mime);
      if (path) toast.ok(t("ui-mermaid-view-saved", { path }));
    } catch (e) {
      fail(e);
    }
  };
  const revealIt = async () => {
    try {
      await revealPath(await namedCopy(artifact));
    } catch (e) {
      fail(e);
    }
  };
  const openWith = async () => {
    try {
      await openArtifactFile(await namedCopy(artifact));
    } catch (e) {
      fail(e);
    }
  };
  const copy = async () => {
    if (text === null) return;
    (await copyText(text)) ? toast.ok(t("ui-field-copied")) : toast.error(t("ui-artifact-view-text-could-not-copied"));
  };
  const request = async () => {
    if (!onRequest) return;
    setAsking(true);
    try {
      await onRequest();
    } catch (e) {
      fail(e);
    } finally {
      setAsking(false);
    }
  };

  // An artifact written inside a root the IDE opens has a door there; one in
  // a run in the workspace's folder has none — the IDE roots at no run.
  const source = artifact.source ?? null;
  const ideScope = source && source.scope !== "run" ? source.scope : null;
  const more = [
    ...(text !== null ? [{ label: t("ui-artifact-view-copy-text"), icon: ICON.copy, onSelect: () => void copy() }] : []),
    ...(shell ? [{ label: reveal, icon: ICON.reveal, onSelect: () => void revealIt() }, { label: t("ui-artifact-view-open-default-app"), icon: ICON.file, onSelect: () => void openWith() }] : []),
    ...(source && ideScope && link
      ? [
          {
            label: t("ui-artifact-view-open-ide"),
            icon: ICON.file,
            separatorBefore: true,
            onSelect: () => link.onLink({ kind: "doc", scope: ideScope, id: source.id, path: source.path, line: null }, { x: 0, y: 0 }, null),
          },
        ]
      : []),
    ...(artifact.kind === "html" || artifact.kind === "svg"
      ? [{ label: asSource ? (artifact.kind === "html" ? t("ui-artifact-view-view-page") : t("ui-artifact-view-view-figure")) : t("ui-artifact-view-view-source"), icon: ICON.code, separatorBefore: true, onSelect: () => setAsSource((s) => !s) }]
      : []),
    ...((artifact.kind === "html" || artifact.kind === "svg") && onOpenInBrowser
      ? [{ label: t("ui-artifact-view-open-browser"), icon: ICON.page, onSelect: () => onOpenInBrowser(artifact) }]
      : []),
  ];

  const openWithVerb = shell ? (
    <Button size="sm" variant="ghost" onClick={() => void openWith()}>{t("ui-artifact-view-open-default-app")}</Button>
  ) : null;

  let body: ReactNode;
  if (bytes.state === "absent") {
    body = (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-2xs text-text-dim">
        <p>{t("ui-artifact-view-bytes-not-machine-yet")}</p>
        {onRequest && (
          <Button size="sm" variant="ghost" disabled={asking} onClick={() => void request()}>
            {asking ? t("ui-artifact-card-asking") : t("ui-artifact-card-request")}
          </Button>
        )}
      </div>
    );
  } else if (bytes.state === "loading") {
    body = (
      <div className="p-3">
        <Spinner label={t("ui-card-loading")} />
      </div>
    );
  } else if (bytes.state === "failed") {
    body = <p className="p-3 text-2xs text-danger">{bytes.error}</p>;
  } else if (asSource && text !== null) {
    body = <TextView artifact={{ ...artifact, kind: "code" }} text={text} />;
  } else {
    // The one renderer per kind, shared with the IDE's rendered documents.
    body = (
      <KindView
        kind={artifact.kind}
        name={artifact.name}
        title={artifact.title}
        mime={artifact.mime}
        size={artifact.size}
        bytes={{ bytes: bytes.bytes, url: bytes.url }}
        text={text}
        libraries={libraries}
        onFacts={onFacts}
        openWith={openWithVerb}
      />
    );
  }

  return (
    <div className={cn("flex min-h-0 flex-col", fill && "h-full", className)}>
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-2">
        <Glyph size={13} aria-hidden className="shrink-0 text-text-dim" />
        <Tooltip label={`${artifact.name} · ${bytesWords(artifact.size)}`}>
          <span className="min-w-0 flex-1 truncate text-xs font-semibold text-text">{artifact.title}</span>
        </Tooltip>
        <Chip tone="quiet">{words.label}</Chip>
        {facts && <span className="tnum shrink-0 text-2xs text-text-dim">{facts}</span>}
        {actions}
        {bytes.state === "ready" && (
          <Tooltip label={t("ui-artifact-view-save-copy-where-choose")}>
            <Button size="sm" variant="ghost" onClick={() => void save()} aria-label={t("ui-artifact-view-save")}>
              <ICON.save size={13} aria-hidden />
            </Button>
          </Tooltip>
        )}
        {more.length > 0 && bytes.state === "ready" && (
          <Menu
            items={more}
            trigger={
              <span aria-label={t("ui-artifact-view-more")} className="anim inline-flex rounded-control px-1 py-0.5 text-text-dim hover:bg-surface-2 hover:text-text">
                <ICON.more size={13} aria-hidden />
              </span>
            }
          />
        )}
      </div>
      <div className={cn("min-h-0", fill ? "flex-1" : "max-h-96")}>{body}</div>
    </div>
  );
}
