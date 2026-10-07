/**
 * Everything about this node, as named groups rather than a strip of tabs.
 *
 * A tab strip is a promise that the things on it are peers you switch between
 * while doing one job. These are not: "which theme" and "which relay" and
 * "who may approve a delivery" belong to different questions on different
 * days, and lining them all up in one row made the reader scan every label to
 * find any of them. A left rail with named groups lets them skip all but one
 * group before they read a single entry. The groups, their panels and the
 * words of each are `_settings/settingsLink.mjs`'s (`SETTINGS_GROUPS`).
 *
 * **The panel stays in the URL.** `?tab=` is what makes Back leave a panel
 * instead of the whole screen, and it is how other screens link straight at
 * one — an empty MCP list elsewhere points at `settings?tab=mcp`.
 *
 * There is no `<h1>` here. The shell's top chrome already renders the screen
 * title, and the views that grew their own header before it existed are why
 * every page had two.
 *
 * The rail and each panel come back where they were scrolled
 * (`shell/useViewScroll`) — and that is all the screen remembers of itself:
 * a panel's forms start from what the node says, never from what was typed
 * and left. So the rail asks before it leaves a panel whose explicit-save
 * form holds unsaved edits (`_settings/unsavedModel.mjs`): leaving drops them,
 * and that is the person's call, not a click's side effect.
 *
 * One content width for every panel (`max-w-3xl` on the column), so moving
 * between panels never moves the edge the eye reads against.
 */

import { useEffect, useRef, useState } from "react";
import { useSearchValue } from "../router";
import { useViewScroll } from "../shell/useViewScroll";
import { useWorkspace } from "../shell/useWorkspaceData";
import { placeOf } from "../shell/viewMemoryStore";
import { ConfirmDialog, ErrorNote, GitMark, ICON, Spinner, cn, yieldKeptScroll } from "../ui";
import type { LucideIcon, Mark } from "../ui";
import { GovernancePanel } from "./_work/GovernancePanel";
import { AppearancePanel } from "./_settings/AppearancePanel";
import { BrowserAccessPanel } from "./_settings/BrowserAccessPanel";
import { MobileDevelopmentPanel } from "./_settings/MobileDevelopmentPanel";
import { AGENTS_KEY as MOBILE_DEVELOPMENT_AGENTS_KEY, ENABLED_KEY as MOBILE_DEVELOPMENT_ENABLED_KEY, PLATFORMS_KEY as MOBILE_DEVELOPMENT_PLATFORMS_KEY } from "./_settings/mobileDevelopmentSettingsModel.mjs";
import { CachePanel } from "./_settings/CachePanel";
import { GoalsPanel } from "./_settings/GoalsPanel";
import { IdePanel } from "./_settings/IdePanel";
import { AUTO_CEILING_KEY, AUTO_PERMISSIONS_KEY, DEFAULT_MODE_KEY } from "./_goal/goalMode.mjs";
import { BOARD_KEYS } from "./_board/boardSettings.mjs";
import { RegistryPanel } from "./_settings/RegistryPanel";
import { GlobalGitPanel } from "./_settings/GlobalGitPanel";
import { GitProfilesPanel } from "./_settings/GitProfilesPanel";
import { CodeHostPanel } from "./_settings/CodeHostPanel";
import { SshPanel } from "./_settings/SshPanel";
import { KeymapPanel } from "./_settings/KeymapPanel";
import CatalogPanel from "./_settings/CatalogPanel";
import { HarnessesPanel } from "./_settings/HarnessesPanel";
import { SystemPanel } from "./_settings/SystemPanel";
import { WhereIWasCard } from "./_settings/WhereIWasCard";
import { IdentityPanel } from "./_settings/IdentityPanel";
import { McpPanel } from "./_settings/McpPanel";
import { PeoplePanel } from "./_settings/PeoplePanel";
import { NetworkPanel } from "./_settings/NetworkPanel";
import { NodePanel } from "./_settings/NodePanel";
import { LoggingPanel } from "./_settings/LoggingPanel";
import { NotesPanel } from "./_settings/NotesPanel";
import { DrawPanel } from "./_settings/DrawPanel";
import { PetPanel } from "./_settings/PetPanel";
import { AddonsPanel } from "./_settings/AddonsPanel";
import { SkillsPanel } from "./_settings/SkillsPanel";
import { RelaysPanel } from "./_settings/RelaysPanel";
import { ConnectorsPanel } from "./_settings/ConnectorsPanel";
import { ClassifierPanel, GuardPanel, RedactorPanel } from "./_settings/SecurityPanels";
import { SCALARS } from "./_settings/securityRules.mjs";
import { DecisionsPanel } from "./_settings/DecisionsPanel";
import { leaveAsks } from "./_settings/unsavedModel.mjs";
import { forgetUnsaved, useUnsavedForms } from "./_settings/unsavedStore";

