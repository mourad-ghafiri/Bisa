/**
 * The catalog: the staff a workspace *may* have.
 *
 * The previous milestone wrote every catalog agent, skill, team and channel
 * into every workspace on its first run — 61 definitions nobody chose, 27
 * keypairs nobody asked for. A workspace now opens with exactly one agent and
 * this screen, and **nothing in the library is written into a workspace until
 * somebody installs it**. So this is the whole difference between a workspace
 * that starts as an organisation nobody hired and one that starts as an offer.
 *
 * It sits in Settings under **Library**, above Capabilities, because the
 * catalog is where skills, agents, teams and channels *come from* rather than a
 * peer of any of them. In the primary nav it was the eighth of eight places
 * to work, competing with Goals and the roster for a reader who visits it
 * a handful of times in a workspace's life; one group above the things it
 * fills says what it is instead.
 *
 * Three facts about an install drive the design, and each of them is stated
 * on screen rather than in this comment, because each is discovered at the
 * moment of the click:
 *
 * - **It is transitive.** A team brings its agents and their skills; a channel
 *   brings its roster. Installing Venture creates ten agents. An install you
 *   cannot predict is one you cannot undo confidently, so every row says what
 *   it will create *before* the click and the screen reports what it created
 *   after — by name, per kind, never as a count alone.
 * - **It is idempotent.** Something already present is left exactly as it is
 *   and appears in no result, so installing twice answers with every list
 *   empty. That is a different sentence from a successful install, not a
 *   silent one and not a failure.
 * - **It is never destructive.** An id that exists and did *not* come from
 *   that entry is a collision: a 400 carrying the store's own message, which
 *   names the id and what holds it. That message is shown verbatim, because
 *   "install failed" leaves the owner guessing whether their own work is at
 *   risk.
 */

import { filterByTagsAndWords } from "../../ui/tagSearchModel.mjs";
import { useMemo, useState } from "react";
import { api } from "../../api";
import { useSearchValue } from "../../router";
import type { CatalogEntry, CatalogKind, Installed } from "../../types";
import {
  Button,
  Card,
  Chip,
  ConfirmDialog,
  EmptyState,
  ErrorNote,
  ICON,
  NO_TAG_FILTER,
  SegmentedControl,
  Pending,
  TagChips,
  TagFilterBar,
  TextInput,
  useToast,
  type LucideIcon,
  type TagFilterState,
} from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

/** The six kinds, in the order a workspace is usually built up. */
const CATALOG_KINDS: readonly CatalogKind[] = ["agent", "skill", "team", "channel", "connector", "workflow"];

/** The kind's plural, in words — *agents*, *skills* — for a search field and a tab. */
const plural = (kind: CatalogKind): string => t("settings-catalog-panel-kind-plural", { kind });

const KIND_ICON: Record<CatalogKind, LucideIcon> = {
  agent: ICON.agent,
  skill: ICON.skill,
  team: ICON.team,
  channel: ICON.channel,
  connector: ICON.connector,
  workflow: ICON.template,
  addon: ICON.addon,
};

/** What each kind *is*, for a reader who has not met the vocabulary yet. */
const KIND_BLURB: Record<CatalogKind, string> = {
  agent: t("settings-catalog-panel-someone-give-work-role-harness-model"),
  skill: t("settings-catalog-panel-procedure-agent-works-through-prompt-says"),
  team: t("settings-catalog-panel-standing-group-agents-people-who-approve"),
  channel: t("settings-catalog-panel-room-standing-roster-so-question-has"),
  connector: t("settings-catalog-panel-declarative-definition-one-outside-platform-s"),
  workflow: t("settings-catalog-panel-shape-goal-runs-few-steps-each"),
  addon: t("settings-catalog-panel-widget-floats-over-app"),
};

/**
 * What a kind's `requires` list names.
 *
 * Resolved by the *parent's* kind rather than by looking the slug up
 * anywhere it matches, because slugs are only unique within a kind:
 * `engineering` is a team **and** a channel, and a plan that resolved a team's
 * member by bare slug could pull in the room instead of the roster.
 */
const REQUIRES_KIND: Record<CatalogKind, CatalogKind | null> = {
  agent: "skill",
  skill: null,
  team: "agent",
  channel: "agent",
  connector: null,
  workflow: "agent",
  addon: null,
};

const countPhrase = (n: number, kind: CatalogKind): string => t("settings-catalog-panel-count-phrase", { n, kind });

