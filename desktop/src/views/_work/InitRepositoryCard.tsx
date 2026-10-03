/**
 * *Initialise a repository* — the one card every plain-folder surface draws
 * (ide/04 the Git panel, ide/07 the Workstreams panel, ide/13 About ›
 * Checkout): the sentence, what the click does, and the button. For an
 * adopted folder a confirm comes first — adopt writes nothing into a folder
 * Bisa did not make, so this is the one write, asked about. The card owns
 * the action: it posts, toasts, tells the rail's marks, and hands the fresh
 * project to `onDone` so the caller reloads what it shows; the bus's
 * `project_changed` and the primary's `workstream_changed` reach every other
 * mounted surface. The words are `initRepositoryModel`'s.
 */
import { useState } from "react";
import { api } from "../../api";
import type { Project } from "../../types";
import { refreshWorkstreamStatuses } from "../../shell/workstreamStatusStore";
import { Button, Card, ConfirmDialog, ErrorNote, GitMark, sayFailure, useToast } from "../../ui";
import { INIT_LABEL, PLAIN_FOLDER, initConsequence, initDoneWords } from "./initRepositoryModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function InitRepositoryCard({
  pid,
  adopted,
  path,
  onDone,
}: {
  pid: string;
  /** An external root — somebody's folder, adopted where it lies. */
  adopted: boolean;
  /** The folder's absolute path, for the confirm's words; null when unknown. */
  path: string | null;
  onDone?: (project: Project) => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const consequence = initConsequence({ adopted, path });

  const run = async () => {
    setAsking(false);
    setBusy(true);
    setError(null);
    try {
      const done = await api.initRepository(pid);
      toast.ok(initDoneWords(done.committer));
      // The primary's frame reaches the store too; told here as well so the
      // rail's mark turns with the click, not a hop later.
      refreshWorkstreamStatuses();
      onDone?.(done.project.project);
    } catch (e) {
      setError(sayFailure("git", t("work-init-repository-card-could-not-init"), e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="flex flex-col gap-2">
      <p className="text-xs text-text">{PLAIN_FOLDER}</p>
      <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{consequence.body}</p>
      {error !== null && <ErrorNote error={error} retry={() => void run()} />}
      <div>
        <Button variant="primary" size="sm" disabled={busy} onClick={() => (consequence.needsConfirm ? setAsking(true) : void run())}>
          <GitMark size={12} aria-hidden />
          {busy ? t("work-init-repository-card-initialising") : INIT_LABEL}
        </Button>
      </div>
      <ConfirmDialog open={asking} onClose={() => setAsking(false)} onConfirm={() => void run()} title={INIT_LABEL} body={consequence.body} confirmLabel={INIT_LABEL} />
    </Card>
  );
}
