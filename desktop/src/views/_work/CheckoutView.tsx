/**
 * About's *Checkout* view: this checkout as it reaches its repository — what
 * it will use when it talks to its remote (`ConnectionCard`), what git thinks
 * of it (`GitFacts`), its remotes (`RemoteCard`, edited row by row), and who
 * commits here (`GitConfigCard`, the local git config). The account pin and
 * the git config are a draft the toolbar at the top saves
 * (`useProjectSettingsDraft`, shared with the *Settings* view). A plain
 * folder shows the one card that offers to make it a repository
 * (`InitRepositoryCard`); a folder not on disk, the facts alone.
 */
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { ErrorNote, SectionHeader, SkeletonRows } from "../../ui";
import { ConnectionCard } from "./ConnectionCard";
import { GitConfigCard } from "./GitConfigCard";
import { GitFacts } from "./ProjectDetail";
import { InitRepositoryCard } from "./InitRepositoryCard";
import { RemoteCard } from "./RemoteCard";
import { SettingsToolbar } from "./SettingsToolbar";
import { useAsync } from "./useAsync";
import { useProjectSettingsDraft } from "./useProjectSettingsDraft";
import { t } from "../../i18n/l10n.mjs";

export function CheckoutView({ pid, wid }: { pid: string; wid: string }) {
  const draft = useProjectSettingsDraft(pid, wid);
  const status = useAsync((s) => api.workstreamGitStatus(wid, s), [wid]);
  useEngineEvents((e) => {
    if (e.payload.type === "git_setup_changed" || e.payload.type === "committer_set") status.reload();
    if (e.payload.type === "project_changed" && e.payload.project === pid) changed();
  });
  const changed = () => {
    status.reload();
    draft.reload();
  };
  if (status.error && !status.data) return <ErrorNote error={status.error} retry={status.reload} />;
  if (!status.data) return <SkeletonRows rows={4} />;
  const git = status.data.status.git === true && status.data.status.exists === true;
  const problem = Object.values(draft.problems)[0] ?? null;
  return (
    <div className="flex flex-col gap-4">
      {git && <SettingsToolbar count={draft.count} valid={draft.valid} saving={draft.saving} problem={problem} whole={draft.whole} onSave={() => void draft.save()} onDiscard={draft.discard} />}
      {git && draft.error && <ErrorNote error={draft.error} retry={draft.reload} />}
      {git ? (
        <>
          <section>
            <SectionHeader title={t("work-checkout-view-connection")} />
            <ConnectionCard wid={wid} draft={draft} />
          </section>
          <section>
            <SectionHeader title={t("work-checkout-view-repository")} />
            <GitFacts status={status.data.status} defaultBranch={status.data.default_branch} />
          </section>
          <section>
            <SectionHeader title={t("work-checkout-view-remotes")} />
            <RemoteCard wid={wid} onChanged={changed} />
          </section>
          <section>
            <SectionHeader title={t("work-checkout-view-git-config")} />
            <GitConfigCard draft={draft} />
          </section>
        </>
      ) : (
        <section>
          <SectionHeader title={t("work-checkout-view-repository")} />
          {status.data.status.exists ? <InitRepositoryCard pid={pid} adopted={draft.project?.project.root.type === "external"} path={draft.project?.path ?? status.data.path} onDone={changed} /> : <GitFacts status={status.data.status} defaultBranch={status.data.default_branch} />}
        </section>
      )}
    </div>
  );
}