/** "31 agents, 20 skills, 9 teams and 8 channels" — an Oxford-free list; the numbers are the catalog's, never typed here. */
function summarize(entries: CatalogEntry[]): string {
  const parts = CATALOG_KINDS.map((k) => {
    const n = entries.filter((e) => e.kind === k).length;
    return n === 0 ? null : countPhrase(n, k);
  }).filter((p): p is string => p !== null);
  if (parts.length <= 1) return parts[0] ?? "";
  return t("settings-catalog-panel-words", { parts: parts.slice(0, -1).join(", "), parts2: parts[parts.length - 1] });
}

// ---------------------------------------------------------------------------
// Loading it
// ---------------------------------------------------------------------------

export interface Catalog {
  entries: CatalogEntry[];
  loading: boolean;
  error: string | null;
  reload: () => void;
}

/**
 * The whole catalog in one request, not a request per kind.
 *
 * `?kind=` exists and the client carries it, but 61 rows compiled into the
 * binary cost nothing to fetch together, and holding all of them is what lets
 * a row resolve its own `requires` into the real entries — a team's plan is
 * its agents *and* their skills, which a kind-narrowed fetch could not see.
 * It is also what lets an empty roster elsewhere quote this build's own
 * numbers instead of a figure typed into a string.
 */
export function useCatalog(): Catalog {
  const { data, error, loading, reload } = useAsync((s) => api.catalog(undefined, s), []);
  return { entries: data?.entries ?? [], loading, error, reload };
}

/**
 * What an empty list should say instead of apologising for being empty.
 *
 * The numbers are this build's own. Before the catalog has arrived — or if
 * the node is down — the sentence keeps its shape and drops the figures,
 * because a hardcoded 27 is a claim that goes stale the milestone somebody
 * adds an agent.
 */
export function catalogOffer(catalog: Catalog, kind: CatalogKind): string {
  if (catalog.entries.length === 0) {
    return t("settings-catalog-panel-catalog-holds-agents-skills-teams-channels");
  }
  const free = catalog.entries.filter((e) => e.kind === kind && !e.installed).length;
  const others = summarize(catalog.entries.filter((e) => e.kind !== kind));
  return t("settings-catalog-panel-waiting-catalog-beside-nothing-written-into", { kind: countPhrase(free, kind), free, others });
}

// ---------------------------------------------------------------------------
// What an install will create
// ---------------------------------------------------------------------------

interface InstallPlan {
  /** Every entry the install touches, dependencies first — install order. */
  entries: CatalogEntry[];
  /** The subset whose id is free, which is exactly what will be created. */
  creates: CatalogEntry[];
}

/**
 * Resolve one entry's transitive plan over the loaded catalog.
 *
 * Dependencies come first because that is the order the installer writes them
 * — a skill before the agent that carries it, an agent before the team that
 * names it — and a plan shown in a different order from the install would be
 * a second story about the same act.
 *
 * A required slug this build does not ship is skipped rather than shown as
 * missing: the catalog's own test proves every reference resolves, so an
 * absence here means the row and the index disagree, and inventing a phantom
 * dependency would be a worse answer than a plan that is one line short.
 */
function planFor(entry: CatalogEntry, all: CatalogEntry[]): InstallPlan {
  const index = new Map(all.map((e) => [`${e.kind}/${e.slug}`, e]));
  const seen = new Set<string>();
  const ordered: CatalogEntry[] = [];
  const walk = (e: CatalogEntry) => {
    const key = `${e.kind}/${e.slug}`;
    if (seen.has(key)) return;
    seen.add(key);
    const childKind = REQUIRES_KIND[e.kind];
    if (childKind) {
      for (const slug of e.requires) {
        const child = index.get(`${childKind}/${slug}`);
        if (child) walk(child);
      }
    }
    ordered.push(e);
  };
  walk(entry);
  return { entries: ordered, creates: ordered.filter((e) => !e.installed) };
}

/** "5 agents and 12 skills", over a plan's creations. */
function creationSummary(creates: CatalogEntry[]): string {
  return summarize(creates) || "nothing";
}

/**
 * The same phrase over what the node actually created. Written from the
 * response rather than from the plan, because the plan was computed against a
 * listing that is one install out of date the moment the call succeeds.
 */
function installedSummary(i: Installed): string {
  const parts = CREATED_ORDER.map(([kind, key]) =>
    i[key].length > 0 ? countPhrase(i[key].length, kind) : null,
  ).filter((p): p is string => p !== null);
  if (parts.length <= 1) return parts[0] ?? "nothing";
  return t("settings-catalog-panel-words", { parts: parts.slice(0, -1).join(", "), parts2: parts[parts.length - 1] });
}

