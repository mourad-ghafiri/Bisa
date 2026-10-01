/**
 * The dialog every project comes into existence through.
 *
 * Provenance is a *choice*, not a guess: the node refuses to infer `new` from
 * `clone` because of which fields happen to be filled in. This dialog mirrors
 * that — one segment per way in, and nothing inferred from which box has text
 * in it.
 *
 * There is no repository question any more. Create makes a folder in the
 * workspace and `git init`s it, because it is a folder Bisa made; the rule
 * that survived is the one about folders it did *not* make, which is why
 * "link it in place" still writes nothing at all. The old checkbox mostly got
 * left off, and a project without a repository gets copy workstreams and
 * `.patch` artifacts instead of branches and commits.
 *
 * # Three ways in, and no question about goals
 *
 * **Create · Clone · Import.** The wire has five `kind`s, and the reason those
 * numbers differ is worth stating.
 *
 * `import` and `adopt` are not two ways a project arrives — they are one way,
 * asked about a folder, with two answers to *where the files then live*. So
 * Import is one segment with a placement under it: **copy it in**, the default,
 * and **link it in place**, which is `adopt` and still writes nothing into the
 * folder. Making them peers in the picker asked a person to already know the
 * word "adopt" in order to find the thing they wanted.
 *
 * `link` went the other way, out of the dialog entirely. It creates nothing,
 * copies nothing and writes nothing: it makes a project that already exists
 * visible to a goal. It sat here twice over — a picker under the segments that
 * asked every new project which goal it was for, and a foot link that turned
 * the dialog into an attach form — and both made the common case, *make me a
 * project*, read as a decision about goals. Neither is here now. A project
 * made here is the workspace's; attaching it is `AttachGoalDialog`'s, from the
 * project's About › Goals and the rail's project menu, one record later. The
 * one exception is a door that already stands in a goal — a goal heading's
 * *Import into this goal…* — which fixes the goal: it is attached in the same
 * call (`POST /goals/{id}/projects`) and the dialog says where the project
 * lands in one line, never as a control.
 *
 * # The folder picker degrades
 *
 * Choosing a folder natively is the shell's, not the node's, so it exists only
 * under Tauri. A browser dev session gets the path input on its own rather than
 * a button that does nothing when clicked — see `api.ts`.
 *
 * Slug and path validation belong to the server (it is the one that has to join
 * the slug onto a real directory), so its refusals land on the field they are
 * about instead of in a toast that vanishes before you can read it. The mapping
 * from a refusal to a field, and everything else here that is arithmetic rather
 * than paint, is in `projectForm.mjs` where a test can reach it.
 */

import { useEffect, useMemo, useState } from "react";
import { ApiError, api, inDesktopShell, pickFolder } from "../../api";
import { GitConfigForm, useConfigEdits } from "./GitConfigForm";
import { navigate } from "../../router";
import { settingsSearch } from "../_settings/settingsLink.mjs";
import { formatIdent, globalIdentityOf } from "./gitConfigModel.mjs";
import { choiceOf } from "../../shell/settingsModel.mjs";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { accountOptions, inspectBody, inspectionLine, pinWrite, signInCaution } from "./projectCodeHostModel.mjs";
import { useAsync } from "./useAsync";
import type { ProjectCreated, PublishPolicy, RemoteInspection } from "../../types";
import type { SettingsTab } from "../_settings/settingsLink.mjs";
import {
  Button,
  Dialog,
  Field,
  ICON,
  Labelled,
  SegmentedControl,
  Select,
  TAG_VOCABULARY,
  TagInput,
  TextInput,
  useToast,
  type Segment,
} from "../../ui";
import {
  creationBody,
  COMMITTER_POLICIES,
  committerNote,
  creationGitConfig,
  gitConfigSection,
  hostChoiceWords,
  primaryLabel,
  refusalPlace,
  publishCaveat,
  importSummary,
  nameFromPath,
  provenancesFor,
  slugFromName,
  type DialogMode,
  type Placement,
  type ProjectField,
  type Provenance,
} from "./projectForm.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/** Stable empties for the form hook before the schema has arrived. */
const NO_SCHEMA: import("../../types").GitConfigKey[] = [];
const NO_ENTRIES: import("../../types").GitConfigEntry[] = [];
/** The keys the dialog shows before *More git config…*. */
const IDENTITY_KEYS = ["user.name", "user.email", "user.useConfigOnly"] as const;