/** The `security.collaboration.*` keys, drawn beside the classifier's (14-collaboration). */
const COLLABORATION_KEYS = ["security.collaboration.classify", "security.collaboration.agent_tools"] as const;
/** The `security.content.*` keys — what an agent reads from outside (11 — Security). */
const READING_KEYS = ["security.content.screen", "security.content.on_harmful"] as const;
import { SETTINGS_GROUPS, settingsTab, type SettingsPanel, type SettingsTab } from "./_settings/settingsLink.mjs";
import { t } from "../i18n/l10n.mjs";

/**
 * The glyph of each panel, from `ui/icons`. The rail's structure and words
 * are `settingsLink.mjs`'s (`SETTINGS_GROUPS`) — the one place a sentence
 * that sends a person to a panel reads them from too; the glyphs live here
 * because a model holds no component. Typed over every panel id, so a panel
 * added to the rail without a glyph is a compile error, not a blank row.
 *
 * They come from `ui/icons` for the reason that file exists: `members` is not
 * `team` and `node` is not `mcpServer`, however close the symbols look, and a
 * rail that borrows another concept's icon teaches the reader the wrong
 * association.
 */
const GLYPH: Record<SettingsTab, LucideIcon | Mark> = {
  identity: ICON.identity,
  appearance: ICON.appearance,
  pet: ICON.pet,
  notes: ICON.note,
  draw: ICON.draw,
  people: ICON.members,
  sync: ICON.sync,
  governance: ICON.governance,
  "catalog-agent": ICON.agent,
  "catalog-skill": ICON.skill,
  "catalog-team": ICON.team,
  "catalog-channel": ICON.channel,
  "catalog-connector": ICON.connector,
  "catalog-workflow": ICON.template,
  addons: ICON.addon,
  skills: ICON.skill,
  mcp: ICON.mcpServer,
  connectors: ICON.connector,
  harnesses: ICON.harness,
  system: ICON.system,
  desktop: ICON.layout,
  network: ICON.network,
  browser: ICON.page,
  "mobile-development": ICON.simulator,
  "security-redactor": ICON.guard,
  "security-guard": ICON.guard,
  "security-classifier": ICON.guard,
  "decision-making": ICON.decisions,
  // Git & code hosts › Identity — who commits: git is the tool, so the Git mark, never You › Identity's key.
  git: GitMark,
  "git-ssh": ICON.fingerprint,
  github: ICON.account,
  gitlab: ICON.account,
  bitbucket: ICON.account,
  ide: ICON.agent,
  editor: ICON.document,
  terminal: ICON.harness,
  workstreams: ICON.workstream,
  board: ICON.project,
  diagrams: ICON.image,
  artifacts: ICON.artifact,
  agents: ICON.agent,
  keymap: ICON.key,
  lsp: ICON.tool,
  events: ICON.signal,
  goals: ICON.goal,
  workflow: ICON.workflow,
  budgets: ICON.usage,
  cache: ICON.pulse,
  node: ICON.node,
  logging: ICON.logs,
};

/** Flattened once, so `?tab=` resolves against one list rather than a group at a time. */
const PANELS: readonly SettingsPanel[] = SETTINGS_GROUPS.flatMap((g) => g.panels);

/** Where the screen keeps its memory. */
const PLACE = placeOf({ name: "settings" });

