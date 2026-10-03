/**
 * Settings › Git & code hosts › Identity — *Your global git config*: the person's
 * global layer, read from git and written at their request through the
 * schema keys' one write. What is set is shown; what is not can be set. Every
 * new repository inherits this layer unless its own config or a profile by
 * organization says otherwise (`GitProfilesPanel`), and a repository is asked
 * who commits only when nothing resolves anywhere.
 *
 * No per-project table: a project's config is the project's, edited in its
 * Git tab. The form is `GitConfigForm` over the schema the node serves.
 */
import { api } from "../../api";
import { useEngineEvents } from "../../bus";
import { useState } from "react";
import { Card, Chip, ErrorNote, ICON, Pending, Section, useToast } from "../../ui";
import { GitConfigForm, useConfigEdits } from "../_work/GitConfigForm";
import { formatIdent, globalIdentityOf } from "../_work/gitConfigModel.mjs";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows } from "./loadModel.mjs";
import { SaveFooter } from "./SaveFooter";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function GlobalGitPanel() {
  const config = useAsync((s) => api.gitConfig(s), []);
  useEngineEvents((e) => {
    if (e.payload.type === "committer_set") config.reload();
  });
  if (config.loading && !config.data) return <Pending what={t("settings-global-git-panel-global-git-config-2")} rows={pendingRows(t("settings-global-git-panel-global-git-config-2"))} />;
  if (config.error && !config.data) return <ErrorNote error={config.error} retry={config.reload} />;
  return <GlobalGitForm view={config.data!} onSaved={config.reload} />;
}

function GlobalGitForm({ view, onSaved }: { view: import("../../types").GitConfigView; onSaved: () => void }) {
  const toast = useToast();
  const form = useConfigEdits(view.schema, view.entries, "global");
  const identity = globalIdentityOf(view.entries);
  const [saving, setSaving] = useState(false);

  const save = async () => {
    if (!form.dirty || !form.valid || saving) return;
    setSaving(true);
    await attempt(() => api.setGitConfig(form.write), toast.error, () => {
      toast.ok(t("settings-global-git-panel-global-git-config-saved"));
      form.reset();
      onSaved();
    });
    setSaving(false);
  };

  return (
    <Section
      title={
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h2 className="text-sm font-semibold text-text">{t("settings-global-git-panel-global-git-config")}</h2>
          {identity ? (
            <>
              <Chip tone="ok" icon={ICON.ok}>{t("settings-global-git-panel-set")}</Chip>
              <span className="min-w-0 truncate font-mono text-2xs text-text-dim">{formatIdent(identity)}</span>
            </>
          ) : (
            <Chip tone="warn" icon={ICON.warn}>{t("settings-global-git-panel-identity-set")}</Chip>
          )}
        </div>
      }
    >
      <Card className="flex flex-col gap-3">
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
          {rich("settings-global-git-panel-read-from-git-global-config", { setting: <em>{t("settings-global-git-panel-who-commits-new-repository")}</em> })}
          {!identity && ` ${t("settings-global-git-panel-set-name-email-once-new-projects")}`}
        </p>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          {/* The default code host account is the GitHub panel's Select, not a text field here. */}
          <GitConfigForm form={form} scope="global" omit={["codehost.account"]} />
          <SaveFooter
            form="global-git"
            submit
            dirty={form.dirty}
            saving={saving}
            canSave={form.valid}
            blockedReason={t("settings-save-footer-fix-first")}
            saveLabel={t("settings-global-git-panel-save-global-config")}
            onDiscard={form.reset}
          />
        </form>
      </Card>
    </Section>
  );
}
