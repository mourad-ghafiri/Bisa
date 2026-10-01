/**
 * One project's About: where its files are, where it was born, the goals it
 * is attached to, who carries it — identity and relations, nothing else.
 * What git thinks of it, who commits in it, how it reaches its remote and
 * how it publishes are its **Settings** view (`ProjectSettingsView`), the
 * other half of About; its workstreams are an occupant of their own.
 *
 * One thing here is deliberately loud rather than convenient.
 *
 * **Removing.** `DELETE /projects/{pid}` forgets the record and leaves every
 * file alone; `?tree=true` also takes the folder off disk — to the Trash or
 * for good, as `editor.delete.trash` says. That is not one action with a
 * modifier — it is separate acts, one of which destroys work — so
 * `RemoveProjectDialog` offers them as separate, differently-worded choices,
 * the rail's own; a folder going for good is asked about once more, by the
 * exact path. An adopted folder is refused by the node, and the choice says
 * so, held, before you press it rather than after.
 */

import { useState } from "react";
import { PLAIN_FOLDER } from "./initRepositoryModel.mjs";
import { api, inDesktopShell, revealPath } from "../../api";
import { AssigneePicker } from "./AssigneePicker";
import { href } from "../../router";
import type { AttachmentRef, GitStatusInfo, Project } from "../../types";
import {
  Avatar,
  Button,
  Chip,
  ConfirmDialog,
  CopyText,
  Dialog,
  Dot,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Menu,
  PageHeader,
  SectionHeader,
  Skeleton,
  SkeletonRows,
  TAG_VOCABULARY,
  TagChips,
  TagInput,
  TextInput,
  Tooltip,
  revealLabel,
  useToast,
  PhotoField,
  usePhotoThumb,
} from "../../ui";
import { assigneeWire } from "./types";
import { attempt, useAsync } from "./useAsync";
import { ProjectOriginChip } from "./ProjectOriginChip";
import { RemoveProjectDialog } from "./RemoveProjectDialog";
import { removeProject } from "./removeProject";
import { isAdopted, type RemoveAct } from "./removeProjectModel.mjs";
import type { Home } from "../_workbench/ideHomeModel.mjs";
import { stepOf } from "./projectOriginModel.mjs";
import { AttachGoalDialog } from "./AttachGoalDialog";
import { goalLabelOf } from "./attachGoalModel.mjs";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/**
 * What this platform calls showing a folder. Resolved once at module load —
 * the user agent does not change while the app is open, and a label that
 * re-derives itself on every render is a label that can disagree with itself.
 *
 * Exported so the Projects list and this pane say the same words for the same
 * act. Two screens each spelling it is how "Reveal in Finder" and "Reveal in
 * file manager" end up one click apart — nothing spells it; `revealLabel` is
 * the one source.
 */
const REVEAL_LABEL = revealLabel(
  typeof navigator === "undefined" ? "" : navigator.userAgent,
);

export function projectIsGit(p: Project): boolean {
  return p.vcs.type === "git";
}


/**
 * One dot for "is this project all right?".
 *
 * Missing folder is the only red: an uncommitted change is normal work, not a
 * fault, so it gets the accent — the colour this app uses for *your turn*.
 */
export function ProjectDot({
  exists,
  status,
}: {
  exists: boolean;
  status?: GitStatusInfo | null;
}) {
  const [tone, label] = ((): ["danger" | "accent" | "neutral", string] => {
    if (!exists) return ["danger", t("work-project-detail-folder-not-disk-2")];
    if (status?.git && !status.clean) {
      const n = status.staged + status.unstaged + status.untracked;
      return ["accent", t("work-project-detail-uncommitted-change-changes", { n })];
    }
    if (status?.git && status.ahead > 0) {
      return ["accent", t("work-project-detail-commit-commits-not-pushed", { ahead: status.ahead })];
    }
    return ["neutral", status?.git ? t("work-project-detail-clean-word") : t("work-project-detail-plain-folder-2")];
  })();
  // A dot is the smallest thing on the screen and says nothing on its own,
  // so its whole meaning is in the label — which a native `title` gives to a
  // mouse and to nobody else.
  return (
    <Tooltip label={label}>
      <span className="inline-flex">
        <Dot tone={tone} />
      </span>
    </Tooltip>
  );
}

/**
 * What git thinks of a project, in the four lines that matter.
 *
 * Exported so the Projects list can show the same facts in a popover off the
 * status dot: the dot says *something is up* and nothing more, and a reader
 * who has to open the detail pane to find out what has already lost the
 * thing the dot was for.
 */