/** Field-level errors, keyed by the input they belong to. */
type FieldErrors = Partial<Record<ProjectField, string>>;

const PROVENANCE: { id: Provenance; label: string; blurb: string }[] = [
  { id: "new", label: t("work-agent-editor-create"), blurb: t("work-new-project-dialog-start-empty-folder-workspace") },
  { id: "clone", label: t("work-new-project-dialog-clone"), blurb: t("work-new-project-dialog-git-clone-repository-into-workspace") },
  {
    id: "import",
    label: t("work-new-project-dialog-import"),
    blurb: t("work-new-project-dialog-bring-folder-already-exists-machine"),
  },
];

const PUBLISH: { id: PublishPolicy; label: string; blurb: string }[] = [
  {
    id: "auto",
    label: t("work-new-project-dialog-automatic-pushes-without-asking"),
    blurb: t("work-new-project-dialog-default-no-gate-branch-leaves-machine"),
  },
  {
    id: "gated",
    label: t("work-new-project-dialog-gated-human-approves-each-push"),
    blurb: t("work-new-project-dialog-push-pr-opens-gate-inbox-waits"),
  },
  {
    id: "manual",
    label: t("work-new-project-dialog-manual-never-publishes-from-here"),
    blurb: t("work-new-project-dialog-bisa-refuses-push-open-pr-run"),
  },
];

/** The segments for a mode: every way in when creating, the two existing ones when importing. */
const segmentsFor = (mode: DialogMode): readonly Segment<Provenance>[] =>
  provenancesFor(mode).map((id) => ({ id, label: PROVENANCE.find((k) => k.id === id)?.label ?? id }));

/**
 * Two options with equal weight, each keeping the sentence that says what it
 * does.
 *
 * Not a `SegmentedControl`: a segment has room for a word, and this decision
 * is consequential enough that the word alone would be a guess. Copying a
 * folder in decides who owns those files from then on, and linking it in
 * place is the one placement Bisa promises never to write into — not
 * something a person should discover after the fact.
 */
function Choice<T extends string>({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: T;
  onChange: (v: T) => void;
  options: { id: T; title: string; blurb: string }[];
}) {
  return (
    <Labelled label={label}>
      <div className="flex gap-2">
        {options.map((o) => (
          <button
            key={o.id}
            type="button"
            onClick={() => onChange(o.id)}
            aria-pressed={value === o.id}
            className={`anim flex-1 rounded-control border p-2 text-left ${
              value === o.id ? "border-accent/60 bg-accent-soft" : "border-border hover:bg-surface-2"
            }`}
          >
            <span className="block text-xs font-medium">{o.title}</span>
            <span className="mt-0.5 block text-2xs text-text-dim">{o.blurb}</span>
          </button>
        ))}
      </div>
    </Labelled>
  );
}

