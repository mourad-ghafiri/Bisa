/**
 * ⌘K: jump to anything, and do the handful of things that have no home.
 *
 * The palette is where someone goes to find out what the app can do, which
 * makes every shortcut printed here a promise. They have been broken before —
 * a `KeyHint` advertised a combination `shortcuts.ts` had never registered —
 * so the rule is now explicit: **a combo shown here must be one
 * `useGlobalShortcuts` actually binds.** Entries without a binding show no
 * hint rather than an aspirational one.
 *
 * The panel is the kit's {@link Dialog}, which is Radix underneath, so focus
 * is trapped, Escape closes, the page behind does not scroll and focus
 * returns where it came from. The hand-rolled version got none of that for
 * free and had drifted from the rest of the app's overlays.
 *
 * The list is a combobox, not a menu: focus stays in the input and
 * `aria-activedescendant` moves. That is why the rows carry `role="option"`
 * over a real `<button>` — a screen reader hears one control with a moving
 * selection rather than forty controls it has to walk, and the element is
 * still a button for the pointer and still not a `div` with a click handler.
 *
 * Three sources feed it, and they are deliberately not the same source.
 * Conversations, goals and agents come from the shell store and are
 * instant. Teams and projects are fetched when the palette opens, because
 * they are not on screen until then and {@link useWorkspaceData} exists to
 * keep the sidebar from fetching, not to load everything the app has. Goal
 * full-text comes from `GET /search`, debounced and merged underneath.
 */

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { errorFields, log } from "../log";
import { navigate, useRoute, type Route, type SearchPatch } from "../router";
import { usePathIndex } from "./pathIndexStore";
import { activeEditor } from "../views/_workbench/editorRegistry";
import { MAX_ITEMS, MAX_ROWS, SECTION_CAPS, admit, groupItems, itemMatches, paletteNeedle, parsePrefix, rankPaths } from "./quickOpenScore.mjs";
import { Avatar, Dialog, Dot, ICON, cn, harnessMark } from "../ui";
import { useSurface } from "../ui/openSurfaces";
import { CommandHint } from "./CommandHint";
import { boolOf, choiceOf } from "./settingsModel.mjs";
import { BOARD_DEFAULTS, BOARD_ENABLED_KEY } from "../views/_board/boardSettings.mjs";
import { useResolvedSettings } from "./useResolvedSettings";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, DOCUMENT_MODE, MODES, nextMode } from "../views/_workbench/ideModeModel.mjs";
import { ideModeNow, setIdeMode, toggleIdeMode } from "../views/_workbench/ideModeStore";
import { BOARD_COMMAND, MODE_COMMAND, modeSwitchLabel, modeWords } from "../views/_workbench/workbenchChromeModel.mjs";
import { HolderBadge } from "../views/_goals/HolderBadge";
import type { ProjectRow, TeamDef, WorkItemRef, WorkstreamRef } from "../types";
import { audiencePrincipals } from "../types";
import { usePrimaryNav } from "./navOrderStore";
import { goToSection } from "./sectionDoor";
import { settingsSearch } from "../views/_settings/settingsLink.mjs";
import { useWorkspace } from "./useWorkspaceData";
import { useConversations } from "../views/_workbench/conversationsStore";
import { originWords, routeOf, titleOf } from "../views/_studio/conversationsModel.mjs";
import { GIT_STASH, NEW_CHANNEL, NEW_DOCUMENT, NEW_GOAL, NEW_MESSAGE, NEW_WORKSTREAM, OPEN_BROWSER, OPEN_FILE, fire, openBoard } from "./shortcuts";
import { openUrlInBrowser } from "./browserDoors";
import { urlRow } from "./omniboxUrlModel.mjs";
import { canOpenBrowser } from "./useBrowsers";
import { useLaunchableHarnesses } from "./useHarnesses";
import { exitNote, hasLaunched, launchKey, terminalTitle, livenessTone, harnessOf } from "./terminalsModel.mjs";
import { terminalTail } from "../terminal/tails";
import { attachContext } from "../views/_workbench/agentPaneStore";
import { openPanelView, showRightPanel } from "../views/_workbench/rightPanelStore";
import { fileChip, selectionChip, terminalChip, workItemChip } from "../views/_workbench/contextChips.mjs";
import { terminalChipName } from "./terminalChipModel.mjs";
import { toWorkspaceSymbols } from "../views/_workbench/lspModel.mjs";
import { stateLabel } from "../views/_work/workstreamCardModel.mjs";
import type { FileScope } from "../types";
import { canOpenTerminal, focusTerminalTab, openTerminalIn, useTerminals } from "./useTerminals";
import type { TerminalScope } from "../terminal/session";
import {
  ACCENTS,
  DENSITIES,
  THEMES,
  getAppearance,
  setAccent,
  setDensity,
  setTheme,
  setFontUi,
  setFontMono,
  setTypeScale,
  stepTypeScale,
  FONTS_UI,
  FONTS_MONO,
  TYPE_SCALE,
  watchAppearance,
  type Appearance,
} from "./theme";
import { t as tr } from "../i18n/l10n.mjs";

interface Item {
  key: string;
  label: string;
  hint?: string;
  group: string;
  /** Extra words the entry should match on but not display. */
  keywords?: string;
  leading?: ReactNode;
  trailing?: ReactNode;
  run: () => void;
}

