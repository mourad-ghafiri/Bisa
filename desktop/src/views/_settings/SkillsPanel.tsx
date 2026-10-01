/**
 * The skill library: what your agents know how to do, as opposed to who they
 * are.
 *
 * A skill used to be a markdown file copied under one agent's directory, so
 * the same code-review checklist existed once per reviewer and drifted from
 * every other copy. It is one library now, and agents carry ids. That is the
 * whole reason this panel exists: a procedure is edited here, once, and every
 * agent referencing it follows the new version on its next launch.
 *
 * It lives in Settings beside the MCP registry because both are *capabilities
 * an agent may be given* rather than definitions of anybody, and a reader
 * looking for either was previously expected to know it was hiding behind a
 * tab inside the agent roster. It is also the **one place a skill is written**:
 * the agent editor picks from this library and opens the door here, never a
 * form of its own (an editor inside an editor was a draft lost with the
 * agent's and a library entry made as a side effect of staffing).
 *
 * Two rules are stated on the panel rather than in this comment, because the
 * mistakes they prevent are made by whoever is looking at the screen:
 * a skill is a *procedure*, not a second copy of the agent's role; and every
 * skill an agent carries is delivered into every session it runs, so the list
 * is a per-launch cost rather than a shelf.
 */

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "../../api";
import { navigate } from "../../router";
import type { AgentDef, SkillDef } from "../../types";
import {
  Button,
  Card,
  Dialog,
  EmptyState,
  ErrorNote,
  Field,
  ICON,
  Labelled,
  Markdown,
  NO_TAG_FILTER,
  Pending,
  ReadLine,
  TAG_VOCABULARY,
  TagChips,
  TagFilterBar,
  TagInput,
  TextArea,
  TextInput,
  passesTagFilter,
  useToast,
  type TagFilterState,
} from "../../ui";
import { catalogOffer, useCatalog } from "./CatalogPanel";
import { settingsSearch } from "./settingsLink.mjs";
import {
  DeleteButton,
  DeleteDialog,
  SKILL_BUDGET,
  SKILL_RULE,
  useSkillLibrary,
  useUsage,
} from "../_work/LibraryRefs";
import { OriginChip } from "../_work/Origin";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, readWords } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";

// ---------------------------------------------------------------------------
// Writing one
// ---------------------------------------------------------------------------

interface SkillDraft {
  id: string;
  name: string;
  description: string;
  tags: string[];
  markdown: string;
}

const emptySkillDraft = (): SkillDraft => ({
  id: "",
  name: "",
  description: "",
  tags: [],
  markdown: "",
});

const draftOf = (s: SkillDef): SkillDraft => ({
  id: s.id,
  name: s.name,
  description: s.description,
  tags: [...(s.tags ?? [])],
  markdown: s.markdown,
});

