/**
 * One workstream as a card (ide/16): its title and project, the chips the
 * rail's row wears (`workstreamCardModel`), its due date, who stands in it —
 * harness marks, session marks and the pulse's line — its ports, and when
 * it last moved. A click opens the workstream; the `⋮` and the right click
 * hold the verbs, the workstream's own (`railMenuModel`) and the Board's.
 *
 * The plain `div` form of a card, not `ui/Card`: it holds links and buttons,
 * so the card itself is no button — the title is the door (a real `<button>`:
 * Enter and Space open it), and a click anywhere else on the card opens it
 * too. Every tone is a role; the drag handle is the whole card, by pointer. While it is
 * dragged the card on the board is a **placeholder** — its footprint,
 * dashed and faded — and the card under the pointer is the same component
 * drawn as the **ghost** (`ghost`: no verbs, lifted); once a drop has
 * landed it wears the accent's wash for a beat (`landed`).
 */

import type { MenuItem, SortableHandle } from "../../ui";
import { Chip, ContextMenu, ICON, Menu, RelativeTime, SessionMark, Tooltip, cn, harnessMark } from "../../ui";
import type { Pulse } from "../_workbench/workstreamPulseModel.mjs";
import type { WorkstreamSessionRow } from "../_workbench/workstreamSessionsModel.mjs";
import type { WorkstreamPort } from "../_workbench/portsModel.mjs";
import { PortChips, PulseLine } from "../_workbench/PulseLine";
import { WorkstreamStatusChips } from "../_work/WorkstreamStatusChips";
import { dueTone, projectWords, type BoardRow } from "./boardModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const DUE_INK = { quiet: "text-text-dim", warn: "text-warn", danger: "text-danger" } as const;

/**
 * The sortable's pointer listeners, without its keyboard listener or its
 * button semantics (`role`, `tabIndex`, `aria-*`): the card is a group that
 * holds buttons, not a button, and the keyboard reaches its title instead.
 */
function pointerDrag(props: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(props).filter(([k]) => k.startsWith("onPointer")));
}