export default function Settings() {
  const [tabParam, setTab] = useSearchValue("tab");
  const id = settingsTab(tabParam);
  const panel = PANELS.find((p) => p.id === id) ?? PANELS[0]!;

  const { info, offline, ready, refresh } = useWorkspace();

  const PanelGlyph = GLYPH[panel.id];

  const blocked = panel.needsWorkspace && !info;
  // One path shows every panel: each is kept under its own name, and put
  // back when it is the one shown.
  const root = useRef<HTMLDivElement>(null);
  useViewScroll(root, `${PLACE}#${panel.id}`, PLACE);
  // The nav keeps the place it was scrolled to; arriving at a panel by its
  // route — a link, the omnibox — must still show the row that is current,
  // so a row the kept place hides is brought into view, the kept place
  // yielding first (`yieldKeptScroll`), as every programmatic scroll does.
  const nav = useRef<HTMLElement>(null);
  useEffect(() => {
    const list = nav.current;
    const row = list?.querySelector<HTMLElement>('[aria-current="page"]');
    if (!list || !row) return;
    const box = list.getBoundingClientRect();
    const own = row.getBoundingClientRect();
    if (own.top >= box.top && own.bottom <= box.bottom) return;
    yieldKeptScroll(list);
    row.scrollIntoView({ block: "nearest" });
  }, [panel.id]);

  // A panel with unsaved edits asks before the rail leaves it.
  const unsaved = useUnsavedForms();
  const [leavingTo, setLeavingTo] = useState<SettingsTab | null>(null);
  const go = (to: SettingsTab) => {
    if (leaveAsks(unsaved, panel.id, to)) setLeavingTo(to);
    else setTab(to);
  };

  return (
    <div ref={root} className="flex h-full min-h-0">
      <nav
        ref={nav}
        aria-label={t("screens-settings-settings-sections")}
        data-scroll-keep="nav"
        className="w-48 shrink-0 overflow-y-auto border-r border-border px-2 py-3"
      >
        {SETTINGS_GROUPS.map((g) => (
          <div key={g.id} className="mb-4 last:mb-0">
            <h2 className="px-2 pb-1 text-2xs font-semibold text-text-dim">
              {g.label}
            </h2>
            <ul className="flex flex-col gap-px">
              {g.panels.map((p) => {
                const on = p.id === panel.id;
                const Glyph = GLYPH[p.id];
                return (
                  <li key={p.id}>
                    <button
                      type="button"
                      // `aria-current` rather than `aria-selected`: these are
                      // places in the URL, not tabs over one panel, and a
                      // screen reader should hear "current page".
                      aria-current={on ? "page" : undefined}
                      onClick={() => go(p.id)}
                      // The current panel is where you are, drawn the way the
                      // sidebar draws its current row: a neutral fill, never
                      // the accent, which means something waits on you.
                      className={cn(
                        "anim flex h-row w-full items-center gap-2 rounded-control px-2 text-left text-xs",
                        on
                          ? "bg-selected font-medium text-text"
                          : "text-text-dim hover:bg-surface-2 hover:text-text focus-visible:bg-surface-2",
                      )}
                    >
                      <Glyph size={14} aria-hidden className="shrink-0 opacity-70" />
                      {p.label}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </nav>

      <div data-scroll-keep={`panel:${panel.id}`} className="min-w-0 flex-1 overflow-y-auto">
        <header className="max-w-3xl px-6 pt-4 pb-3">
          <h2 className="flex items-center gap-2 text-base font-semibold">
            <PanelGlyph size={16} aria-hidden className="shrink-0 text-text-dim" />
            {panel.label}
          </h2>
          <p className="mt-1 max-w-measure text-2xs leading-relaxed text-text-dim">{panel.blurb}</p>
        </header>

        {/* A column with a gap, so a hand-written panel and the registry's
            settings under it read as two blocks rather than one run-on. */}
        <div className="flex max-w-3xl flex-col gap-6 px-6 pb-10">
          {blocked ? (
            offline ? (
              <ErrorNote error={offline} retry={refresh} />
            ) : ready ? (
              <ErrorNote error={t("screens-settings-could-not-read-workspace")} retry={refresh} />
            ) : (
              <Spinner label={t("screens-settings-reading-workspace")} />
            )
          ) : (
            <>
              {panel.id === "identity" && info && <IdentityPanel ws={info} />}
              {panel.id === "appearance" && <AppearancePanel />}
              {panel.id === "pet" && <PetPanel />}
              {panel.id === "addons" && <AddonsPanel />}
              {panel.id === "notes" && <NotesPanel />}
              {panel.id === "draw" && (
                <>
                  <DrawPanel />
                  <RegistryPanel group="draw" />
                </>
              )}
              {panel.id === "people" && info && <PeoplePanel />}
              {panel.id === "sync" && info && <RelaysPanel />}
              {panel.id === "governance" && <GovernancePanel />}
              {panel.id === "catalog-agent" && <CatalogPanel kind="agent" />}
              {panel.id === "catalog-skill" && <CatalogPanel kind="skill" />}
              {panel.id === "catalog-team" && <CatalogPanel kind="team" />}
              {panel.id === "catalog-channel" && <CatalogPanel kind="channel" />}
              {panel.id === "catalog-connector" && <CatalogPanel kind="connector" />}
              {panel.id === "catalog-workflow" && <CatalogPanel kind="workflow" />}
              {panel.id === "ide" && (
                <>
                  <IdePanel />
                  <RegistryPanel group="ide" omit={["ide.default_mode"]} />
                </>
              )}
              {panel.id === "editor" && <RegistryPanel group="editor" />}
              {panel.id === "terminal" && <RegistryPanel group="terminal" />}
              {panel.id === "git" && (
                <>
                  <GlobalGitPanel />
                  <GitProfilesPanel />
                  <RegistryPanel group="git" />
                </>
              )}
              {panel.id === "git-ssh" && <SshPanel />}
              {panel.id === "github" && <CodeHostPanel kind="github" />}
              {panel.id === "gitlab" && <CodeHostPanel kind="gitlab" />}
              {panel.id === "bitbucket" && <CodeHostPanel kind="bitbucket" />}
              {/* The three script texts are About › Settings' card; the trust list is the engine's, written only by Approve. */}
              {panel.id === "workstreams" && (
                <RegistryPanel
                  group="workstreams"
                  omit={["workstreams.script.pre_create", "workstreams.script.post_create", "workstreams.script.clean", "workstreams.script.trusted", ...BOARD_KEYS]}
                />
              )}
              {panel.id === "board" && <RegistryPanel group="workstreams" only={BOARD_KEYS} />}
              {panel.id === "diagrams" && <RegistryPanel group="diagrams" />}
              {panel.id === "artifacts" && <RegistryPanel group="artifacts" />}
              {panel.id === "agents" && <RegistryPanel group="agents" />}
              {panel.id === "keymap" && (
                <>
                  <KeymapPanel />
                  <RegistryPanel group="keymap" />
                </>
              )}
              {panel.id === "lsp" && <RegistryPanel group="lsp" />}
              {panel.id === "skills" && <SkillsPanel />}
              {panel.id === "mcp" && <McpPanel />}
              {panel.id === "connectors" && <ConnectorsPanel />}
              {panel.id === "harnesses" && <HarnessesPanel />}
              {panel.id === "system" && <SystemPanel />}
              {panel.id === "desktop" && (
                <>
                  <RegistryPanel group="desktop" />
                  <WhereIWasCard />
                </>
              )}
              {panel.id === "network" && <NetworkPanel />}
              {panel.id === "browser" && (
                <>
                  <BrowserAccessPanel />
                  <RegistryPanel group="browser" omit={["browser.enabled", "browser.agents", "browser.agents.headless"]} />
                </>
              )}
              {panel.id === "mobile-development" && (
                <>
                  <MobileDevelopmentPanel />
                  <RegistryPanel group="mobile_development" omit={[MOBILE_DEVELOPMENT_ENABLED_KEY, MOBILE_DEVELOPMENT_PLATFORMS_KEY, MOBILE_DEVELOPMENT_AGENTS_KEY]} intro={t("screens-settings-where-tools-machine-when-search-above")} />
                </>
              )}
              {panel.id === "security-redactor" && (
                <>
                  <RedactorPanel />
                  <RegistryPanel group="security" only={SCALARS.redactor} />
                </>
              )}
              {panel.id === "security-guard" && (
                <>
                  <GuardPanel />
                  <RegistryPanel group="security" only={SCALARS.guard} />
                  <RegistryPanel
                    group="security"
                    only={SCALARS.net}
                    intro={t("screens-settings-what-platform-s-own-calls-may")}
                  />
                </>
              )}
              {panel.id === "security-classifier" && (
                <>
                  <ClassifierPanel />
                  <RegistryPanel group="security" only={SCALARS.classifier} />
                  <RegistryPanel
                    group="security"
                    only={READING_KEYS}
                    intro={t("screens-settings-what-agents-read-from-outside-page")}
                  />
                  <RegistryPanel
                    group="security"
                    only={COLLABORATION_KEYS}
                    intro={t("screens-settings-people-other-nodes-whether-message-from")}
                  />
                </>
              )}
              {panel.id === "decision-making" && <DecisionsPanel />}
              {panel.id === "events" && <RegistryPanel group="events" />}
              {panel.id === "goals" && (
                <>
                  <GoalsPanel />
                  <RegistryPanel group="goals" omit={[DEFAULT_MODE_KEY, AUTO_CEILING_KEY, AUTO_PERMISSIONS_KEY]} />
                </>
              )}
              {panel.id === "workflow" && <RegistryPanel group="workflow" />}
              {panel.id === "budgets" && <RegistryPanel group="budget" />}
              {panel.id === "cache" && <CachePanel />}
              {panel.id === "node" && <NodePanel />}
              {panel.id === "logging" && <LoggingPanel />}
            </>
          )}
        </div>
      </div>
      <ConfirmDialog
        open={leavingTo !== null}
        onClose={() => setLeavingTo(null)}
        onConfirm={() => {
          const to = leavingTo;
          setLeavingTo(null);
          forgetUnsaved();
          if (to) setTab(to);
        }}
        title={t("settings-unsaved-leave-title")}
        body={t("settings-unsaved-leave-body")}
        confirmLabel={t("settings-unsaved-leave-confirm")}
        danger
      />
    </div>
  );
}
