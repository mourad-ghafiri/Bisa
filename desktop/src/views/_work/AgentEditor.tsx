/**
 * An agent is a definition: prompt + harness + model plan + skills + MCPs.
 *
 * The model control is a **plan**, not a single choice — an ordered list plus
 * the strategy for choosing among it — so a quota wall on the first model is a
 * retry rather than a dead agent. That control is {@link ModelPlanEditor},
 * which also carries the engine's live health ledger — so this dialog is
 * where you find out *why* an agent is running on its second-choice model.
 *
 * Two agents render differently: `general-agent` and
 * `workflow-agent`, the two core agents that hold a record. Only the
 * harness, the model plan and the decision-making switch of each may be
 * changed, and the store refuses an
 * update that touches anything else **by name**. So the rest of its
 * definition is shown read-only rather than hidden — a user should be able to
 * read the prompt that is driving their workspace — and this dialog sends
 * only the three fields that can change, so a save is never a refusal it
 * provoked itself.
 *
 * Skills and MCP servers are **pickers over shared libraries**, never content
 * typed in here. Both used to be authored per agent — a markdown body in one
 * textarea, a raw JSON transport in another — which is how twenty reviewers
 * ended up with twenty drifting checklists, and how one machine's command
 * lines rode out inside a public agent snapshot. Writing a skill is Settings
 * › Skills' job and registering a server the registry's: this dialog picks
 * references and opens the door to where each is made — an editor inside an
 * editor was a draft lost with the agent's and a library entry made as a
 * side effect of staffing. Wide (`max-w-4xl`) for what it holds: the model
 * plan's rows on one line, the two pickers side by side.
 */

import { useEffect, useRef, useState } from "react";
import { api } from "../../api";
import { href } from "../../router";
import type { AgentDef, HarnessRow } from "../../types";
import { isCoreAgent } from "../../types";
import {
  Button,
  Dialog,
  ErrorNote,
  Field,
  Labelled,
  PhotoField,
  Select,
  Switch,
  TagChips,
  TagInput,
  TAG_VOCABULARY,
  TextArea,
  TextInput,
  useToast,
} from "../../ui";
import {
  AttachedRefs,
  MCP_REGISTRY_NOTE,
  RefPicker,
  resolveRefs,
  SKILL_BUDGET,
  SKILL_RULE,
  type Library,
} from "./LibraryRefs";
import { ModelPlanEditor } from "./ModelPlanEditor";
import { draftOf, emptyDraft, harnessChoices, maySaveAgent, saveOf, startingHarness, type AgentDraft } from "./agentDraftModel.mjs";
import { CORE_AGENT_LOCKED, CORE_AGENT_PURPOSE } from "./Origin";
import { settingsSearch } from "../_settings/settingsLink.mjs";
import { attempt } from "./useAsync";
import { t } from "../../i18n/l10n.mjs";

/**
 * How a control that cannot be written looks. `readOnly` rather than
 * `disabled`, deliberately: a disabled textarea cannot be focused, scrolled
 * with the keyboard or selected, and the whole point of showing the core
 * agent's prompt is that somebody can read and copy it.
 */
const READ_ONLY = "bg-surface-2 text-text-dim focus:border-border";

