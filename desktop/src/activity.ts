/**
 * Activity rendering: one line per thing that happened, as a sentence —
 * *verb, object, outcome*.
 *
 * The sentences themselves live in {@link ../activityModel.mjs activityModel.mjs},
 * which is plain JavaScript so `node --test` can import it. This file is the
 * two things that cannot: turning an icon *name* into a component, and
 * failing the build when the wire types grow a variant the model does not
 * render.
 *
 * That second job is not ceremony. Six `EnginePayload` variants went a whole
 * milestone without a `switch` arm — the hand-written type was missing them,
 * so the switch stayed exhaustive and nobody heard a thing. Moving the switch
 * to `.mjs` would give up TypeScript's check entirely, so the check is kept
 * here explicitly instead, as a map every variant must appear in.
 */

import {
  GATE_ICON,
  ICON,
  GOAL_STATUS_ICON,
  STEP_STATE_ICON,
  WORK_ITEM_STATE_ICON,
  type LucideIcon,
} from "./ui";
import {
  appendCapped as appendCappedModel,
  cents as centsModel,
  engineLine as engineLineModel,
  journalLine as journalLineModel,
  payloadLine as payloadLineModel,
  pulseLine as pulseLineModel,
  shortId as shortIdModel,
  conversationLine as conversationLineModel,
  truncate as truncateModel,
  type DetailField,
  type ActivityLine as ModelActivityLine,
  type ModelLine,
  type Tone,
} from "./activityModel.mjs";
import type {
  EngineEvent,
  JournalEntry,
  JournalPayload,
  PulseRow,
  ConversationFrame,
} from "./types";

export type { DetailField, Tone };

/** A rendered line: the model's, with its icon name resolved to a component. */
export interface ActivityLine extends Omit<ModelActivityLine, "icon"> {
  /** The concept this line is about. Absent when the name has no entry. */
  icon?: LucideIcon;
}

export const truncate = truncateModel;
export const shortId = shortIdModel;
export const cents = centsModel;
export const appendCapped = appendCappedModel as (
  lines: ActivityLine[],
  next: ActivityLine,
  cap?: number,
) => ActivityLine[];

/**
 * Name → glyph, across the five maps the rest of the app draws from.
 *
 * Looking it up rather than casting means an unrecognised name — a newer node
 * writing a gate kind this build has never heard of — draws *no* glyph
 * instead of the wrong one. That is the same reason the model emits names at
 * all: a near-miss symbol here would be the reader's first meeting with it,
 * and the association would be wrong everywhere else in the app.
 */
function resolveIcon(name: string | undefined): LucideIcon | undefined {
  if (!name) return undefined;
  const sep = name.indexOf(":");
  if (sep < 0) return undefined;
  const key = name.slice(sep + 1);
  switch (name.slice(0, sep)) {
    case "icon":
      return key in ICON ? ICON[key as keyof typeof ICON] : undefined;
    case "gate":
      return key in GATE_ICON ? GATE_ICON[key as keyof typeof GATE_ICON] : undefined;
    case "work":
      return WORK_ITEM_STATE_ICON[key];
    case "status":
      return key in GOAL_STATUS_ICON
        ? GOAL_STATUS_ICON[key as keyof typeof GOAL_STATUS_ICON]
        : undefined;
    case "step":
      return key in STEP_STATE_ICON ? STEP_STATE_ICON[key as keyof typeof STEP_STATE_ICON] : undefined;
    default:
      return undefined;
  }
}

function resolve<T extends ModelLine>(line: T): Omit<T, "icon"> & { icon?: LucideIcon } {
  return { ...line, icon: resolveIcon(line.icon) };
}

export function payloadLine(payload: JournalPayload): Omit<ModelLine, "icon"> & {
  icon?: LucideIcon;
} {
  return resolve(payloadLineModel(payload));
}

export function journalLine(e: JournalEntry, index: number): ActivityLine {
  return resolve(journalLineModel(e, index));
}