export function NewProjectDialog({
  open,
  onClose,
  mode = "create",
  goal,
  note,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  /**
   * `create` offers every way in and starts on Create; `import` (the rail's
   * import button) offers Import and Clone and starts on Import.
   */
  mode?: DialogMode;
  /**
   * Fixed by the door the dialog was opened from — a goal heading's *Import
   * into this goal…* — and attached in the same call. Never a picker: the
   * dialog asks nothing about goals, and with none the project is the
   * workspace's.
   */
  goal?: string;
  /** The opener's one sentence about where the project lands, shown under the segments. */
  note?: string | null;
  /** `goal` is what the project ended up attached to; `null` is the workspace alone. */
  onCreated: (created: ProjectCreated, goal: string | null) => void;
}) {
  const toast = useToast();
  const segments = segmentsFor(mode);
  const [provenance, setProvenance] = useState<Provenance>(segments[0].id);
  const [placement, setPlacement] = useState<Placement>("copy");
  const [slug, setSlug] = useState("");
  const [slugTouched, setSlugTouched] = useState(false);
  const [name, setName] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [url, setUrl] = useState("");
  const [path, setPath] = useState("");
  const [publish, setPublish] = useState<PublishPolicy>("auto");
  const [tags, setTags] = useState<string[]>([]);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [busy, setBusy] = useState(false);
  // Git config for the new repository: nothing is asked by default — one
  // footnote says who it will commit as (the global pair, inherited or pinned
  // by the workspace's `git.committer`) with *Change…* for local values; the
  // identity fields are open from the start only when the policy asks or no
  // identity resolves globally, so the repository is born with an author
  // rather than asking a moment later. Whatever is typed travels on the body
  // (`creationGitConfig`), whether or not a global identity resolves.
  const gitInfo = useAsync(async (s) => (open ? api.gitConfig(s) : null), [open]);
  const { resolved } = useResolvedSettings(null);
  const committerPolicy = choiceOf(resolved, "git.committer", COMMITTER_POLICIES, "inherit");
  const gitForm = useConfigEdits(gitInfo.data?.schema ?? NO_SCHEMA, gitInfo.data?.entries ?? NO_ENTRIES, "local");
  const globalIdent = gitInfo.data ? globalIdentityOf(gitInfo.data.entries) : null;
  /** *Override…* pressed: the identity fields are open although a global identity resolves. */
  const [overriding, setOverriding] = useState(false);
  /** *More git config…* pressed: every local key, not only the identity. */
  const [showingAll, setShowingAll] = useState(false);

  // The form is reset when the dialog opens or changes what it is for — the
  // mode, or the goal a door fixed — never on a parent's paint.
  useEffect(() => {
    if (!open) return;
    setOverriding(false);
    setShowingAll(false);
    gitForm.reset();
    setProvenance(segmentsFor(mode)[0].id);
    setPlacement("copy");
    setSlug("");
    setSlugTouched(false);
    setName("");
    setNameTouched(false);
    setUrl("");
    setPath("");
    setPublish("auto");
    setTags([]);
    setErrors({});
    // A reset is for an opening: the git form it clears must not re-open it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, goal, mode]);

  // Name first: a clone or an import starts its name from the tail
  // of the URL or the folder; the slug follows the name until it is typed.
  const derivedName = useMemo(
    () => (provenance === "clone" ? nameFromPath(url) : provenance === "import" ? nameFromPath(path) : ""),
    [provenance, path, url],
  );
  const effectiveName = nameTouched || !derivedName ? name : derivedName;
  const derived = slugFromName(effectiveName);
  const effectiveSlug = slugTouched ? slug : derived;

  // What the body carries is exactly what the person set — nothing means
  // "inherit the global config" — and it is carried whether or not a global
  // identity resolves. A linked-in-place import carries it too: the engine
  // writes an adopted repository's local config when the request asks.
  const gitSection = gitConfigSection(committerPolicy, globalIdent !== null);
  const gitConfigBody = creationGitConfig(gitForm.write);
  /** The identity fields are open: the section opens on them, or *Change…* was pressed under the footnote. */
  const gitFormOpen = gitSection === "open" || overriding;

  // What the URL or the folder is on — the code host, the accounts that could
  // speak for it — read offline from the node once the person stops typing.
  // The account chosen becomes the repository's `codehost.account`, carried
  // by the same git-config write as the identity.
  const inspectSource = inspectBody(provenance, url, path);
  const inspectKey = inspectSource ? `${provenance}:${"url" in inspectSource ? inspectSource.url : inspectSource.path}` : "";
  const [debouncedKey, setDebouncedKey] = useState("");
  useEffect(() => {
    const timer = window.setTimeout(() => setDebouncedKey(inspectKey), 350);
    return () => window.clearTimeout(timer);
  }, [inspectKey]);
  const inspection = useAsync(
    async (s) => {
      if (!open || !debouncedKey || !inspectSource) return null;
      return api.inspectRemote(inspectSource, s);
    },
    [open, debouncedKey],
  );
  const chosenAccount = gitForm.edits["codehost.account"] ?? null;
  const chooseAccount = (login: string | null) => {
    const write = pinWrite(login);
    gitForm.setEdit("codehost.account", write ? write.value : "");
  };

  /** A goal to ask about a gated push: only the one a door fixed — the dialog itself never asks. */
  const hasGoal = !!goal;
  const ready =
    effectiveName.trim() !== "" &&
    effectiveSlug.trim() !== "" &&
    (provenance !== "clone" || url.trim() !== "") &&
    (provenance !== "import" || path.trim() !== "") &&
    (!gitFormOpen || gitForm.valid);

  /**
   * The native picker, when there is one. A cancelled dialog returns `null`
   * and must leave the typed path alone — a person who typed a path and then
   * changed their mind about browsing has not asked for it to be cleared.
   */
  const browse = async () => {
    try {
      const chosen = await pickFolder(t("work-new-project-dialog-choose-folder-import"));
      if (!chosen) return;
      setPath(chosen);
      setErrors((e) => ({ ...e, path: undefined }));
    } catch (e) {
      // The shell could not open its picker: said beside the folder, where a path can still be typed.
      setErrors((was) => ({ ...was, path: e instanceof Error ? e.message : String(e) }));
    }
  };

  const announce = (created: ProjectCreated) => {
    if (placement === "link" && provenance === "import") {
      toast.ok(t("work-new-project-dialog-linked-place-nothing-written-into", { project: created.project.name }));
      return;
    }
    const summary = importSummary(created.imported);
    toast.ok(summary ? t("work-new-project-dialog-imported", { project: created.project.name, summary }) : t("work-new-project-dialog-project-created", { project: created.project.name }));
  };

  const submit = async () => {
    if (!ready || busy) return;
    setBusy(true);
    setErrors({});
    try {
      // A goal a door fixed is attached by the route as it creates; with none the project is the workspace's.
      const created = await api.createProject(creationBody({ provenance, placement, slug: effectiveSlug, name: effectiveName, publish, tags, gitConfig: gitConfigBody, url, path }), goal ?? null);
      announce(created);
      onCreated(created, goal ?? null);
      onClose();
    } catch (e) {
      // The node's own sentence, beside the input it is about when the form shows it, else on the form.
      // Placed by the refusal's id — the same in every language — never by its words.
      const answer = e instanceof ApiError ? e : null;
      setErrors(refusalPlace({ message: e instanceof Error ? e.message : String(e), refusal: answer?.refusal, status: answer?.status }, provenance));
    } finally {
      setBusy(false);
    }
  };

  const err = (key: ProjectField) =>
    errors[key] ? <span className="text-danger">{errors[key]}</span> : undefined;

  const copying = provenance === "import" && placement === "copy";

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={mode === "import" ? t("work-new-project-dialog-import-project") : t("work-new-project-dialog-new-project")}
      description={
        mode === "import"
          ? t("work-new-project-dialog-folder-from-machine-repository-cloned-from")
          : t("work-new-project-dialog-project-real-folder-workspace-without-git")
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t("work-agent-editor-cancel")}</Button>
          <Button variant="primary" disabled={!ready || busy} onClick={() => void submit()}>
            {busy ? t("work-after-merge-dialog-working") : primaryLabel(provenance, placement)}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <SegmentedControl
          label={t("work-new-project-dialog-how-project-arrives")}
          options={segments}
          value={provenance}
          onChange={(k) => {
            setProvenance(k);
            setErrors({});
          }}
        />
        <p className="text-2xs text-text-dim">
          {PROVENANCE.find((k) => k.id === provenance)?.blurb}
        </p>

        {errors.form && (
          <p
            role="alert"
            className="flex items-start gap-1.5 rounded-control border border-border bg-danger-soft px-2 py-1.5 text-2xs text-danger"
          >
            <ICON.danger size={12} aria-hidden className="mt-px shrink-0" />
            {errors.form}
          </p>
        )}

        {/* Where the project lands, in the opener's words: a goal heading's door
            names its goal, the toolbar's says the Workspace tab. The dialog
            itself asks nothing about goals — attaching is About's and the
            rail's dialog, one record later. */}
        {note && <p className="text-2xs text-text-dim">{note}</p>}

        {provenance === "clone" && (
          <>
            <Field label={t("work-new-project-dialog-repository-url")} hint={err("url") ?? t("work-new-project-dialog-anything-git-can-clone")}>
              <TextInput
                autoFocus
                value={url}
                placeholder="https://github.com/owner/repo.git" // for the machine
                onChange={(e) => setUrl(e.target.value)}
              />
            </Field>
            <CodeHostLine inspection={inspection.data ?? null} reading={inspection.loading} chosen={chosenAccount} onChoose={chooseAccount} />
          </>
        )}

        {provenance === "import" && (
          <>
            <Field
              label={t("work-new-project-dialog-folder")}
              hint={
                err("path") ??
                t("work-new-project-dialog-absolute-path-folder-already-exists-anywhere")
              }
            >
              <div className="flex gap-2">
                <TextInput
                  autoFocus
                  value={path}
                  placeholder="/Users/you/code/storefront" // for the machine
                  onChange={(e) => setPath(e.target.value)}
                  className="min-w-0 flex-1 font-mono"
                />
                {/* Only under the desktop shell: a browser has no picker
                    to ask, and a button that does nothing is worse than no
                    button. The typed path works in both. */}
                {inDesktopShell() && (
                  <Button type="button" onClick={() => void browse()}>
                    <ICON.folder size={13} aria-hidden />{t("work-new-project-dialog-choose")}</Button>
                )}
              </div>
            </Field>

            <CodeHostLine inspection={inspection.data ?? null} reading={inspection.loading} chosen={chosenAccount} onChoose={chooseAccount} />

            <Choice<Placement>
              label={t("work-new-project-dialog-where-files-live")}
              value={placement}
              onChange={setPlacement}
              options={[
                {
                  id: "copy",
                  title: t("work-new-project-dialog-copy"),
                  blurb: t("work-new-project-dialog-files-become-workspace-s-history-all"),
                },
                {
                  id: "link",
                  title: t("work-new-project-dialog-link-place"),
                  blurb: t("work-new-project-dialog-stays-where-bisa-never-writes-into"),
                },
              ]}
            />
          </>
        )}

        <Field label={t("work-agent-editor-name")} hint={err("name") ?? t("work-new-project-dialog-what-project-called-everywhere-app")}>
          <TextInput
            autoFocus={provenance === "new"}
            value={effectiveName}
            placeholder={t("work-new-project-dialog-storefront")}
            onChange={(e) => {
              setNameTouched(true);
              setName(e.target.value);
            }}
          />
        </Field>

        <Field
          label={t("work-new-project-dialog-slug")}
          hint={
            err("slug") ??
            t("work-new-project-dialog-folder-name-from-name-lowercase-letters")
          }
        >
          <TextInput
            value={effectiveSlug}
            placeholder="storefront" // content, never translated: an example of a project's name
            onChange={(e) => {
              setSlugTouched(true);
              setSlug(e.target.value);
            }}
            className="font-mono"
          />
        </Field>

        {provenance === "new" && (
          <p className="text-2xs text-text-dim">{t("work-new-project-dialog-folder-created-git-repository-so-workstreams")}</p>
        )}

        {provenance === "import" && (
          <p className="text-2xs text-text-dim">
            {copying ? rich("work-new-project-dialog-import-copies-git", { code: (inner) => <code>{inner}</code> }) : rich("work-new-project-dialog-import-reads-git")}
          </p>
        )}

        <Field label={t("work-new-project-dialog-publishing")} hint={[PUBLISH.find((p) => p.id === publish)?.blurb, publishCaveat(publish, hasGoal)].filter(Boolean).join(" ")}>
          <Select value={publish} onChange={(e) => setPublish(e.target.value as PublishPolicy)}>
            {PUBLISH.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </Select>
        </Field>

        <Labelled label={t("work-agent-editor-tags")} hint={t("work-new-goal-dialog-how-files-optional")}>
          <TagInput value={tags} onChange={setTags} suggestions={[...TAG_VOCABULARY]} />
        </Labelled>

        {/* Who commits in the new repository. `note`: one footnote and
            *Change…*, nothing asked. `open`: the identity fields, because
            the workspace's policy asks or nothing resolves globally.
            Whatever is typed here is written to the repository's local
            git config at creation and read back under About › Checkout. */}
        {gitSection === "note" && globalIdent && !gitFormOpen && (
          <p className="flex flex-wrap items-center gap-x-2 text-2xs text-text-dim">
            <span>{committerNote(committerPolicy, formatIdent(globalIdent))}</span>
            <button type="button" className="underline decoration-dotted underline-offset-2 hover:text-text" onClick={() => setOverriding(true)}>{t("work-new-project-dialog-change")}</button>
          </p>
        )}
        {gitInfo.data && gitFormOpen && (
          <Labelled
            label={t("work-checkout-view-git-config")}
            hint={
              globalIdent
                ? t("work-new-project-dialog-repository-s-own-git-config-over")
                : t("work-new-project-dialog-no-identity-global-git-config-set")
            }
          >
            <div className="flex flex-col gap-2">
              <GitConfigForm form={gitForm} scope="local" only={showingAll ? undefined : IDENTITY_KEYS} />
              <div className="flex flex-wrap items-center gap-3">
                {!showingAll && (
                  <button
                    type="button"
                    className="self-start text-2xs text-text-dim underline decoration-dotted underline-offset-2 hover:text-text"
                    onClick={() => setShowingAll(true)}
                  >{t("work-new-project-dialog-more-git-config")}</button>
                )}
                {overriding && globalIdent && (
                  <button
                    type="button"
                    className="self-start text-2xs text-text-dim underline decoration-dotted underline-offset-2 hover:text-text"
                    onClick={() => {
                      setOverriding(false);
                      setShowingAll(false);
                      gitForm.reset();
                    }}
                  >{t("work-new-project-dialog-inherit-global-config-instead")}</button>
                )}
              </div>
            </div>
          </Labelled>
        )}
      </div>
    </Dialog>
  );
}

/**
 * One line under the URL or the folder: what it is on — *GitHub · acme/web ·
 * SSH via github-acme · as @ada* — the accounts that could speak for it as a
 * select (the suggested one first; choosing one pins the repository to it
 * through `codehost.account`), and the caution with its Settings door when
 * nobody is signed in. The words are `projectCodeHostModel.mjs`'s.
 */
function CodeHostLine({
  inspection,
  reading,
  chosen,
  onChoose,
}: {
  inspection: RemoteInspection | null;
  reading: boolean;
  chosen: string | null;
  onChoose: (login: string | null) => void;
}) {
  const line = inspectionLine(inspection, chosen);
  if (!line) return reading ? <p className="text-2xs text-text-dim">{t("work-new-project-dialog-reading-remote")}</p> : null;
  const options = accountOptions(inspection);
  const caution = signInCaution(inspection);
  const tone = line.tone === "ok" ? "text-text" : line.tone === "warn" ? "text-warn" : "text-text-dim";
  return (
    <div className="flex flex-col gap-1 rounded-control border border-border bg-surface-2 px-2 py-1.5 text-2xs">
      <div className="flex flex-wrap items-center gap-2">
        <ICON.repository size={13} aria-hidden className="shrink-0 text-text-dim" />
        <span className={`min-w-0 flex-1 truncate ${tone}`} title={line.text}>
          {line.text}
        </span>
        {options.length > 0 && (
          <Select aria-label={t("work-new-project-dialog-account-repository-speaks")} value={chosen ?? ""} className="w-64" onChange={(e) => onChoose(e.target.value || null)}>
            <option value="">{hostChoiceWords(inspection?.suggested)}</option>
            {options.map((o) => (
              <option key={o.login} value={o.login}>
                {o.words}
                {o.suggested ? ` · ${t("work-new-project-dialog-suggested")}` : ""}
              </option>
            ))}
          </Select>
        )}
      </div>
      {caution && (
        <div className="flex flex-wrap items-center gap-2 text-warn">
          <ICON.warn size={12} aria-hidden className="shrink-0" />
          <span className="min-w-0 flex-1">{caution.text}</span>
          {caution.tab && (
            <button
              type="button"
              className="anim text-2xs text-accent-ink underline underline-offset-2 hover:text-text"
              onClick={() => navigate({ name: "settings" }, settingsSearch(caution.tab as SettingsTab))}
            >{t("work-new-project-dialog-sign-under-settings")}</button>
          )}
        </div>
      )}
    </div>
  );
}
