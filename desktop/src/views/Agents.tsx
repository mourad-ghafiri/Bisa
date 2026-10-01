/**
 * Agents: your staff, not a config table.
 *
 * An agent here is a definition — a prompt bound to a harness, a model,
 * skills and MCP servers, with its own keypair so its work is signed as
 * itself. The live roster and its memory are consequences of that, shown
 * beside it rather than on separate screens.
 *
 * A fresh workspace opens with **exactly two** of them — `general-agent` and
 * `workflow-agent`, the platform's own — and the other definitions live in the Catalog
 * until somebody installs one. Teams, skills and the catalog each used to be
 * a tab here, which made this screen four screens wearing one title; they are
 * their own destinations now, and what is left is agents: the roster, the
 * running sessions, and the detail pane. The roster is browsed rather than
 * scrolled once it grows — a tag filter built from its own categories,
 * composing with a text search that narrows within whatever the filter left —
 * and neither control mutates the list it filters, so a refetch lands in
 * place and nothing loses its scroll position.
 *
 * Deleting one asks first. Nothing is removed while something still points at
 * it, and the node's refusal names what — so the screen asks the same question
 * before offering the button, and shows the holders instead of a control whose
 * only possible outcome is a red toast.
 *
 * The three core agents sit **above both controls**, pinned and unfilterable —
 * the General Agent, then the Workflow Agent (`addressModel.coreAgents`),
 * then the Decision-Making Agent. The first two hold a record — a key, a
 * prompt, a conversation — and their definitions carry no tags on purpose: a
 * filter that can hide the agents you can always reach is a filter that can
 * strand you, so the filtering here is written over the rest of the roster
 * and cannot see them. Neither offers Delete or Disable — absent rather than
 * greyed — and the detail pane says why. The third holds no record: it judges
 * where a decision point asks it, so its card is drawn from
 * `GET /decisions/status` and opens Settings › Decision Making.
 *
 * An agent's skills and MCP servers are **ids** into a shared library and a
 * local registry. They are resolved here for display, and an id that resolves
 * to nothing is shown as a dangling reference rather than dropped — see
 * {@link AttachedRefs}.
 *
 * The roster's search, the tags picked and where each tab was scrolled are
 * the screen's memory (`shell/viewMemoryStore`), kept under the roster's
 * place whichever agent is open beside it — so the roster comes back as it
 * was left. The editor and the dialogs start empty, as every form does.
 */

import { filterByTagsAndWords } from "../ui/tagSearchModel.mjs";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { href, navigate, useSearchValue } from "../router";
import { stopSession, useSessions } from "../shell/sessionsStore";
import { stopWords } from "../shell/sessionRosterModel.mjs";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { textValue } from "../shell/viewValuesModel.mjs";
import { settingsSearch } from "./_settings/settingsLink.mjs";
import { coreLine, movesStatus } from "./_settings/decisionsModel.mjs";
import type { AgentDef, SessionRow } from "../types";
import { isCoreAgent, planSummary } from "../types";
import { coreAgents, rosterable } from "./_studio/addressModel.mjs";
import { attachedTo, respondsTo } from "./rosterModel.mjs";
import {
  AnimatedList,
  Avatar,
  Button,
  Card,
  Chip,
  ConfirmDialog,
  CopyText,
  EmptyState,
  ErrorNote,
  ICON,
  Menu,
  NO_TAG_FILTER,
  RelativeTime,
  Section,
  ExternalLink,
  SessionMark,
  SessionStateChip,
  sessionState,
  SkeletonRows,
  Spinner,
  Tabs,
  TagChips,
  TagFilterBar,
  TextInput,
  Tooltip,
  parseTagFilter,
  useToast,
  type TagFilterState,
} from "../ui";
import { AgentEditor } from "./_work/AgentEditor";
import {
  AttachedRefs,
  AttachSelect,
  DeleteButton,
  DeleteDialog,
  MCP_REGISTRY_NOTE,
  resolveRefs,
  SKILL_BUDGET,
  SKILL_RULE,
  useMcpRegistry,
  useSkillLibrary,
  useUsage,
  type Library,
} from "./_work/LibraryRefs";
import { useEffortSetting } from "./_work/EffortPicker";
import { ModelHealthBadges, useModelHealth } from "./_work/ModelPlanEditor";
import { answeredBy, healthOf, strategyOf, weightWords } from "./_work/modelPlanModel.mjs";
import { effortWords, effortsFor, planEfforts, resolveEffort } from "./_work/effortModel.mjs";
import {
  CORE_AGENT_LOCKED,
  CORE_AGENT_PERMANENT,
  CORE_AGENT_PURPOSE,
  OriginChip,
} from "./_work/Origin";
import { RecallPanel } from "./_work/RecallPanel";
import { readKey } from "./_work/keptReadsModel.mjs";
import { attempt, useAsync } from "./_work/useAsync";
import { t } from "../i18n/l10n.mjs";
import { rich } from "../i18n/rich";
import { AbsentRecord } from "./_work/AbsentRecord";