export function BoardCard({
  row,
  handle,
  sessions,
  pulse,
  ports,
  harnessLabels,
  today,
  soonDays,
  lastActivity,
  items,
  onOpen,
  onOpenProject,
  openPort,
  stopPort,
  ghost = false,
  landed = false,
}: {
  row: BoardRow;
  handle: SortableHandle;
  /** Drawn under the pointer: lifted, and holding no verb. */
  ghost?: boolean;
  /** A drop just put it here. */
  landed?: boolean;
  sessions: readonly WorkstreamSessionRow[];
  pulse: Pulse | null;
  ports: readonly WorkstreamPort[];
  harnessLabels: Record<string, string>;
  today: string;
  soonDays: number;
  /** Unix seconds of the newest session activity here, else when the workstream was opened. */
  lastActivity: number;
  items: MenuItem[];
  onOpen: () => void;
  onOpenProject: () => void;
  openPort: (port: number) => void;
  stopPort: (p: WorkstreamPort) => void;
}) {
  const due = dueTone(row.due, today, soonDays);
  const harnesses = [...new Set(sessions.map((s) => s.harness).filter((h): h is string => !!h))];
  const agents = sessions.filter((s) => s.kind === "agent" && !s.parent);
  const shells = sessions.filter((s) => s.kind === "terminal");

  const body = (
      <div
        ref={ghost ? undefined : handle.ref}
        {...(ghost ? {} : pointerDrag(handle.props))}
        style={ghost ? undefined : handle.style}
        aria-hidden={ghost || undefined}
        onClick={ghost ? undefined : onOpen}
        className={cn(
          "anim group flex cursor-grab flex-col gap-1.5 rounded-card border border-border bg-surface px-3 py-2.5 shadow-sm hover:border-text-dim/30",
          ghost && "board-card-lift w-64",
          !ghost && handle.dragging && "board-card-placeholder",
          !ghost && landed && "board-card-landed",
        )}
      >
        {/* The name, the pin, the verbs. */}
        <div className="flex items-start gap-1.5">
          {/* A pin is a fact about the card, not a summons: dim ink. */}
          {row.workstream.pinned && <ICON.pin size={11} aria-label={t("board-board-card-pinned")} className="mt-1 shrink-0 text-text-dim" />}
          {ghost ? (
            <span className="min-w-0 flex-1 truncate text-sm font-medium text-text">{row.title}</span>
          ) : (
            <button
              type="button"
              title={row.title}
              aria-label={t("board-board-card-open", { title: row.title, project: row.project.name })}
              className="anim min-w-0 flex-1 cursor-pointer truncate rounded text-left text-sm font-medium text-text hover:underline"
              onClick={(e) => {
                // The card's own click opens it too; one open, not two.
                e.stopPropagation();
                onOpen();
              }}
            >
              {row.title}
            </button>
          )}
          {!ghost && (
            <span onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()} className="row-actions -mr-1">
              <Menu
                label={t("board-board-card-more", { title: row.title })}
                items={items}
                trigger={
                  <button type="button" aria-label={t("board-board-card-more", { title: row.title })} className="anim rounded-control p-0.5 text-text-dim hover:bg-surface-2 hover:text-text">
                    <ICON.more size={13} aria-hidden />
                  </button>
                }
              />
            </span>
          )}
        </div>

        {/* The project — a door of its own — and the branch. */}
        <div className="flex min-w-0 items-center gap-1.5 text-2xs text-text-dim">
          <button
            type="button"
            className="anim min-w-0 truncate hover:text-text hover:underline"
            title={t("board-board-card-open-project", { project: row.project.name })}
            onClick={(e) => {
              e.stopPropagation();
              onOpenProject();
            }}
          >
            {projectWords(row.project)}
          </button>
          {row.branch && !row.primary && (
            <>
              <ICON.collapsed size={10} aria-hidden className="shrink-0" />
              <span className="min-w-0 truncate font-mono" title={row.branch}>
                {row.branch}
              </span>
            </>
          )}
        </div>

        {/* The state chips the rail's row wears, then the due date. */}
        <div className="flex flex-wrap items-center gap-1">
          <WorkstreamStatusChips w={row.workstream} s={row.status} />
          {due.kind !== "none" && (
            <Tooltip label={t("board-board-card-due", { due: row.due })}>
              <span>
                <Chip tone={due.tone} icon={ICON.waiting}>
                  <span className={DUE_INK[due.tone]}>{due.label}</span>
                </Chip>
              </span>
            </Tooltip>
          )}
        </div>

        {/* Who stands in it: the harness marks, the sessions' marks, the pulse. */}
        {(harnesses.length > 0 || pulse) && (
          <div className="flex min-w-0 items-center gap-1.5">
            {harnesses.map((h) => {
              const Mark = harnessMark(h);
              return (
                <Tooltip key={h} label={harnessLabels[h] ?? h}>
                  <span className="flex shrink-0">
                    <Mark size={13} aria-label={harnessLabels[h] ?? h} className="text-text-dim" />
                  </span>
                </Tooltip>
              );
            })}
            {agents.map((a) => (
              <Tooltip key={a.id} label={`${a.label} — ${a.activity ?? ""}`}>
                <span className="flex shrink-0">
                  <SessionMark state={a.state ?? { state: "idle" }} size={11} />
                </span>
              </Tooltip>
            ))}
            {shells.length > 0 && (
              <span className="tnum inline-flex items-center gap-0.5 text-2xs text-text-dim" title={t("board-board-card-shell-shells", { shells: shells.length })}>
                <ICON.shell size={11} aria-hidden />
                {shells.length}
              </span>
            )}
            {pulse && <PulseLine pulse={pulse} harnessLabels={harnessLabels} />}
          </div>
        )}

        {/* The note's first line, dimmed. */}
        {row.workstream.note && (
          <p className="line-clamp-1 text-2xs text-text-dim" title={row.workstream.note}>
            {row.workstream.note.split("\n")[0]}
          </p>
        )}

        {/* Ports, and when it last moved. */}
        <div className="flex items-center gap-1.5 text-2xs text-text-dim">
          {ports.length > 0 && (
            <span onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
              <PortChips ports={ports} harnessLabels={harnessLabels} openPort={openPort} stopPort={stopPort} />
            </span>
          )}
          <span className="flex-1" />
          {!row.ref.exists && <span className="text-warn">{t("board-board-card-no-checkout")}</span>}
          <RelativeTime at={lastActivity} className="tnum" />
        </div>
      </div>
  );
  if (ghost) return body;
  return (
    <ContextMenu items={items} className="block">
      {body}
    </ContextMenu>
  );
}
