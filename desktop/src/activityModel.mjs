/**
 * What an activity line *says* — the whole of it, with no React and no icon
 * components in it.
 *
 * Plain `.mjs` with a `.d.mts` beside it, for the same reason
 * `views/_studio/inboxModel.mjs` is: `node --test` imports the real module
 * rather than a transcription of it, and there is no jsdom here. This file
 * was previously `activity.ts` in its entirety and had no test at all — which is
 * how six `EnginePayload` variants went a milestone without a `switch` arm
 * while the CLI rendered every one of them.
 *
 * An `icon` here is a **name**, not a component: `icon:<key>`,
 * `gate:<GateKind>`, `work:<WorkItemState>`, `status:<GoalStatus>`,
 * `step:<StepState>`. `activity.ts`
 * resolves it against the same maps the Goals screen and the Catalog read, and a
 * name with no entry resolves to nothing. That is deliberate in both halves:
 * **a line with no obvious glyph carries none**, because an approximate
 * symbol teaches the reader an association the rest of the app contradicts.
 * Two kinds of line are blank for different reasons — a note, because
 * `ui/icons` has no entry for the concept; a posted message, because it
 * already carries its author's face, which says more than a speech bubble
 * would (and a speech bubble reads as "direct message" everywhere else).
 *
 * Tone drives salience, not decoration: `spine` is the story of the goal
 * (runs, steps, gates, results), `dim` is the ambient work behind it, `fail`
 * rises, `wait` is anything owed to a human. Four values, and only four —
 * the Pulse used to add a fifth for conversation rows, which is a *category*
 * rather than a level, so the same posted message rendered dim while live and
 * by its category after a reload.
 *
 * A line may also carry `detail`: the fields the one-line summary had to drop.
 * Nothing is truncated *away* here — a verification's evidence, a decision's
 * rationale, a result's output all ride along so the row can be opened
 * instead of only glanced at.
 */

import { describe as describeState, label as stateLabel } from "./ui/sessionState.mjs";
import { titleOf } from "./shell/sessionOriginModel.mjs";
import { bornWords } from "./views/_work/projectOriginModel.mjs";
import { formatBytes } from "./views/_studio/attachmentModel.mjs";
import { answerSummary, askOptions, isAnswerAsk } from "./askModel.mjs";
import { t } from "./i18n/l10n.mjs";

const MAX = 96;

/**
 * A step's fact, as one line and a tone. Shared by the journal arm and the
 * live `step_changed` arm so a stored row and a live one say the same thing.
 */
function stepWords(step, state, extra = {}) {
  const id = String(step);
  switch (state) {
    case "started":
    case "running":
      return {
        tone: "dim",
        icon: "step:running",
        text: extra.work_item ? t("app-activity-step-started-item", { id, work_item: shortId(extra.work_item) }) : t("app-activity-step-started", { id }),
      };
    case "waiting":
      return { tone: "wait", icon: "step:waiting", text: t("app-activity-step-waiting", { id }) };
    case "done": {
      // A gateway names the branches it chose — one, or every one that held.
      const branches = Array.isArray(extra.branches) ? extra.branches : [];
      return {
        tone: "spine",
        icon: "step:done",
        text: branches.length > 0 ? t("app-activity-step-done", { id, branch: branches.join(", ") }) : t("app-activity-step-done-2", { id }),
      };
    }
    // Stopped by a boundary event that diverts: the run took its path.
    case "diverted":
      return {
        tone: "spine",
        icon: "step:diverted",
        text: extra.by ? t("app-activity-step-diverted", { step: id, boundary: String(extra.by) }) : t("app-activity-step-diverted-2", { id }),
      };
    // A boundary event acted beside the live step — a post, a signal — and the step went on.
    case "boundary":
      return { tone: "dim", text: t("app-activity-boundary-acted", { step: id, boundary: String(extra.boundary ?? "") }) };
    case "failed":
      return {
        tone: "fail",
        icon: "step:failed",
        text: extra.error ? t("app-activity-step-failed-error", { id, error: truncate(extra.error, 64) }) : t("app-activity-step-failed", { id }),
      };
    case "skipped":
      return { tone: "dim", icon: "step:skipped", text: t("app-activity-step-skipped", { id }) };
    case "cancelled":
      return { tone: "dim", icon: "step:cancelled", text: t("app-activity-step-cancelled", { id }) };
    case "pending":
      return { tone: "dim", icon: "step:pending", text: t("app-activity-step-pending-again", { id }) };
    default:
      return { tone: "dim", text: t("app-activity-step-in-state", { id, state: String(state).replace(/_/g, " ") }) };
  }
}