/** The id rule, applied optimistically so the field is usually already right. */
function slugify(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Enough to send: the node refuses a blank description rather than shipping one. */
const skillDraftReady = (d: SkillDraft): boolean =>
  Boolean(d.id.trim() && d.name.trim() && d.description.trim() && d.markdown.trim());

/**
 * The fields of a skill, shared by this panel and the agent editor's inline
 * "write one now" — so a skill created while staffing an agent is the same
 * object, with the same required description, as one created here.
 */
function SkillFields({
  d,
  onChange,
  existing,
  disabled,
}: {
  d: SkillDraft;
  onChange: (patch: Partial<SkillDraft>) => void;
  /** True when editing: the id is immutable, so it is shown and not offered. */
  existing: boolean;
  disabled?: boolean;
}) {
  // The id tracks the name until somebody types an id of their own; comparing
  // against the slug of the current name is what detects that, with no extra
  // state to reset when the dialog is re-seeded.
  const onName = (name: string) => {
    const tracking = !existing && (d.id === "" || d.id === slugify(d.name));
    onChange(tracking ? { name, id: slugify(name) } : { name });
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label={t("settings-git-profiles-panel-name")}>
          <TextInput
            value={d.name}
            autoFocus
            disabled={disabled}
            placeholder={t("settings-skills-panel-incident-comms")}
            onChange={(e) => onName(e.target.value)}
          />
        </Field>
        <Field
          label={t("settings-mcp-panel-id")}
          hint={
            existing
              ? t("settings-skills-panel-immutable-agents-reference-rename-silently-detached")
              : t("settings-skills-panel-slug-agents-reference-fixed-once-created")
          }
        >
          <TextInput
            value={d.id}
            className="font-mono"
            disabled={disabled || existing}
            placeholder="incident-comms" // for the machine
            onChange={(e) => onChange({ id: e.target.value })}
          />
        </Field>
      </div>

      <Field
        label={t("settings-mcp-panel-description")}
        hint={t("settings-skills-panel-one-line-model-reads-before-deciding")}
      >
        <TextInput
          value={d.description}
          disabled={disabled}
          placeholder={t("settings-skills-panel-use-when-incident-needs-status-page")}
          onChange={(e) => onChange({ description: e.target.value })}
        />
      </Field>

      <Labelled label={t("settings-mcp-panel-tags")} hint={t("settings-skills-panel-how-files-draw-from-vocabulary-where")}>
        <TagInput
          value={d.tags}
          onChange={(tags) => onChange({ tags })}
          suggestions={[...TAG_VOCABULARY]}
        />
      </Labelled>

      <Field label={t("settings-skills-panel-procedure")} hint={SKILL_RULE}>
        <TextArea
          rows={12}
          value={d.markdown}
          className="font-mono"
          disabled={disabled}
          placeholder={t("settings-skills-panel-procedure-skeleton")}
          onChange={(e) => onChange({ markdown: e.target.value })}
        />
      </Field>
    </div>
  );
}

/** Create or edit a skill in one pass. `skill === null` creates. */
function SkillDialog({
  open,
  skill,
  onClose,
  onSaved,
}: {
  open: boolean;
  skill: SkillDef | null;
  onClose: () => void;
  onSaved: (s: SkillDef) => void;
}) {
  const toast = useToast();
  const [d, setD] = useState<SkillDraft>(emptySkillDraft());
  const [busy, setBusy] = useState(false);

  // Re-seed whenever the dialog opens, so editing never shows the last skill.
  useEffect(() => {
    if (!open) return;
    setD(skill ? draftOf(skill) : emptySkillDraft());
  }, [open, skill]);

  const save = async () => {
    if (!skillDraftReady(d)) return;
    setBusy(true);
    let saved: SkillDef | null = null;
    await attempt(async () => {
      const res = skill
        ? await api.patchSkill(skill.id, {
            name: d.name.trim(),
            description: d.description.trim(),
            tags: d.tags,
            markdown: d.markdown,
          })
        : await api.createSkill({
            id: d.id.trim(),
            name: d.name.trim(),
            description: d.description.trim(),
            markdown: d.markdown,
            tags: d.tags,
          });
      saved = res.skill;
    }, toast.error);
    setBusy(false);
    if (saved) {
      toast.ok(skill ? t("settings-skills-panel-skill-updated") : t("settings-skills-panel-skill-added-library"));
      onSaved(saved);
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={skill ? t("settings-skills-panel-edit", { skill: skill.name }) : t("settings-skills-panel-new-skill")}
      description={SKILL_RULE}
      width="max-w-2xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>{t("settings-connectors-panel-cancel")}</Button>
          <Button
            variant="primary"
            onClick={() => void save()}
            disabled={busy || !skillDraftReady(d)}
          >
            {busy ? t("settings-connectors-panel-saving") : skill ? t("settings-decisions-panel-save") : t("settings-skills-panel-create-skill")}
          </Button>
        </>
      }
    >
      <SkillFields
        d={d}
        onChange={(patch) => setD((p) => ({ ...p, ...patch }))}
        existing={!!skill}
        disabled={busy}
      />
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Browsing the library
// ---------------------------------------------------------------------------

function SkillCard({
  s,
  carriers,
  expanded,
  onToggle,
  onEdit,
  deleteControl,
}: {
  s: SkillDef;
  /** The agents currently referencing it, by name. */
  carriers: string[];
  expanded: boolean;
  onToggle: () => void;
  onEdit: () => void;
  /**
   * Passed in rather than built here, because what this control says depends
   * on a usage check the panel runs for the one expanded skill — see the
   * panel. A card that ran its own would be one request per row.
   */
  deleteControl: ReactNode;
}) {
  return (
    <Card>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          className="anim flex min-w-0 items-center gap-1.5 text-left"
        >
          <ICON.collapsed
            size={11}
            aria-hidden
            className={`anim shrink-0 text-text-dim ${expanded ? "rotate-90" : ""}`}
          />
          <span className="truncate text-xs font-medium">{s.name}</span>
        </button>
        <OriginChip origin={s.origin} id={s.id} />
        <TagChips tags={s.tags ?? []} max={4} />
        <span className="ml-auto shrink-0 text-2xs text-text-dim">
          {carriers.length === 0
            ? t("settings-skills-panel-carried-nobody")
            : t("settings-skills-panel-carried-agent-agents", { carriers: carriers.length })}
        </span>
      </div>

      <p className="mt-1 text-2xs text-text-dim">{s.description}</p>
      <code className="mt-0.5 block font-mono text-2xs text-text-dim">{s.id}</code>

      {expanded && (
        <div className="mt-3 flex flex-col gap-3 border-t border-border pt-3">
          {carriers.length > 0 && (
            <p className="text-2xs text-text-dim">{t("settings-skills-panel-delivered-into-every-session-run-by", { carriers: carriers.join(", ") })}</p>
          )}
          {s.markdown.trim() ? (
            <Markdown text={s.markdown} />
          ) : (
            <p className="text-2xs text-text-dim">{t("settings-skills-panel-skill-has-body")}</p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button size="sm" onClick={onEdit}>{t("settings-connectors-panel-edit")}</Button>
            {deleteControl}
          </div>
        </div>
      )}
    </Card>
  );
}

export function SkillsPanel() {
  const toast = useToast();
  const { skills, error, loading, reload } = useSkillLibrary();
  // The roster, for the "carried by N agents" line on each row. It is a
  // convenience, not the authority: whether a delete may go through is
  // decided by `GET /usage/skill/{id}`, asked below before Delete is offered.
  const roster = useAsync((s) => api.agents(s), []);
  const agentData = roster.data;
  // Only so an empty library can quote this build's real numbers rather than
  // a figure typed into a string that goes stale.
  const catalog = useCatalog();

  const [query, setQuery] = useState("");
  const [tagFilter, setTagFilter] = useState<TagFilterState>(NO_TAG_FILTER);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<SkillDef | null>(null);
  const [deleting, setDeleting] = useState<SkillDef | null>(null);

  // One check, for the one skill whose body is open — Delete lives inside the
  // expanded card, so this is every skill whose control could be pressed. A
  // check per row would be eighteen requests to draw a list nobody has acted
  // on yet. The dialog's target wins while it is open, so the answer on
  // screen is always about the skill named in the title.
  const usage = useUsage("skill", deleting?.id ?? expanded);

  const agents: AgentDef[] = useMemo(() => agentData?.agents ?? [], [agentData?.agents]);

  /** Who references each id. Computed once rather than per card. */
  const carriers = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const a of agents) {
      for (const id of a.skills) m.set(id, [...(m.get(id) ?? []), a.name]);
    }
    return m;
  }, [agents]);

  // The facets come from the whole library, not from what the search left, so
  // a selected tag cannot vanish under you as you type.
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return skills.filter(
      (s) =>
        passesTagFilter(s.tags ?? [], tagFilter) &&
        (!q ||
          s.name.toLowerCase().includes(q) ||
          s.id.includes(q) ||
          s.description.toLowerCase().includes(q)),
    );
  }, [skills, query, tagFilter]);

  // Narrowed once, so the dialog's callbacks close over a plain id rather
  // than over a nullable that TypeScript cannot prove is still set by the
  // time `remove` runs.
  const deletingId = deleting?.id ?? "";

  const editors = (
    <>
      <SkillDialog
        open={creating || editing !== null}
        skill={editing}
        onClose={() => {
          setCreating(false);
          setEditing(null);
        }}
        onSaved={(s) => {
          reload();
          if (!editing) setExpanded(s.id);
        }}
      />

      <DeleteDialog
        open={deleting !== null}
        title={deleting ? t("settings-skills-panel-delete", { deleting: deleting.name }) : ""}
        usage={usage}
        consequence={t("settings-skills-panel-procedure-markdown-gone-cannot-undone-nothing")}
        note={t("settings-skills-panel-agent-carrying-skill-broken-keeping-detaching")}
        remove={() => api.deleteSkill(deletingId)}
        onClose={() => setDeleting(null)}
        onDeleted={() => {
          toast.ok(t("settings-skills-panel-skill-deleted"));
          if (expanded === deletingId) setExpanded(null);
          reload();
        }}
      />
    </>
  );

  if (loading && skills.length === 0) return <Pending what={t("settings-skills-panel-library")} rows={pendingRows(t("settings-skills-panel-library"))} />;
  if (error && skills.length === 0) return <ErrorNote error={error} retry={reload} />;
  // The roster is a side read for the *carried by* line: its failure is said, not swallowed.
  const rosterWords = roster.error && !roster.data ? readWords({ what: t("settings-security-panels-agents"), error: roster.error, data: null }) : null;

  if (skills.length === 0) {
    return (
      <>
        <EmptyState
          title={t("settings-skills-panel-skills-library-yet")}
          icon={ICON.skill}
          hint={`${catalogOffer(catalog, "skill")} ${SKILL_RULE}`}
          action={
            <div className="flex flex-wrap justify-center gap-2">
              <Button
                variant="primary"
                onClick={() =>
                  navigate({ name: "settings" }, settingsSearch("catalog-skill"))
                }
              >{t("settings-skills-panel-browse-catalog")}</Button>
              <Button onClick={() => setCreating(true)}>{t("settings-skills-panel-write-one-yourself")}</Button>
            </div>
          }
        />
        {editors}
      </>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {rosterWords && <ReadLine words={rosterWords} onReload={roster.reload} />}
      <div className="flex flex-wrap items-start gap-2">
        <p className="max-w-xl text-2xs text-text-dim">
          <strong className="font-medium text-text">{SKILL_RULE}</strong> {SKILL_BUDGET}
        </p>
        <Button size="sm" variant="ghost" className="ml-auto shrink-0" onClick={reload}>{t("settings-catalog-panel-refresh")}</Button>
        <Button
          size="sm"
          variant="primary"
          className="shrink-0"
          onClick={() => setCreating(true)}
        >{t("settings-skills-panel-new-skill")}</Button>
      </div>

      {error && <ErrorNote error={error} retry={reload} />}

      <div className="flex flex-wrap items-center gap-2">
        <TextInput
          value={query}
          placeholder={t("settings-skills-panel-search-skills")}
          aria-label={t("settings-skills-panel-search-skills-2")}
          className="h-7 max-w-64 py-0"
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="tnum text-2xs text-text-dim">{t("settings-skills-panel-words", { shown: shown.length, skills: skills.length })}</span>
      </div>

      <TagFilterBar items={skills} tagsOf={tagsOfSkill} value={tagFilter} onChange={setTagFilter} />

      {shown.length === 0 ? (
        <EmptyState
          title={t("settings-catalog-panel-nothing-matches")}
          icon={ICON.skill}
          hint={t("settings-skills-panel-skill-library-matches-search-tags-have")}
          action={
            <Button
              onClick={() => {
                setQuery("");
                setTagFilter(NO_TAG_FILTER);
              }}
            >{t("settings-catalog-panel-clear-filters")}</Button>
          }
        />
      ) : (
        <div className="flex flex-col gap-2">
          {shown.map((s) => (
            <SkillCard
              key={s.id}
              s={s}
              carriers={carriers.get(s.id) ?? []}
              expanded={expanded === s.id}
              onToggle={() => setExpanded(expanded === s.id ? null : s.id)}
              onEdit={() => setEditing(s)}
              deleteControl={
                <DeleteButton usage={usage} onOpen={() => setDeleting(s)} />
              }
            />
          ))}
        </div>
      )}

      {editors}
    </div>
  );
}

/** Stable accessor — {@link TagFilterBar} keys its facets on the list, not on this. */
const tagsOfSkill = (s: SkillDef): string[] => s.tags ?? [];
