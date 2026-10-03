/**
 * The library: this workspace's workflows and the catalog's templates, every
 * one a card with a thumbnail of its graph — the picture the designer opens
 * on — in sections by domain.
 *
 * Two views, one screen, one filter bar. *Yours* lists what is installed —
 * templates that were used and workflows drawn here — with each one's
 * problems and holders; *Templates* is the catalog's own, with *Use
 * template*. The bar narrows either: a search over the name, the description,
 * the slug, a step's name or kind and the tags; a status — runs, has problems,
 * in use; installed, not yet installed; the tag facets; *Archived* on yours.
 * The filters live in the address (`libraryModel.mjs`), so a link carries a
 * search and Back restores it; the view is also remembered per machine
 * (`bisa.workflow.library.view`) because which half you live in depends on
 * how long you have had the workspace, not on the workspace.
 *
 * The tags picked and where each view was scrolled are the screen's memory
 * (`shell/viewMemoryStore`), and both lists are drawn from what the window
 * last read while the node is read again behind them — so the library comes
 * back as it was left, after a designer was opened and after a restart.
 *
 * The screen does not name itself: the shell's chrome renders the `<h1>`.
 */

import { readPref, webStorage, writePref } from "../shell/storedPrefModel.mjs";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { useEngineEvents } from "../bus";
import { navigate, setSearch, useSearchParams } from "../router";
import { Button, EmptyState, ErrorNote, ICON, NO_TAG_FILTER, ScreenBar, SegmentedControl, SkeletonRows, Switch, TagFilterBar, Tabs, TextInput, parseTagFilter, useToast, type TabDef, type TagFilterState } from "../ui";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf, useViewState } from "../shell/viewMemoryStore";
import { readKey } from "./_work/keptReadsModel.mjs";
import { attempt, useAsync } from "./_work/useAsync";
import { projectsMadeByWorkflow } from "./_work/projectOriginModel.mjs";
import { TemplateGallery } from "./_workflow/TemplateGallery";
import { WorkflowCard } from "./_workflow/WorkflowCard";
import { blankWorkflow } from "./_workflow/stepKinds.mjs";
import { definitionBody } from "./_workflow/designerSession.mjs";
import { domainLabel, groupByDomain, libraryReads, narrowed, parseFilters, searchTemplates, searchWorkflows, serializeFilters, statusSegments, viewOf, type LibraryFilters, type LibraryView } from "./_workflow/libraryModel.mjs";
import { LIBRARY_GRID } from "./_workflow/libraryLayout.mjs";
import { t as tr } from "../i18n/l10n.mjs";

const VIEW_KEY = "bisa.workflow.library.view";
const VIEWS: readonly TabDef[] = [
  { id: "yours", label: tr("screens-workflows-yours"), icon: ICON.workflow },
  { id: "templates", label: tr("screens-workflows-templates"), icon: ICON.template },
];

/** Where the screen keeps its memory. */
const PLACE = placeOf({ name: "workflows" });

function remembered(): LibraryView {
  return readPref(webStorage(), VIEW_KEY, (raw) => viewOf(raw), "yours");
}