/** Where the screen keeps its memory: the roster's place, whichever agent is open beside it. */
const PLACE = placeOf({ name: "agents" });

function AgentCard({
  a,
  selected,
  onOpen,
  onMessage,
  onEdit,
  onDelete,
}: {
  a: AgentDef;
  selected: boolean;
  onOpen: () => void;
  onMessage: () => void;
  onEdit: () => void;
  /**
   * Absent on the core agent, and absent rather than disabled: a menu entry
   * that exists only to refuse you is worse than no entry.
   */
  onDelete?: () => void;
}) {
  return (
    <Card className={selected ? "border-accent/60" : ""}>
      <div className="mb-2 flex items-start gap-2">
        <button type="button" onClick={onOpen} className="flex min-w-0 flex-1 items-start gap-2 text-left">
          <Avatar id={a.pubkey} name={a.name} photo={a.photo} size={28} />
          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-1.5">
              <span className="truncate text-xs font-medium">{a.name}</span>
              <OriginChip origin={a.origin} id={a.id} />
              {!a.enabled && <Chip tone="warn">{t("screens-agents-disabled")}</Chip>}
            </span>
            <span className="mt-0.5 line-clamp-2 block text-2xs text-text-dim">
              {a.description ?? a.system_prompt.split("\n")[0]}
            </span>
          </span>
        </button>
        <Menu
          label={t("screens-agents-actions", { a: a.name })}
          trigger={
            <span className="anim flex h-6 w-6 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text">
              <ICON.more size={14} aria-hidden />
            </span>
          }
          items={[
            { label: t("screens-inbox-open"), icon: ICON.forward, onSelect: onOpen },
            { label: t("screens-agents-message"), icon: ICON.dm, onSelect: onMessage },
            { label: t("screens-agents-edit-definition"), icon: ICON.edit, onSelect: onEdit },
            ...(onDelete
              ? [
                  {
                    label: t("screens-agents-delete-2"),
                    icon: ICON.delete,
                    danger: true,
                    separatorBefore: true,
                    onSelect: onDelete,
                  },
                ]
              : []),
          ]}
        />
      </div>

      <div className="mb-2 flex flex-wrap gap-1.5">
        <Chip tone="accent">{a.harness}</Chip>
        <Chip tone="quiet">{planSummary(a.models)}</Chip>
        {a.skills.length > 0 && (
          <Chip tone="quiet">{t("screens-agents-skill-count", { skills: a.skills.length })}</Chip>
        )}
        {a.mcps.length > 0 && <Chip tone="quiet">{t("screens-agents-mcp", { mcps: a.mcps.length })}</Chip>}
        <TagChips tags={a.tags ?? []} max={3} />
      </div>

      <Button size="sm" variant="primary" className="w-full" onClick={onMessage}>{t("screens-agents-message")}</Button>
    </Card>
  );
}

