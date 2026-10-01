/**
 * What can be done to a remote and to a remote branch in the Branches view's
 * Remotes section (ide/04 §Remotes), said once — the twin of
 * `branchActionsModel.mjs`, which holds a remote branch row's verbs. A
 * remote row's hover verb is *Fetch* and its `⋮` holds the rest. The words
 * say the verbs `gitWords.mjs` fixes: *Fetch*, *Delete*. Pure: nothing here
 * runs git.
 */

import { VERB, confirmLabel } from "./gitWords.mjs";
import { parseRemote, protocolWords, remoteSummary } from "./remoteModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The actions on one remote row, in order: the hover verb first, then the
 * `⋮` menu's. `ctx.busy` is an operation in flight.
 * @param {{name: string, url: string}} remote
 * @param {{busy?: boolean}} [ctx]
 * @returns {{id: string, label: string, icon: string, hover: boolean, consented: boolean, danger: boolean, disabled: boolean, reason: string | null, separatorBefore?: boolean}[]}
 */
export function remoteActions(remote, { busy = false } = {}) {
  const running = busy ? t("work-branch-actions-another-operation-running") : null;
  const page = hostPage(remote.url);
  const out = [
    { id: "fetch", label: `${VERB.fetch} ${remote.name}`, icon: "refresh", hover: true, consented: false, danger: false, disabled: running !== null, reason: running },
    { id: "edit_url", label: t("work-remote-actions-edit-url"), icon: "edit", hover: false, consented: false, danger: false, disabled: running !== null, reason: running, separatorBefore: true },
    { id: "copy_url", label: t("work-remotes-section-copy-url"), icon: "copy", hover: false, consented: false, danger: false, disabled: false, reason: null },
  ];
  if (page) {
    out.push({ id: "open_host", label: t("work-remote-actions-open", { host: new URL(page).host }), icon: "open", hover: false, consented: false, danger: false, disabled: false, reason: null });
  }
  out.push({ id: "delete", label: `${VERB.delete}…`, icon: "delete", hover: false, consented: true, danger: true, disabled: running !== null, reason: running, separatorBefore: true });
  return out;
}

/**
 * The web page a remote's repository has on its host — `https://host/owner/name`
 * for an https, ssh or scp-style URL with all three parts; null for a local
 * path or anything else. The one place the `git@` form is turned into a link.
 * @param {string | null | undefined} url
 * @returns {string | null}
 */
export function hostPage(url) {
  const parsed = parseRemote(url);
  if (!parsed || !parsed.host || !parsed.owner || !parsed.name) return null;
  if (parsed.protocol === "local" || parsed.protocol === "other") return null;
  return `https://${parsed.host}/${parsed.owner}/${parsed.name}`;
}

/**
 * The remote branches under each remote, newest first, each naming the
 * local branch that tracks it (`trackedBy`) when one does; a remote with
 * nothing fetched still has a group. The tree and the list the section
 * draws are `remoteTreeModel.mjs`'s, over these.
 * @param {readonly {name: string, url: string}[]} remotes
 * @param {readonly {remote: string, name: string, timestamp: number}[]} remoteBranches
 * @param {readonly {name: string, upstream?: string | null}[]} branches
 */
export function groupRemoteBranches(remotes, remoteBranches, branches) {
  const trackedBy = new Map();
  for (const b of branches) if (b.upstream && !trackedBy.has(b.upstream)) trackedBy.set(b.upstream, b.name);
  return remotes.map((r) => ({
    remote: r,
    branches: remoteBranches
      .filter((b) => b.remote === r.name)
      .slice()
      .sort((a, b) => b.timestamp - a.timestamp)
      .map((b) => ({ ...b, full: `${b.remote}/${b.name}`, trackedBy: trackedBy.get(`${b.remote}/${b.name}`) ?? null })),
  }));
}

/**
 * The remote's row shows its name alone; this is what the name's tooltip
 * says — where it lives, how it is reached, and the URL as written.
 * @param {{name: string, url: string}} remote
 */
export function remoteTitle(remote) {
  const parsed = parseRemote(remote.url);
  const summary = remoteSummary(remote.url);
  const how = protocolWords(parsed?.protocol).label;
  const parts = [summary && summary !== remote.url ? summary : null, how === "—" ? null : how, remote.url].filter(Boolean);
  return parts.join(" · ");
}

/** *Fetch all* · *Fetching…*, and the toast once every remote answered. */
export function fetchWords(count, { running = false } = {}) {
  if (running) return t("work-new-workstream-dialog-fetching");
  return count === 1 ? VERB.fetch : t("work-remote-actions-all", { fetch: VERB.fetch });
}

/** The toast after fetching: what came in, per remote. */
export function fetchedWords(results) {
  const failed = results.filter((r) => !r.ok);
  if (failed.length === 0) {
    return results.length === 1 ? t("work-remote-actions-fetched", { results: results[0].name }) : t("work-remote-actions-fetched-remotes", { results: results.length });
  }
  return t("work-remote-actions-fetched-some-failed", { ok: results.length - failed.length, total: results.length, failures: failed.map((r) => `${r.name}: ${r.error}`).join("; ") });
}

/**
 * The confirmation before deleting a remote: what goes, what stays, and the
 * truth about Safety — nothing is pinned for a remote, its URL is
 * configuration, so the dialog offers the URL to copy.
 * @param {{name: string, url: string}} remote
 */
export function deleteRemoteWords(remote) {
  return {
    title: t("work-remote-actions-remote", { delete: VERB.delete, remote: remote.name }),
    body: t("work-remote-actions-url-remote-tracking-branches-go-branches"),
    confirm: confirmLabel("delete"),
    danger: true,
    url: remote.url,
    done: t("work-remote-actions-deleted-remote", { remote: remote.name }),
  };
}

/** The empty group's sentence. */
export function noBranchesWords(remote) {
  return t("work-remote-actions-nothing-fetched-yet-brings-what-has", { fetch: VERB.fetch, remote });
}
