/**
 * Who commits in a checkout, and the repository's local git config, as one
 * draft with its own hand to save (ide/04 §Who commits). Lifted out of the
 * project settings draft because it used to save only through it: a
 * project read that had not landed left the toolbar at *All saved* with no
 * button, and an identity the person had typed was never written. Now the
 * git config is read, counted, refused (the pair rule) and written here;
 * the project draft composes it so one Save still lands everything.
 *
 * The one-click door: a repository nobody commits in whose connection
 * resolves a code host account is offered that account's identity
 * (`suggested` on the read); `commitAs` writes the pair the person clicked,
 * through the same `PUT`, which raises `committer_set` for every surface.
 */

import { useCallback, useMemo, useState } from "react";
import { api } from "../../api";
import { isRefusal } from "../../apiModel.mjs";
import { useEngineEvents } from "../../bus";
import type { GitConfigView, GitIdentityView } from "../../types";
import { useToast } from "../../ui";
import { useConfigEdits } from "./GitConfigForm";
import { identityPairProblem } from "./gitConfigModel.mjs";
import type { ConfigWrite } from "./gitConfigModel.mjs";
import { identityMoved } from "./gitIdentityModel.mjs";
import { attempt, useAsync } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

const NO_SCHEMA: readonly never[] = [];

export interface GitConfigDraft {
  identity: GitIdentityView | null;
  config: GitConfigView | null;
  /** The local layer's form — its edits are the draft. */
  form: ReturnType<typeof useConfigEdits>;
  /** The write the edits amount to, or `null` when nothing changed. */
  write: ConfigWrite | null;
  /** The form's refusals, the pair rule among them, each under its key. */
  problems: Record<string, string>;
  /** The reads failed — what the card says instead of a form. */
  error: string | null;
  /** Every read has landed. */
  whole: boolean;
  saving: boolean;
  /** Write the edits; resolves `true` when they landed. */
  save: () => Promise<boolean>;
  /** Write one pair — the suggested account's — as this repository's identity. */
  commitAs: (ident: { name: string; email: string }) => Promise<boolean>;
  reload: () => void;
}

export function useGitConfigDraft(wid: string): GitConfigDraft {
  const toast = useToast();
  // A plain folder has no identity and no config to read — the node refuses, and that is *none*; a node
  // that fell over or is away is a failure the card says, never an empty form.
  const none = (e: unknown) => {
    if (isRefusal(e)) return null;
    throw e;
  };
  const identity = useAsync<GitIdentityView | null>((s) => api.gitIdentity(wid, s).catch(none), [wid]);
  const config = useAsync<GitConfigView | null>((s) => api.workstreamGitConfig(wid, s).catch(none), [wid]);
  const form = useConfigEdits(config.data?.schema ?? NO_SCHEMA, config.data?.entries ?? NO_SCHEMA, "local", `workstream:${wid}|gitconfig`);
  const [saving, setSaving] = useState(false);
  // The two reloads are stable doors (`useAsync`), so `reload` is too.
  const reloadIdentity = identity.reload;
  const reloadConfig = config.reload;
  const reload = useCallback(() => {
    reloadIdentity();
    reloadConfig();
  }, [reloadIdentity, reloadConfig]);
  useEngineEvents((e) => {
    if (identityMoved(e.payload.type)) reload();
  });

  const write = form.dirty ? form.write : null;
  const problems = useMemo(() => {
    const pair = write ? identityPairProblem(form.rows, write) : null;
    return pair ? { ...form.problems, "user.email": pair } : form.problems;
  }, [form.problems, form.rows, write]);

  const put = useCallback(
    async (body: ConfigWrite, saidOk: string) => {
      if (saving) return false;
      setSaving(true);
      try {
        return await attempt(
          () => api.setWorkstreamGitConfig(wid, body),
          toast.error,
          () => {
            toast.ok(saidOk);
            form.reset();
            reload();
          },
        );
      } finally {
        setSaving(false);
      }
    },
    [saving, wid, toast, form, reload],
  );

  return {
    identity: identity.data ?? null,
    config: config.data ?? null,
    form,
    write,
    problems,
    error: config.error ?? identity.error ?? null,
    whole: !config.loading && !identity.loading,
    saving,
    save: () => (write && Object.keys(problems).length === 0 ? put(write, t("work-use-git-config-draft-saved-repository-s-git-config")) : Promise.resolve(false)),
    commitAs: (ident) => put({ set: { "user.name": ident.name, "user.email": ident.email }, unset: [] }, t("work-use-git-config-draft-repository-commits", { ident: ident.name })),
    reload,
  };
}