export function AgentEditor({
  open,
  agent,
  harnesses,
  skills,
  mcps,
  onClose,
  onSaved,
}: {
  open: boolean;
  /** null = create a new agent. */
  agent: AgentDef | null;
  harnesses: HarnessRow[];
  /** The two libraries, loaded once for the screen and shared with the roster. */
  skills: Library;
  mcps: Library;
  onClose: () => void;
  onSaved: () => void;
}) {
  const toast = useToast();
  // The harness a new agent starts on is the first installed here — none is made up, and none is picked where none is installed.
  const starting = startingHarness(harnesses);
  const startingAtOpen = useRef(starting);
  startingAtOpen.current = starting;
  const [d, setD] = useState<AgentDraft>(() => emptyDraft(starting));
  const [busy, setBusy] = useState(false);
  const core = agent !== null && isCoreAgent(agent);

  const set = (patch: Partial<AgentDraft>) => setD((prev) => ({ ...prev, ...patch }));

  // Seeded when the dialog opens, and never again while it is open: the
  // harness list arriving, or read again, must not take what was typed.
  useEffect(() => {
    if (!open) return;
    setD(agent ? draftOf(agent) : emptyDraft(startingAtOpen.current));
  }, [open, agent]);
  // The list may land after the dialog opened: a draft that has no harness yet takes the first installed, and nothing else of it moves.
  useEffect(() => {
    if (open && starting !== null) setD((prev) => (prev.harness === null ? { ...prev, harness: starting } : prev));
  }, [open, starting]);
  const choices = harnessChoices(harnesses, d.harness);

  const save = async () => {
    setBusy(true);
    const ok = await attempt(async () => {
      // What is sent is the model's to say (`saveOf`): a core agent's three
      // fields — a stale draft is never a refusal naming a field nobody
      // touched — an agent's whole definition in one request, or a new one.
      const save = saveOf(d, agent, core);
      if (save.kind === "patch") await api.patchAgent(save.id, save.body);
      else await api.createAgent(save.body);
    }, toast.error);
    setBusy(false);
    if (ok) {
      onSaved();
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={agent ? t("work-agent-editor-edit", { agent: agent.name }) : t("work-agent-editor-new-agent")}
      description={
        core
          ? t("work-agent-editor-harness-model-plan-decision-making-switch")
          : t("work-agent-editor-prompt-harness-model-plan-tags-skills")
      }
      width="max-w-4xl"
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>{t("work-agent-editor-cancel")}</Button>
          <Button
            variant="primary"
            onClick={() => void save()}
            disabled={!maySaveAgent(d, busy)}
          >
            {busy ? t("work-agent-editor-saving") : agent ? t("work-agent-editor-save") : t("work-agent-editor-create")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        {core && (
          <p className="rounded-control border border-border bg-surface-2/70 px-3 py-2 text-2xs leading-relaxed text-text">
            {CORE_AGENT_PURPOSE} {t("work-agent-editor-harness-model-plan-decision-making-switch-editable")}
          </p>
        )}

        {!core && (
          <Field label={t("work-agent-editor-photo")} hint={t("work-agent-editor-picture-agent-s-card-every-message")}>
            <PhotoField id={agent?.pubkey ?? "agent"} name={d.name || agent?.name} photo={d.photo} onChange={(photo) => set({ photo })} disabled={busy} />
          </Field>
        )}
        <div className="grid grid-cols-2 gap-3">
          <Field label={t("work-agent-editor-name")} hint={core ? CORE_AGENT_LOCKED : undefined}>
            <TextInput
              value={d.name}
              readOnly={core}
              className={core ? READ_ONLY : ""}
              autoFocus={!core}
              onChange={(e) => set({ name: e.target.value })}
            />
          </Field>
          <Field
            label={t("work-agent-editor-description")}
            hint={core ? CORE_AGENT_LOCKED : t("work-agent-editor-one-line-list")}
          >
            <TextInput
              value={d.description}
              readOnly={core}
              className={core ? READ_ONLY : ""}
              onChange={(e) => set({ description: e.target.value })}
            />
          </Field>
        </div>

        <Field
          label={t("work-agent-editor-system-prompt")}
          hint={core ? CORE_AGENT_LOCKED : t("work-agent-editor-role-how-works-what-refuses-do")}
        >
          <TextArea
            rows={core ? 12 : 8}
            value={d.system_prompt}
            readOnly={core}
            className={`font-mono ${core ? READ_ONLY : ""}`}
            onChange={(e) => set({ system_prompt: e.target.value })}
          />
        </Field>

        {core ? (
          <Labelled label={t("work-agent-editor-tags")} hint={CORE_AGENT_LOCKED}>
            {(d.tags ?? []).length === 0 ? (
              <p className="text-2xs text-text-dim">{t("work-agent-editor-none-purpose-filter-can-hide-one")}</p>
            ) : (
              <div className="flex flex-wrap gap-1">
                <TagChips tags={d.tags} />
              </div>
            )}
          </Labelled>
        ) : (
          <Labelled
            label={t("work-agent-editor-tags")}
            hint={t("work-agent-editor-how-files-roster-grown-from-catalog")}
          >
            <TagInput
              value={d.tags}
              onChange={(tags) => set({ tags })}
              suggestions={[...TAG_VOCABULARY]}
            />
          </Labelled>
        )}

        <Field label={t("work-agent-editor-harness")} hint={t("work-agent-editor-what-actually-runs-session-model-plan")}>
          <Select value={d.harness ?? ""} onChange={(e) => set({ harness: e.target.value || null })}>
            {d.harness === null && <option value="">{t("work-agent-editor-pick-harness")}</option>}
            {choices.map((h) => (
              <option key={h.id} value={h.id}>
                {h.label}
              </option>
            ))}
          </Select>
          {choices.length === 0 && <p className="mt-1 text-2xs text-warn">{t("work-agent-editor-no-harness-installed")}</p>}
        </Field>

        <fieldset className="rounded-control border border-hairline p-3">
          <legend className="px-1 text-2xs font-semibold text-text-dim">{t("work-agent-editor-model-plan")}</legend>
          {/* A plan is of a harness's models: there is none to pick from until a harness is. */}
          {d.harness !== null && <ModelPlanEditor harness={d.harness} plan={d.plan} onChange={(plan) => set({ plan })} />}
        </fieldset>

        <Switch
          checked={d.decision_making}
          onChange={(decision_making) => set({ decision_making })}
          label={t("work-agent-editor-let-decision-making-agent-decide-agent")}
        />

        <Field
          label={t("work-agent-editor-who-may-instruct")}
          hint={core ? CORE_AGENT_LOCKED : t("work-agent-editor-owner-only-safe-default")}
        >
          {core ? (
            <TextInput
              readOnly
              className={READ_ONLY}
              value={
                d.respond === "members"
                  ? t("work-agent-editor-any-workspace-member")
                  : t("work-agent-editor-only-me")
              }
            />
          ) : (
            <Select
              value={d.respond}
              onChange={(e) => set({ respond: e.target.value as AgentDraft["respond"] })}
            >
              <option value="owner_only">{t("work-agent-editor-only-me")}</option>
              <option value="members">{t("work-agent-editor-any-workspace-member")}</option>
            </Select>
          )}
        </Field>

        {/* The two references side by side: a skill from the library, a server from the registry — each a search box with chips, each with its door to Settings. */}
        <div className="grid gap-3 md:grid-cols-2">
          <fieldset className="rounded-control border border-hairline p-3">
            <legend className="px-1 text-2xs font-semibold text-text-dim">
              {t("work-agent-editor-skills-count", { n: d.skills.length })}
            </legend>
            <div className="flex flex-col gap-2">
              <p className="text-2xs text-text-dim">
                <strong className="font-medium text-text">{SKILL_RULE}</strong> {SKILL_BUDGET}
              </p>
              {core ? (
                <>
                  {d.skills.length === 0 ? (
                    <p className="text-2xs text-text-dim">{t("work-agent-editor-carries-none")}</p>
                  ) : (
                    <AttachedRefs entries={resolveRefs(d.skills, skills.entries)} />
                  )}
                  <p className="text-2xs text-text-dim">{CORE_AGENT_LOCKED}</p>
                </>
              ) : skills.loading && !skills.entries.length ? (
                <p className="text-2xs text-text-dim">{t("work-agent-editor-reading-library")}</p>
              ) : skills.error && !skills.entries.length ? (
                <ErrorNote error={t("work-agent-editor-couldn-t-read-library", { error: skills.error })} retry={skills.reload} />
              ) : skills.entries.length === 0 ? (
                <p className="text-2xs text-text-dim">{t("work-agent-editor-library-empty-write-first-skill-under")}</p>
              ) : (
                <RefPicker
                  library={skills.entries}
                  value={d.skills}
                  onChange={(next) => set({ skills: next })}
                  placeholder={t("work-agent-editor-search-skill-library")}
                  disabled={busy}
                />
              )}
              {!core && (
                <p className="text-2xs text-text-dim">
                  {t("work-agent-editor-skill-written-once-library-picked-here")}{" "}
                  <a href={href({ name: "settings" }, settingsSearch("skills"))} className="text-accent-ink underline underline-offset-2">{t("work-agent-editor-open-settings-skills")}</a>
                  .
                </p>
              )}
            </div>
          </fieldset>

          <fieldset className="rounded-control border border-hairline p-3">
            <legend className="px-1 text-2xs font-semibold text-text-dim">
              {t("work-agent-editor-mcp-servers-count", { n: d.mcps.length })}
            </legend>
            {core ? (
              <div className="flex flex-col gap-2">
                {d.mcps.length === 0 ? (
                  <p className="text-2xs text-text-dim">{t("work-agent-editor-references-none")}</p>
                ) : (
                  <AttachedRefs entries={resolveRefs(d.mcps, mcps.entries)} />
                )}
                <p className="text-2xs text-text-dim">{CORE_AGENT_LOCKED}</p>
              </div>
            ) : mcps.loading && !mcps.entries.length ? (
              <p className="text-2xs text-text-dim">{t("work-agent-editor-reading-registry")}</p>
            ) : mcps.error && !mcps.entries.length ? (
              <ErrorNote error={t("work-agent-editor-couldn-t-read-registry", { error: mcps.error })} retry={mcps.reload} />
            ) : mcps.entries.length === 0 ? (
              <p className="text-2xs text-text-dim">
                {MCP_REGISTRY_NOTE}{" "}
                <a
                  href={href({ name: "settings" }, settingsSearch("mcp"))}
                  className="text-accent-ink underline underline-offset-2"
                >{t("work-agent-editor-open-settings-mcp-servers")}</a>
                .
              </p>
            ) : (
              <RefPicker
                library={mcps.entries}
                value={d.mcps}
                onChange={(next) => set({ mcps: next })}
                placeholder={t("work-agent-editor-search-mcp-registry")}
                disabled={busy}
              />
            )}
          </fieldset>
        </div>
      </div>
    </Dialog>
  );
}
