/**
 * The live sessions standing in one workstream: its harnesses with
 * their real state (running / waiting on you / done / failed) — engine-driven
 * or opened by a person in a terminal, since both report as roster sessions —
 * each session's sub-agents nested under it and collapsible on their own, and
 * its remaining shells with their liveness — the same picture, and the same
 * `SessionMark` words, the rail draws. Both the Workstreams list and a
 * workstream's own panel mount this, so the right panel reads identically to
 * the rail. The rows are `workstreamSessionsModel.mjs`'s; this paints them.
 */

import { Dot, ICON, LiveDuration, RelativeTime, SessionMark, harnessMark, modelWords, useCollapsed } from "../../ui";
import { isLive } from "../../ui/sessionState.mjs";
import type { SessionRow, SessionState } from "../../types";
import type { TerminalSessionState, Liveness } from "../../shell/terminalsModel.mjs";
import { workstreamSessionRows } from "../_workbench/workstreamSessionsModel.mjs";
import type { WorkstreamSessionRow } from "../_workbench/workstreamSessionsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * A terminal's liveness as a dot tone and a short word. A live shell is
 * *open*, in the neutral tone: it says nothing about what runs in it — a
 * harness that reports is a session row of its own, and a working dot is only
 * ever a state a session reported.
 */
function livenessDot(l: Liveness): { tone: "danger" | "neutral"; word: string } {
  if (l.status === "live") return { tone: "neutral", word: t("work-workstream-sessions-open") };
  if (l.status === "exited") return l.code ? { tone: "danger", word: t("work-workstream-sessions-exited-code", { code: l.code }) } : { tone: "neutral", word: t("work-workstream-sessions-exited") };
  return { tone: "neutral", word: t("work-workstream-sessions-unverifiable") };
}

/** How long a session has been in its state: counting up while live, else ago. */
function Elapsed({ row }: { row: WorkstreamSessionRow }) {
  if (row.started != null && isLive(row.state as SessionState)) return <LiveDuration since={row.started} className="tnum shrink-0 text-text-dim" />;
  if (row.since != null) return <RelativeTime at={row.since} className="shrink-0" />;
  return null;
}

/** One terminal row: its glyph, its label, its liveness word and a tone dot. */
function TerminalRow({ row, harnessLabels }: { row: WorkstreamSessionRow; harnessLabels: Record<string, string> }) {
  const l = livenessDot(row.liveness!);
  const Glyph = row.harness ? harnessMark(row.harness) : ICON.shell;
  return (
    <li className="flex h-row-sm items-center gap-2 pl-6 text-2xs text-text-dim">
      <Glyph size={12} aria-hidden className="shrink-0" />
      <span className="min-w-0 flex-1 truncate">{row.harness ? harnessLabels[row.harness] ?? row.harness : t("work-workstream-sessions-shell")}</span>
      <span className="tnum shrink-0">{l.word}</span>
      <Dot tone={l.tone} />
    </li>
  );
}

/** The model after a row's name, then the effort it runs at, dimmed, the full id on hover; nothing when unknown. */
function ModelAfterName({ model, effort }: { model: string | null; effort: string | null }) {
  const words = modelWords(model, effort);
  if (!words) return null;
  return (
    <span className="min-w-0 shrink truncate text-text-dim" title={words.full}>
      {t("work-workstream-sessions-dot-model", { model: words.short })}
    </span>
  );
}

/** One sub-agent row, nested a step deeper under its harness. */
function SubAgentRow({ row }: { row: WorkstreamSessionRow }) {
  const Glyph = harnessMark(row.harness);
  return (
    <li className="flex h-row-sm items-center gap-2 pl-10 text-2xs text-text-dim">
      <Glyph size={11} aria-hidden className="shrink-0 opacity-70" />
      <span className="min-w-0 shrink-0 truncate font-medium">{row.label}</span>
      <span className="min-w-0 flex-1 truncate">{row.activity}</span>
      <Elapsed row={row} />
      <SessionMark state={row.state!} title={row.activity} />
    </li>
  );
}

/**
 * A harness session and its sub-agents: the session's own row with a chevron
 * that folds the sub-agents under it, then those rows when open. The fold is
 * the rail's (`rail.agent.<id>` in the one collapsed store), so a harness
 * folded there is folded here and back. The row carries no usage line —
 * what the harness's account has left is the window footer's, as it is in
 * the rail (ide/07).
 */
function SessionGroup({ row, subs, harnessLabels }: { row: WorkstreamSessionRow; subs: WorkstreamSessionRow[]; harnessLabels: Record<string, string> }) {
  const [collapsed, toggle] = useCollapsed(`rail.agent.${row.id}`);
  const Glyph = harnessMark(row.harness);
  const count = row.childCount ?? subs.length;
  return (
    <>
      <li className="flex h-row-sm items-center gap-2 pl-2 text-2xs text-text">
        {count > 0 ? (
          <button
            type="button"
            aria-label={collapsed ? t("work-workstream-sessions-show-sub-agents") : t("work-workstream-sessions-hide-sub-agents")}
            onClick={toggle}
            className="anim flex h-4 w-4 shrink-0 items-center justify-center rounded text-text-dim hover:text-text"
          >
            {collapsed ? <ICON.collapsed size={12} aria-hidden /> : <ICON.expanded size={12} aria-hidden />}
          </button>
        ) : (
          <span className="w-4 shrink-0" />
        )}
        <Glyph size={12} aria-hidden className="shrink-0 text-text-dim" />
        <span className="min-w-0 shrink-0 truncate font-medium">{row.harness ? harnessLabels[row.harness] ?? row.harness : row.label}</span>
        <ModelAfterName model={row.model} effort={row.effort} />
        <span className="min-w-0 flex-1 truncate text-text-dim">{row.activity}</span>
        {count > 0 && (
          <span className="tnum shrink-0 rounded-full border border-border px-1.5 text-3xs text-text-dim" title={t("work-workstream-sessions-sub-agents", { n: count })}>
            {count}
          </span>
        )}
        <Elapsed row={row} />
        <SessionMark state={row.state!} title={row.activity} />
      </li>
      {!collapsed && subs.map((s) => <SubAgentRow key={s.id} row={s} />)}
    </>
  );
}

export function WorkstreamSessions({
  sessions,
  terminals,
  workstream,
  harnessLabels = {},
}: {
  sessions: readonly SessionRow[];
  terminals: readonly TerminalSessionState[];
  workstream: string;
  /** Harness id → label, so a row reads *Claude Code*, not `claude-code`. */
  harnessLabels?: Record<string, string>;
}) {
  const rows = workstreamSessionRows(sessions, terminals, workstream);
  if (rows.length === 0) return null;
  // Group each top-level session with its sub-agents; terminals stand alone.
  const subsOf = (id: string) => rows.filter((r) => r.parent === id);
  return (
    <ul className="mt-1 flex flex-col gap-0.5">
      {rows
        .filter((r) => r.parent === null)
        .map((r) =>
          r.kind === "terminal" ? (
            <TerminalRow key={r.id} row={r} harnessLabels={harnessLabels} />
          ) : (
            <SessionGroup key={r.id} row={r} subs={subsOf(r.id)} harnessLabels={harnessLabels} />
          ),
        )}
    </ul>
  );
}
