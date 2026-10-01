/**
 * Open a workstream on a project: a branch and a checkout of it — or a copy,
 * for a folder with no repository. Every entry point — the rail's `+`, its
 * toolbar and menu, the palette, ⌘⇧W, and the doors on a branch row, a tag
 * row or a ref chip — opens this one dialog, which owns the guard
 * (`workstreamCreation.mjs`): a repository with no commit says so and points
 * at the fix instead of failing after the click.
 *
 * **Start from** is the one choice (ide/07 §Where a workstream starts): a
 * new branch, a branch the repository has, a remote's branch, a tag, an open
 * pull request. Only that source's fields show; the branch the engine will
 * stand on is previewed as you type; a branch another checkout already holds
 * is offered greyed with who holds it. The base — the branch the work goes
 * back to — is its own field, apart from where the branch starts; a pull
 * request brings its own.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { ApiError, api } from "../../api";
import { scriptRefusal } from "./workstreamScripts.mjs";
import { Button, Checkbox, Dialog, EmptyState, ErrorNote, Field, ICON, SegmentedControl, Select, SkeletonRows, TextInput, useToast } from "../../ui";
import { openPanelView } from "../_workbench/rightPanelStore";
import { useAsync } from "../_work/useAsync";
import { ReasonLine } from "./ReasonLine";
import { SOURCES, canOpenWorkstream, fieldsOf, labelShown as labelFieldShown, openBody, openWords, previewBranch, takenBranches } from "./workstreamCreation.mjs";
import type { SourceFields, SourceKind, SourcePreset } from "./workstreamCreation.mjs";
import { t as tr } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

const OTHER_REF = "__other__";

/**
 * A `Select` of refs with an *Other ref…* row that opens a free-text field.
 * The free-text field shows when the person asked for it, or when the value
 * is one the loaded list does not have — never before the list is loaded,
 * and never taking focus: the caret stays where the person is.
 */
function RefPick({
  value,
  onChange,
  options,
  loaded,
  defaultBranch,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  options: readonly string[];
  /** The list has been read — an unknown value is then really unknown. */
  loaded: boolean;
  defaultBranch: string | null;
  placeholder: string;
}) {
  const [asked, setAsked] = useState(false);
  const other = asked || (loaded && value !== "" && !options.includes(value));
  return (
    <div className="flex flex-col gap-1">
      <Select
        value={other ? OTHER_REF : value}
        onChange={(e) => {
          if (e.target.value === OTHER_REF) {
            setAsked(true);
            onChange("");
          } else {
            setAsked(false);
            onChange(e.target.value);
          }
        }}
        className="font-mono"
      >
        <option value="">{placeholder}</option>
        {options.map((o) => (
          <option key={o} value={o}>
            {o}
            {o === defaultBranch ? ` ${tr("work-new-workstream-dialog-default")}` : ""}
          </option>
        ))}
        <option value={OTHER_REF}>{tr("work-new-workstream-dialog-other-ref")}</option>
      </Select>
      {other && <TextInput value={value} placeholder={tr("work-new-workstream-dialog-branch-tag-commit")} className="font-mono" onChange={(e) => onChange(e.target.value)} />}
    </div>
  );
}