export default function Workflows() {
  const params = useSearchParams();
  const filters = useMemo(() => parseFilters(params, remembered()), [params]);
  const { view, q, status, archived } = filters;
  // A pick is one Back away; the words typed are one entry however many keys made them (`replace`).
  const set = (next: Partial<LibraryFilters>, opts?: { replace?: boolean }) => setSearch(serializeFilters({ ...filters, ...next }), opts);
  // A preference, not state.
  useEffect(() => {
    writePref(webStorage(), VIEW_KEY, view);
  }, [view]);
  const [tagFilter, setTagFilter] = useViewState<TagFilterState>(PLACE, "tags", NO_TAG_FILTER, parseTagFilter);
  const toast = useToast();
  const [creating, setCreating] = useState(false);
  const library = useAsync((s) => api.workflows({ scope: "library", archived }, s), [archived], { keep: readKey("workflows", archived ? "archived" : "library") });
  const catalog = useAsync((s) => api.catalog({ kind: "workflow" }, s), [], { keep: readKey("catalog", "workflow") });
  // One scrollport shows two views: each is kept under its own name, and put back when it is the one shown.
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, `${PLACE}#${view}`, PLACE);
  // The projects each workflow's steps made, for the card's footer.
  const ws = useWorkspace();

  useEngineEvents((e) => {
    // Which read a fact moves is the model's to say (`libraryReads`): a
    // workflow's own news both, a run or its listening the rows alone.
    const reads = libraryReads(e.payload);
    if (reads.library) library.reload();
    if (reads.catalog) catalog.reload();
  });

  const rows = useMemo(() => library.data?.workflows ?? [], [library.data?.workflows]);
  const entries = useMemo(() => catalog.data?.entries ?? [], [catalog.data?.entries]);
  const shownRows = useMemo(() => searchWorkflows(rows, tagFilter, q, status), [rows, tagFilter, q, status]);
  const shownEntries = useMemo(() => searchTemplates(entries, tagFilter, q, status), [entries, tagFilter, q, status]);
  const total = view === "yours" ? rows.length : entries.length;
  const shown = view === "yours" ? shownRows.length : shownEntries.length;
  const isNarrowed = narrowed(filters, tagFilter);
  const clear = () => {
    setTagFilter(NO_TAG_FILTER);
    set({ q: undefined, status: "all" });
  };

  /**
   * A new workflow is a workflow from its first second: a draft recorded
   * on the node at once — *Untitled workflow*, one start by hand
   * (`blankWorkflow`) — so the designer opens on a thing the Workflow
   * Agent, the conversations and the CLI all have. The
   * `workflow_changed` frame it announces reloads the library on return.
   */
  const create = () => {
    if (creating) return;
    setCreating(true);
    void attempt(
      () => api.createWorkflow(definitionBody(blankWorkflow())),
      (message) => {
        setCreating(false);
        toast.error(message);
      },
      (r) => navigate({ name: "workflow", id: r.workflow.id }),
    );
  };

  const current = view === "yours" ? library : catalog;
  const nothingAtAll = view === "yours" ? rows.length === 0 : entries.length === 0;

  return (
    <div ref={root} className="flex h-full flex-col">
      {/* The screen's band (`ui/ScreenBar.tsx`): which workflows, as tabs; then the words, the status and the put-away ones narrow them; the count and *New workflow* last. */}
      <ScreenBar
        stack
        tabs={<Tabs bare label={tr("screens-workflows-which-workflows")} tabs={[...VIEWS]} active={view} onChange={(v) => set({ view: v as LibraryView, status: "all" })} />}
        end={
          <>
            <span className="tnum text-2xs text-text-dim">{tr("screens-goals-words", { shown, rows: total })}</span>
            {isNarrowed && (
              <Button size="sm" variant="ghost" onClick={clear}>{tr("screens-workflows-clear-filters")}</Button>
            )}
            <Button size="sm" variant="primary" disabled={creating} onClick={create}>
              <ICON.add size={12} aria-hidden />{tr("screens-workflows-new-workflow")}</Button>
          </>
        }
      >
        <span className="relative" data-library-filters>
          <ICON.search size={12} aria-hidden className="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-text-dim" />
          <TextInput value={q ?? ""} placeholder={tr("screens-workflows-search")} aria-label={tr("screens-workflows-search-2")} className="h-7 w-56 py-0 pl-7" onChange={(e) => set({ q: e.target.value || undefined }, { replace: true })} />
        </span>
        <SegmentedControl options={statusSegments(view)} value={status} onChange={(s) => set({ status: s })} label={tr("screens-workflows-which-of-them")} size="sm" />
        {view === "yours" && <Switch checked={archived} onChange={(on) => set({ archived: on })} label={tr("screens-workflows-archived")} />}
      </ScreenBar>

      <div data-scroll-keep={`list:${view}`} className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
        {/* The tags, at the head of the list as on Goals, Agents and Teams: what they narrow is right under them. */}
        {view === "yours" ? (
          <TagFilterBar items={rows} tagsOf={(r) => [...(r.workflow.tags ?? [])]} value={tagFilter} onChange={setTagFilter} className="mb-4" />
        ) : (
          <TagFilterBar items={entries} tagsOf={(e) => [...e.tags]} value={tagFilter} onChange={setTagFilter} className="mb-4" />
        )}
        {current.loading && !current.data ? (
          <SkeletonRows rows={6} />
        ) : current.error && !current.data ? (
          <ErrorNote error={current.error} retry={current.reload} />
        ) : nothingAtAll ? (
          view === "yours" ? (
            <EmptyState
              icon={ICON.workflow}
              title={tr("screens-workflows-no-workflows-yet")}
              hint={tr("screens-workflows-workflow-shape-goal-runs-few-steps")}
              action={
                <div className="flex gap-2">
                  <Button variant="primary" onClick={() => set({ view: "templates", status: "all" })}>{tr("screens-workflows-browse-templates")}</Button>
                  <Button disabled={creating} onClick={create}>{tr("screens-workflows-draw-one")}</Button>
                </div>
              }
            />
          ) : (
            <EmptyState icon={ICON.template} title={tr("workflow-template-gallery-no-templates-build")} hint={tr("workflow-template-gallery-catalog-compiled-into-binary")} action={null} />
          )
        ) : shown === 0 ? (
          <EmptyState icon={ICON.filter} title={tr("screens-agents-nothing-matches")} hint={tr("screens-workflows-nothing-matches-search")} action={<Button variant="ghost" onClick={clear}>{tr("screens-workflows-clear-filters")}</Button>} />
        ) : view === "templates" ? (
          <TemplateGallery
            entries={shownEntries}
            onInstalled={() => {
              library.reload();
              catalog.reload();
            }}
          />
        ) : (
          <div className="flex flex-col gap-6">
            {groupByDomain(shownRows, (r) => r.workflow.tags).map(([domain, list]) => (
              <section key={domain}>
                {/* A domain is a lowercase tag from the data, so it is capitalised rather than shouted; an `h2`, the level under the chrome's `h1`. */}
                <h2 className="mb-2 text-sm font-semibold text-text capitalize">{domainLabel(domain)}</h2>
                <div className={LIBRARY_GRID}>
                  {list.map((r) => (
                    <WorkflowCard key={r.workflow.id} row={r} projects={projectsMadeByWorkflow(ws.projects, r.workflow.id)} onChanged={library.reload} />
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