const LIST_ID = "omnibox-list";

/**
 * What the palette is listing.
 *
 * `all` is ⌘K: everything the app can do or go to. `places` is ⌘P: only the
 * things a workbench can be rooted at — projects, workstreams and work items —
 * which is the answer to "switch between what I am working on" and is pressed
 * far more often.
 *
 * A mode rather than a second component, because the hard parts here are the
 * combobox (`role="option"` with a moving `aria-activedescendant`, the cursor
 * clamp, the scroll nudge, the focus trap), and a bespoke switcher would
 * reimplement all four subtly differently.
 */
export type OmniboxMode = "all" | "places" | "commands" | "line";

export function Omnibox({
  open,
  mode = "all",
  onClose,
}: {
  open: boolean;
  mode?: OmniboxMode;
  onClose: () => void;
}) {
  const ws = useWorkspace();
  // The destinations in the person's order, as the sidebar shows them.
  const nav = usePrimaryNav();
  // A native layer (the browser tab) yields while the palette is open.
  useSurface(open);
  // Every live conversation (13 — Conversations), read while the palette is open.
  const conversations = useConversations({ archived: false, limit: 50 }, open);
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const [remote, setRemote] = useState<{ id: string; title: string | null; statement: string }[]>(
    [],
  );
  const [teams, setTeams] = useState<TeamDef[]>([]);
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [workstreams, setWorkstreams] = useState<WorkstreamRef[]>([]);
  const [workItems, setWorkItems] = useState<WorkItemRef[]>([]);
  const [appearance, setAppearance] = useState<Appearance>(getAppearance);
  const input = useRef<HTMLInputElement>(null);
  const listEl = useRef<HTMLDivElement>(null);

  // Quick open: the files of the root the workbench is on.
  const route = useRoute();
  const rootScope = route.name === "workbench" ? route.scope : null;
  const rootId = route.name === "workbench" ? route.id : null;
  const root = useMemo(() => (rootScope && rootId ? { scope: rootScope, id: rootId } : null), [rootScope, rootId]);
  // The IDE's default mode, for the mode entry's fallback (ide/09 §Agent Mode).
  const { resolved } = useResolvedSettings(null);
  const harnesses = useLaunchableHarnesses();
  const { launched, sessions: terminalSessions, active: activeTerminal } = useTerminals();
  // `@` symbols: `workspace/symbol` through the node, for the language of the
  // document that has focus — the server that knows this root.
  const [symbols, setSymbols] = useState<{ name: string; kind: string; path: string; line: number }[]>([]);
  const [symbolNote, setSymbolNote] = useState(tr("shell-omnibox-symbols-arrive-language-server-open-source"));
  useEffect(() => {
    const typed = q.trim();
    if (!typed.startsWith("@") || !root) return;
    const needle = typed.slice(1).trim();
    const ed = activeEditor();
    if (!ed?.path) {
      setSymbols([]);
      setSymbolNote(tr("shell-omnibox-open-source-file-root-first-language"));
      return;
    }
    let alive = true;
    api
      .lspRequest(root.scope as FileScope, root.id, ed.path, "workspace/symbol", { query: needle })
      .then((r) => {
        if (!alive) return;
        setSymbols(toWorkspaceSymbols(r.result));
        setSymbolNote(needle ? tr("shell-omnibox-symbol-matches") : tr("shell-omnibox-type-search-symbols"));
      })
      .catch((e: unknown) => {
        if (!alive) return;
        // What the server said is the log's; the palette says it in words a person reads.
        log.warn("omnibox", "the language server could not be asked for symbols", { path: ed.path, ...errorFields(e) });
        setSymbols([]);
        setSymbolNote(tr("shell-omnibox-language-server-file"));
      });
    return () => {
      alive = false;
    };
  }, [q, root]);
  // The path index per root (ide/12), shared with the Files tab's name search
  // through `pathIndexStore`: fetched once per visit, patched from
  // `file_changed`, never walked per keystroke.
  const pathIndex = usePathIndex(open && mode !== "commands" && root ? (root.scope as FileScope) : null, open && mode !== "commands" && root ? root.id : null);
  const index: readonly string[] = useMemo(() => pathIndex?.paths ?? [], [pathIndex?.paths]);
  const indexTruncated = pathIndex?.truncated ?? false;

  // Another surface may change the appearance while the palette is open.
  useEffect(() => watchAppearance(setAppearance), []);

  useEffect(() => {
    if (!open) return;
    setQ(mode === "commands" ? ">" : mode === "line" ? ":" : "");
    setCursor(0);
    setRemote([]);
    // Radix focuses the panel's first control on mount; this runs after that
    // and puts the caret where typing should go.
    const t = setTimeout(() => input.current?.focus(), 0);
    return () => clearTimeout(t);
    // `mode` is read for the opening prefix only: a mode changed while the
    // palette is open is the person typing a prefix, not a reason to clear it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Teams and projects, once per opening. Stale lists in a jump-to palette
  // send people to things that have been renamed or deleted.
  useEffect(() => {
    if (!open) return;
    const ac = new AbortController();
    api
      .teams(ac.signal)
      .then((r) => setTeams(r.teams))
      .catch((e: unknown) => {
        // The palette still jumps to everything else.
        if (ac.signal.aborted) return;
        log.debug("palette", "the teams could not be read; the palette still jumps to everything else", errorFields(e));
      });
    api
      .allProjects({}, ac.signal)
      .then((r) => setProjects(r.projects))
      .catch((e: unknown) => {
        // As above.
        if (ac.signal.aborted) return;
        log.debug("palette", "the projects could not be read; the palette still jumps to everything else", errorFields(e));
      });
    // The other two things a workbench roots at. Workspace-wide routes rather
    // than per-project and per-goal ones: asking every project for its
    // workstreams and every goal for its items would be N requests to draw one
    // list, which is the same argument `GET /projects` already makes.
    api
      .allWorkstreams(ac.signal)
      .then((r) => setWorkstreams(r.workstreams))
      .catch((e: unknown) => {
        // The palette still jumps to everything else.
        if (ac.signal.aborted) return;
        log.debug("palette", "the workstreams could not be read; the palette still jumps to everything else", errorFields(e));
      });
    api
      .allWorkItems(ac.signal)
      .then((r) => setWorkItems(r.work_items))
      .catch((e: unknown) => {
        // As above.
        if (ac.signal.aborted) return;
        log.debug("palette", "the work items could not be read; the palette still jumps to everything else", errorFields(e));
      });
    return () => ac.abort();
  }, [open]);

  // Full-text search, debounced; local results never wait for it.
  useEffect(() => {
    if (!open || q.trim().length < 2) {
      setRemote([]);
      return;
    }
    const ac = new AbortController();
    const t = setTimeout(() => {
      api
        .search(q.trim(), ac.signal)
        .then((r) =>
          setRemote(r.results.map((h) => ({ id: h.id, title: h.title, statement: h.statement }))),
        )
        .catch((e: unknown) => {
          // Search is an accelerator; local matches still stand.
          if (ac.signal.aborted) return;
          log.debug("palette", "the search could not be run; the local matches still stand", errorFields(e));
        });
    }, 180);
    return () => {
      clearTimeout(t);
      ac.abort();
    };
  }, [open, q]);

  const go = useCallback(
    (route: Route, search?: SearchPatch) => {
      navigate(route, search);
      onClose();
    },
    [onClose],
  );
  // A section's door, as the sidebar's row is one: where the person was in it.
  const enter = useCallback(
    (route: Route) => {
      goToSection(route);
      onClose();
    },
    [onClose],
  );

  const act = useCallback(
    (event: string) => {
      fire(event);
      onClose();
    },
    [onClose],
  );

  const items = useMemo<Item[]>(() => {
    const prefix = mode === "all" ? null : parsePrefix(q).prefix;
    const needle = paletteNeedle(q, mode);
    const out: Item[] = [];
    const counts = new Map<string, number>();
    // The match and the per-section cap are `quickOpenScore`'s rules.
    const push = (item: Item) => {
      if (!itemMatches(item, needle)) return;
      if (!admit(counts, item.group)) return;
      out.push(item);
    };

    /** Files under the current root, fuzzy-scored — the quick-open half of ⌘P. */
    const pushFiles = () => {
      if (!root || index.length === 0) return;
      for (const { path } of rankPaths(needle, index, SECTION_CAPS.Files)) {
        const at = path.lastIndexOf("/");
        out.push({
          key: `f:${path}`,
          label: at === -1 ? path : path.slice(at + 1),
          hint: at === -1 ? undefined : path.slice(0, at),
          group: tr("shell-omnibox-files"),
          leading: <ICON.file size={14} aria-hidden />,
          run: () => {
            // A document is shown in the Project centre: the root goes there first.
            setIdeMode(`${root.scope}:${root.id}`, DOCUMENT_MODE);
            go({ name: "workbench", scope: root.scope, id: root.id }, { doc: `file:${path}` });
          },
        });
      }
      if (indexTruncated && needle === "") {
        out.push({
          key: "f:truncated",
          label: tr("shell-omnibox-index-stops-cap-type-narrow"),
          group: tr("shell-omnibox-files"),
          run: () => {},
        });
      }
    };

    /** `:42` — a line in the document that is open. */
    const pushLine = () => {
      const editor = activeEditor();
      const line = Number.parseInt(needle, 10);
      if (!editor?.revealLine || !Number.isFinite(line) || line <= 0) {
        out.push({
          key: "line:hint",
          label: editor?.revealLine ? tr("shell-omnibox-type-line-number") : tr("shell-omnibox-editable-document-open"),
          group: tr("shell-omnibox-go-line"),
          run: () => {},
        });
        return;
      }
      out.push({
        key: `line:${line}`,
        label: tr("shell-omnibox-go-line-2", { line }),
        group: tr("shell-omnibox-go-line"),
        leading: <ICON.forward size={14} aria-hidden />,
        run: () => {
          editor.revealLine?.(line);
          onClose();
        },
      });
    };

    /**
     * Everything a workbench can be rooted at.
     *
     * Ordered projects → workstreams → work items rather than alphabetically:
     * that is roughly widest-to-narrowest, and somebody who types a branch name
     * wants the branch, not the repository that contains it.
     */
    const pushPlaces = () => {
      for (const p of projects) {
        push({
          key: `p:${p.project.id}`,
          label: p.project.name,
          hint: p.path,
          group: tr("shell-omnibox-projects"),
          keywords: (p.project.tags ?? []).join(" "),
          leading: <ICON.project size={14} aria-hidden />,
          // A project opens on its primary workstream — the two share one id.
          run: () => go({ name: "workbench", scope: "workstream", id: p.project.id }),
        });
      }
      // Every shell everywhere (ide/06): the terminal's title, where it is
      // rooted, and how it is doing — so a build you left running is a keystroke away.
      for (const t of terminalSessions) {
        const note = exitNote(t);
        push({
          key: `t:${t.key}`,
          label: terminalTitle(t),
          hint: [t.scope === "workstream" ? projects.find((p) => p.project.id === t.id)?.project.name ?? workstreams.find((w) => w.workstream.id === t.id)?.workstream.name : t.id, note].filter(Boolean).join(" · "),
          group: tr("shell-resource-terminals"),
          leading: (() => {
            const Mark = harnessOf(t) ? harnessMark(harnessOf(t)) : ICON.harness;
            return <Mark size={14} aria-hidden />;
          })(),
          trailing: <Dot tone={livenessTone(t) === "danger" ? "danger" : "neutral"} />,
          run: () => {
            focusTerminalTab(t.key);
            // A shell in the home directory (a code host sign-in) has no
            // workbench root to open; focusing its tab is the whole act.
            if (t.scope !== "machine") go({ name: "workbench", scope: t.scope, id: t.id }, { doc: `terminal:${t.key}` });
          },
        });
      }
      for (const w of workstreams) {
        // The primary is the project row above.
        if (w.workstream.kind.kind === "primary") continue;
        const branch =
          w.workstream.name ?? (w.workstream.kind.kind === "worktree" ? w.workstream.kind.branch : null);
        push({
          key: `w:${w.workstream.id}`,
          label: branch ?? tr("shell-omnibox-copy-id", { id: w.workstream.id.slice(-6) }),
          // The project's name, because a branch on its own does not say which
          // repository it is in and two projects may both have a `work/fix-…`.
          hint: [w.project_name, stateLabel(w.workstream.state), w.exists ? null : tr("shell-omnibox-checkout")]
            .filter(Boolean)
            .join(" · "),
          group: tr("shell-omnibox-workstreams"),
          leading: <ICON.workstream size={14} aria-hidden />,
          run: () => go({ name: "workbench", scope: "workstream", id: w.workstream.id }),
        });
      }
      for (const r of workItems) {
        push({
          key: `wi:${r.item.id}`,
          label: r.item.instructions.split("\n")[0]?.slice(0, 70) || tr("shell-omnibox-work-item-tail", { tail: r.item.id.slice(-6) }),
          hint: [r.item.state.state, r.label].filter(Boolean).join(" · "),
          group: tr("shell-omnibox-work-items"),
          leading: <ICON.workItem size={14} aria-hidden />,
          run: () => go({ name: "workbench", scope: "work_item", id: r.item.id }),
        });
      }
      // Every live conversation with the agents (13 — Conversations): found by
      // its name or first line; one about a checkout opens in the IDE on that
      // checkout's Agent panel, the rest on their own page.
      for (const c of conversations.rows) {
        const door = routeOf(c);
        push({
          key: `conv:${c.id}`,
          label: titleOf(c),
          hint: originWords(c.origin),
          group: tr("shell-aux-pane-conversations"),
          leading: <ICON.dm size={14} aria-hidden />,
          run: () => go(door.route as Parameters<typeof go>[0], door.search ?? undefined),
        });
      }
    };

    // ⌘P is one list and nothing else: no actions, no destinations, no search.
    /** The things the app can do or go to — the `>` half, and all of ⌘K's head. */
    const pushCommands = () => {
    // The harness picker, from the keyboard (ide/06): one entry per harness
    // the node can run, in the root you are looking at, plus the shell.
    // Absent outside the workbench and outside the desktop shell, because a
    // terminal needs somewhere to be rooted and a PTY to open.
    // Context for the Agent tab, from the keyboard (ide/09): the open file,
    // the selection, the active terminal's last lines, this work item.
    if (root && root.scope !== "goal") {
      const ed = activeEditor();
      const sel = ed?.selection?.();
      const rootKey = `${root.scope}:${root.id}`;
      const attach = (label: string, key: string, make: () => Parameters<typeof attachContext>[0] | null, hint?: string) =>
        push({
          key: `attach:${key}`,
          label,
          hint,
          group: tr("shell-omnibox-do"),
          keywords: tr("shell-omnibox-attach-context-chip-agents"),
          leading: <ICON.attach size={14} aria-hidden />,
          run: () => {
            const chip = make();
            if (chip) attachContext(chip, rootKey);
            showRightPanel("agents", rootKey);
            onClose();
          },
        });
      if (ed?.path) attach(tr("shell-omnibox-attach-open-file", { name: ed.path.split("/").pop() }), "file", () => fileChip(ed.path as string));
      if (ed?.path && sel && sel.text.trim()) attach(tr("shell-omnibox-attach-editor-s-selection"), "selection", () => selectionChip(ed.path as string, sel.start, sel.end, sel.text));
      if (activeTerminal) {
        attach(tr("shell-omnibox-attach-active-terminal-s-last-lines"), "terminal", () => {
          const lines = terminalTail(activeTerminal);
          const s = terminalSessions.find((t) => t.key === activeTerminal);
          // The tab's own words, as every other Send-to-agent door names it (`terminalChipModel`).
          return lines ? terminalChip(s ? terminalChipName(s, terminalSessions) : activeTerminal, lines) : null;
        });
      }
      if (root.scope === "work_item") attach(tr("shell-omnibox-attach-work-item"), "work-item", () => workItemChip(root.id));
    }
    // Board Mode — every workstream a card in the centre (ide/16) — from
    // anywhere, unless the setting hides it.
    const boardEnabled = boolOf(resolved, BOARD_ENABLED_KEY, BOARD_DEFAULTS.enabled);
    if (boardEnabled) {
      const board = modeWords("board");
      push({
        key: "go:board",
        label: tr("shell-omnibox-mode-every-workstream-card", { board: board.label }),
        hint: tr("shell-omnibox-backlog-todo-doing-done-archived"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-board-kanban-workstreams-cards-columns-due"),
        leading: <ICON.board size={14} aria-hidden />,
        trailing: <CommandHint id={BOARD_COMMAND} />,
        run: () => {
          openBoard();
          onClose();
        },
      });
    }
    // A workstream on the project you are in: the rail's dialog, from the keyboard.
    if (root && root.scope === "workstream") {
      // The centre's next mode (ide/09 §Agent Mode, ide/16), from the keyboard.
      const modeRoot = `${root.scope}:${root.id}`;
      const fallback = choiceOf(resolved, DEFAULT_MODE_KEY, MODES, DEFAULT_MODE);
      const modeNow = ideModeNow(modeRoot, fallback, boardEnabled);
      const next = modeWords(nextMode(modeNow, boardEnabled));
      const NextIcon = ICON[next.icon];
      push({
        key: "a:ide-mode",
        label: modeSwitchLabel(modeNow, boardEnabled),
        hint: next.hint,
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-agent-mode-project-mode-board-mode"),
        leading: <NextIcon size={14} aria-hidden />,
        trailing: <CommandHint id={MODE_COMMAND} />,
        run: () => {
          toggleIdeMode(modeRoot, fallback, boardEnabled);
          onClose();
        },
      });
      push({
        key: "a:document",
        label: tr("shell-omnibox-new-file-untitled-document-here"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-new-file-untitled-document-create-blank"),
        leading: <ICON.file size={14} aria-hidden />,
        trailing: <CommandHint id="new_document" />,
        run: () => act(NEW_DOCUMENT),
      });
      push({
        key: "a:open-file",
        label: tr("shell-omnibox-open-file-from-machine"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-open-file-machine-disk-loose-finder"),
        leading: <ICON.folderOpen size={14} aria-hidden />,
        trailing: <CommandHint id="open_file" />,
        run: () => act(OPEN_FILE),
      });
      push({
        key: "a:workstream",
        label: tr("shell-omnibox-workstream-open-one-project"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-workstream-branch-checkout-new-open"),
        leading: <ICON.workstream size={14} aria-hidden />,
        trailing: <CommandHint id="new_workstream" />,
        run: () => act(NEW_WORKSTREAM),
      });
      // Stash, from the keyboard (ide/04 §Stash): Git › Stashes opens with
      // its dialog; the verb itself stays consented behind the dialog.
      push({
        key: "a:stash",
        label: tr("shell-omnibox-stash-park-working-tree-s-changes"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-stash-git-park-changes-shelve"),
        leading: <ICON.stash size={14} aria-hidden />,
        run: () => {
          openPanelView("git", "stashes", `${root.scope}:${root.id}`);
          act(GIT_STASH);
        },
      });
    }
    if (root && canOpenTerminal()) {
      const scope = root.scope as TerminalScope;
      push({
        key: "t:shell",
        label: tr("shell-omnibox-terminal-shell-here"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-terminal-shell-console"),
        leading: <ICON.harness size={14} aria-hidden />,
        run: () => openTerminalIn({ scope, id: root.id }),
      });
      for (const h of harnesses) {
        if (!h.installed) continue;
        const resumes =
          (h.launch?.resume_args?.length ?? 0) > 0 && hasLaunched(launched, launchKey(h.id, scope, root.id));
        push({
          key: `t:${h.id}`,
          label: resumes ? tr("shell-omnibox-terminal-here-resume", { h: h.label }) : tr("shell-omnibox-terminal-here", { h: h.label }),
          group: tr("shell-omnibox-do"),
          keywords: tr("shell-omnibox-terminal-harness-agent", { h: h.id }),
          leading: (() => {
            const Mark = harnessMark(h.id);
            return <Mark size={14} aria-hidden />;
          })(),
          run: () => openTerminalIn({ scope, id: root.id, harness: h.id }),
        });
        if (resumes) {
          push({
            key: `t:${h.id}:fresh`,
            label: tr("shell-omnibox-terminal-here-fresh-session", { h: h.label }),
            group: tr("shell-omnibox-do"),
            keywords: tr("shell-omnibox-terminal-harness-agent-new", { h: h.id }),
            leading: <ICON.add size={14} aria-hidden />,
            run: () => openTerminalIn({ scope, id: root.id, harness: h.id, resume: false }),
          });
        }
      }
    }
    push({
      key: "a:goal",
      label: tr("shell-omnibox-new-goal"),
      group: tr("shell-omnibox-do"),
      keywords: tr("shell-omnibox-create-add"),
      leading: <ICON.goal size={14} aria-hidden />,
      trailing: <CommandHint id="new_goal" />,
      run: () => act(NEW_GOAL),
    });
    if (canOpenBrowser()) {
      // What was typed reads as an address: one row opens it where the person is (ide/18).
      const address = urlRow(q);
      if (address) {
        out.push({
          key: "a:url",
          label: address.label,
          hint: address.hint,
          group: tr("shell-omnibox-do"),
          leading: <ICON.page size={14} aria-hidden />,
          run: () => openUrlInBrowser(address.url),
        });
      }
      push({
        key: "a:browser",
        label: tr("shell-omnibox-browser-show-hide-browser-pane"),
        group: tr("shell-omnibox-do"),
        keywords: tr("shell-omnibox-browser-web-page-tab-pane-screenshot"),
        leading: <ICON.page size={14} aria-hidden />,
        trailing: <CommandHint id="open_browser" />,
        run: () => act(OPEN_BROWSER),
      });
    }
    push({
      key: "a:channel",
      label: tr("shell-omnibox-new-channel"),
      group: tr("shell-omnibox-do"),
      keywords: tr("shell-omnibox-create-add-channel"),
      leading: <ICON.channel size={14} aria-hidden />,
      trailing: <CommandHint id="new_channel" />,
      run: () => act(NEW_CHANNEL),
    });
    push({
      key: "a:dm",
      label: tr("shell-omnibox-new-message"),
      group: tr("shell-omnibox-do"),
      keywords: tr("shell-omnibox-create-dm-direct"),
      leading: <ICON.dm size={14} aria-hidden />,
      trailing: <CommandHint id="new_message" />,
      run: () => act(NEW_MESSAGE),
    });

    for (const entry of nav) {
      const Icon = entry.icon;
      push({
        key: `n:${entry.key}`,
        label: tr("shell-omnibox-go", { entry: entry.label }),
        group: tr("shell-omnibox-go-2"),
        leading: <Icon size={14} aria-hidden />,
        trailing: entry.key === "inbox" ? <CommandHint id="inbox" /> : undefined,
        run: () => enter(entry.route),
      });
    }
    // The four destinations that are not nav entries — you can still want to
    // go to the list rather than to one channel, and the catalog is a Settings
    // panel now, so nothing generates it any more.
    push({
      key: "n:catalog",
      label: tr("shell-omnibox-go-catalog"),
      group: tr("shell-omnibox-go-2"),
      keywords: tr("shell-omnibox-install-agents-skills-teams-library"),
      leading: <ICON.catalog size={14} aria-hidden />,
      run: () => go({ name: "settings" }, settingsSearch("catalog-agent")),
    });
    push({
      key: "n:channels",
      label: tr("shell-omnibox-go-channels"),
      group: tr("shell-omnibox-go-2"),
      leading: <ICON.channel size={14} aria-hidden />,
      run: () => enter({ name: "channels" }),
    });
    push({
      key: "n:messages",
      label: tr("shell-omnibox-go-messages"),
      group: tr("shell-omnibox-go-2"),
      leading: <ICON.dm size={14} aria-hidden />,
      run: () => enter({ name: "messages" }),
    });
    push({
      key: "n:settings",
      label: tr("shell-omnibox-go-settings"),
      group: tr("shell-omnibox-go-2"),
      leading: <ICON.settings size={14} aria-hidden />,
      trailing: <CommandHint id="settings" />,
      run: () => go({ name: "settings" }),
    });

    // Appearance is twenty-six entries. Offering them unprompted would make the
    // palette's resting state a settings screen, so they are reachable by
    // typing — "dark", "teal", "compact", "inter", "larger" — and invisible otherwise.
    if (needle || prefix === ">" || mode === "commands") {
      const on = (yes: boolean) =>
        yes ? <ICON.check size={13} aria-hidden className="text-text" /> : undefined;
      for (const t of THEMES) {
        push({
          key: `t:${t.id}`,
          label: tr("shell-omnibox-theme", { t: t.label }),
          group: tr("shell-omnibox-appearance"),
          keywords: tr("shell-omnibox-theme-appearance", { scheme: t.scheme ?? "auto" }),
          trailing: on(appearance.theme === t.id),
          run: () => {
            setTheme(t.id);
            onClose();
          },
        });
      }
      for (const a of ACCENTS) {
        push({
          key: `ac:${a.id}`,
          label: tr("shell-omnibox-accent", { a: a.label }),
          group: tr("shell-omnibox-appearance"),
          keywords: tr("shell-omnibox-accent-colour-color-appearance"),
          trailing: on(appearance.accent === a.id),
          run: () => {
            setAccent(a.id);
            onClose();
          },
        });
      }
      for (const d of DENSITIES) {
        push({
          key: `d:${d.id}`,
          label: tr("shell-omnibox-density", { d: d.label }),
          group: tr("shell-omnibox-appearance"),
          keywords: tr("shell-omnibox-density-spacing-rows-appearance"),
          trailing: on(appearance.density === d.id),
          run: () => {
            setDensity(d.id);
            onClose();
          },
        });
      }
      for (const f of FONTS_UI) {
        push({
          key: `fu:${f.id}`,
          label: tr("shell-omnibox-font", { f: f.label }),
          group: tr("shell-omnibox-appearance"),
          keywords: tr("shell-omnibox-font-face-typeface-interface-appearance"),
          trailing: on(appearance.fontUi === f.id),
          run: () => {
            setFontUi(f.id);
            onClose();
          },
        });
      }
      for (const f of FONTS_MONO) {
        push({
          key: `fm:${f.id}`,
          label: tr("shell-omnibox-code-font", { f: f.label }),
          group: tr("shell-omnibox-appearance"),
          keywords: tr("shell-omnibox-font-mono-monospace-code-editor-terminal"),
          trailing: on(appearance.fontMono === f.id),
          run: () => {
            setFontMono(f.id);
            onClose();
          },
        });
      }
      const text: Array<{ key: string; label: string; keywords: string; run: () => void }> = [
        { key: "ts:+", label: tr("shell-omnibox-text-larger"), keywords: tr("shell-omnibox-text-size-bigger-zoom-appearance"), run: () => stepTypeScale(1) },
        { key: "ts:-", label: tr("shell-omnibox-text-smaller"), keywords: tr("shell-omnibox-text-size-zoom-out-appearance"), run: () => stepTypeScale(-1) },
        { key: "ts:0", label: tr("shell-omnibox-text-reset-size"), keywords: tr("shell-omnibox-text-size-default-reset-appearance"), run: () => setTypeScale(TYPE_SCALE.default) },
      ];
      for (const t of text) {
        push({
          key: t.key,
          label: t.label,
          group: tr("shell-omnibox-appearance"),
          keywords: t.keywords,
          trailing: <span className="tnum text-2xs text-text-dim">{Math.round(appearance.typeScale * 100)}%</span>,
          run: t.run,
        });
      }
    }

    };

    if (mode === "places" || mode === "commands" || mode === "line") {
      if (prefix === ":") {
        pushLine();
        return out;
      }
      if (prefix === "#") {
        for (const r of workItems) {
          push({
            key: `wi:${r.item.id}`,
            label: r.item.instructions.split("\n")[0]?.slice(0, 70) || tr("shell-omnibox-work-item-tail", { tail: r.item.id.slice(-6) }),
            hint: [r.item.state.state, r.label].filter(Boolean).join(" · "),
            group: tr("shell-omnibox-work-items"),
            leading: <ICON.workItem size={14} aria-hidden />,
            run: () => go({ name: "workbench", scope: "work_item", id: r.item.id }),
          });
        }
        return out;
      }
      if (prefix === ">" || mode === "commands") {
        pushCommands();
        return out;
      }
      if (prefix === "@") {
        if (symbols.length === 0) {
          out.push({
            key: "sym:hint",
            label: symbolNote,
            group: tr("shell-omnibox-symbols"),
            run: () => {},
          });
        }
        for (const s of symbols.slice(0, SECTION_CAPS.Files)) {
          push({
            key: `sym:${s.path}:${s.line}:${s.name}`,
            label: s.name,
            hint: `${s.kind} · ${s.path}:${s.line}`,
            group: tr("shell-omnibox-symbols"),
            leading: <ICON.file size={14} aria-hidden />,
            run: () => {
              if (!root) return;
              setIdeMode(`${root.scope}:${root.id}`, DOCUMENT_MODE);
              go({ name: "workbench", scope: root.scope, id: root.id }, { doc: `file:${s.path}` } as never);
              window.setTimeout(() => activeEditor()?.revealLine?.(s.line), 300);
            },
          });
        }
        return out;
      }
      pushFiles();
      pushPlaces();
      return out.slice(0, MAX_ROWS);
    }

    pushCommands();

    for (const i of ws.goals) {
      push({
        key: `i:${i.id}`,
        label: i.title ?? tr("shell-omnibox-goal-tail", { tail: i.id.slice(0, 6) }),
        group: tr("shell-omnibox-goals"),
        keywords: i.statement ?? "",
        trailing: <HolderBadge holder={i.holder} strip={i.strip} />,
        run: () => go({ name: "goal", id: i.id }),
      });
    }

    for (const { channel } of ws.channels) {
      push({
        key: `c:${channel.id}`,
        label: `#${channel.name}`,
        hint: channel.topic ?? undefined,
        group: tr("shell-sidebar-channels"),
        leading: <ICON.channel size={14} aria-hidden />,
        run: () => go({ name: "channel", id: channel.id }),
      });
    }

    for (const { channel } of ws.dms) {
      const others = audiencePrincipals(channel).filter((p) => p !== ws.me);
      const label = others.map((p) => ws.nameOf(p)).join(", ") || tr("shell-omnibox-just");
      const head = others[0] ?? channel.id;
      push({
        key: `d:${channel.id}`,
        label,
        group: tr("shell-sidebar-messages"),
        leading: <Avatar id={head} name={label} photo={ws.photoOf(head)} size={16} />,
        run: () => go({ name: "dm", id: channel.id }),
      });
    }

    for (const a of ws.agents) {
      push({
        key: `g:${a.id}`,
        label: a.name,
        hint: a.harness,
        group: tr("shell-omnibox-agents"),
        keywords: (a.tags ?? []).join(" "),
        leading: <Avatar id={a.pubkey} name={a.name} photo={a.photo} size={16} />,
        run: () => go({ name: "agent", id: a.id }),
      });
    }

    for (const t of teams) {
      push({
        key: `tm:${t.id}`,
        label: t.name,
        hint: t.purpose ?? undefined,
        group: tr("shell-omnibox-teams"),
        keywords: (t.tags ?? []).join(" "),
        leading: <ICON.team size={14} aria-hidden />,
        // There is no team *route* yet, so the selection travels in the hash
        // query the way every other in-screen selection does. The Teams
        // screen reads it when it lands; until then this is a landing.
        run: () => go({ name: "teams" }, { team: t.id }),
      });
    }

    pushPlaces();

    const seen = new Set(ws.goals.map((i) => i.id));
    for (const r of remote) {
      if (seen.has(r.id)) continue;
      // Already a server-side match, so it is not filtered again here.
      out.push({
        key: `s:${r.id}`,
        label: r.title ?? r.statement.slice(0, 60),
        hint: "full-text match",
        group: tr("shell-omnibox-search"),
        run: () => go({ name: "goal", id: r.id }),
      });
    }

    return out.slice(0, MAX_ITEMS);
  }, [act, appearance, conversations.rows, enter, go, mode, nav, onClose, projects, q, remote, teams, workItems, workstreams, ws, root, index, indexTruncated, activeTerminal, harnesses, launched, resolved, symbolNote, symbols, terminalSessions]);

  useEffect(() => {
    setCursor((c) => Math.min(c, Math.max(0, items.length - 1)));
  }, [items.length]);

  // Keep the highlighted row on screen when the arrow keys walk past the fold.
  useEffect(() => {
    listEl.current
      ?.querySelector<HTMLElement>('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor, items]);

  // The sections in the order they were first met — the build order is the
  // ranking — and the flat list the cursor walks (`quickOpenScore.groupItems`).
  const { groups, flat } = useMemo(() => groupItems(items), [items]);
  const activeId = flat[cursor] ? `omnibox-${flat[cursor].key}` : undefined;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={mode === "commands" ? tr("shell-keymap-commands") : mode === "places" ? tr("shell-omnibox-quick-open") : mode === "line" ? tr("shell-omnibox-go-line") : tr("shell-omnibox-search-jump")}
      width="max-w-xl"
    >
      <div className="control-group anim flex items-center gap-2 rounded-control border border-border bg-bg px-2">
        <ICON.search size={14} aria-hidden className="shrink-0 text-text-dim" />
        <input
          ref={input}
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setCursor(0);
          }}
          placeholder={
            mode === "commands"
              ? tr("shell-omnibox-commands-placeholder")
              : mode === "line"
              ? tr("shell-omnibox-line-number")
              : mode === "places"
              ? root
                ? tr("shell-omnibox-files-projects-workstreams-prefixes")
                : tr("shell-omnibox-projects-workstreams-work-items")
              : tr("shell-omnibox-channels-messages-goals-agents-teams-projects")
          }
          role="combobox"
          aria-expanded
          aria-controls={LIST_ID}
          aria-activedescendant={activeId}
          aria-autocomplete="list"
          autoComplete="off"
          spellCheck={false}
          className="h-9 w-full bg-transparent text-sm text-text outline-none placeholder:text-text-dim"
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setCursor((c) => Math.min(c + 1, flat.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setCursor((c) => Math.max(c - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              flat[cursor]?.run();
            }
          }}
        />
      </div>

      <div ref={listEl} id={LIST_ID} role="listbox" aria-label={tr("shell-omnibox-results")} className="mt-2 max-h-[52vh] overflow-y-auto">
        {flat.length === 0 && (
          <p className="px-3 py-6 text-center text-2xs text-text-dim">{tr("shell-omnibox-nothing-matches", { q })}</p>
        )}
        {groups.map(([group, bucket]) => (
          <div key={group} className="mb-1">
            <p className="px-2 pb-1 pt-2 text-2xs font-semibold text-text-dim">
              {group}
            </p>
            {bucket.map((it) => {
              const idx = flat.indexOf(it);
              const selected = idx === cursor;
              return (
                <button
                  key={it.key}
                  id={`omnibox-${it.key}`}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  // Focus stays in the input; these are reached with the
                  // arrow keys, so they must not also be tab stops.
                  tabIndex={-1}
                  onMouseEnter={() => setCursor(idx)}
                  onClick={it.run}
                  className={cn(
                    "anim flex h-row w-full items-center gap-2 rounded-control px-2 text-left text-xs",
                    selected ? "bg-selected text-text" : "text-text hover:bg-surface-2",
                  )}
                >
                  {it.leading && <span className="flex shrink-0 items-center text-text-dim">{it.leading}</span>}
                  <span className="min-w-0 flex-1 truncate">{it.label}</span>
                  {it.hint && (
                    <span className="max-w-48 truncate text-2xs text-text-dim">{it.hint}</span>
                  )}
                  {it.trailing}
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </Dialog>
  );
}
