/**
 * "Who commits in <project>?" — the one dialog for a repository nobody is set
 * to commit in: a project just created with no identity resolving
 * anywhere, or a commit refused for the same reason. Mounted once in the
 * shell; `committerPromptModel.mjs` owns the queue, seeded from
 * `GET /git/committer` and moved by `committer_needed` / `committer_set`. A
 * project that inherits a global identity never gets here.
 *
 * The answer is the repository's **local** git config — the identity first,
 * the rest of the schema under *More* — written through
 * `PUT /workstreams/{wid}/git/config`. *Also save as my global git config*
 * writes the name and email to the global layer too, so the next project
 * inherits them. *Not now* skips for this session; the engine asks again next
 * launch — its desk is read from git at every start (`identity::rearm`).
 *
 * Mounted under its own `OverlayBoundary`: a throw here costs the dialog,
 * never the window. The queue is seeded again when the bus comes back —
 * a stream that lagged is closed by the node and nothing is re-sent — and
 * the form reads the config view's schema through the same guard every
 * other git-config form does, so a view with none draws an empty form.
 */
import { useCallback, useEffect, useState } from "react";
import { api } from "../../api";
import { useEngineEvents, watchConnection } from "../../bus";
import { reloadOnReconnect } from "../../shell/workspaceLoadModel.mjs";
import type { GitConfigEntry, GitConfigKey, GitConfigView } from "../../types";
import { Button, Checkbox, Dialog, ErrorNote, SkeletonRows, useToast } from "../../ui";
import { emptyQueue, head, onFrame, remaining, remove, seed, skip } from "./committerPromptModel.mjs";
import type { Queue } from "./committerPromptModel.mjs";
import { GitConfigForm, useConfigEdits } from "./GitConfigForm";
import { formatIdent } from "./gitConfigModel.mjs";
import { committerReasonSentence } from "./gitIdentityModel.mjs";
import { attempt, useAsync } from "./useAsync";
import { t as tr } from "../../i18n/l10n.mjs";

const IDENTITY_KEYS = ["user.name", "user.email", "user.useConfigOnly"] as const;
const NO_KEYS: readonly GitConfigKey[] = Object.freeze([]);
const NO_ENTRIES: readonly GitConfigEntry[] = Object.freeze([]);

export function ProjectGitDialog() {
  const [queue, setQueue] = useState<Queue>(emptyQueue);
  const ask = head(queue);

  // What the engine was already asking when this window opened.
  const refresh = useCallback(async () => {
    try {
      const o = await api.gitCommitter();
      setQueue((q) => seed(q, o.pending, o.global ?? null));
    } catch {
      // No overview, no seed: the frames still arrive.
    }
  }, []);
  useEffect(() => {
    void refresh();
    return reloadOnReconnect(watchConnection, () => void refresh());
  }, [refresh]);

  useEngineEvents((e) => {
    const t = e.payload.type;
    if (t === "committer_needed" || t === "committer_set") setQueue((q) => onFrame(q, e.payload));
  });

  if (!ask) return null;
  return (
    <AskBody
      key={ask.project}
      ask={ask}
      more={remaining(queue)}
      onAnswered={() => setQueue((q) => remove(q, ask.project))}
      onSkip={() => setQueue((q) => skip(q, ask.project))}
    />
  );
}

function AskBody({
  ask,
  more,
  onAnswered,
  onSkip,
}: {
  ask: NonNullable<ReturnType<typeof head>>;
  more: number;
  onAnswered: () => void;
  onSkip: () => void;
}) {
  const config = useAsync((s) => api.workstreamGitConfig(ask.workstream, s), [ask.workstream]);
  return (
    <Dialog
      open
      onClose={onSkip}
      title={tr("work-project-git-dialog-who-commits", { slug: ask.slug })}
      description={committerReasonSentence(ask.reason, ask.slug, ask.origin)}
      footer={
        <>
          {more > 0 && <span className="mr-auto self-center text-2xs text-text-dim">{tr("work-project-git-dialog-more-waiting", { more })}</span>}
          <Button size="sm" variant="ghost" onClick={onSkip}>{tr("work-project-git-dialog-not-now")}</Button>
        </>
      }
    >
      {config.loading && !config.data && <SkeletonRows rows={3} />}
      {config.error && !config.data && <ErrorNote error={config.error} retry={config.reload} />}
      {config.data && <AskForm view={config.data} workstream={ask.workstream} slug={ask.slug} onAnswered={onAnswered} />}
    </Dialog>
  );
}

function AskForm({ view, workstream, slug, onAnswered }: { view: GitConfigView; workstream: string; slug: string; onAnswered: () => void }) {
  const toast = useToast();
  const form = useConfigEdits(view.schema ?? NO_KEYS, view.entries ?? NO_ENTRIES, "local");
  const [more, setMore] = useState(false);
  const [alsoGlobal, setAlsoGlobal] = useState(false);
  const [busy, setBusy] = useState(false);
  const name = (form.edits["user.name"] ?? "").trim();
  const email = (form.edits["user.email"] ?? "").trim();
  const identityReady = name !== "" && email !== "" && form.valid;

  const submit = async () => {
    if (!identityReady || busy) return;
    setBusy(true);
    const ok = await attempt(
      async () => {
        await api.setWorkstreamGitConfig(workstream, form.write);
        if (alsoGlobal) await api.setGitConfig({ set: { "user.name": name, "user.email": email }, unset: [] });
      },
      toast.error,
    );
    setBusy(false);
    if (ok) {
      toast.ok(tr("work-project-git-dialog-commits-now-authored-by", { slug, ident: formatIdent({ name, email }) }));
      onAnswered();
    }
  };

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <GitConfigForm form={form} scope="local" only={more ? undefined : IDENTITY_KEYS} autoFocus />
      {!more && (
        <button type="button" className="self-start text-2xs text-text-dim underline decoration-dotted underline-offset-2 hover:text-text" onClick={() => setMore(true)}>{tr("work-new-project-dialog-more-git-config")}</button>
      )}
      <p className="text-2xs text-text-dim">{tr("work-project-git-dialog-written-repository-s-own-git-config")}</p>
      <Checkbox
        label={tr("work-project-git-dialog-also-save-name-email-my-global")}
        hint={tr("work-project-git-dialog-so-next-project-inherits-them-instead")}
        checked={alsoGlobal}
        onChange={setAlsoGlobal}
      />
      <div>
        <Button size="sm" type="submit" variant="primary" disabled={busy || !identityReady}>{tr("work-project-git-dialog-set-repository")}</Button>
      </div>
    </form>
  );
}
