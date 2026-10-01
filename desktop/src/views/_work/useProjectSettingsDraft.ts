/**
 * The one draft behind About's *Checkout* and *Settings* views (ide/04): what
 * the node holds for this project and this checkout — the record's policy,
 * the resolved settings, the scripts, the connection's pin, the local git
 * config — and what a person changed on top, in session memory, until the
 * toolbar's Save writes it all. The rules are `projectSettingsDraftModel.mjs`'s;
 * this hook only reads, keeps and applies.
 *
 * One hook for both views, keyed by the project, so a change made under
 * *Settings* still counts when the person switches to *Checkout* and saves
 * there: the draft is the project's, not a view's.
 */

import { identityMoved } from "./gitIdentityModel.mjs";
import { useCallback, useMemo, useState } from "react";
import { api } from "../../api";
import { isRefusal } from "../../apiModel.mjs";
import { useEngineEvents } from "../../bus";
import type { GitConfigView, GitIdentityView, ProjectDetail, RepoConnection, ResolvedSetting, SettingDef, WorkstreamScriptsView } from "../../types";
import { useToast } from "../../ui";
import type { useConfigEdits } from "./GitConfigForm";
import { useSessionDraft } from "./gitPanelStore";
import { useGitConfigDraft } from "./useGitConfigDraft";
import { changeCount, emptyDraft, problems as draftProblems, savedWords, staleReads, withAccount, withInherit, withPublish, withScripts, withSetting, withoutSetting, writes } from "./projectSettingsDraftModel.mjs";
import type { Current, Draft } from "./projectSettingsDraftModel.mjs";
import { projectRow } from "./projectSettingsModel.mjs";
import { publishOf } from "./publishPolicy.mjs";
import { attempt, useAsync } from "./useAsync";
import { needsApproval, scriptEdits } from "./workstreamScripts.mjs";
import type { ScriptEdits } from "./workstreamScripts.mjs";
import { useResolvedSettingsRead, useSettingsRegistry } from "../../shell/settingsStore";
import { t } from "../../i18n/l10n.mjs";

export interface ProjectSettingsDraft {
  /** What the node holds, once every read is in; `null` while any is loading. */
  current: Current | null;
  draft: Draft;
  /** The reads the cards draw from. */
  project: ProjectDetail | null;
  registry: SettingDef[];
  resolved: Map<string, ResolvedSetting>;
  scripts: WorkstreamScriptsView | null;
  connection: RepoConnection | null;
  identity: GitIdentityView | null;
  config: GitConfigView | null;
  /** The local git config form — its edits are part of the draft. */
  form: ReturnType<typeof useConfigEdits>;
  error: string | null;
  loading: boolean;
  /** Every read the count compares against has landed — *All saved* is said only then. */
  whole: boolean;
  /** Write one pair as this repository's identity — the connected account's, on its one click. */
  commitAs: (ident: { name: string; email: string }) => Promise<boolean>;
  /** The drafted value of a setting, else what the node holds. */
  settingValue: (key: string) => unknown;
  editSetting: (key: string, value: unknown) => void;
  inheritSetting: (key: string) => void;
  forgetSetting: (key: string) => void;
  editPublish: (policy: string) => void;
  editAccount: (login: string | null) => void;
  editScripts: (edits: ScriptEdits) => void;
  count: number;
  problems: Record<string, string>;
  valid: boolean;
  saving: boolean;
  /** A synced script waits for this machine's approval and nothing is drafted. */
  awaitingApproval: boolean;
  save: () => Promise<void>;
  approve: () => Promise<void>;
  discard: () => void;
  reload: () => void;
}