/**
 * The Decision-Making Agent's card — the third core agent, drawn under the
 * General Agent and the Workflow Agent and wearing the chip they wear. It
 * judges: a decision point asks it a typed question and gets an answer with
 * how sure it is. It holds no record — no key, no prompt, no conversation —
 * so the card has nothing to message or edit and opens Settings › Decision
 * Making, where who answers for it, and whether it is on, is set. Its name
 * and description are `GET /decisions/status`'s; the line under them is
 * `decisionsModel.coreLine`'s, so this card and that panel never disagree on
 * whether it is on.
 */
function DecisionMakingAgentCard() {
  const status = useAsync((s) => api.decisionsStatus(s), [], { keep: readKey("agents", "decisions") });
  // Switched on, moved to another provider, a judgement recorded: the card follows, as the panel does.
  const { reload } = status;
  useEngineEvents((e) => {
    if (movesStatus(e.payload)) reload();
  });
  useReloadOnReconnect(reload);
  return (
    <Card>
      <button
        type="button"
        onClick={() => navigate({ name: "settings" }, settingsSearch("decision-making"))}
        className="flex w-full min-w-0 items-start gap-2 text-left"
      >
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-surface-2 text-text-dim">
          <ICON.decisions size={14} aria-hidden />
        </span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-1.5">
            <span className="truncate text-xs font-medium">{status.data?.agent.name ?? t("screens-agents-decision-making-agent")}</span>
            <Chip tone="accent">{t("work-origin-platform-agent")}</Chip>
          </span>
          <span className="mt-0.5 line-clamp-2 block text-2xs text-text-dim">
            {status.data?.agent.description ?? t("screens-agents-model-judges-place-place-s-own")}
          </span>
          <span className="mt-1 block text-2xs text-text-dim">{coreLine(status.data, status.error) || t("screens-agents-reading")}</span>
        </span>
      </button>
    </Card>
  );
}

/**
 * The plan as the engine will walk it, with what the engine currently knows
 * about each model beside it.
 *
 * This is where "why is it running on its second choice?" gets an answer: a
 * cooling model is not dropped from the plan, it is demoted to the end of the
 * order, and the badge says how long is left.
 */
