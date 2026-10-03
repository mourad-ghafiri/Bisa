/**
 * About's *Settings* view: what is saved on the project itself and so is the
 * same on every checkout — the publishing policy first, because a refused
 * push or pull request sends a person here; the project's own layer of the
 * `git.*`, `workstreams.*` and `editor.*`/`terminal.*` keys; and the scripts
 * that run around its workstreams. Every control edits one draft, and the
 * toolbar at the top saves it all (`useProjectSettingsDraft`, shared with the
 * *Checkout* view). Settings holds the workspace's values and never a
 * project's (ide/13).
 */
import { ErrorNote, SectionHeader, Skeleton } from "../../ui";
import { ProjectSettingsCard } from "./ProjectSettingsCard";
import { PublishingCard } from "./PublishingCard";
import { SettingsToolbar } from "./SettingsToolbar";
import { WorkstreamScriptsCard } from "./WorkstreamScriptsCard";
import { PROJECT_AGENT_KEYS, PROJECT_BROWSER_KEYS, PROJECT_EDITOR_KEYS, PROJECT_GIT_KEYS, PROJECT_WORKSTREAM_KEYS } from "./projectSettingsModel.mjs";
import { useProjectSettingsDraft } from "./useProjectSettingsDraft";
import type { PublishPolicy } from "../../types";
import { t } from "../../i18n/l10n.mjs";

export function ProjectSettingsView({ pid, wid }: { pid: string; wid: string }) {
  const draft = useProjectSettingsDraft(pid, wid);
  const problem = Object.values(draft.problems)[0] ?? null;
  return (
    <div className="flex flex-col gap-6">
      <SettingsToolbar
        count={draft.count}
        valid={draft.valid}
        saving={draft.saving}
        problem={problem}
        onSave={() => void draft.save()}
        onDiscard={draft.discard}
        awaitingApproval={draft.awaitingApproval}
        onApprove={() => void draft.approve()}
      />
      {draft.error && !draft.current && <ErrorNote error={draft.error} retry={draft.reload} />}
      <section>
        <SectionHeader flush title={t("work-new-project-dialog-publishing")} />
        {draft.current ? (
          <PublishingCard policy={draft.current.publish as PublishPolicy} drafted={(draft.draft.publish as PublishPolicy | null) ?? null} onEdit={draft.editPublish} />
        ) : (
          <Skeleton className="h-10 w-full" />
        )}
      </section>
      <section>
        <SectionHeader flush title={t("work-project-settings-view-git")} />
        <ProjectSettingsCard keys={PROJECT_GIT_KEYS} intro={t("work-project-settings-view-how-project-s-branches-move-merge")} draft={draft} />
      </section>
      <section>
        <SectionHeader flush title={t("work-project-settings-view-workstreams")} />
        <ProjectSettingsCard keys={PROJECT_WORKSTREAM_KEYS} intro={t("work-project-settings-view-how-project-s-checkouts-cleaned-up")} draft={draft} />
      </section>
      <section>
        <SectionHeader flush title={t("work-project-settings-view-workstream-scripts")} />
        <WorkstreamScriptsCard draft={draft} />
      </section>
      <section>
        <SectionHeader flush title={t("work-project-settings-view-browser-devices")} />
        <ProjectSettingsCard keys={PROJECT_BROWSER_KEYS} intro={t("work-project-settings-view-where-browser-tab-project-opens-what")} draft={draft} />
      </section>
      <section>
        <SectionHeader flush title={t("work-project-settings-view-agents-decisions")} />
        <ProjectSettingsCard keys={PROJECT_AGENT_KEYS} intro={t("work-project-settings-view-mode-conversation-about-one-project-s")} draft={draft} />
      </section>
      <section>
        <SectionHeader flush title={t("work-project-settings-view-editor-terminal")} />
        <ProjectSettingsCard keys={PROJECT_EDITOR_KEYS} intro={t("work-project-settings-view-how-files-project-edited-which-harness")} draft={draft} />
      </section>
    </div>
  );
}
