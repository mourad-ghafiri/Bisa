/**
 * An artifact as a document in the workbench's centre (ide/12): the
 * `{kind: "artifact"}` tab, loaded by its message like the aux pane's
 * occupant, drawn whole in the tab's frame.
 */

import { useEffect, useState } from "react";
import { openArtifactInBrowser } from "../../shell/browserDoors";
import { canOpenBrowser } from "../../shell/useBrowsers";
import { api } from "../../api";
import type { MessageRow } from "../../types";
import { ArtifactView, EmptyState, ErrorNote, Spinner, failureText } from "../../ui";
import { useArtifactLibraries } from "../../shell/artifactSettings";
import { t } from "../../i18n/l10n.mjs";

export function ArtifactDocument({ message: id, ordinal }: { message: string; ordinal: number }) {
  const [message, setMessage] = useState<MessageRow | null>(null);
  const [error, setError] = useState<string | null>(null);
  const libraries = useArtifactLibraries();
  useEffect(() => {
    const ac = new AbortController();
    setMessage(null);
    setError(null);
    api
      .message(id, ac.signal)
      .then((r) => setMessage(r.message))
      .catch((e) => !ac.signal.aborted && setError(failureText("workbench", "artifact-document-failed", e)));
    return () => ac.abort();
  }, [id]);
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
        <Spinner label={t("workbench-artifact-document-loading")} />
      </div>
    );
  }
  const artifact = message.artifacts?.[ordinal];
  if (!artifact) return <EmptyState title={t("workbench-artifact-document-no-such-artifact")} hint={t("workbench-artifact-document-message-tab-points-carries-nothing-place")} action={null} />;
  return (
    <ArtifactView
      artifact={artifact}
      present={artifact.present}
      libraries={libraries}
      onOpenInBrowser={canOpenBrowser() ? (a) => void openArtifactInBrowser(a) : null}
      onRequest={async () => {
        await api.fetchAttachment(artifact.sha256);
        const r = await api.message(id);
        setMessage(r.message);
      }}
      className="h-full"
    />
  );
}