export function truncate(s, max = MAX) {
  const flat = String(s).replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

/** The word for why a run was cancelled: its cause's tag, `cancelled` when unknown. */
function cancelCauseWord(cause) {
  const c = cause?.cause;
  return c === "stopped" || c === "restarted" || c === "withdrawn" || c === "closed" ? c : "cancelled";
}

/** `next` for the first place in line, else `2nd`, `3rd`, `4th`… */
function ordinalWord(position) {
  const n = Number(position) || 1;
  // The ordinal's ending is the language's rule (CLDR's ordinal categories), chosen in the message.
  return n === 1 ? t("app-activity-next-in-line") : t("app-activity-ordinal", { n });
}

export function shortId(id) {
  return id ? String(id).slice(-6) : "";
}

/**
 * Who listens, from a listener's or a host's wire word — `goal:<goal>…` a
 * goal, anything else (`workspace:<workflow>…`) a library workflow. The
 * word is otherwise opaque: the line says which kind of thing listens, and
 * the detail carries the word whole.
 * @param {unknown} word
 * @returns {"goal" | "workflow"}
 */
export function hostOf(word) {
  return String(word ?? "").startsWith("goal:") ? "goal" : "workflow";
}

export function cents(v) {
  return v >= 100 ? `$${(v / 100).toFixed(2)}` : `${v}¢`;
}

/** One detail field, dropped only if it is genuinely absent. */
function field(label, value) {
  if (value === undefined || value === null) return null;
  const text = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  return text.trim() === "" ? null : { label, value: text };
}

function details(...rows) {
  const kept = rows.filter(Boolean);
  return kept.length > 0 ? kept : undefined;
}

// ---------------------------------------------------------------------------
// Journal payloads (the durable record, and what the Pulse now ships)
// ---------------------------------------------------------------------------

/**
 * A journal payload → the words, the tone, the glyph and the detail.
 *
 * Split from the envelope because two callers need exactly this and nothing
 * around it: the goal timeline, which has the author and the goal
 * already, and the Pulse, whose rows carry the payload verbatim from
 * `dto::PulseEvent`. One function, so a stored row and a live one cannot
 * describe the same fact differently.
 */
/**
 * The Workflow Agent's standing on a guided goal, in words — one sentence per
 * status, shared by the live event and the journal fact so the activity and the
 * pulse never disagree. The tone says whether it is news (`wait`: the person
 * is needed), trouble (`fail`), or the platform at work (`spine`).
 */
export function guidedWords(phase, status, detail) {
  const job = phase === "repair" ? t("app-activity-repairing-workflow") : t("app-activity-designing-workflow");
  const why = detail ? ` — ${truncate(String(detail), 72)}` : "";
  switch (status) {
    case "scheduled":
      return { tone: "dim", icon: "icon:coreAgent", text: t("app-activity-workflow-agent-about-start", { job }) };
    case "working":
      return { tone: "spine", icon: "icon:coreAgent", text: t("app-activity-workflow-agent", { job, why }) };
    case "asking":
      return { tone: "wait", icon: "icon:question", text: t("app-activity-workflow-agent-asked", { why }) };
    case "proposed":
      return { tone: "wait", icon: "icon:workflow", text: t("app-activity-workflow-agent-proposed-adopt", { why }) };
    case "stalled":
      return { tone: "fail", icon: "icon:warn", text: t("app-activity-workflow-agent-stopped", { job, why }) };
    case "failed":
      return { tone: "fail", icon: "icon:danger", text: t("app-activity-workflow-agent-could-not-start", { job, why }) };
    case "off":
      return { tone: "dim", icon: "icon:coreAgent", text: t("app-activity-designing-off-node-nobody-designs-workflow") };
    default:
      return { tone: "dim", icon: "icon:coreAgent", text: t("app-activity-workflow-agent-2", { status: String(status), why }) };
  }
}

/**
 * The guard's line, shared by the journal fact and the live event so the
 * pulse and the activity agree. `verdict` is `allowed` · `denied` · `asked`;
 * `by` is `rule` · `classifier` · `person`.
 */
function guardWords(tool, verdict, by, rule, reason, subject) {
  const who = by === "classifier" ? t("app-activity-classifier") : by === "person" ? t("app-activity-you") : rule ? t("app-activity-rule-named", { rule }) : t("app-activity-a-rule");
  const call = subject ? ` — ${truncate(String(subject), 60)}` : "";
  const why = reason ? `: ${truncate(String(reason), 80)}` : "";
  switch (verdict) {
    case "denied":
      return { tone: "fail", icon: "icon:guard", text: t("app-activity-refused", { tool, who, call, why }) };
    case "asked":
      return { tone: "wait", icon: "icon:guard", text: t("app-activity-needs-answer", { tool, who, call, why }) };
    default:
      return { tone: "dim", icon: "icon:guard", text: t("app-activity-allowed", { tool, who, call }) };
  }
}

/**
 * One answer of a judgement, as a short clause — the chosen branch or level
 * with how sure it was, or the probability for a noul. `null` when there is
 * nothing to show (no answer at all, as on a `failed` judgement).
 */
function judgementAnswerWords(answers) {
  const list = Object.values(answers ?? {});
  if (list.length === 0) return null;
  return list
    .map((a) => {
      if (a.type === "choice") return `${a.choice} (${Number(a.confidence).toFixed(2)})`;
      if (a.type === "score") return `${a.score} (${Number(a.confidence).toFixed(2)})`;
      if (a.type === "noul") return `${Number(a.noul) >= 0.5 ? "yes" : "no"} (${(Math.abs(Number(a.noul) - 0.5) * 2).toFixed(2)})`;
      return null;
    })
    .filter(Boolean)
    .join(", ");
}

/**
 * The Decision-Making Agent's line, shared by the journal fact and the live event so
 * the pulse and the activity agree: which point, what it answered (when it
 * did), and how the outcome stands — applied, unsure with the reason the
 * engine already composed (how sure it was against the threshold this point
 * acts from), or failed with the reason.
 */
function judgedWords(judgement) {
  const point = String(judgement.point);
  const chosen = judgementAnswerWords(judgement.answers);
  switch (judgement.outcome) {
    case "applied":
      return {
        tone: "spine",
        icon: "icon:decisions",
        text: t("app-activity-decision-making-agent-applied", { point, chosen, flag: (chosen) ? "yes" : "no" }),
      };
    case "unsure":
      return {
        tone: "wait",
        icon: "icon:decisions",
        text: t("app-activity-decision-making-agent-unsure-unsure", { point, reason: judgement.reason, flag: (judgement.reason) ? "yes" : "no" }),
      };
    default:
      return {
        tone: "fail",
        icon: "icon:decisions",
        text: t("app-activity-decision-making-agent-failed-failed", { point, reason: judgement.reason, flag: (judgement.reason) ? "yes" : "no" }),
      };
  }
}

export function payloadLine(p) {
  switch (p.type) {
    case "note":
      return {
        tone: "dim",
        text: t("app-activity-noted", { text: truncate(String(p.text)) }),
        detail: details(field(t("app-activity-note"), String(p.text))),
      };

    // Where the Workflow Agent stands on a guided goal. The same words the
    // live `guided` event renders with, so the pulse and the activity agree.
    case "guidance":
      return {
        ...guidedWords(p.phase, p.status, p.detail),
        detail: details(
          field(t("app-activity-phase"), p.phase),
          field(t("app-activity-status"), p.status),
          field(t("app-activity-detail"), p.detail),
          field(t("app-activity-session"), p.session),
        ),
      };

    // The guard's judgement on a tool call. A refusal is the line a person
    // came to read; an allow is ambient; a question is a wait. The subject
    // is already redacted — the journal never carried the secret.
    case "guard":
      return {
        ...guardWords(p.tool, p.verdict, p.by, p.rule, p.reason, p.subject),
        detail: details(
          field(t("app-activity-tool"), p.tool),
          field(t("app-activity-call-2"), p.subject),
          field(t("app-activity-verdict"), p.verdict),
          field(t("app-activity-decided"), p.by),
          field(t("app-activity-rule"), p.rule),
          field(t("app-activity-reason"), p.reason),
        ),
      };

    // `expects` decides the verb. Without it a question that wants a yes/no
    // and a question that wants a paragraph read identically, and the reader
    // cannot tell which one is blocking a gate they could clear in a click.
    //
    // It is an `AskKind` object, not the string it used to be. `String(p.expects)`
    // on one is `[object Object]` and `=== "decision"` on one is never true, and
    // neither is a build error — which is how this line quietly took the wrong
    // branch for every question in the activity.
    case "question": {
      const offered = askOptions(p.expects);
      return {
        tone: "wait",
        icon: "icon:question",
        text: isAnswerAsk(p.expects)
          ? t("app-activity-asked-quoted", { p: truncate(String(p.text)) })
          : t("app-activity-asked-decision", { p: truncate(String(p.text)) }),
        detail: details(
          field(t("app-activity-question"), String(p.text)),
          field(t("app-activity-expects"), p.expects?.kind),
          // The options are half of what was asked. A detail panel that showed
          // the prose and dropped them describes a question nobody was asked.
          field(t("app-activity-options"), offered.length > 0 ? offered.map((o) => o.label).join(" · ") : null),
          field(t("app-activity-gate"), p.gate),
          field(t("app-activity-work-item"), p.work_item),
        ),
      };
    }

    // A question taken back — by its asker, or by a restart that found the
    // asker dead. The subject says which door it was (`workstream:` a
    // publish, `guard:` an escalation, `permission:` a tool), the reason why.
    case "withdrawn":
      return {
        tone: "dim",
        icon: "icon:question",
        text: t("app-activity-withdrew-question", { reason: truncate(String(p.reason)) }),
        detail: details(field(t("app-activity-subject"), p.subject), field(t("app-activity-reason"), p.reason)),
      };

    // One step of a run moved. `event` is the fact — `{ fact: "done",
    // branches }`, `{ fact: "diverted", by }`, `{ fact: "boundary", boundary }`
    // — and its tag says which way; the answer and the error ride along as
    // detail because they are what a reader opens the row for.
    case "step": {
      const f = p.event ?? {};
      const fact = String(f.fact);
      const words =
        fact === "answered"
          ? {
              tone: "spine",
              icon: "step:done",
              text: t("app-activity-answered-step", { step: String(p.step), answer: truncate(answerSummary(f.answer) ?? "", 64) }),
            }
          : fact === "decided"
            ? {
                tone: "spine",
                icon: "gate:approval",
                text: t("app-activity-approved-declined-step", { flag: (f.approve) ? "yes" : "no", step: String(p.step) }),
              }
            : stepWords(p.step, fact, { work_item: f.work_item, branches: f.branches, by: f.by, boundary: f.boundary, error: f.error });
      return {
        ...words,
        detail: details(
          field(t("app-activity-run"), p.run),
          field(t("app-activity-step"), p.step),
          field(t("app-activity-fact"), fact),
          field(t("app-activity-work-item"), f.work_item),
          field(t("app-activity-branch"), Array.isArray(f.branches) && f.branches.length > 0 ? f.branches.join(", ") : null),
          field(t("app-activity-boundary"), f.by ?? f.boundary),
          field(t("app-activity-error"), f.error),
          field(t("app-activity-answer"), answerSummary(f.answer)),
        ),
      };
    }

    // The run itself: queued, started on a workflow, amended, finished,
    // cancelled — a cancel says its cause and is never a failure.
    case "run": {
      const f = p.event ?? {};
      const fact = String(f.fact);
      const run = shortId(String(p.run));
      const line =
        fact === "queued"
          ? { tone: "dim", icon: "icon:queued", text: t("app-activity-run-queued-workflow-rev", { run, workflow: shortId(String(f.workflow)), revision: f.revision }) }
          : fact === "started"
            ? { tone: "spine", icon: "icon:run", text: t("app-activity-run-started-workflow-rev", { run, workflow: shortId(String(f.workflow)), revision: f.revision }) }
            : fact === "amended"
              ? { tone: "spine", icon: "icon:workflow", text: t("app-activity-run-amended-revision", { run, revision: f.revision }) }
              : fact === "finished"
                ? f.outcome === "failed"
                  ? { tone: "fail", icon: "status:failed", text: t("app-activity-run-failed", { run }) }
                  : { tone: "spine", icon: "status:done", text: t("app-activity-run-finished-done", { run }) }
                : { tone: "dim", icon: "status:closed", text: t("app-activity-run-cancelled", { run, cause: cancelCauseWord(f.cause) }) };
      return {
        ...line,
        detail: details(
          field(t("app-activity-run"), p.run),
          field(t("app-activity-fact"), fact),
          field(t("app-activity-workflow"), f.workflow),
          // The start it began at, and the signal of the event that began it.
          field(t("app-activity-step"), f.start),
          field(t("app-activity-signal"), f.signal),
          field(t("app-activity-cause"), f.cause?.cause),
          field(t("app-activity-rationale"), f.cause?.rationale),
          field(t("app-activity-reason"), f.cause?.reason?.reason),
          field(t("app-activity-revision"), f.revision === undefined ? null : String(f.revision)),
          field(t("app-activity-outcome"), f.outcome),
          field(t("app-activity-reason"), f.reason?.rationale ?? f.reason?.reason),
        ),
      };
    }

    // The attachment record itself — a project joining or leaving the goal.
    case "attachment":
      return {
        tone: "spine",
        icon: "icon:project",
        text: t("app-activity-project-attached-detached", { project: shortId(String(p.project)), flag: p.attached ? "yes" : "no" }),
        detail: details(field(t("app-activity-project"), p.project)),
      };

    // A file the person gave the goal as context, now under its documents
    // folder. Spine, because whoever works on the goal is told about it.
    case "document":
      return {
        tone: "spine",
        icon: "icon:document",
        text: t("app-activity-gave-document", { file: p.file.name }),
        detail: details(field(t("app-activity-name"), p.file.name), field(t("app-activity-type"), p.file.mime), field(t("app-activity-size"), formatBytes(p.file.size)), field(t("app-activity-hash"), p.file.sha256)),
      };

    // The Decision-Making Agent's own answer, standing beside the journal's
    // `decision` (a person's signed approval) and `guard` (a rule's or the
    // classifier's) — a judgement never signs a gate.
    case "judgement":
      return {
        ...judgedWords(p.judgement),
        detail: details(
          field(t("app-activity-point"), p.judgement.point),
          field(t("app-activity-provider"), p.judgement.provider),
          field(t("app-activity-model"), p.judgement.model),
          field(t("app-activity-calibrated"), p.judgement.calibrated),
          field(t("app-activity-outcome"), p.judgement.outcome),
          field(t("app-activity-reason"), p.judgement.reason),
          field(t("app-activity-run"), p.run),
          field(t("app-activity-step"), p.step),
        ),
      };

    case "decision": {
      const verb = p.approve ? "approved" : "declined";
      // `answer` is an `Answer` — chosen ids, what was typed, whether they
      // said they were not sure — and interpolating one directly reads as
      // `[object Object]` in the one place the record is meant to be legible.
      const said = answerSummary(p.answer);
      return {
        tone: "spine",
        icon: `gate:${String(p.gate)}`,
        text: t("app-activity-gate-gate", { verb, gate: String(p.gate), said: truncate(said, 64), flag: (said) ? "yes" : "no" }),
        detail: details(
          field(t("app-activity-subject"), p.subject),
          field(t("app-activity-rationale"), p.rationale),
          field(t("app-activity-answer"), said),
        ),
      };
    }

    case "claim":
      return {
        tone: "dim",
        icon: "work:claimed",
        text: t("app-activity-claimed-item-via", { work_item: shortId(String(p.work_item)), harness: String(p.harness) }),
        detail: details(
          field(t("app-activity-work-item"), p.work_item),
          field(t("app-activity-harness"), p.harness),
          // The session id is the handle on the transcript. Dropping it made
          // "which run did this?" unanswerable from the activity.
          field(t("app-activity-session"), p.session),
        ),
      };

    case "progress": {
      const outcome = p.outcome ? ` → ${truncate(String(p.outcome), 48)}` : "";
      return {
        tone: "dim",
        icon: "work:in_progress",
        text: `${String(p.verb)} ${truncate(String(p.object), 56)}${outcome}`,
        // The triple as three fields, not one flattened string: `verb`,
        // `object` and `outcome` are the activity grammar, and a row that
        // cannot show them apart cannot be read as one.
        detail: details(
          field(t("app-activity-verb"), p.verb),
          field(t("app-activity-object"), p.object),
          field(t("app-activity-outcome"), p.outcome),
          field(t("app-activity-work-item"), p.work_item),
        ),
      };
    }

    case "result": {
      const artifacts = Array.isArray(p.artifacts) ? p.artifacts : [];
      return {
        tone: "spine",
        icon: "work:review",
        text:
          artifacts.length > 0
            ? t("app-activity-submitted-result-artifact-artifacts", { work_item: shortId(String(p.work_item)), artifacts: artifacts.length })
            : t("app-activity-submitted-result", { work_item: shortId(String(p.work_item)) }),
        detail: details(
          field(t("app-activity-work-item"), p.work_item),
          field(t("app-activity-output"), p.output),
          artifacts.length > 0 ? field(t("app-activity-artifacts"), artifacts.join("\n")) : null,
        ),
      };
    }

    // The event that started a run, recorded on the run's home: a named
    // signal says its name, any other event its source — a schedule, a hook,
    // a message, a project's change.
    case "signal":
      return {
        tone: "dim",
        icon: "icon:signal",
        text: p.name
          ? t("app-activity-signal-started-this-run", { name: String(p.name) })
          : t("app-activity-event-started-this-run", { source: String(p.source) }),
        detail: details(
          field(t("app-activity-source"), p.source),
          field(t("app-activity-name"), p.name),
          field(t("app-activity-listener"), p.listener),
          field(t("app-activity-signal"), p.signal),
          field(t("app-activity-payload"), p.payload),
        ),
      };

    case "turn_metrics":
      return {
        tone: "dim",
        icon: "icon:cost",
        text: t("app-activity-tokens", { output_tokens: Number(p.input_tokens) + Number(p.output_tokens), usd_cents: cents(Number(p.usd_cents)) }),
        detail: details(
          field(t("app-activity-input-tokens"), String(p.input_tokens)),
          field(t("app-activity-output-tokens"), String(p.output_tokens)),
          field(t("app-activity-cost"), cents(Number(p.usd_cents))),
          field(t("app-activity-session"), p.session),
        ),
      };

    // A payload this build has never heard of — a newer node writing into a
    // shared workspace. Say its name rather than nothing: an unnamed row is
    // indistinguishable from a bug in this file.
    default:
      return { tone: "dim", text: String(p.type).replace(/_/g, " ") };
  }
}

/**
 * A journal entry (payload plus envelope) → one activity line. The entry is
 * filed at a home — a goal, or a run of the workspace — and the line names
 * the goal only when there is one.
 */
export function journalLine(e, index) {
  const home = e.home;
  const id = home?.home === "run" ? home.run : home?.goal;
  return {
    ...payloadLine(e.payload),
    key: `j:${id}:${e.at}:${index}`,
    at: e.at,
    author: e.author,
    goal: home?.home === "goal" ? home.goal : undefined,
  };
}

// ---------------------------------------------------------------------------
// Pulse rows (the same facts, read back from the node)
// ---------------------------------------------------------------------------

/**
 * A `PulseRow` → one activity line.
 *
 * The row carries the event, not a sentence, so this is the *same*
 * `payloadLine` the live timeline uses. That equality is the whole reason the
 * route stopped rendering prose: the two used to be different functions in
 * different languages, and the same event changed appearance on reload.
 */
export function pulseLine(row) {
  const e = row.event ?? {};
  const base = {
    key: `p:${row.seq}`,
    at: row.at,
    author: row.author ?? undefined,
    goal: row.source?.kind === "goal" ? row.source.id : undefined,
    title: row.title ?? undefined,
    concept: row.concept,
    source: row.source,
  };
  if (e.type === "message") {
    const membership = e.body_kind === "membership";
    return { ...base, tone: "dim", text: truncate(String(e.snippet ?? "")) || (membership ? t("app-activity-changed-who-here") : t("app-activity-posted-message")) };
  }
  if (JOURNAL_TAGS.has(e.type)) return { ...base, ...payloadLine(e) };
  // A note written or a drawing changed has no live line — its overlay is
  // where it shows (`engineLine`) — but the Pulse's Workspace tab lists it
  // (`pulseModel`): said as what changed and what it is about, never the
  // fact's own name.
  if (e.type === "note_changed" || e.type === "drawing_changed") {
    const what = e.type === "note_changed" ? "note" : "drawing";
    return { ...base, tone: "dim", icon: what === "note" ? "icon:note" : "icon:draw", text: t("app-activity-record-changed", { what, scope: String(e.scope ?? "workspace") }) };
  }
  // An engine fact, stored as its frame was: the live words, so a row reads
  // the same after a reload as it did the moment it happened.
  const live = engineWords(e, row.at);
  // Under its subject's heading — the agent's name, the goal's title — a row
  // does not say the subject again by its id: the live line needs the id,
  // having no heading; the Pulse has one (`row.title`).
  if (live) return { ...base, tone: live.tone, text: (row.title && titledWords(e)) || live.text, icon: live.icon, detail: live.detail };
  return { ...base, tone: "dim", text: String(e.type ?? "unknown").replace(/_/g, " ") };
}

/** A fact's words under a heading that already names its subject; `null` for a fact whose live words never name it. */
function titledWords(e) {
  switch (e.type) {
    case "goal_created":
      return t("app-activity-goal-created-titled", { origin: String(e.origin?.origin ?? "captured"), run: shortId(e.origin?.run) });
    case "agent_replied":
      return e.posted ? t("app-activity-replied-titled") : t("app-activity-acted-without-replying-titled");
    default:
      return null;
  }
}

/**
 * The live words for a stored engine fact — and never a throw. The feed is
 * written by every version of the code that ever ran here; a payload whose
 * shape this build did not expect still gets its type as a dim line, so one
 * stored row cannot take the whole page down. `engineLine` guards every
 * field it reads; this is the belt for the one it does not.
 */
function engineWords(e, at) {
  try {
    return engineLine({ payload: e }, at);
  } catch {
    return null;
  }
}

/** The journal's tags — what `payloadLine` words; everything else stored is an engine fact. */
const JOURNAL_TAGS = new Set(["note", "guidance", "guard", "judgement", "decision", "question", "withdrawn", "claim", "progress", "result", "step", "run", "attachment", "document", "signal", "turn_metrics"]);

// ---------------------------------------------------------------------------
// Live engine events
// ---------------------------------------------------------------------------

let liveSeq = 0;

export function sessionLine(e) {
  if (!e || e.tier === "raw") return null;
  if (e.tier === "lifecycle") {
    const ev = e.event ?? {};
    switch (ev.type) {
      case "started":
        return { text: t("app-activity-session-started"), tone: "dim", icon: "icon:working" };
      case "ended": {
        if (!ev.is_terminal) return null; // a turn boundary, not the end
        const o = ev.outcome ?? {};
        if (o.outcome === "completed")
          return { text: t("app-activity-session-completed"), tone: "dim", icon: "icon:ok" };
        if (o.outcome === "aborted")
          return { text: t("app-activity-session-aborted"), tone: "fail", icon: "icon:danger" };
        if (o.outcome === "failed")
          return {
            text: t("app-activity-session-failed-error", { error: truncate(o.error, 64) }),
            tone: "fail",
            icon: "icon:danger",
          };
        if (o.outcome === "model_unavailable")
          return {
            text: t("app-activity-model-went-away", { model: o.model, reason: truncate(o.reason, 56) }),
            tone: "wait",
            icon: "icon:warn",
          };
        return {
          text: t("app-activity-session-suspended", { reason: truncate(o.reason, 64) }),
          tone: "fail",
          icon: "icon:warn",
        };
      }
      case "parked":
        return { text: t("app-activity-session-parked"), tone: "dim" };
      case "revived":
        return { text: t("app-activity-session-revived"), tone: "dim" };
      // `permission`, not `tool`: the sentence is about work that has stopped
      // until somebody answers, which is a different fact from work happening.
      case "input_requested":
        return { text: t("app-activity-asked", { request: inputSummary(ev.request) }), tone: "wait", icon: "icon:permission" };
      case "input_resolved":
        return { text: t("app-activity-answered-going"), tone: "dim", icon: "icon:ok" };
      default:
        return null;
    }
  }
  return progressLine(e.event, null);
}

/** One short clause for an input request: *run Bash*, the question, *sign in to X*. */
function inputSummary(request) {
  if (!request) return t("app-activity-answer-verb");
  if (request.kind === "permission") return t("app-activity-run-tool", { tool: request.tool_name });
  if (request.kind === "question") return t("app-activity-answer-question", { text: truncate(request.text, 56) });
  if (request.kind === "auth") return t("app-activity-sign", { provider: request.provider });
  return t("app-activity-answer-verb");
}

/**
 * A progress event as a line; `parent` names the sub-agent it happened in,
 * which prefixes the line so a sub-agent's tools never read as the agent's.
 */
function progressLine(ev, parent) {
  const pre = parent ? "↳ " : "";
  switch (ev.type) {
    case "subagent_started":
      return { text: t("app-activity-sub-agent", { ev: ev.name, description: truncate(ev.description, 48) }).trim(), tone: "dim", icon: "icon:agent" };
    case "subagent_ended":
      return ev.ok ? null : { text: t("app-activity-sub-agent-failed"), tone: "fail", icon: "icon:agent" };
    case "nested":
      return progressLine(ev.event, ev.parent);
    case "tool_started":
      return {
        text: `${pre}${ev.name} ${truncate(ev.args_summary, 56)}`.trim(),
        tone: "dim",
        icon: "icon:tool",
      };
    // A failed tool keeps the tool's glyph and lets the tone carry the
    // failure: swapping in the danger symbol would lose which *kind* of thing
    // failed, and the row is already red.
    case "tool_ended":
      return ev.ok ? null : { text: t("app-activity-failed", { pre, ev: ev.name }), tone: "fail", icon: "icon:tool" };
    case "cost_delta":
      return {
        text: t("app-activity-tokens-2", { pre, output_tokens: ev.input_tokens + ev.output_tokens, usd_cents: cents(ev.usd_cents) }),
        tone: "dim",
        icon: "icon:cost",
      };
    // Turn boundaries and streaming text are noise in a stream of actions.
    default:
      return null;
  }
}

/** The words for each part of the git setup a `git_setup_changed` names. */
const GIT_SETUP_WORDS = Object.freeze({ profiles: "profiles", keys: t("app-activity-ssh-keys"), accounts: t("app-activity-code-host-accounts"), identity: t("app-activity-global-git-identity") });

export function engineLine(e, at = Math.floor(Date.now() / 1000)) {
  const base = { at, goal: e.goal ?? undefined };
  const key = `e:${at}:${liveSeq++}`;
  const p = e.payload;

  switch (p.type) {
    case "session": {
      const line = sessionLine(p.event);
      return line ? { ...base, key, ...line } : null;
    }
    case "scheduled":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:harness",
        text: t("app-activity-scheduled-onto", { harness: p.harness }),
      };
    case "execution_ended": {
      const o = p.outcome ?? {};
      const done = o.outcome === "completed";
      const why = o.outcome === "failed" || o.outcome === "cancelled" ? `: ${truncate(o.reason, 56)}` : "";
      return {
        ...base,
        key,
        tone: done ? "dim" : "fail",
        icon: done ? "icon:ok" : "icon:danger",
        text: t("app-activity-execution-ended", { outcome: o.outcome, why }),
      };
    }
    // Presence: a line when a session waits, fails, is done or is parked;
    // the streaming states are the roster's, not the activity's.
    case "session_state": {
      const pr = p.presence ?? {};
      const who = titleOf(pr);
      const d = describeState(pr.state);
      const word = pr.state?.state;
      if (word === "waiting" || word === "failed" || word === "aborted" || word === "done" || word === "parked") {
        return {
          ...base,
          key,
          tone: d.tone === "accent" ? "wait" : d.tone === "danger" ? "fail" : "dim",
          icon: d.icon,
          text: t("app-activity-session-said", { who, state: stateLabel(pr.state) }),
        };
      }
      return null;
    }
    case "session_gone":
      return { ...base, key, tone: "dim", icon: "icon:agent", text: t("app-activity-finished-session-left-roster") };
    case "gate_opened":
      return {
        ...base,
        key,
        tone: "wait",
        icon: `gate:${p.gate}`,
        text: t("app-activity-waiting-on-you-question", { question: truncate(p.question) }),
      };
    case "question_asked":
      return {
        ...base,
        key,
        tone: "wait",
        icon: "icon:question",
        text: t("app-activity-asked-you", { text: truncate(p.text) }),
      };
    // Where the Workflow Agent stands on a guided goal — the same sentence
    // the journal fact renders with.
    case "guided":
      return {
        ...base,
        key,
        ...guidedWords(p.phase, p.status, p.detail),
      };
    case "gate_decided":
      return {
        ...base,
        key,
        tone: "spine",
        icon: `gate:${p.gate}`,
        text: t("app-activity-approved-declined-gate", { flag: (p.approve) ? "yes" : "no", gate: p.gate }),
      };
    case "result_accepted":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "work:accepted",
        text: t("app-activity-result-accepted"),
      };
    case "run_started":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "icon:run",
        text: t("app-activity-run-started-workflow", { run: shortId(p.run), workflow: shortId(p.workflow) }),
      };
    case "run_queued":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:queued",
        text: t("app-activity-run-queued-line-workflow", { run: shortId(p.run), position: ordinalWord(p.position), workflow: shortId(p.workflow) }),
      };
    case "run_finished":
      return {
        ...base,
        key,
        tone: p.outcome === "failed" ? "fail" : "spine",
        icon: p.outcome === "failed" ? "status:failed" : "status:done",
        text: p.outcome === "failed" ? t("app-activity-run-failed", { run: shortId(p.run) }) : t("app-activity-run-finished-done", { run: shortId(p.run) }),
      };
    // A person's own act, or the goal's close: dim, never a failure.
    case "run_cancelled":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "status:closed",
        text: t("app-activity-run-cancelled", { run: shortId(p.run), cause: cancelCauseWord(p.cause) }),
        detail: details(field("run", p.run), field("cause", p.cause?.cause), field("rationale", p.cause?.rationale)),
      };
    case "step_changed":
      return { ...base, key, ...stepWords(p.step, p.state) };
    // Captured by a person or an agent, spawned from a parent goal, or made
    // by a run's step — the three ways a goal comes to exist.
    case "goal_created":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "icon:goal",
        text: t("app-activity-goal-created", { goal: shortId(p.goal), origin: String(p.origin?.origin ?? "captured"), run: shortId(p.origin?.run) }),
        detail: details(field("goal", p.goal), field("origin", p.origin?.origin)),
      };
    case "goal_closed":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "status:closed",
        text: t("app-activity-goal-closed", { closed: String(p.reason?.reason ?? "closed") }),
      };
    // A proposal is owed a decision, so it is `wait`, and it names the
    // designer rather than a generic gate.
    case "workflow_proposed":
      return {
        ...base,
        key,
        tone: "wait",
        icon: "icon:workflow",
        text: t("app-activity-workflow-agent-proposed-workflow-rev-adopt", { workflow: shortId(p.workflow), revision: p.revision }),
      };
    // The Workflow Agent's hand is news (an Inbox notice on the workflow's
    // row); a save by hand is the dim line it always was.
    case "workflow_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:workflow",
        text: p.designed
          ? t("app-activity-workflow-designed-revision", { workflow: shortId(p.workflow), revision: p.revision })
          : t("app-activity-workflow-saved-revision", { workflow: shortId(p.workflow), revision: p.revision }),
      };
    // Out of the library, or put away and taken back out. Spine: the
    // library and every picker move on it.
    case "workflow_deleted":
      return { ...base, key, tone: "spine", icon: "icon:workflow", text: t("app-activity-workflow-deleted", { workflow: shortId(p.workflow) }) };
    case "workflow_archived":
      return { ...base, key, tone: "spine", icon: "icon:workflow", text: t("app-activity-workflow-archived", { workflow: shortId(p.workflow), flag: p.archived ? "yes" : "no" }) };
    // One record added or removed; nothing on disk moved. Spine, because the
    // goal's agents now see (or stop seeing) a project.
    case "attachment_changed":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "icon:project",
        text: t("app-activity-project-attached-detached", { project: shortId(p.project), flag: p.attached ? "yes" : "no" }),
      };
    // Put away or taken back out — the record leaves or rejoins the lists;
    // deleted — it is gone from this node. Spine: every list moves on it.
    case "goal_archived":
      return { ...base, key, tone: "spine", icon: "icon:goal", text: t("app-activity-goal-archived", { goal: shortId(p.goal), flag: p.archived ? "yes" : "no" }) };
    case "goal_deleted":
      return { ...base, key, tone: "spine", icon: "icon:goal", text: t("app-activity-goal-deleted", { goal: shortId(p.goal) }) };
    case "project_archived":
      return { ...base, key, tone: "spine", icon: "icon:project", text: t("app-activity-project-archived", { project: shortId(p.project), flag: p.archived ? "yes" : "no" }) };
    case "project_deleted":
      return { ...base, key, tone: "spine", icon: "icon:project", text: t("app-activity-project-deleted", { project: shortId(p.project) }) };
    // A file given to the goal as context. Spine: the goal's agents are told
    // about it, and the Documents card on the goal page moves.
    case "document_added":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "icon:document",
        text: t("app-activity-gave-document", { file: p.file?.name ?? "?" }),
        detail: details(field(t("app-activity-name"), p.file?.name), field(t("app-activity-type"), p.file?.mime), field(t("app-activity-size"), formatBytes(p.file?.size ?? 0))),
      };
    // Ambient: a preference moved. Named so a person can see what an agent or
    // the CLI changed, dim because nothing is waiting on it.
    // A file moved under an open root. Ambient: the explorer and an open
    // buffer act on it; a person reading the activity sees what changed where.
    case "lsp":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:file",
        text:
          p.method === "bisa/serverFailed"
            ? t("app-activity-language-server-failed", { language: p.language })
            : p.method === "bisa/serverStarted"
              ? t("app-activity-language-server-started", { language: p.language })
              : p.method === "bisa/serverStopped"
                ? t("app-activity-language-server-stopped", { language: p.language })
                : t("app-activity-language-server-method", { language: p.language, method: p.method }),
      };
    case "file_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:file",
        text:
          p.kind === "rescan"
            ? t("app-activity-files-under-need-rescan", { scope: p.scope, p: shortId(p.id) })
            : t("app-activity-file-changed-from", { path: p.path, kind: p.kind, from: p.from, flag: p.from ? "yes" : "no" }),
      };
    case "settings_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:settings",
        text: t("app-activity-settings-changed-at", { scope: p.scope, keys: p.keys.join(", ") }),
      };
    // A project is a folder on disk this workspace now owns, so it reads as
    // spine rather than ambient noise. It is also the line that says the
    // Projects screen has something new to show: before this variant existed
    // a project made by an agent or another surface appeared only on a
    // remount, and the `KIND_PROJECT` snapshot that does ride the conversation
    // channel is no answer — a project is not a conversation.
    case "project_created": {
      // Where it was born is the interesting half of the sentence: a project
      // an agent step made mid-run is news of a different kind than one a
      // person clicked into being.
      const born = bornWords(p.origin);
      return {
        ...base,
        key,
        tone: "spine",
        icon: "icon:project",
        text: t("app-activity-project-created", { slug: p.slug, born }),
      };
    }
    // A repository nobody can commit in is a question for a person, whoever
    // made the project — so it reads as a warning, and says why it is asked.
    case "committer_needed": {
      const why =
        p.reason === "settlement_refused"
          ? t("app-activity-agent-s-work-kept-uncommitted")
          : p.reason === "commit_refused"
            ? t("app-activity-commit-refused")
            : t("app-activity-just-created");
      return {
        ...base,
        key,
        tone: "warn",
        icon: "icon:project",
        text: t("app-activity-project-needs-someone-commit", { slug: p.slug, why }),
      };
    }
    // A person's own git setup moved — a profile, a key, an account, their
    // global identity — never a value: the Settings panels, About › Settings
    // and the workbench header's committer chip re-read on it.
    case "git_setup_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:settings",
        text: t("app-activity-git-setup-changed", { what: GIT_SETUP_WORDS[p.what] ?? p.what }),
      };
    // A connector definition or one of this machine's accounts changed —
    // never a value: Settings › Connectors and the designer's pickers re-read.
    // An addon moved — installed, removed, switched, re-granted: Settings ›
    // Addons and the addon layer re-read; the line names which.
    case "addons_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:settings",
        text: t("app-activity-addon-changed", { id: p.id, what: p.what }),
      };
    case "connectors_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:settings",
        text: p.what === "accounts" ? t("app-activity-connector-accounts-changed") : t("app-activity-connector-definitions-changed"),
      };
    case "conversation_created":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:dm",
        text: t("app-activity-conversation-started-about", { kind: p.origin?.kind ?? "thing" }),
      };
    case "conversation_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:dm",
        text: t("app-activity-conversation-change", { change: String(p.change ?? "changed") }),
      };
    // The change ledger of a conversation about a checkout (09-agents-in-the-ide):
    // a re-read signal, and the settle itself — an activity fact, Projects'.
    case "changes_moved":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:file",
        text: t("app-activity-file-files-review", { pending: p.pending }),
      };
    case "changes_settled":
      return {
        ...base,
        key,
        tone: p.act === "keep" ? "spine" : "dim",
        icon: "icon:file",
        text: t("app-activity-files-kept-undone-restored", { act: p.act, files: p.files, skipped: p.skipped?.length ?? 0 }),
      };
    case "ask_opened":
      return {
        ...base,
        key,
        tone: "wait",
        icon: "icon:guard",
        text: t("app-activity-waits-conversation", { tool: p.ask?.tool ?? t("app-activity-call") }),
      };
    case "ask_settled":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:guard",
        text: p.allowed ? t("app-activity-ask-allowed") : t("app-activity-ask-denied"),
      };
    case "committer_set":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:project",
        text: t("app-activity-commits", { identity: p.identity.name, email: p.identity.email }),
      };
    case "project_changed":
      return { ...base, key, tone: "dim", icon: "icon:project", text: t("app-activity-project-edited") };
    case "workstream_edited":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:workstream",
        text: t("app-activity-workstream-edited"),
      };
    // Named, and deliberately renders nothing — the same answer `session`
    // gives a turn boundary. A note is somebody's private scratchpad: it does
    // not sync, nothing depends on it, and it is not workspace activity. The
    // notes overlay is where a note changing is visible, and it is on screen
    // when it matters. Keeping the arm rather than deleting it is what keeps
    // `RENDERED_ENGINE`'s exhaustiveness check honest: the model knows this
    // variant exists and has decided about it.
    case "note_changed":
      return null;
    // A drawing changing is the Draw overlay's to show, as a note's is the
    // notes overlay's — and unlike a note it is announced for its own editor's
    // sake (the hash), not as workspace activity.
    case "drawing_changed":
      return null;
    // An event durably queued for the start event that heard it — or, heard
    // by none, kept for the waits that may hear it later.
    case "signal_received":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:signal",
        text: p.listener
          ? t("app-activity-event-queued", { source: String(p.source), host: hostOf(p.listener) })
          : t("app-activity-event-recorded", { source: String(p.source) }),
        detail: details(field(t("app-activity-source"), p.source), field(t("app-activity-listener"), p.listener), field(t("app-activity-signal"), p.signal)),
      };
    // What a start event's occurrence did: a run on the goal that listens, a
    // run of the workspace — which belongs to no goal — or, refused by the
    // start's guard, nothing, and why.
    case "listener_fired": {
      const o = p.outcome ?? {};
      const started = o.outcome === "started";
      return {
        ...base,
        key,
        tone: started ? "spine" : "dim",
        icon: "icon:signal",
        text: started
          ? o.goal
            ? t("app-activity-event-started-run-goal", { run: shortId(o.run), goal: shortId(o.goal) })
            : t("app-activity-event-started-run-workspace", { run: shortId(o.run) })
          : t("app-activity-event-started-no-run", { reason: truncate(String(o.reason ?? o.outcome ?? ""), 72) }),
        detail: details(field(t("app-activity-listener"), p.listener), field(t("app-activity-signal"), p.signal), field(t("app-activity-run"), o.run), field(t("app-activity-reason"), o.reason)),
      };
    }
    // A start event that could not start its run, or could not be armed —
    // the line nobody was looking at, so it rises.
    case "listener_failed":
      return {
        ...base,
        key,
        tone: "fail",
        icon: "icon:signal",
        text: t("app-activity-start-event-could-not-start-run", { host: hostOf(p.listener), error: truncate(String(p.error ?? ""), 72) }),
        detail: details(field(t("app-activity-listener"), p.listener), field(t("app-activity-signal"), p.signal), field(t("app-activity-error"), p.error)),
      };
    // A library workflow turned On or Off, a goal armed or cleared: ambient,
    // the person's own act or the goal's.
    case "listening_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:signal",
        text: t("app-activity-listening-changed", { host: hostOf(p.host), flag: p.on ? "yes" : "no" }),
        detail: details(field(t("app-activity-host"), p.host)),
      };
    // An event on a live step. A divert stopped the step and took the
    // boundary's path — the run's story; an act beside it (a post, a signal)
    // left the step running. No glyph: a boundary is a timer, a message or a
    // signal, and a near-miss symbol would say which one wrongly.
    case "boundary_fired":
      return {
        ...base,
        key,
        tone: p.diverts ? "spine" : "dim",
        text: p.diverts
          ? t("app-activity-step-diverted", { step: String(p.step), boundary: String(p.boundary) })
          : t("app-activity-boundary-acted", { step: String(p.step), boundary: String(p.boundary) }),
        detail: details(field(t("app-activity-run"), p.run), field(t("app-activity-workflow"), p.workflow), field(t("app-activity-step"), p.step), field(t("app-activity-boundary"), p.boundary)),
      };
    // The same glyph the top chrome puts on its "paused" chip, so the activity
    // line and the status strip are visibly reporting one fact.
    case "paused":
      return { ...base, key, tone: "wait", icon: "icon:waiting", text: t("app-activity-engine-paused") };
    case "resumed":
      return { ...base, key, tone: "dim", icon: "icon:working", text: t("app-activity-engine-resumed") };
    // People on other nodes (14-collaboration): who joined, left or moved,
    // an invitation's state, a message the classifier held or the owner let
    // through, a hosted workspace that moved, the relays.
    case "people_changed": {
      const who = p.label || shortId(p.pubkey);
      const c = p.change?.change;
      const text =
        c === "joined" ? t("app-activity-joined", { who, role: p.change.role }) : c === "left" ? t("app-activity-left", { who }) : c === "role_changed" ? t("app-activity-now", { who, role: p.change.role }) : t("app-activity-changed", { who });
      return { ...base, key, tone: c === "left" ? "dim" : "spine", icon: "icon:members", text };
    }
    case "invite_changed": {
      const st = p.invite?.state?.state;
      const text =
        st === "requested"
          ? t("app-activity-asked-join-waits", { by: p.invite.state.label || shortId(p.invite.state.by) })
          : st === "accepted"
            ? t("app-activity-invitation-accepted", { role: p.invite.role })
            : st === "refused"
              ? t("app-activity-invitation-refused")
              : st === "revoked"
                ? t("app-activity-invitation-withdrawn")
                : st === "expired"
                  ? t("app-activity-invitation-expired")
                  : t("app-activity-invitation-made", { role: p.invite?.role ?? "guest" });
      return { ...base, key, tone: st === "requested" ? "wait" : "dim", icon: "icon:members", text };
    }
    case "message_held":
      return {
        ...base,
        key,
        tone: p.reason?.reason === "pending" ? "dim" : "fail",
        icon: "icon:guard",
        text: t("app-activity-message-from-being-read-held", { author: shortId(p.author), scope: p.scope, why: p.reason?.why ?? t("app-activity-no-verdict"), flag: (p.reason?.reason === "pending") ? "yes" : "no" }),
      };
    case "message_released":
      return { ...base, key, tone: "dim", icon: "icon:guard", text: t("app-activity-held-message-let-through", { scope: p.scope }) };
    case "content_screened":
      return {
        ...base,
        key,
        tone: p.verdict === "withheld" ? "fail" : "dim",
        icon: "icon:guard",
        text:
          p.verdict === "withheld"
            ? t("app-activity-content-from-withheld-from", { source: p.source, agent: p.agent })
            : p.verdict === "allowed"
              ? t("app-activity-read-content-from-allowed-after-screen", { agent: p.agent, source: p.source })
              : p.verdict === "unscreened"
                ? t("app-activity-read-content-from-not-screened", { agent: p.agent, source: p.source })
                : t("app-activity-read-content-from-screened-safe", { agent: p.agent, source: p.source }),
      };
    case "hosted_changed":
      return { ...base, key, tone: "dim", icon: "icon:members", text: p.scope ? t("app-activity-something-moved", { scope: p.scope, host: shortId(p.host) }) : t("app-activity-membership-moved", { host: shortId(p.host) }) };
    case "relays_changed":
      return { ...base, key, tone: "dim", icon: "icon:sync", text: t("app-activity-relays-changed") };

    // These six were emitted by the engine and rendered by the CLI's activity view
    // (`crates/bisa-cli/src/activity.rs`) while this switch had no arm for
    // them — not because anyone decided they were noise, but because the
    // hand-written `EnginePayload` in `types.hand.ts` was missing the
    // variants, so the switch stayed exhaustive and the compiler stayed
    // quiet. A commit landing on a branch simply never reached Pulse.
    case "workstream_opened":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:workstream",
        text: p.branch ? t("app-activity-workstream-opened", { branch: p.branch }) : t("app-activity-workstream-opened-2"),
      };
    case "workstream_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:workstream",
        text: t("app-activity-workstream", { g: String(p.state?.state ?? "changed").replace(/_/g, " ") }),
      };
    // The commit *is* the artifact for a git project, so it carries the
    // spine tone the other two do not.
    case "workstream_committed":
      return {
        ...base,
        key,
        tone: "spine",
        icon: "mark:git",
        text: t("app-activity-committed", { commit: p.commit.slice(0, 12), branch: p.branch }),
      };
    // A script that ran is an ambient fact; one that failed is the line
    // nobody was looking at, so it carries the danger tone and its first
    // words — the reason — rather than the whole tail.
    case "server_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:page",
        text: p.workstream ? t("app-activity-served-folders-changed-workstream", { workstream: shortId(p.workstream) }) : t("app-activity-served-artifact-pages-changed"),
      };
    case "browser_request":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:page",
        text: t("app-activity-agent-asked-embedded-browser", { action: p.request.action, url: p.request.url, flag: (p.request.url) ? "yes" : "no" }),
      };
    case "drawing_request":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:draw",
        text: t("app-activity-agent-asked-canvas", { action: p.request.action }),
      };
    case "mobile_development_changed":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:device",
        text: p.what === "toolchain" ? t("app-activity-mobile-development-toolchain-examined-again") : t("app-activity-mobile-development-devices-changed"),
      };
    case "mcp_probed":
      return {
        ...base,
        key,
        tone: p.ok ? "dim" : "danger",
        icon: "icon:mcpServer",
        text: p.ok ? t("app-activity-mcp-server-answered-probe", { p: p.id }) : t("app-activity-mcp-server-failed-probe", { p: p.id }),
      };
    case "workstream_script_ran": {
      const first = String(p.output ?? "").split("\n")[0];
      return {
        ...base,
        key,
        tone: p.ok ? "dim" : "danger",
        icon: "icon:workstream",
        text: t("app-activity-script", { g: p.phase.replace(/_/g, "-"), first }),
      };
    }
    // What a person approved and did not go out: the act, and the first line
    // of why — the rest is the journal's.
    case "workstream_publish_failed":
      return {
        ...base,
        key,
        tone: "danger",
        icon: "mark:git",
        text: t("app-activity-publish-failed", { what: p.what, reason: String(p.reason ?? "").split("\n")[0] }),
      };
    case "guard_decided":
      return { ...base, key, ...guardWords(p.tool, p.verdict, p.by, p.rule, p.reason, p.subject) };
    case "judged":
      return { ...base, key, ...judgedWords(p.judgement) };
    // Secrets that never left: a count and the kinds, never a value. Ambient
    // on purpose — the line says the redactor is working, not that something
    // went wrong.
    case "redacted": {
      const n = Number(p.count) || 0;
      const kinds = Array.isArray(p.kinds) && p.kinds.length > 0 ? ` (${p.kinds.join(", ")})` : "";
      const where = p.at === "mcp_reply" ? t("app-activity-tool-s-answer") : t("app-activity-redacted-before-the", { at: String(p.at).replace(/_/g, " ") });
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:guard",
        text: t("app-activity-redacted-secret-secrets-before", { n, where, kinds }),
      };
    }
    case "agent_thinking":
      return { ...base, key, tone: "dim", icon: "icon:agent", text: t("app-activity-writing", { agent: p.agent }) };
    // Each frame of a reply as it is written: the timeline's, not a line here.
    case "agent_streamed":
      return null;
    case "agent_replied":
      return {
        ...base,
        key,
        tone: "dim",
        icon: "icon:agent",
        text: p.posted ? t("app-activity-replied", { agent: p.agent }) : t("app-activity-acted-without-replying", { agent: p.agent }),
      };
    // A failover nobody can see is indistinguishable from magic — the reader
    // of a transcript has to be able to tell which model wrote which half.
    case "model_switched":
      return {
        ...base,
        key,
        tone: "wait",
        icon: "icon:warn",
        text:
          t("app-activity-switched-from-to", { from: p.from, to: p.to, reason: p.reason }) +
          (p.after_progress ? ` ${t("app-activity-work-had-already-started")}` : ""),
      };
    default:
      return null;
  }
}

export function conversationLine(f, at = Math.floor(Date.now() / 1000)) {
  if (f.snapshot) return null;
  // No glyph on purpose. Every message line carries its author, and a face is
  // a stronger identifier than a speech bubble — which would also read as
  // "direct message", since that is what `dm` means everywhere else.
  return {
    key: `s:${f.event_id ?? `${at}:${liveSeq++}`}`,
    at,
    author: f.author,
    // The scope, which this used to drop — leaving every live message row in
    // the Pulse a button that pointed nowhere and rendered itself disabled.
    goal: f.scope,
    tone: "dim",
    text: f.snippet ? truncate(f.snippet) : t("app-activity-posted-message"),
  };
}

/** Newest last, capped — the shape every activity view renders. */
export function appendCapped(lines, next, cap = 400) {
  const out = [...lines, next];
  return out.length > cap ? out.slice(out.length - cap) : out;
}