// ---------------------------------------------------------------------------
// One row
// ---------------------------------------------------------------------------

function PlanLine({ plan }: { plan: InstallPlan }) {
  // The entry itself is last in the plan, so everything before it is what the
  // entry drags along — and a plan of one brings nothing and needs no line.
  const brought = plan.entries.slice(0, -1);
  if (brought.length === 0) return null;
  const already = brought.filter((e) => e.installed).length;
  return (
    <p className="mt-1 text-2xs text-text-dim">
      {t("settings-catalog-panel-brings-with-it", { brought: summarize(brought) })}
      {already > 0 &&
        ` ${t("settings-catalog-panel-them-those-already-here-will-left", { already, brought: brought.length })}`}
    </p>
  );
}

function PlanList({ plan }: { plan: InstallPlan }) {
  return (
    <ul className="mt-2 flex flex-col gap-1 border-t border-border pt-2">
      {plan.entries.map((e) => (
        <li
          key={`${e.kind}/${e.slug}`}
          className="flex flex-wrap items-center gap-1.5 rounded-control border border-border px-2 py-1"
        >
          <Chip tone="quiet">{e.kind}</Chip>
          <span className="truncate text-2xs font-medium">{e.name}</span>
          <code className="truncate font-mono text-2xs text-text-dim">{e.slug}</code>
          <span className="ml-auto shrink-0 text-2xs text-text-dim">
            {e.installed ? t("settings-catalog-panel-already-here") : t("settings-catalog-panel-will-created")}
          </span>
        </li>
      ))}
    </ul>
  );
}

function CatalogRow({
  e,
  plan,
  expanded,
  busy,
  onToggle,
  onInstall,
}: {
  e: CatalogEntry;
  plan: InstallPlan;
  expanded: boolean;
  busy: boolean;
  onToggle: () => void;
  onInstall: () => void;
}) {
  const brings = plan.entries.length > 1;
  return (
    <Card>
      <div className="flex flex-wrap items-center gap-2">
        {brings ? (
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
            <span className="truncate text-xs font-medium">{e.name}</span>
          </button>
        ) : (
          <span className="truncate text-xs font-medium">{e.name}</span>
        )}
        <code className="truncate font-mono text-2xs text-text-dim">{e.slug}</code>
        <TagChips tags={e.tags} max={4} />
        <span className="ml-auto shrink-0">
          {e.installed ? (
            <Chip
              tone="ok"
              icon={ICON.ok}
              title={t("settings-catalog-panel-object-already-answers-id-here-may")}
            >{t("settings-catalog-panel-installed")}</Chip>
          ) : (
            <Button size="sm" variant="primary" disabled={busy} onClick={onInstall}>
              {busy ? t("settings-catalog-panel-installing") : t("settings-catalog-panel-install")}
            </Button>
          )}
        </span>
      </div>

      <p className="mt-1 text-2xs text-text-dim">{e.description}</p>
      <PlanLine plan={plan} />
      {expanded && <PlanList plan={plan} />}
    </Card>
  );
}

// ---------------------------------------------------------------------------
// What the last install did
// ---------------------------------------------------------------------------

/** Report order, matching the order the installer writes in. */
const CREATED_ORDER: [CatalogKind, "agents" | "skills" | "teams" | "channels" | "connectors" | "addons"][] = [
  ["skill", "skills"],
  ["agent", "agents"],
  ["team", "teams"],
  ["channel", "channels"],
  ["connector", "connectors"],
  ["addon", "addons"],
];

const totalCreated = (i: Installed): number =>
  CREATED_ORDER.reduce((n, [, key]) => n + i[key].length, 0);

/**
 * The receipt. It stays until the next install rather than fading with the
 * toast, because the interesting answer to a one-line request is the list of
 * nine things it produced, and a list nobody had time to read is a list
 * nobody can act on.
 */