export function NewWorkstreamDialog({
  open,
  onClose,
  pid,
  projectName,
  projectSlug,
  preset,
  onOpened,
}: {
  open: boolean;
  onClose: () => void;
  pid: string;
  projectName: string;
  projectSlug: string;
  /** What a door chose — a branch, a tag — before the dialog opened; `null` from the plain entry points. */
  preset?: SourcePreset | null;
  onOpened: (wid: string) => void;
}) {
  const toast = useToast();
  const [label, setLabel] = useState("");
  const [fields, setFields] = useState<SourceFields>(() => fieldsOf(preset));
  const [base, setBase] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const kind = fields.kind;
  // The caret lands in Label: the fields mount only once the repository's
  // status is read, after the dialog itself, so the dialog's initial focus
  // cannot reach them — this effect does, the first time they are there.
  const labelRef = useRef<HTMLInputElement>(null);

  // The primary workstream shares the project's id, so its git status, its
  // branches, its tags and its remotes are the project's.
  const git = useAsync((s) => (open ? api.workstreamGitStatus(pid, s) : Promise.resolve(null)), [pid, open]);
  const check = canOpenWorkstream({ exists: true }, git.data?.status ?? null);
  const isGit = !!git.data?.status.git;
  const ready = open && check.ok && isGit;
  const branches = useAsync((s) => (ready ? api.gitBranches(pid, s) : Promise.resolve(null)), [pid, ready]);
  const workstreams = useAsync((s) => (ready ? api.projectWorkstreams(pid, s) : Promise.resolve(null)), [pid, ready]);
  const remotes = useAsync((s) => (ready && kind === "remote" ? api.gitRemoteBranches(pid, s) : Promise.resolve(null)), [pid, ready, kind]);
  const tags = useAsync((s) => (ready && kind === "tag" ? api.gitTags(pid, s) : Promise.resolve(null)), [pid, ready, kind]);
  const prs = useAsync((s) => (ready && kind === "pr" ? api.projectPullRequests(pid, s) : Promise.resolve(null)), [pid, ready, kind]);
  const [fetching, setFetching] = useState(false);

  const defaultBranch = git.data?.default_branch ?? null;
  useEffect(() => {
    if (open) {
      setBase(defaultBranch ?? "");
      setFields(fieldsOf(preset));
    }
    // The preset is the door's choice at opening; a later change is the person's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, defaultBranch]);
  const labelShown = open && check.ok && labelFieldShown(isGit, fields);
  useEffect(() => {
    if (labelShown) labelRef.current?.focus();
  }, [labelShown]);

  const localNames = useMemo(() => (branches.data?.branches ?? []).map((b) => b.name), [branches.data]);
  const current = branches.data?.branches.find((b) => b.current)?.name ?? null;
  const taken = useMemo(() => takenBranches(workstreams.data?.workstreams ?? [], current), [workstreams.data, current]);
  const pickedPr = kind === "pr" ? (prs.data?.prs ?? []).find((p) => p.number === fields.pr) ?? null : null;
  const preview = useMemo(
    () => previewBranch({ label, project: projectSlug, source: kind === "pr" ? { ...fields, head: pickedPr?.head } : fields }),
    [label, projectSlug, fields, kind, pickedPr],
  );
  // What the click sends, built by the model from what the dialog shows — and the one problem with it.
  const request = openBody({ git: isGit, label, fields, base, defaultBranch });
  const baseShown = kind === "pr" ? (pickedPr?.base ?? null) : base.trim() || defaultBranch;

  const reset = () => {
    setLabel("");
    setFields(fieldsOf(null));
    setRefused(null);
  };

  const submit = async () => {
    if (busy || !check.ok || request.body === null) return;
    setBusy(true);
    setRefused(null);
    try {
      const { workstream } = await api.openWorkstream(pid, request.body);
      toast.ok(openWords(isGit ? fields : null, workstream.kind.kind === "worktree" ? workstream.kind.branch : ""));
      reset();
      onClose();
      onOpened(workstream.id);
    } catch (e) {
      // A 409 is the engine refusing (an unborn HEAD raced the guard, the
      // pre-create script said no, the pull request is not open, the branch
      // exists); it stays in the dialog rather than vanishing into a toast —
      // with the script's own words when it has any.
      if (e instanceof ApiError && e.code === "script_failed") {
        const r = scriptRefusal(e);
        setRefused(r.output ? `${r.message}\n\n${r.output}` : r.message);
      } else {
        setRefused(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(false);
    }
  };

  const fetchRemotes = () => {
    setFetching(true);
    void api
      .gitFetch(pid)
      .then(() => remotes.reload(), (e: unknown) => toast.error(e instanceof Error ? e.message : String(e)))
      .finally(() => setFetching(false));
  };

  const remedyLink = check.ok || check.reason !== "unborn" ? null : (
    <button
      type="button"
      className="text-accent-ink underline underline-offset-2"
      onClick={() => {
        openPanelView("git", "changes", `workstream:${pid}`);
        onClose();
      }}
    >{tr("work-new-workstream-dialog-open-git-changes")}</button>
  );
  const settingsDoor = { label: tr("work-new-workstream-dialog-about-checkout"), onClick: () => openPanelView("about", "checkout", `workstream:${pid}`) };
  const hint = SOURCES.find((s) => s.id === kind)?.hint ?? "";

  /** A branch list with the ones another checkout holds greyed, and who holds them. */
  const branchOptions = (names: readonly string[]) =>
    names.map((n) => {
      const who = taken.get(n);
      return (
        <option key={n} value={n} disabled={who !== undefined}>
          {n}
          {who ? ` ${tr("work-new-workstream-dialog-checked-out", { who })}` : ""}
        </option>
      );
    });

  const baseField = (
    <Field label={tr("work-conflict-view-base")} hint={tr("work-new-workstream-dialog-branch-work-goes-back-what-lifecycle")}>
      <RefPick value={base} onChange={setBase} options={localNames} loaded={branches.data !== null} defaultBranch={defaultBranch} /* for the machine */ placeholder={defaultBranch ?? "main"} />
    </Field>
  );

  return (
    <Dialog
      open={open}
      onClose={() => {
        reset();
        onClose();
      }}
      title={tr("work-new-workstream-dialog-new-workstream", { projectName })}
      description={
        !git.data
          ? tr("work-new-workstream-dialog-checking-repository")
          : isGit
            ? tr("work-new-workstream-dialog-branch-checkout-under-project-project-s")
            : tr("work-new-workstream-dialog-copy-folder-under-project-there-no")
      }
      width="max-w-md"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{tr("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={busy || !check.ok || request.body === null} onClick={() => void submit()}>
            {busy ? tr("work-new-workstream-dialog-opening") : tr("work-new-workstream-dialog-open")}
          </Button>
        </>
      }
    >
      <div
        className="flex flex-col gap-3"
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void submit();
          }
        }}
      >
        {git.error && <ErrorNote error={git.error} retry={git.reload} />}
        {!git.data && !git.error && <SkeletonRows rows={2} />}
        {git.data && !check.ok && (
          <p className="rounded-control border border-warn/40 bg-warn-soft px-3 py-2 text-xs text-text">
            {check.remedy} {remedyLink}
          </p>
        )}
        {git.data && check.ok && !isGit && (
          <Field label={tr("work-new-workstream-dialog-label")} hint={tr("work-new-workstream-dialog-optional-how-copy-listed")}>
            <TextInput ref={labelRef} value={label} /* content, never translated: an example of a label */ placeholder="cart-total" onChange={(e) => setLabel(e.target.value)} />
          </Field>
        )}
        {git.data && check.ok && isGit && (
          <>
            <div className="flex flex-col gap-1">
              <SegmentedControl
                label={tr("work-new-workstream-dialog-start-from")}
                size="sm"
                value={kind}
                onChange={(next: SourceKind) => {
                  setRefused(null);
                  setFields(next === "tag" ? { kind: "tag", newTag: false } : next === "new" ? { kind: "new", name: "", start: "" } : ({ kind: next } as SourceFields));
                }}
                options={SOURCES.map((s) => ({ id: s.id, label: s.label }))}
              />
              <p className="text-2xs text-text-dim">{hint}</p>
            </div>

            {fields.kind === "new" && (
              <>
                <Field label={tr("work-new-workstream-dialog-label")} hint={tr("work-new-workstream-dialog-optional-readable-half-derived-branch-name")}>
                  <TextInput ref={labelRef} value={label} /* content, never translated: an example of a label */ placeholder="cart-total" onChange={(e) => setLabel(e.target.value)} />
                </Field>
                <Field label={tr("work-commit-action-dialogs-branch-name")} hint={tr("work-new-workstream-dialog-optional-name-own-instead-derived-one")}>
                  <TextInput value={fields.name ?? ""} /* for the machine */ placeholder="feature/dark-mode" className="font-mono" onChange={(e) => setFields({ ...fields, name: e.target.value })} />
                </Field>
                <Field label={tr("work-new-branch-dialog-start")} hint={tr("work-new-workstream-dialog-where-new-branch-begins-base-unless")}>
                  <RefPick value={fields.start ?? ""} onChange={(v) => setFields({ ...fields, start: v })} options={localNames} loaded={branches.data !== null} defaultBranch={defaultBranch} placeholder={baseShown ?? tr("work-new-workstream-dialog-base")} />
                </Field>
                {baseField}
              </>
            )}

            {fields.kind === "branch" && (
              <>
                <Field label={tr("work-new-workstream-dialog-branch")} hint={tr("work-new-workstream-dialog-checked-out-branch-another-checkout-holds")}>
                  {branches.data ? (
                    <Select autoFocus value={fields.branch ?? ""} onChange={(e) => setFields({ ...fields, branch: e.target.value })} className="font-mono">
                      <option value="">{tr("work-new-workstream-dialog-pick-branch")}</option>
                      {branchOptions(localNames)}
                    </Select>
                  ) : (
                    <SkeletonRows rows={1} />
                  )}
                </Field>
                {baseField}
              </>
            )}

            {fields.kind === "remote" && (
              <>
                <Field label={tr("work-new-workstream-dialog-remote-branch")} hint={tr("work-new-workstream-dialog-last-fetch-local-branch-same-name")}>
                  <div className="flex items-center gap-2">
                    {remotes.data ? (
                      <Select
                        autoFocus
                        value={fields.remote && fields.branch ? `${fields.remote}/${fields.branch}` : ""}
                        onChange={(e) => {
                          const hit = remotes.data?.remote_branches.find((b) => `${b.remote}/${b.name}` === e.target.value);
                          setFields({ kind: "remote", remote: hit?.remote, branch: hit?.name });
                        }}
                        className="min-w-0 flex-1 font-mono"
                      >
                        <option value="">{tr("work-new-workstream-dialog-pick-remote-branch")}</option>
                        {remotes.data.remote_branches.map((b) => {
                          const who = taken.get(b.name);
                          return (
                            <option key={`${b.remote}/${b.name}`} value={`${b.remote}/${b.name}`} disabled={who !== undefined}>
                              {b.remote}/{b.name}
                              {who ? ` ${tr("work-new-workstream-dialog-checked-out", { who })}` : ""}
                            </option>
                          );
                        })}
                      </Select>
                    ) : (
                      <div className="flex-1">
                        <SkeletonRows rows={1} />
                      </div>
                    )}
                    <Button size="sm" variant="ghost" disabled={fetching} onClick={fetchRemotes} aria-label={tr("work-new-workstream-dialog-fetch-every-remote")}>
                      <ICON.refresh size={12} aria-hidden className={fetching ? "animate-spin" : undefined} />
                      <span className="ml-1">{fetching ? tr("work-new-workstream-dialog-fetching") : tr("work-new-workstream-dialog-fetch")}</span>
                    </Button>
                  </div>
                </Field>
                {remotes.error && <ErrorNote error={remotes.error} retry={remotes.reload} />}
                {remotes.data && remotes.data.remote_branches.length === 0 && (
                  <EmptyState icon={ICON.branchHere} title={tr("work-new-workstream-dialog-no-remote-branches-known")} hint={tr("work-new-workstream-dialog-fetch-brings-what-remotes-have")} className="py-2" action={null} />
                )}
                {baseField}
              </>
            )}

            {fields.kind === "tag" && (
              <>
                <Checkbox label={tr("work-new-workstream-dialog-new-tag-made-now")} checked={fields.newTag === true} onChange={(v) => setFields({ ...fields, newTag: v })} />
                {fields.newTag ? (
                  <>
                    <Field label={tr("work-new-workstream-dialog-tag-name")} hint={tr("work-new-workstream-dialog-lightweight-tag-git-s-ref-rules")}>
                      <TextInput autoFocus value={fields.tagName ?? ""} /* for the machine */ placeholder="v1.2.0" className="font-mono" onChange={(e) => setFields({ ...fields, tagName: e.target.value })} />
                    </Field>
                    <Field label={tr("work-new-workstream-dialog-words")} hint={tr("work-new-workstream-dialog-where-tag-goes-branch-tag-commit")}>
                      <RefPick value={fields.tagAt ?? ""} onChange={(v) => setFields({ ...fields, tagAt: v })} options={localNames} loaded={branches.data !== null} defaultBranch={defaultBranch} placeholder={tr("work-new-workstream-dialog-pick-ref")} />
                    </Field>
                  </>
                ) : (
                  <Field label={tr("work-new-workstream-dialog-tag")} hint={tr("work-new-workstream-dialog-newest-first")}>
                    {tags.data ? (
                      <Select autoFocus value={fields.tag ?? ""} onChange={(e) => setFields({ ...fields, tag: e.target.value })} className="font-mono">
                        <option value="">{tr("work-new-workstream-dialog-pick-tag")}</option>
                        {tags.data.tags.map((t) => (
                          <option key={t.name} value={t.name}>
                            {t.name}
                            {t.subject ? ` — ${t.subject}` : ""}
                          </option>
                        ))}
                      </Select>
                    ) : (
                      <SkeletonRows rows={1} />
                    )}
                  </Field>
                )}
                {tags.data && !fields.newTag && tags.data.tags.length === 0 && (
                  <EmptyState icon={ICON.tag} title={tr("work-branches-panel-no-tags")} hint={tr("work-new-workstream-dialog-tag-name-commit-make-one-now")} className="py-2" action={null} />
                )}
                <Field label={tr("work-commit-action-dialogs-branch-name")} hint={tr("work-new-workstream-dialog-optional-branch-checkout-stands-from-tag")}>
                  <TextInput value={fields.branch ?? ""} /* for the machine */ placeholder={preview.branch || "from/…"} className="font-mono" onChange={(e) => setFields({ ...fields, branch: e.target.value })} />
                </Field>
                {baseField}
              </>
            )}

            {fields.kind === "pr" && (
              <>
                <Field label={tr("work-new-workstream-dialog-pull-request")} hint={tr("work-new-workstream-dialog-open-ones-code-host-behind-origin")}>
                  <div className="flex items-center gap-2">
                    {prs.data ? (
                      <Select autoFocus value={fields.pr ? String(fields.pr) : ""} onChange={(e) => setFields({ kind: "pr", pr: e.target.value ? Number(e.target.value) : null })} className="min-w-0 flex-1 font-mono">
                        <option value="">{tr("work-new-workstream-dialog-pick-pull-request")}</option>
                        {prs.data.prs.map((p) => {
                          const who = taken.get(p.head);
                          return (
                            <option key={p.number} value={String(p.number)} disabled={who !== undefined}>
                              #{p.number} · {p.title} · {p.head} → {p.base}
                              {p.author ? ` · @${p.author}` : ""}
                              {who ? ` ${tr("work-new-workstream-dialog-checked-out", { who })}` : ""}
                            </option>
                          );
                        })}
                      </Select>
                    ) : prs.error ? (
                      <span className="flex-1" />
                    ) : (
                      <div className="flex-1">
                        <SkeletonRows rows={1} />
                      </div>
                    )}
                    <Button size="sm" variant="ghost" disabled={prs.loading} onClick={prs.reload} aria-label={tr("work-new-workstream-dialog-read-pull-requests-again")}>
                      <ICON.refresh size={12} aria-hidden className={prs.loading ? "animate-spin" : undefined} />
                      <span className="ml-1">{tr("work-commit-graph-refresh")}</span>
                    </Button>
                  </div>
                </Field>
                {prs.error && <ReasonLine tone="warn" door={settingsDoor}>{prs.error}</ReasonLine>}
                {prs.data && prs.data.prs.length === 0 && (
                  <EmptyState icon={ICON.pullRequest} title={tr("work-new-workstream-dialog-no-open-pull-requests")} hint={tr("work-new-workstream-dialog-code-host-lists-none-repository")} className="py-2" action={null} />
                )}
              </>
            )}

            <p className="text-2xs text-text-dim">
              {rich("work-new-workstream-dialog-branch-slot", { branch: <span className="font-mono text-text">{preview.branch || "—"}</span> })}
              {preview.derived && ` ${tr("work-new-workstream-dialog-engine-appends-id-s-tail")}`}
              {baseShown && (
                <>
                  {" · "}
                  {rich("work-new-workstream-dialog-base-slot", { base: <span className="font-mono text-text">{baseShown}</span> })}
                </>
              )}
              <span className="ml-2">{tr("work-new-workstream-dialog-opens")}</span>
            </p>
            {request.problem && <ReasonLine>{request.problem}</ReasonLine>}
          </>
        )}
        {refused && <pre className="max-h-40 overflow-auto whitespace-pre-wrap font-mono text-2xs text-danger">{refused}</pre>}
      </div>
    </Dialog>
  );
}