export function useProjectSettingsDraft(pid: string, wid: string): ProjectSettingsDraft {
  const toast = useToast();
  const project = useAsync((s) => api.project(pid, s), [pid]);
  // Kept by the settings store across views, and read again there on every
  // `settings_changed` — the draft below compares against the live values.
  const registry = useSettingsRegistry();
  const resolved = useResolvedSettingsRead(pid);
  const scripts = useAsync((s) => api.workstreamScripts(pid, s), [pid]);
  // A plain folder has no connection to read: the node's refusal is *none*; a failure stays a failure.
  const connection = useAsync(
    (s) =>
      api.gitConnection(wid, s).catch((e: unknown) => {
        if (isRefusal(e)) return null;
        throw e;
      }),
    [wid],
  );
  // Who commits and the local git config: their own draft, with its own hand
  // to save, composed here so one Save still lands everything.
  const git = useGitConfigDraft(wid);
  // Every reload here is a stable door: `useAsync`'s for the life of the
  // hook, the settings store's for the life of the project it is keyed on.
  const reloadProject = project.reload;
  const reloadResolved = resolved.reload;
  const reloadScripts = scripts.reload;
  const reloadConnection = connection.reload;
  const reloadGit = git.reload;
  const reload = useCallback(() => {
    reloadProject();
    reloadResolved();
    reloadScripts();
    reloadConnection();
    reloadGit();
  }, [reloadProject, reloadResolved, reloadScripts, reloadConnection, reloadGit]);
  useEngineEvents((e) => {
    if (identityMoved(e.payload.type)) connection.reload();
    // Changed elsewhere while this is open (`staleReads`): what the edits are compared with is read again.
    for (const read of staleReads(e.payload, pid)) (read === "project" ? project : scripts).reload();
  });

  const [draft, setDraft] = useSessionDraft<Draft>(`project:${pid}|settings-draft`, emptyDraft());
  const form = git.form;
  const [saving, setSaving] = useState(false);

  const resolvedMap = useMemo(() => new Map((resolved.data?.settings ?? []).map((r) => [r.key, r])), [resolved.data]);
  const current = useMemo<Current | null>(() => {
    if (!project.data || !resolved.data || !scripts.data) return null;
    const settings: Current["settings"] = {};
    for (const r of resolved.data.settings) {
      const row = projectRow(r);
      settings[r.key] = { value: row.value, own: row.own };
    }
    const c = connection.data;
    return {
      settings,
      publish: publishOf(project.data.project),
      account: c && c.account.source === "local" ? (c.account.login ?? null) : null,
      scripts: scriptEdits(scripts.data),
    };
  }, [project.data, resolved.data, scripts.data, connection.data]);

  const gitWrite = git.write;
  // The git config counts on its own: a project read still out never hides
  // an identity the person typed.
  const count = changeCount(draft, current, gitWrite);
  const problems = draftProblems(draft, git.problems);
  const valid = Object.keys(problems).length === 0;
  const awaitingApproval = count === 0 && needsApproval(scripts.data);

  const settingValue = (key: string) => (key in draft.settings ? draft.settings[key] : resolvedMap.get(key)?.value);

  const discard = () => {
    setDraft(emptyDraft());
    form.reset();
  };

  const save = async () => {
    if (count === 0 || !valid || saving) return;
    setSaving(true);
    // Without the project reads, the git config is the one thing to write.
    const plan = current ? writes(draft, current, gitWrite) : gitWrite ? [{ op: "git_config" as const, write: gitWrite }] : [];
    const approved = plan.some((w) => w.op === "approve_scripts");
    await attempt(
      async () => {
        // In order, stopping at the first refusal: what landed stays landed,
        // what did not stays in the draft for the next try.
        for (const w of plan) {
          switch (w.op) {
            case "patch_project":
              await api.patchProject(pid, { publish: w.publish as ProjectDetail["project"]["publish"] });
              break;
            case "set_settings":
              await api.setSettings("project", w.values, pid);
              break;
            case "unset_setting":
              await api.unsetSetting("project", w.key, pid);
              break;
            case "git_config":
              await api.setWorkstreamGitConfig(wid, w.write);
              break;
            case "approve_scripts":
              await api.approveWorkstreamScripts(pid);
              break;
            case "account":
              await api.setWorkstreamAccount(wid, w.login);
              break;
          }
        }
      },
      toast.error,
      () => {
        toast.ok(savedWords(count, approved));
        discard();
        reload();
      },
    ).finally(() => setSaving(false));
  };

  const approve = async () => {
    if (saving) return;
    setSaving(true);
    await attempt(() => api.approveWorkstreamScripts(pid), toast.error, () => {
      toast.ok(t("work-use-project-settings-draft-approved-machine"));
      scripts.reload();
    }).finally(() => setSaving(false));
  };

  return {
    current,
    draft,
    project: project.data,
    registry: registry.data?.settings ?? [],
    resolved: resolvedMap,
    scripts: scripts.data,
    connection: connection.data,
    identity: git.identity,
    config: git.config,
    form,
    error: project.error ?? resolved.error ?? scripts.error ?? registry.error ?? git.error ?? null,
    loading: !current || !registry.data,
    whole: !!current && git.whole,
    commitAs: git.commitAs,
    settingValue,
    editSetting: (key, value) => setDraft((d) => withSetting(d, key, value)),
    inheritSetting: (key) => setDraft((d) => withInherit(d, key)),
    forgetSetting: (key) => setDraft((d) => withoutSetting(d, key)),
    editPublish: (policy) => setDraft((d) => withPublish(d, policy)),
    editAccount: (login) => setDraft((d) => withAccount(d, login)),
    editScripts: (edits) => setDraft((d) => withScripts(d, edits)),
    count,
    problems,
    valid,
    saving,
    awaitingApproval,
    save,
    approve,
    discard,
    reload,
  };
}