function InstallReport({
  entry,
  installed,
  onDismiss,
}: {
  entry: CatalogEntry;
  installed: Installed;
  onDismiss: () => void;
}) {
  const total = totalCreated(installed);
  return (
    <Card className="border-accent/40">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="text-xs font-medium">
            {total === 0 ? t("settings-catalog-panel-already-here-2", { entry: entry.name }) : t("settings-catalog-panel-installed-2", { entry: entry.name })}
          </p>
          <p className="mt-0.5 text-2xs text-text-dim">
            {total === 0
              ? t("settings-catalog-panel-everything-needs-already-answers-id-so")
              : t("settings-catalog-panel-created-definition-definitions-anything-needs-already", { total })}
          </p>
        </div>
        <Button size="sm" variant="ghost" onClick={onDismiss}>{t("settings-catalog-panel-dismiss")}</Button>
      </div>
      {total > 0 && (
        <ul className="mt-2 flex flex-col gap-1">
          {CREATED_ORDER.filter(([, key]) => installed[key].length > 0).map(([kind, key]) => (
            <li key={kind} className="flex flex-wrap items-start gap-1.5">
              <Chip tone="quiet">{countPhrase(installed[key].length, kind)}</Chip>
              <span className="min-w-0 flex-1 font-mono text-2xs text-text-dim">
                {installed[key].join(", ")}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

/**
 * The first thing a fresh workspace can usefully do, said once.
 *
 * Shown only while *nothing* has been installed, which is the one moment the
 * reader has no way to infer what any of these words mean from the rest of
 * the app: there is no roster to compare an agent against and no library to
 * compare a skill against. It disappears after the first install rather than
 * becoming permanent furniture on a screen people will visit for years.
 */
function FirstRun({ entries }: { entries: CatalogEntry[] }) {
  return (
    <Card className="border-accent/40">
      <p className="text-xs font-medium">{t("settings-catalog-panel-nobody-works-here-yet")}</p>
      <p className="mt-1 text-2xs text-text-dim">{t("settings-catalog-panel-workspace-holds-one-agent-nothing-else", { catalog: summarize(entries) })}</p>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// The screen
// ---------------------------------------------------------------------------

export default function CatalogPanel({ kind: fixedKind }: { kind?: CatalogKind } = {}) {
  const toast = useToast();
  const { entries, loading, error, reload } = useCatalog();
  // A fixed kind (each Library entry is one kind now) hides the kind
  // switcher; without one the panel still reads `?kind=` and offers the strip.
  const [kindParam, setKind] = useSearchValue("kind");
  const kind = fixedKind ?? ((CATALOG_KINDS.includes(kindParam as CatalogKind) ? kindParam : "agent") as CatalogKind);
  const [query, setQuery] = useState("");
  const [tagFilter, setTagFilter] = useState<TagFilterState>(NO_TAG_FILTER);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<CatalogEntry | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [report, setReport] = useState<{ entry: CatalogEntry; installed: Installed } | null>(null);

  const ofKind = useMemo(() => entries.filter((e) => e.kind === kind), [entries, kind]);

  // The two controls compose, and the facets are built from the whole kind
  // rather than from what the search left, so a selected tag cannot vanish
  // from under you as you type.
  const shown = useMemo(
    () => filterByTagsAndWords(ofKind, tagFilter, query, (e) => e.tags, (e) => [e.name, e.slug, e.description]),
    [ofKind, query, tagFilter],
  );

  const install = async (entry: CatalogEntry) => {
    setBusy(`${entry.kind}/${entry.slug}`);
    let installed: Installed | null = null;
    // A refusal — a collision on an id this entry did not create, or an
    // unknown slug — arrives as the store's own message naming what is in the
    // way. It is surfaced exactly as written: "install failed" would leave the
    // owner guessing whether their own definition had been overwritten.
    await attempt(async () => {
      const res = await api.installCatalogEntry(entry.kind, entry.slug);
      installed = res.installed;
    }, toast.error);
    setBusy(null);
    if (!installed) return;
    const created = totalCreated(installed);
    setReport({ entry, installed });
    toast.ok(
      created === 0
        ? t("settings-catalog-panel-already-here-nothing-changed", { entry: entry.name })
        : t("settings-catalog-panel-installed-3", { entry: entry.name, installed: installedSummary(installed) }),
    );
    // Every row carries `installed`, so the whole listing is stale the moment
    // this succeeds — including rows of other kinds, which a transitive
    // install may have just filled in.
    reload();
  };

  const confirmPlan = confirming ? planFor(confirming, entries) : null;
  const nothingInstalled = entries.length > 0 && entries.every((e) => !e.installed);

  // Bare, with no padding of its own: the Settings content area already
  // supplies the gutter, and a panel that adds a second one reads as inset.
  if (loading && entries.length === 0) return <Pending what={t("settings-catalog-panel-catalog")} rows={pendingRows(t("settings-catalog-panel-catalog"))} />;
  if (error && entries.length === 0) return <ErrorNote error={error} retry={reload} />;

  return (
    <div className="flex max-w-4xl flex-col gap-4">
      <div className="flex flex-col gap-3">
        <p className="max-w-3xl text-2xs text-text-dim">
          <strong className="font-medium text-text">{t("settings-catalog-panel-nothing-here-exists-workspace-until-install")}</strong>{" "}
          {t("settings-catalog-panel-install-brings-what-entry-needs")}
        </p>

        <div className="flex flex-wrap items-center gap-2">
          {/* One kind per Library entry now: the switcher only shows
              when the panel is not pinned to a kind. */}
          {!fixedKind && (
            <SegmentedControl
              label={t("settings-catalog-panel-kind")}
              value={kind}
              onChange={(k) => {
                setKind(k);
                setExpanded(null);
              }}
              options={CATALOG_KINDS.map((k) => ({
                id: k,
                icon: KIND_ICON[k],
                // The count is in the label rather than a badge because the
                // interesting number here is how much there is to choose from,
                // and a segmented control has no second line to put it on.
                label: t("settings-catalog-panel-kind-tab", { kind: k, count: entries.filter((e) => e.kind === k).length }),
              }))}
            />
          )}
          <TextInput
            value={query}
            placeholder={t("settings-catalog-panel-search", { kind: plural(kind) })}
            aria-label={t("settings-catalog-panel-search-2", { kind: plural(kind) })}
            className="h-7 max-w-64 py-0"
            onChange={(ev) => setQuery(ev.target.value)}
          />
          <span className="tnum text-2xs text-text-dim">
            {t("settings-catalog-panel-shown-of-not-installed", { shown: shown.length, all: ofKind.length, free: ofKind.filter((e) => !e.installed).length })}
          </span>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={reload}>{t("settings-catalog-panel-refresh")}</Button>
        </div>
      </div>

      <div className="flex flex-col gap-3">
        {error && <ErrorNote error={error} retry={reload} />}

        {nothingInstalled && <FirstRun entries={entries} />}

        {report && (
          <InstallReport
            entry={report.entry}
            installed={report.installed}
            onDismiss={() => setReport(null)}
          />
        )}

        <p className="text-2xs text-text-dim">{KIND_BLURB[kind]}</p>

        <TagFilterBar
          items={ofKind}
          tagsOf={tagsOfEntry}
          value={tagFilter}
          onChange={setTagFilter}
        />

        {shown.length === 0 ? (
          <EmptyState
            title={t("settings-catalog-panel-nothing-matches")}
            icon={KIND_ICON[kind]}
            hint={t("settings-catalog-panel-catalog-matches-search-tags-have", { kind })}
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
            {shown.map((e) => {
              const key = `${e.kind}/${e.slug}`;
              const plan = planFor(e, entries);
              return (
                <CatalogRow
                  key={key}
                  e={e}
                  plan={plan}
                  expanded={expanded === key}
                  busy={busy === key}
                  onToggle={() => setExpanded(expanded === key ? null : key)}
                  // An entry that creates more than itself gets a
                  // confirmation naming every object first: nine agents is
                  // nine keypairs and nine candidates every unassigned work
                  // item routes across, and none of it is undone by one
                  // button.
                  onInstall={() => (plan.creates.length > 1 ? setConfirming(e) : void install(e))}
                />
              );
            })}
          </div>
        )}
      </div>

      <ConfirmDialog
        open={confirming !== null}
        onClose={() => setConfirming(null)}
        title={confirming ? t("settings-catalog-panel-install-2", { confirming: confirming.name }) : ""}
        confirmLabel={t("settings-catalog-panel-install")}
        body={
          confirmPlan ? (
            <>
              {rich(
                "settings-catalog-panel-this-creates",
                { slugs: <span className="font-mono">{confirmPlan.creates.map((c) => c.slug).join(", ")}</span> },
                { what: creationSummary(confirmPlan.creates) },
              )}{" "}
              {confirmPlan.entries.length > confirmPlan.creates.length &&
                `${t("settings-catalog-panel-everything-else-needs-already-here-left")} `}
              {t("settings-catalog-panel-each-agent-created-mints-own-keypair")}
            </>
          ) : (
            ""
          )
        }
        onConfirm={() => {
          if (confirming) void install(confirming);
        }}
      />
    </div>
  );
}

/** Stable accessor — {@link TagFilterBar} keys its facets on the list, not on this. */
const tagsOfEntry = (e: CatalogEntry): string[] => e.tags;