export function pulseLine(row: PulseRow): ActivityLine {
  return resolve(pulseLineModel(row));
}

export function engineLine(e: EngineEvent): ActivityLine | null {
  const line = engineLineModel(e);
  return line ? resolve(line) : null;
}

export function conversationLine(f: ConversationFrame): ActivityLine | null {
  const line = conversationLineModel(f);
  return line ? resolve(line) : null;
}

// ---------------------------------------------------------------------------
// Exhaustiveness, held here because the model cannot hold it
// ---------------------------------------------------------------------------

/**
 * Every engine payload `activityModel.mjs` has an arm for.
 *
 * This map produces no code and is read by nothing. Its only job is to fail
 * `tsc` when `EnginePayload` gains a variant the model does not name — the
 * exact check that would have caught `workstream_committed` and its five
 * siblings arriving from the engine and reaching no screen. Adding a variant
 * to `types.hand.ts` without adding it to `activityModel.mjs` is a build error,
 * which is the only kind of reminder that survives a milestone.
 */
const RENDERED_ENGINE: Record<EngineEvent["payload"]["type"], true> = {
  session: true,
  agent_streamed: true,
  addons_changed: true,
  scheduled: true,
  server_changed: true,
  browser_request: true,
  mobile_development_changed: true,
  execution_ended: true,
  session_state: true,
  session_gone: true,
  gate_opened: true,
  question_asked: true,
  guided: true,
  gate_decided: true,
  result_accepted: true,
  run_started: true,
  run_queued: true,
  run_finished: true,
  run_cancelled: true,
  step_changed: true,
  goal_closed: true,
  goal_created: true,
  workflow_proposed: true,
  workflow_changed: true,
  workflow_deleted: true,
  workflow_archived: true,
  note_changed: true,
  drawing_changed: true,
  drawing_request: true,
  attachment_changed: true,
  goal_archived: true,
  goal_deleted: true,
  project_archived: true,
  project_deleted: true,
  document_added: true,
  settings_changed: true,
  file_changed: true,
  lsp: true,
  project_created: true,
  committer_needed: true,
  committer_set: true,
  git_setup_changed: true,
  connectors_changed: true,
  project_changed: true,
  workstream_edited: true,
  agent_thinking: true,
  agent_replied: true,
  workstream_opened: true,
  workstream_changed: true,
  workstream_committed: true,
  workstream_script_ran: true,
  workstream_publish_failed: true,
  guard_decided: true,
  judged: true,
  redacted: true,
  content_screened: true,
  mcp_probed: true,
  model_switched: true,
  signal_received: true,
  listener_fired: true,
  listener_failed: true,
  listening_changed: true,
  boundary_fired: true,
  paused: true,
  resumed: true,
  conversation_created: true,
  conversation_changed: true,
  changes_moved: true,
  changes_settled: true,
  ask_opened: true,
  ask_settled: true,
  people_changed: true,
  invite_changed: true,
  message_held: true,
  message_released: true,
  hosted_changed: true,
  relays_changed: true,
};

/**
 * The same guard for journal payloads, which the Pulse now ships verbatim.
 *
 * `JournalPayload` is *generated* rather than hand-written, so this one is
 * stronger than the engine's: a variant added in `bisa-core` reaches
 * `types.gen.ts` on the next `just gen-types` and breaks this line before
 * anybody has to notice a row missing from the activity.
 */
const RENDERED_JOURNAL: Record<JournalPayload["type"], true> = {
  note: true,
  guidance: true,
  guard: true,
  judgement: true,
  step: true,
  run: true,
  attachment: true,
  document: true,
  decision: true,
  question: true,
  withdrawn: true,
  claim: true,
  progress: true,
  result: true,
  signal: true,
  turn_metrics: true,
};

// Referenced so the two maps are not dead code to a bundler that would then
// have licence to drop the check with them.
export const RENDERED = { engine: RENDERED_ENGINE, journal: RENDERED_JOURNAL } as const;