function ModelPlanSummary({ a }: { a: AgentDef }) {
  const { rows } = useModelHealth();
  const models = a.models?.models ?? [];
  const def = strategyOf(a.models);
  const strategy = def.value;

  // How hard each model works (06 §Effort), fitted to what it takes on this
  // harness — until the harness has answered, the level as it was asked.
  const harness = useAsync((s) => api.models(a.harness, s), [a.harness], { keep: readKey("harness-models", a.harness) });
  const answered = answeredBy(a.harness, harness.data, harness.error);
  const known = answered !== null;
  const { setting, origin } = useEffortSetting(null);
  const effortOf = (model: string | null, own: unknown) =>
    effortWords(resolveEffort(null, own, a.models?.effort, setting), { available: model === null ? planEfforts(answered, a.models) : effortsFor(answered, model), known, origin });

  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-2xs text-text-dim">
        {rich("screens-agents-label-explain", { label: <span className="font-medium">{def.label}</span> }, { explain: def.explain })}
      </p>
      <p className="text-2xs text-text-dim">
        {rich("screens-agents-label-explain", { label: <span className="font-medium">{t("screens-agents-effort")}</span> }, { explain: effortOf(null, null).hint })}
      </p>
      {models.length === 0 ? (
        <div className="flex flex-wrap items-center gap-1.5 rounded-control border border-dashed border-border px-2 py-1.5">
          <span className="font-mono text-2xs text-text-dim">{t("screens-agents-default", { harness: a.harness })}</span>
          <ModelHealthBadges row={healthOf(rows, a.harness, null)} />
          <span className="text-2xs text-text-dim">{t("screens-agents-no-models-pinned-so-there-nothing")}</span>
        </div>
      ) : (
        <ul className="flex flex-col gap-1">
          {models.map((m, i) => {
            const effort = effortOf(m.model, m.effort);
            return (
              <li
                key={m.model}
                className="flex flex-wrap items-center gap-1.5 rounded-control border border-border px-2 py-1"
              >
                <span className="tnum w-5 shrink-0 text-right text-2xs text-text-dim">{i + 1}.</span>
                <span
                  className={`min-w-0 flex-1 truncate font-mono text-2xs ${
                    m.enabled === false ? "text-text-dim line-through" : ""
                  }`}
                >
                  {m.model}
                </span>
                {strategy === "weighted" && <Chip tone="quiet">{weightWords(m.weight)}</Chip>}
                {strategy === "auto_route" && m.suited_for && (
                  <Chip tone="quiet">{t("screens-agents-best", { suited_for: m.suited_for })}</Chip>
                )}
                {/* The level this model runs at, or Auto; the sentence that says who decided on hover. */}
                {effort.label && (
                  <Chip tone="quiet" title={effort.hint}>
                    {effort.label}
                  </Chip>
                )}
                <ModelHealthBadges row={healthOf(rows, a.harness, m.model)} />
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/**
 * One reference list — skills or MCP servers — resolved against its library.
 *
 * Writes go through `PATCH /agents/{id}` with the whole list rather than the
 * per-id attach and detach routes, matching {@link AgentEditor}: both lists
 * are ordered, and one request cannot leave an agent half-changed.
 */
function AgentRefs({
  title,
  ids,
  library,
  attachLabel,
  emptyLibrary,
  note,
  locked,
  onChange,
}: {
  title: string;
  ids: string[];
  library: Library;
  attachLabel: string;
  /** What to say when the library itself holds nothing — not a bare zero. */
  emptyLibrary: ReactNode;
  /** A rule this particular list holds, under the entries. */
  note?: ReactNode;
  /**
   * Why this list cannot be written, when it cannot. The core agent's skills
   * and MCP servers are fixed, and the store refuses a changed list by name —
   * so offering an Attach control here would be offering a click whose only
   * possible outcome is a red toast.
   */
  locked?: ReactNode;
  onChange: (next: string[]) => Promise<unknown>;
}) {
  const [busy, setBusy] = useState(false);
  const entries = useMemo(() => resolveRefs(ids, library.entries), [ids, library.entries]);

  const run = (next: string[]) => {
    setBusy(true);
    void onChange(next).finally(() => setBusy(false));
  };

  return (
    <Section
      title={ids.length ? `${title} (${ids.length})` : title}
      action={
        locked ? null : (
          <AttachSelect
            library={library.entries}
            attached={ids}
            label={attachLabel}
            busy={busy}
            onAttach={(id) => run([...ids, id])}
          />
        )
      }
    >
      <div className="flex flex-col gap-1.5">
        {library.loading && library.entries.length === 0 ? (
          // Resolving against a library that has not arrived would mark every
          // id unresolved for a frame, which is a lie about the agent.
          <Spinner />
        ) : library.error && library.entries.length === 0 ? (
          <ErrorNote error={library.error} retry={library.reload} />
        ) : library.entries.length === 0 && ids.length === 0 ? (
          <p className="text-2xs text-text-dim">{emptyLibrary}</p>
        ) : ids.length === 0 ? (
          <p className="text-2xs text-text-dim">{t("screens-agents-carries-none")}</p>
        ) : (
          <AttachedRefs
            entries={entries}
            busy={busy}
            onDetach={locked ? undefined : (id) => run(ids.filter((v) => v !== id))}
          />
        )}
        {locked && <p className="text-2xs text-text-dim">{locked}</p>}
        {note && <p className="text-2xs text-text-dim">{note}</p>}
      </div>
    </Section>
  );
}

function AgentDetail({
  a,
  skills,
  mcps,
  usage,
  onEdit,
  onDelete,
}: {
  a: AgentDef;
  skills: Library;
  mcps: Library;
  /** Asked at the screen, so this pane and the dialog read one answer. */
  usage: ReturnType<typeof useUsage>;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const toast = useToast();
  const { refresh } = useWorkspace();
  const core = isCoreAgent(a);
  const patchRefs = (patch: { skills: string[] } | { mcps: string[] }) =>
    attempt(() => api.patchAgent(a.id, patch), toast.error, refresh);

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <div className="flex items-start gap-2">
          <Avatar id={a.pubkey} name={a.name} photo={a.photo} size={32} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <h2 className="truncate text-sm font-semibold">{a.name}</h2>
              <OriginChip origin={a.origin} id={a.id} />
            </div>
            <p className="mt-0.5 text-2xs text-text-dim">{respondsTo(a)}</p>
            {core && <p className="mt-1 text-2xs text-text-dim">{CORE_AGENT_PURPOSE}</p>}
          </div>
        </div>

        {(a.tags ?? []).length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1">
            <TagChips tags={a.tags ?? []} />
          </div>
        )}

        <div className="mt-2 flex items-center gap-2">
          <CopyText value={a.pubkey} label={`${a.pubkey.slice(0, 12)}…`} />
          <span className="text-2xs text-text-dim">{t("screens-agents-own-key-work-signed-itself")}</span>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Button size="sm" variant="ghost" onClick={onEdit}>
            {core ? t("screens-agents-harness-models") : t("screens-agents-edit-definition")}
          </Button>
          {/* No Delete and no Disable on the core agent, and neither of them
              rendered greyed out: a control that exists only to refuse you is
              worse than no control, so the sentence below stands in for both.
              Every other agent gets whichever control its usage earns — a red
              Delete when nothing points at it, and a way to see the holders
              when something does. */}
          {!core && <DeleteButton usage={usage} onOpen={onDelete} />}
        </div>

        {core && <p className="mt-2 text-2xs text-text-dim">{CORE_AGENT_PERMANENT}</p>}
      </Card>

      <Section title={t("screens-agents-model-plan")} action={<Chip tone="accent">{a.harness}</Chip>}>
        <ModelPlanSummary a={a} />
      </Section>

      <AgentRefs
        title={t("screens-agents-skills")}
        ids={a.skills}
        library={skills}
        attachLabel={t("screens-agents-attach-skill")}
        emptyLibrary={t("screens-agents-skill-library-empty", { rule: SKILL_RULE })}
        note={a.skills.length > 0 ? SKILL_BUDGET : undefined}
        locked={core ? CORE_AGENT_LOCKED : undefined}
        onChange={(next) => patchRefs({ skills: next })}
      />

      <AgentRefs
        title={t("screens-agents-mcp-servers")}
        ids={a.mcps}
        library={mcps}
        attachLabel={t("screens-agents-attach-server")}
        emptyLibrary={
          <>
            {MCP_REGISTRY_NOTE}{" "}
            <a
              href={href({ name: "settings" })}
              className="text-accent-ink underline underline-offset-2"
            >{t("screens-agents-open-settings")}</a>
            .
          </>
        }
        locked={core ? CORE_AGENT_LOCKED : undefined}
        onChange={(next) => patchRefs({ mcps: next })}
      />

      <Section
        title={t("screens-agents-recall")}
        action={<span className="text-2xs text-text-dim">{t("screens-agents-only-can-read")}</span>}
      >
        <RecallPanel agent={a.id} />
      </Section>
    </div>
  );
}

function Sessions() {
  const toast = useToast();
  // The roster is the presence store's: seeded once, moved by `session_state`.
  const sessions = sessionState.sortRows(useSessions());
  const [aborting, setAborting] = useState<SessionRow | null>(null);

  if (sessions.length === 0) {
    return (
      <EmptyState
        title={t("screens-agents-nothing-running")}
        hint={t("screens-agents-sessions-appear-while-agent-working-goal")}
        action={
          <Button variant="primary" onClick={() => navigate({ name: "goals" })}>{t("screens-agents-see-all-goals")}</Button>
        }
      />
    );
  }

  return (
    <>
      {/* Sessions start and end while this list is on screen, which is the one
          case the animated list is for: a row that simply appears is
          indistinguishable from a row you had not noticed. */}
      <AnimatedList
        className="flex flex-col gap-1.5"
        items={sessions}
        keyOf={(s) => s.id}
        render={(s) => {
          const at = attachedTo(s);
          const link = at.route ? href(at.route) : null;
          return (
            <div className="flex flex-col gap-1 rounded-control border border-border px-2 py-1.5">
              <div className="flex items-center gap-2">
                <SessionMark state={s.state} />
                <SessionStateChip state={s.state} />
                <Chip tone="quiet">{s.agent ?? s.harness}</Chip>
                <Chip tone="quiet">{s.kind}</Chip>
                {link ? (
                  <ExternalLink href={link} className="truncate text-2xs text-accent-ink underline underline-offset-2">
                    {at.label}
                  </ExternalLink>
                ) : (
                  <span className="truncate text-2xs text-text-dim">{at.label}</span>
                )}
                <RelativeTime at={s.since} className="ml-auto" />
                {s.state.state === "waiting" && s.state.on.on !== "auth" && (
                  <Button size="sm" variant="primary" onClick={() => navigate({ name: "inbox" })}>{t("screens-agents-answer")}</Button>
                )}
                {sessionState.isStoppable(s.state) && (
                  <Button size="sm" variant="danger" onClick={() => setAborting(s)}>{t("screens-agents-abort")}</Button>
                )}
              </div>
              {s.children.length > 0 && (
                <ul className="flex flex-col gap-0.5 pl-5 text-2xs">
                  {s.children.map((c) => (
                    <li key={c.id} className="flex items-center gap-2">
                      <span className="font-mono text-text-dim">↳</span>
                      <SessionMark state={c.state} />
                      <span>{c.name}</span>
                      <span className="text-text-dim">{sessionState.label(c.state)}</span>
                      {c.description && <span className="truncate text-text-dim">{c.description}</span>}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          );
        }}
      />
      <ConfirmDialog
        open={aborting !== null}
        onClose={() => setAborting(null)}
        title={t("screens-agents-abort-session")}
        danger
        confirmLabel={t("screens-agents-abort")}
        body={t("screens-agents-aborted-terminal-session-cannot-revived-whatever")}
        onConfirm={() => {
          if (!aborting) return;
          const id = aborting.id;
          // The store's one door: a row the node no longer has is dropped, and said — never an error for a session that had ended.
          void attempt(() => stopSession(id), toast.error, (how) => {
            if (how === "gone") toast.ok(stopWords(how, ""));
          });
        }}
      />
    </>
  );
}

/** Stable accessor — {@link TagFilterBar} keys its facets on the list, not on this. */
const tagsOfAgent = (a: AgentDef): string[] => a.tags ?? [];

export default function Agents({ id }: { id?: string }) {
  const toast = useToast();
  const [tabParam, setTab] = useSearchValue("tab");
  const tab = tabParam === "sessions" ? "sessions" : "agents";

  const { agents, refresh, ready, offline } = useWorkspace();
  const { data: harnessData } = useAsync((s) => api.harnesses(s), [], { keep: readKey("agents", "harnesses") });
  // Both libraries are read once for the whole screen: the roster resolves
  // every agent's ids against them, and the editor picks from the same lists.
  const skills = useSkillLibrary();
  const mcps = useMcpRegistry();

  const [editing, setEditing] = useState<AgentDef | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [query, setQuery] = useViewState(PLACE, "search", "", textValue);
  const [tagFilter, setTagFilter] = useViewState<TagFilterState>(PLACE, "tags", NO_TAG_FILTER, parseTagFilter);
  // One scrollport shows two tabs: each is kept under its own name, and put
  // back when it is the one shown — and once the roster is read, which is
  // when there is something to scroll.
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, `${PLACE}#${tab}:${ready ? "read" : "reading"}`, PLACE);

  const selected = id ? (agents.find((a) => a.id === id) ?? null) : null;

  // The core agents are lifted out of the roster before either control sees
  // them, which is the only way to guarantee no filter can hide one. Checking
  // it inside the predicate would work until somebody added a second condition.
  const cores = useMemo(() => coreAgents(agents), [agents]);
  const rest = useMemo(() => rosterable(agents), [agents]);

  // Asked at the screen so the detail pane's control and the delete dialog
  // read one answer rather than running the same question twice. The core
  // agent is never the subject: it cannot be deleted at all, so asking what
  // points at it would be a request whose answer nothing reads.
  const deletable = selected !== null && !isCoreAgent(selected) ? selected : null;
  const usage = useUsage("agent", deletable?.id ?? null);

  // The two controls compose: the tag filter narrows the roster, the search
  // narrows within whatever it left. Facets are built from the whole roster,
  // so a tag you have selected cannot disappear from under you as you type.
  const roster = useMemo(
    () => filterByTagsAndWords(rest, tagFilter, query, (a) => a.tags, (a) => [a.name, a.id, a.description]),
    [rest, query, tagFilter],
  );

  /** Open (or reuse) the DM with an agent and go there. */
  const openDm = (a: AgentDef) =>
    void attempt(async () => {
      const { channel } = await api.openDm([a.pubkey]);
      navigate({ name: "dm", id: channel.id });
    }, toast.error);

  /** Select an agent and ask to delete it, from wherever the row was. */
  const askDelete = (a: AgentDef) => {
    // Selecting first is what puts the usage check on this agent; the dialog
    // opens on its "checking" body and fills in when the answer lands.
    if (a.id !== id) navigate({ name: "agent", id: a.id });
    setDeleting(true);
  };

  return (
    <div ref={root} className="flex h-full flex-col">
      {/* No <h1>: the shell's top chrome already names the screen, and a
          second title on the page is the thing this wave is removing. */}
      <header className="flex flex-col gap-2 border-b border-border px-6 pt-3">
        <Tabs
          tabs={[
            { id: "agents", label: t("screens-agents-definitions") },
            { id: "sessions", label: t("screens-agents-running") },
          ]}
          active={tab}
          onChange={setTab}
        />
      </header>

      <div data-scroll-keep={`tab:${tab}`} className="min-h-0 flex-1 overflow-y-auto p-6">
        {tab === "sessions" ? (
          <Sessions />
        ) : !ready ? (
          <SkeletonRows rows={6} />
        ) : offline ? (
          <ErrorNote error={offline} retry={refresh} />
        ) : (
          <div className="grid gap-6 lg:grid-cols-[1.3fr_1fr]">
            <div className="flex min-w-0 flex-col gap-2">
              {cores.length > 0 && (
                <div className="flex flex-col gap-1">
                  {cores.map((core) => (
                    <AgentCard
                      key={core.id}
                      a={core}
                      selected={core.id === id}
                      onOpen={() =>
                        navigate(core.id === id ? { name: "agents" } : { name: "agent", id: core.id })
                      }
                      onMessage={() => openDm(core)}
                      onEdit={() => setEditing(core)}
                    />
                  ))}
                  <DecisionMakingAgentCard />
                  <p className="text-2xs text-text-dim">{t("screens-agents-three-core-agents-pinned-above-filters")}</p>
                </div>
              )}

              {rest.length === 0 ? (
                <EmptyState
                  icon={ICON.agent}
                  title={t("screens-agents-no-other-agents-yet")}
                  hint={t("screens-agents-twenty-seven-agent-definitions-catalog-installing")}
                  action={
                    <div className="flex flex-wrap justify-center gap-2">
                      <Button
                        variant="primary"
                        onClick={() =>
                          navigate({ name: "settings" }, settingsSearch("catalog-agent"))
                        }
                      >{t("screens-agents-open-catalog")}</Button>
                      <Button onClick={() => setCreating(true)}>{t("screens-agents-write-one-yourself")}</Button>
                    </div>
                  }
                />
              ) : (
                <>
                  <div className="flex flex-wrap items-center gap-2">
                    <TextInput
                      value={query}
                      placeholder={t("screens-agents-search-roster")}
                      aria-label={t("screens-agents-search-roster-2")}
                      className="h-7 max-w-64 py-0"
                      onChange={(e) => setQuery(e.target.value)}
                    />
                    <span className="text-2xs text-text-dim">{t("screens-agents-words", { roster: roster.length, rest: rest.length })}</span>
                    <Tooltip label={t("screens-agents-re-read-roster")}>
                      <button
                        type="button"
                        onClick={refresh}
                        aria-label={t("screens-agents-refresh-roster")}
                        className="anim ml-auto flex h-7 w-7 items-center justify-center rounded-control text-text-dim hover:bg-surface-2 hover:text-text"
                      >
                        <ICON.refresh size={14} aria-hidden />
                      </button>
                    </Tooltip>
                    <Button size="sm" variant="primary" onClick={() => setCreating(true)}>{t("screens-agents-new-agent")}</Button>
                  </div>

                  <TagFilterBar
                    items={rest}
                    tagsOf={tagsOfAgent}
                    value={tagFilter}
                    onChange={setTagFilter}
                  />

                  {roster.length === 0 ? (
                    <EmptyState
                      icon={ICON.agent}
                      title={t("screens-agents-nothing-matches")}
                      hint={t("screens-agents-no-agent-roster-matches-search-tags")}
                      action={
                        <Button
                          onClick={() => {
                            setQuery("");
                            setTagFilter(NO_TAG_FILTER);
                          }}
                        >{t("screens-agents-clear-filters")}</Button>
                      }
                    />
                  ) : (
                    <AnimatedList
                      className="grid gap-2 md:grid-cols-2"
                      items={roster}
                      keyOf={(a) => a.id}
                      render={(a) => (
                        <AgentCard
                          a={a}
                          selected={a.id === id}
                          onOpen={() =>
                            navigate(a.id === id ? { name: "agents" } : { name: "agent", id: a.id })
                          }
                          onMessage={() => openDm(a)}
                          onEdit={() => setEditing(a)}
                          onDelete={() => askDelete(a)}
                        />
                      )}
                    />
                  )}
                </>
              )}
            </div>

            <div className="min-w-0">
              {selected ? (
                <AgentDetail
                  a={selected}
                  skills={skills}
                  mcps={mcps}
                  usage={usage}
                  onEdit={() => setEditing(selected)}
                  onDelete={() => setDeleting(true)}
                />
              ) : id && ready ? (
                // The roster is read and does not hold it: the node is asked for that one agent, and what it says is what is drawn.
                <AbsentRecord
                  key={id}
                  id={id}
                  what={t("screens-agents-the-agent")}
                  read={(aid, s) => api.agent(aid, s)}
                  onHere={refresh}
                  gone={
                    <Card>
                      <p className="text-xs font-medium">{t("screens-agents-agent-not-here")}</p>
                      <p className="mt-1 text-2xs text-text-dim">{rich("screens-agents-nothing-answers-to-id", { id: <code className="font-mono">{id}</code> })}</p>
                    </Card>
                  }
                />
              ) : (
                <Card>
                  <p className="text-xs font-medium">{t("screens-agents-pick-agent")}</p>
                  <p className="mt-1 text-2xs text-text-dim">{t("screens-agents-definition-skills-servers-references-memory-show")}</p>
                </Card>
              )}
            </div>
          </div>
        )}
      </div>

      <AgentEditor
        open={creating || editing !== null}
        agent={editing}
        harnesses={harnessData?.harnesses ?? []}
        skills={skills}
        mcps={mcps}
        onClose={() => {
          setCreating(false);
          setEditing(null);
        }}
        onSaved={refresh}
      />

      {/* Mounted only with a deletable agent in hand, so `remove` cannot be
          called without an id — and never for the core agent. */}
      {deletable && (
        <DeleteDialog
          open={deleting}
          title={t("screens-agents-delete", { deletable: deletable.name })}
          usage={usage}
          consequence={t("screens-agents-definition-keypair-removed-work-already-signed")}
          note={t("screens-agents-agent-s-own-key-gate-policy")}
          remove={() => api.deleteAgent(deletable.id)}
          onClose={() => setDeleting(false)}
          onDeleted={() => {
            toast.ok(t("screens-agents-agent-deleted"));
            if (id === deletable.id) navigate({ name: "agents" });
            refresh();
          }}
        />
      )}
    </div>
  );
}