export function GitFacts({
  status,
  defaultBranch,
  showCounts = true,
}: {
  status: GitStatusInfo;
  defaultBranch: string | null;
  /**
   * The `N staged · N unstaged · N untracked` line.
   *
   * Off where the file list is on screen directly under it. The two numbers
   * come from different routes — this one from `GET /status`, the list from
   * `GET /git/files` — so a summary sitting above the thing it summarises is
   * a second copy that can disagree with it for as long as one of them is
   * stale. The popover off the Projects dot keeps it: there, it is the only
   * answer there is.
   */
  showCounts?: boolean;
}) {
  if (!status.exists) {
    return <p className="text-2xs text-danger">{t("work-project-detail-folder-not-disk")}</p>;
  }
  if (!status.git) return <p className="text-2xs text-text-dim">{PLAIN_FOLDER}</p>;
  const dirty = status.staged + status.unstaged + status.untracked + status.conflicted;
  return (
    <div className="flex flex-col gap-1 text-2xs text-text-dim">
      <p>
        <span className="font-mono text-text">
          {status.branch ?? (status.detached ? t("work-project-detail-detached-head") : t("work-project-detail-no-commits-yet"))}
        </span>
        {defaultBranch && status.branch !== defaultBranch && (
          <span>{t("work-project-detail-default", { defaultBranch })}</span>
        )}
        {status.head && <span> · {status.head.slice(0, 7)}</span>}
      </p>
      {status.upstream ? (
        <p className="tnum">
          {status.upstream} · {status.ahead}↑ {status.behind}↓
        </p>
      ) : (
        <p>{t("work-project-detail-no-upstream-branch")}</p>
      )}
      {showCounts && (
        <p className="tnum">
          {dirty === 0
            ? t("work-project-detail-clean")
            : t("work-project-detail-staged-unstaged-untracked-conflicted-untracked", { staged: status.staged, unstaged: status.unstaged, untracked: status.untracked, conflicted: status.conflicted, flag: (status.conflicted > 0) ? "yes" : "no" })}
        </p>
      )}
      {status.remote ? (
        <Tooltip label={status.remote}>
          <p className="truncate">{t("work-project-detail-origin", { remote: status.remote })}</p>
        </Tooltip>
      ) : (
        <p>{t("work-project-detail-no-remote-nothing-push-yet")}</p>
      )}
      {status.error && <p className="text-danger">{status.error}</p>}
    </div>
  );
}

export function ProjectDetail({
  pid,
  goals,
  onChanged,
  onLeft,
}: {
  pid: string;
  /** Every goal, for the link picker and for naming the owner. */
  goals: { id: string; label: string }[];
  onChanged?: () => void;
  /**
   * The project was archived or removed from here — About is always the
   * root's own project, so the person stands on what went: the workbench
   * leaves for `home`, the next one or none (the landing).
   */
  onLeft?: (home: Home | null) => void;
}) {
  const toast = useToast();
  const { data, error, loading, reload } = useAsync((s) => api.project(pid, s), [pid]);
  const status = useAsync((s) => api.workstreamGitStatus(pid, s), [pid]);
  // The name of the workflow whose step made this project, when one did.
  const originWorkflow = useAsync(
    async (s) => {
      const by = stepOf(data?.project.origin);
      return by ? (await api.workflow(by.workflow, s)).workflow.name : null;
    },
    [data?.project.origin],
  );
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  // The rail's grouping and the project's picture (W2). Both are
  // display only; the picture is an attachment by content hash.
  const [group, setGroup] = useState("");
  const [photo, setPhoto] = useState<AttachmentRef | null>(null);
  // The header's picture is the shared thumbnail (ide/14 §Photos) — read before any early return, as hooks must be.
  const headerThumb = usePhotoThumb(data?.project.photo?.sha256 ?? null);
  const known = useWorkspace();
  const groups = [...new Set(known.projects.map((r) => r.project.group).filter((g): g is string => !!g))].sort();
  const [attaching, setAttaching] = useState(false);
  /** The goal a detach is being confirmed for. */
  const [detaching, setDetaching] = useState<string | null>(null);
  const [savingAssignees, setSavingAssignees] = useState(false);
  const [assigneeError, setAssigneeError] = useState<string | null>(null);

  /**
   * Assignees save on change rather than behind an editing mode: the picker is
   * already an explicit commit (you added or removed somebody), and a second
   * Save button would just be a way to lose the change by navigating away.
   */
  async function saveAssignees(next: string[]) {
    setSavingAssignees(true);
    setAssigneeError(null);
    try {
      await api.setProjectAssignees(p.id, next);
      await reload();
    } catch (e) {
      setAssigneeError(e instanceof Error ? e.message : String(e));
    } finally {
      setSavingAssignees(false);
    }
  }
  const [deleting, setDeleting] = useState(false);
  const [archiving, setArchiving] = useState(false);
  const [busy, setBusy] = useState(false);

  if (loading && !data) {
    return (
      <div className="flex flex-col gap-3" aria-busy>
        <Skeleton className="h-6 w-56" />
        <SkeletonRows rows={5} />
      </div>
    );
  }
  if (error || !data) {
    return <ErrorNote error={error ?? t("work-project-detail-project-not-found")} retry={reload} />;
  }

  const p = data.project;
  const adopted = isAdopted(p);
  const goalLabel = (id: string) => goalLabelOf(goals, id);

  const refresh = () => {
    reload();
    status.reload();
    onChanged?.();
  };

  const act = (label: string, fn: () => Promise<unknown>) =>
    void attempt(fn, toast.error, () => {
      toast.ok(label);
      refresh();
    });

  const save = async () => {
    if (busy) return;
    setBusy(true);
    const ok = await attempt(
      () =>
        api.patchProject(pid, {
          name: name.trim(),
          tags,
          group: group.trim() ? group.trim() : null,
          photo,
        }),
      toast.error,
    );
    setBusy(false);
    if (ok) {
      toast.ok(t("work-project-detail-project-updated"));
      setEditing(false);
      refresh();
    }
  };

  // The one door (`removeProject.ts`), the rail's too. Archived or removed,
  // the project is no longer a place to stand: the workbench leaves for the
  // home the door named, once the node has answered.
  const remove = (chosen: RemoveAct, trash: boolean) =>
    void attempt(() => removeProject(pid, chosen, trash, known.workstreams, known.projects), toast.error, ({ said, home }) => {
      // The node's own word on the folder: one that stayed is said, and stays longer on screen.
      if (said.tone === "warn") toast.error(said.text);
      else toast.ok(said.text);
      onLeft?.(home);
    });

  return (
    <div className="flex min-w-0 flex-col gap-4">
      {/*
        A `PageHeader` at its default `h2`: this is a heading *below* the
        screen's, which the shell's chrome already renders. Everything is
        behind the menu: starting work is the Workstreams occupant's job, and
        this header only names the thing.
      */}
      <PageHeader
        className="px-0 py-0"
        icon={ICON.project}
        title={p.name}
        meta={
          <>
            <Avatar id={p.id} name={p.name} url={headerThumb} size={18} />
            <ProjectDot exists={data.exists} status={status.data?.status} />
            <Chip tone="quiet">{p.slug}</Chip>
            {p.group && <Chip tone="neutral">{p.group}</Chip>}
            {projectIsGit(p) ? (
              <Chip tone="accent">{t("work-goal-inspector-git")}</Chip>
            ) : (
              <Chip tone="quiet">{t("work-project-detail-plain-folder")}</Chip>
            )}
            {adopted && (
              <Tooltip label={t("work-project-detail-adopted-from-elsewhere-disk-bisa-never")}>
                <span>
                  <Chip tone="warn">{t("work-project-detail-adopted")}</Chip>
                </span>
              </Tooltip>
            )}
            <TagChips tags={p.tags ?? []} />
          </>
        }
        actions={
          <>
            <Menu
              label={t("work-project-detail-actions", { p: p.name })}
              trigger={
                <span className="anim flex h-7 w-7 items-center justify-center rounded-control border border-border text-text-dim hover:bg-surface-2 hover:text-text">
                  <ICON.more size={14} aria-hidden />
                </span>
              }
              items={[
                {
                  label: t("work-project-detail-edit-project-2"),
                  icon: ICON.edit,
                  onSelect: () => {
                    setName(p.name);
                    setTags([...(p.tags ?? [])]);
                    setGroup(p.group ?? "");
                    setPhoto(p.photo ?? null);
                    setEditing(true);
                  },
                },
                {
                  label: t("work-project-detail-attach-goal"),
                  icon: ICON.attach,
                  onSelect: () => setAttaching(true),
                },
                p.archived
                  ? { label: t("work-project-detail-unarchive"), icon: ICON.archive, separatorBefore: true, onSelect: () => act(t("work-project-detail-back", { p: p.name }), () => api.archiveProject(pid, false)) }
                  : { label: t("work-project-detail-archive-3"), icon: ICON.archive, separatorBefore: true, onSelect: () => setArchiving(true) },
                {
                  label: t("work-project-detail-delete"),
                  icon: ICON.delete,
                  danger: true,
                  onSelect: () => setDeleting(true),
                },
              ]}
            />
          </>
        }
      />

      {/*
        The path stays, because it is the thing people paste into a terminal
        and no tree replaces that. What it no longer has to be is the *only*
        answer to t("work-project-detail-what-there") — a question this screen was previously
        making you leave it to ask.
      */}
      <section className="min-w-0">
        <SectionHeader title={t("work-goal-inspector-files")} />
        <CopyText value={data.path} />
        {/*
          Beside the path rather than in the overflow menu, because both of
          these are answers to the question the path is being read for: people
          copy it to open the folder or to `cd` into it. Each is gated on the
          folder actually being there — a file manager asked to reveal a path
          that does not exist opens somewhere unrelated, and a shell rooted in
          a missing directory is a shell in whatever its parent turned out to
          be.
        */}
        <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
          {inDesktopShell() && (
            <button
              type="button"
              disabled={!data.exists}
              onClick={() => {
                // Silence is exactly the wrong answer to "the shell said no",
                // which is what a missing capability line looks like here.
                revealPath(data.path).catch((e: unknown) => {
                  toast.error(e instanceof Error ? e.message : String(e));
                });
              }}
              className="anim inline-flex h-6 items-center gap-1.5 rounded-control border border-border px-2 text-2xs text-text-dim hover:bg-surface-2 hover:text-text disabled:opacity-45"
            >
              <ICON.folderOpen size={12} aria-hidden />
              {REVEAL_LABEL}
            </button>
          )}
        </div>
        <p className="mt-1 text-2xs text-text-dim">
          {adopted
            ? t("work-project-detail-adopted-bisa-never-writes-into-folder")
            : t("work-project-detail-managed-bisa-inside-workspace-s-projects")}
        </p>
        {!data.exists && (
          <p className="mt-1 text-2xs text-danger">
            {t("work-project-detail-nothing-at-path-record-still-here")}
          </p>
        )}
      </section>

      {/*
        Origin is history, attachment is the relation. Where the project was
        born is recorded once, by the engine, and never changes; who works in
        it is the list below, and changes whenever you like.
      */}
      <section>
        <SectionHeader title={t("work-goal-inspector-origin")} />
        <div className="flex flex-wrap items-center gap-2">
          <ProjectOriginChip
            origin={p.origin}
            goalTitle={goalLabel}
            workflowName={() => originWorkflow.data ?? undefined}
          />
          {stepOf(p.origin) && (
            <span className="text-2xs text-text-dim">
              {rich("work-project-detail-step-of", { step: <span className="font-mono">{stepOf(p.origin)?.step}</span> }, { of: p.origin.origin === "goal" ? t("work-project-detail-goal-s-own-design") : t("work-project-detail-run", { string: goalLabel(p.origin.goal as string) }) })}
              {originWorkflow.data && <> — {originWorkflow.data}</>}
            </span>
          )}
        </div>
      </section>

      {/*
        Attachment is a relation, not ownership: zero or more goals, each one
        record, none of them the project's home. Detaching asks; attaching
        does not — it is additive, reversible and moves nothing.
      */}
      <section>
        <SectionHeader title={t("work-project-detail-goals")} />
        {data.goals.length === 0 ? (
          <EmptyState
            icon={ICON.goal}
            title={t("work-project-detail-attached-no-goal")}
            hint={t("work-project-detail-workspace-s-alone-agents-reach-through")}
            className="py-4"
            action={
              <Button size="sm" onClick={() => setAttaching(true)}>
                <ICON.attach size={12} aria-hidden />{t("work-project-detail-attach-goal")}</Button>
            }
          />
        ) : (
          <>
            <ul className="flex flex-col gap-1">
              {data.goals.map((i) => (
                <li key={i} className="flex items-center gap-2">
                  <a
                    href={href({ name: "goal", id: i })}
                    className="anim inline-flex min-w-0 items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-2xs text-text-dim hover:bg-surface-2 hover:text-text"
                  >
                    <ICON.goal size={11} aria-hidden className="shrink-0" />
                    <span className="truncate">{goalLabel(i)}</span>
                  </a>
                  <Button size="sm" variant="ghost" onClick={() => setDetaching(i)}>
                    <ICON.detach size={12} aria-hidden />{t("work-library-refs-detach")}</Button>
                </li>
              ))}
            </ul>
            <Button size="sm" className="mt-1.5" onClick={() => setAttaching(true)}>
              <ICON.attach size={12} aria-hidden />{t("work-project-detail-attach-goal")}</Button>
          </>
        )}
      </section>

      <section>
        <SectionHeader title={t("work-goal-inspector-assignees")} />
        <p className="mb-1.5 text-2xs text-text-dim">
          {t("work-project-detail-work-goes-to-these-agents-first")}
        </p>
        <AssigneePicker
          value={(p.assignees ?? []).map(assigneeWire)}
          onChange={(next) => void saveAssignees(next)}
          disabled={savingAssignees}
        />
        {assigneeError && (
          <p className="mt-1 text-2xs text-danger" role="alert">
            {assigneeError}
          </p>
        )}
      </section>

      <Dialog
        open={editing}
        onClose={() => setEditing(false)}
        title={t("work-project-detail-edit-project")}
        description={t("work-project-detail-slug-directory-name-cannot-change", { slug: p.slug })}
        footer={
          <>
            <Button variant="ghost" onClick={() => setEditing(false)}>{t("work-agent-editor-cancel")}</Button>
            <Button variant="primary" disabled={!name.trim() || busy} onClick={() => void save()}>
              {busy ? t("work-agent-editor-saving") : t("work-agent-editor-save")}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-3">
          <Field label={t("work-agent-editor-name")}>
            <TextInput autoFocus value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <div>
            <span className="mb-1 block text-2xs font-medium text-text-dim">{t("work-agent-editor-tags")}</span>
            <TagInput value={tags} onChange={setTags} suggestions={[...TAG_VOCABULARY]} />
          </div>
          <Field label={t("work-project-detail-group")} hint={t("work-project-detail-projects-same-group-sit-together-rail")}>
            <TextInput
              list="project-groups"
              value={group}
              placeholder={t("work-project-detail-eg-shop")}
              onChange={(e) => setGroup(e.target.value)}
            />
            <datalist id="project-groups">
              {groups.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
          </Field>
          <Field label={t("work-agent-editor-photo")} hint={t("work-project-detail-picture-project-s-row-header-png")}>
            <PhotoField id={p.id} name={name || p.name} photo={photo} onChange={setPhoto} disabled={busy} />
          </Field>
        </div>
      </Dialog>

      {/* The one attach dialog — the rail's project menu opens the same one. */}
      <AttachGoalDialog
        project={attaching ? { id: p.id, name: p.name, goals: data.goals } : null}
        goals={goals}
        onClose={() => setAttaching(false)}
        onAttached={refresh}
      />

      {/* The dialog's whole job is to be believed: every line above the last
          removes a fear, and the last is the one real consequence. */}
      <ConfirmDialog
        open={detaching !== null}
        onClose={() => setDetaching(null)}
        title={t("work-project-detail-detach-from", { name: p.name, goal: detaching ? goalLabel(detaching) : t("work-project-detail-this-goal") })}
        confirmLabel={t("work-library-refs-detach")}
        body={
          <>
            <p>{rich("work-project-detail-nothing-deleted-stays-at", { path: <code className="break-all">{data.path}</code> })}</p>
            <p className="mt-1.5">{t("work-project-detail-work-items-already-ran-keep-their")}</p>
            <p className="mt-1.5">
              {t("work-project-detail-agents-will-stop-seeing-project", { goal: detaching ? goalLabel(detaching) : t("work-project-detail-goal") })}
            </p>
          </>
        }
        onConfirm={() => {
          const g = detaching;
          setDetaching(null);
          if (g) act(t("work-project-detail-detached-from", { p: p.name, g: goalLabel(g) }), () => api.detachProject(pid, g));
        }}
      />

      <ConfirmDialog
        open={archiving}
        onClose={() => setArchiving(false)}
        title={t("work-project-detail-archive", { p: p.name })}
        confirmLabel={t("work-project-detail-archive-2")}
        body={t("work-project-detail-every-agent-harness-working-stopped-leaves")}
        onConfirm={() => {
          setArchiving(false);
          act(t("work-project-detail-project-archived"), () => api.archiveProject(pid, true));
        }}
      />

      {/* Removing: one question, the rail's own (`RemoveProjectDialog`). */}
      <RemoveProjectDialog
        project={deleting ? { name: p.name, archived: Boolean(p.archived), adopted, path: data.path, checkouts: data.workstreams.length } : null}
        onClose={() => setDeleting(false)}
        onChoose={(chosen, trash) => {
          setDeleting(false);
          remove(chosen, trash);
        }}
      />
    </div>
  );
}
