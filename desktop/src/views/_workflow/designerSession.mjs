/**
 * The designer's document: what is stored, what is being edited, what is on
 * its way to the node, and what happened to the last save — with no React and
 * no network in it.
 *
 * The one rule this model exists to keep: **a save never loses a keystroke and
 * undo never lies about the revision.** Bodies (`NewWorkflowBody`) live in the
 * undo history; the revision lives on `base`, the stored workflow as last
 * confirmed by the node. A save sends the body at the head of the history
 * with `base.revision`; when it lands, `base` moves to what the node stored
 * and `savedBody` to the body that was sent — the history is untouched, so an
 * edit typed while the save was in flight is still there and still unsaved.
 * Undo after a save walks bodies, not revisions, so the next save cannot
 * resend a revision the node has already moved past.
 *
 * A conflict is a choice, not a reload. When the node answers 409 — or a
 * `workflow_changed` arrives for a revision newer than `base` while there are
 * unsaved edits — the session records *theirs* and blocks saving until the
 * person picks: keep mine (rebase my body on their revision) or take theirs
 * (their body becomes the head; undo brings mine back).
 *
 * A save that failed for any other reason re-arms: the draft stays, the error
 * is beside it, and the next attempt waits an exponentially longer delay.
 * One failure does not re-arm: a body the node **could not read** (a 400
 * with no `problems`) is `rejected`, and the same body is never sent again —
 * only an edit, which makes a different body, is. Resending what was
 * refused would be refused again, forever, on a growing backoff.
 *
 * **When a save is due** is this model's too (`saveDue`): after `delay` of
 * quiet, or `MAX_QUIET_MULTIPLE × delay` after the first unsaved edit,
 * whichever comes first — so a person typing without pause still has their
 * work saved along the way. The driver (`useAutosave.ts`) only keeps the
 * clock and the wire.
 */

import { canRedo, canUndo, create, push, redo, undo } from "./history.mjs";
import { t } from "../../i18n/l10n.mjs";

/** The editable content of a stored workflow — what a body is. */
export function bodyOf(workflow) {
  return definitionBody(workflow);
}

/**
 * A definition as the wire takes it (`NewWorkflowBody`): its six keys, **by
 * name**. A body on the canvas is edited through spreads, and one read back
 * from the memory may have been written by another version
 * (`designDraftModel.draftBody` hands it over as it was kept) — and the node
 * refuses a body carrying a key the definition does not have. So what is
 * validated, saved or recorded is built here, never passed through. The
 * steps and the inputs travel as they stand: their keys are the forms' and
 * `blankStep`'s, and the core holds each kind to its own.
 * @param {{name?: string, description?: string, inputs?: object[], steps?: object[], tags?: readonly string[], decision_making?: boolean}} body
 */
export function definitionBody(body) {
  return {
    name: body.name,
    description: body.description ?? "",
    inputs: body.inputs ?? [],
    steps: body.steps ?? [],
    tags: [...(body.tags ?? [])],
    decision_making: body.decision_making ?? false,
  };
}

/** What `PUT /workflows/{id}` takes (`PutWorkflowBody`): the definition, and the revision it was edited from. */
export function putBody(body, revision) {
  return { ...definitionBody(body), revision };
}

function session(fields) {
  return {
    base: null,
    history: null,
    savedBody: null,
    inflight: null,
    status: "idle",
    failure: null,
    failures: 0,
    rejected: null,
    conflict: null,
    problems: [],
    editedAt: null,
    dirtySince: null,
    ...fields,
  };
}

/** Open a stored workflow: clean, at its revision, with the node's problems. */
export function open(workflow, problems = []) {
  const body = bodyOf(workflow);
  return session({ base: workflow, history: create(body), savedBody: body, problems });
}

/**
 * A new body at the head of the history. Allowed in every status, a
 * conflict included — the person may keep working while they decide. `at`
 * is when: the quiet the save waits for is measured from it, and the first
 * edit since the last save starts the clock the max wait is measured on.
 */
export function edit(s, body, at = Date.now()) {
  return { ...s, history: push(s.history, body), editedAt: at, dirtySince: s.dirtySince ?? at };
}

/** Undo and redo move the head like an edit does, so the save follows them. */
export function undoEdit(s, at = Date.now()) {
  return { ...s, history: undo(s.history), editedAt: at, dirtySince: s.dirtySince ?? at };
}

export function redoEdit(s, at = Date.now()) {
  return { ...s, history: redo(s.history), editedAt: at, dirtySince: s.dirtySince ?? at };
}

export const canUndoEdit = (s) => canUndo(s.history);
export const canRedoEdit = (s) => canRedo(s.history);

/** The body being edited. */
export const present = (s) => s.history.present;

/** The node's latest word on what is wrong. */
export function withProblems(s, problems) {
  return { ...s, problems };
}

/** Unsaved edits exist: the head of the history is not the body last saved. */
export function dirty(s) {
  return s.history.present !== s.savedBody;
}

/**
 * What a save should send now, or `null` when nothing should: not dirty, a
 * save already in flight, a conflict waiting on the person, or the head
 * still the body the node could not read. Every workflow the designer
 * holds is stored — a new one is created before its designer opens — so a
 * save is always an `update` at `base.revision`.
 */
export function saveRequest(s) {
  if (!dirty(s) || s.status === "saving" || s.status === "conflict") return null;
  const body = s.history.present;
  if (s.status === "rejected" && body === s.rejected) return null;
  return { kind: "update", id: s.base.id, revision: s.base.revision, body };
}

/** How many quiet delays a person may keep typing through before a save goes anyway. */
export const MAX_QUIET_MULTIPLE = 10;

/**
 * Milliseconds until the save that `saveRequest` describes should leave, at
 * `now`, or `null` when nothing should be sent. After a failure it is the
 * backoff; otherwise the sooner of `delay` since the last edit and
 * `MAX_QUIET_MULTIPLE × delay` since the first unsaved one.
 */
export function saveDue(s, now, delay) {
  if (saveRequest(s) === null) return null;
  if (s.status === "failed") return retryDelayMs(s);
  const quiet = s.editedAt === null ? 0 : Math.max(0, s.editedAt + delay - now);
  const atMost = s.dirtySince === null ? quiet : Math.max(0, s.dirtySince + MAX_QUIET_MULTIPLE * delay - now);
  return Math.min(quiet, atMost);
}

/**
 * A save left with `body`. The clock for the max wait restarts: what is
 * typed from now on is the next save's.
 */
export function saveStarted(s, body) {
  return { ...s, inflight: body, status: "saving", failure: null, rejected: null, dirtySince: null };
}

/**
 * The node stored `saved` and reported `problems`. `base` moves to it and
 * `savedBody` to the body that was sent; the history is untouched, so
 * anything typed meanwhile is still the head and still dirty.
 */
export function saveSucceeded(s, saved, problems = s.problems) {
  return {
    ...s,
    base: saved,
    savedBody: s.inflight ?? s.savedBody,
    inflight: null,
    status: "idle",
    failure: null,
    failures: 0,
    problems,
  };
}

/**
 * The save did not land and the node did not say the object moved. The
 * draft is still dirty, so the max-wait clock starts again here.
 */
export function saveFailed(s, message) {
  return { ...s, inflight: null, status: "failed", failure: message, failures: s.failures + 1, dirtySince: s.dirtySince ?? s.editedAt };
}

/**
 * The node could not read the body — a 400 that carried no problems, which
 * is the extractor speaking, not the validator. The body is remembered so
 * it is not sent again; the next edit makes a body that may be.
 */
export function saveRejected(s, message) {
  return { ...s, inflight: null, status: "rejected", failure: message, failures: 0, rejected: s.inflight, dirtySince: null };
}

/**
 * The one place a save's outcome becomes a session — shared by the driver's
 * ordinary path and by its flush on leaving, which reconstructs the session
 * after the screen has gone.
 */
export function settle(s, outcome) {
  switch (outcome.kind) {
    case "stored":
      return saveSucceeded(s, outcome.workflow, outcome.problems);
    case "conflict":
      return conflict(s, outcome.theirs);
    case "rejected":
      return saveRejected(s, outcome.message);
    case "failed": {
      const failed = saveFailed(s, outcome.message);
      return outcome.problems.length > 0 ? withProblems(failed, outcome.problems) : failed;
    }
    default:
      throw new Error(`not a save outcome: ${outcome.kind}`);
  }
}

const RETRY_BASE_MS = 1000;
export const RETRY_CAP_MS = 30_000;

/** How long to wait before trying again after `failures` failures in a row. */
export function retryDelayMs(s) {
  if (s.status !== "failed" || s.failures === 0) return 0;
  return Math.min(RETRY_CAP_MS, RETRY_BASE_MS * 2 ** (s.failures - 1));
}

/** Somebody saved `theirs` first. Saving stops until the person chooses. */
export function conflict(s, theirs) {
  return { ...s, inflight: null, status: "conflict", conflict: { theirs }, failure: null };
}

/**
 * Keep my edits on top of theirs: their revision becomes the base, their body
 * what is saved, and my head — unchanged — is dirty against it, so the next
 * save carries my work at the revision the node has.
 */
export function keepMine(s) {
  if (!s.conflict) return s;
  const theirs = s.conflict.theirs;
  return { ...s, base: theirs, savedBody: bodyOf(theirs), status: "idle", conflict: null };
}

/**
 * Take theirs: their body becomes the head **as a new edit**, so undo brings
 * my fork back rather than losing it; clean against their revision.
 */
export function takeTheirs(s) {
  if (!s.conflict) return s;
  const theirs = s.conflict.theirs;
  const body = bodyOf(theirs);
  return {
    ...s,
    base: theirs,
    history: push(s.history, body),
    savedBody: body,
    status: "idle",
    conflict: null,
  };
}

/**
 * A `workflow_changed` for this workflow arrived, at `revision`. Our own save
 * emits one too — so a revision not newer than `base` is ours (or old news),
 * and nothing arriving *while* we save can be judged yet: ignore. Anything
 * newer from a quiet session is worth a look.
 */
export function remoteDecision(s, revision) {
  if (s.status === "saving") return "ignore";
  if (s.base && revision <= s.base.revision) return "ignore";
  if (s.status === "conflict") return "ignore";
  return "reload";
}

/**
 * The stored workflow was re-read as `theirs` after a `reload` decision.
 * Clean: adopt it as a new head (undo still works). Dirty: a conflict, for
 * the person to settle. Not newer than `base` after all: nothing.
 */
export function remoteLoaded(s, theirs, problems = s.problems) {
  if (s.base && theirs.revision <= s.base.revision) return s;
  if (!dirty(s)) {
    const body = bodyOf(theirs);
    return { ...s, base: theirs, history: push(s.history, body), savedBody: body, problems };
  }
  return conflict(s, theirs);
}

/**
 * The session was opened on the answer kept from the last visit — drawn at
 * once, before the node was read — and the node's own has landed. Newer
 * than what the session stands on: a session nobody edited is opened again
 * on it, whole, so undo never walks back to the stale drawing; one edited
 * in the meantime takes it as any remote change (`remoteLoaded`). Not
 * newer: nothing.
 */
export function caughtUp(s, stored, problems = s.problems) {
  if (s.base && stored.revision <= s.base.revision) return s;
  if (dirty(s) || s.status !== "idle") return remoteLoaded(s, stored, problems);
  return open(stored, problems);
}

/**
 * Whether the row the designer read — the node's reading of the stored
 * workflow: what it starts on, what turning On asks, whether it runs in the
 * workspace, the inputs *Run…* asks — is behind what the session stands on.
 * A save of the designer's own moves the stored workflow and announces
 * nothing the designer acts on (its own `workflow_changed` is ignored), so
 * the row is read again when the session's base is a later revision of the
 * same workflow. A row that is ahead is another writer's save, which the
 * session takes by its own road (`remoteAction`).
 * @param {{workflow?: {id?: string, revision?: number}} | null | undefined} row
 * @param {{base?: {id?: string, revision?: number} | null} | null | undefined} s
 */
export function rowBehind(row, s) {
  const stored = row?.workflow;
  const base = s?.base;
  if (!stored || !base || stored.id !== base.id) return false;
  return Number(base.revision) > Number(stored.revision);
}

/** The problems an error body carries, when it carries any. */
export function problemsFromErrorBody(body) {
  return body && typeof body === "object" && Array.isArray(body.problems) ? body.problems : [];
}

/**
 * What a save the node did not store came back as. A **conflict** is one
 * thing: the node answered 409 *and* the stored copy is no longer at the
 * revision the save was sent at — somebody saved first. A 409 over a copy
 * that has not moved is the node refusing the definition for what it would
 * do — a public hook start a listening host still answers on, taken away —
 * and says so in its own sentence: read as a conflict it would offer *Keep
 * mine on top of theirs* over a copy nobody changed, and every choice would
 * be refused again. It is a failure, said beside the draft, tried again at
 * the backoff's pace and stored once its cause is gone. A 409 whose stored
 * copy could not be read is a failure too, and the next attempt reads it
 * again. A 400 with no problems is a body the node could not read
 * (`rejected`).
 * @param {{status: number | null, message: string, body?: unknown}} refusal `status` is `null` when no answer came
 * @param {number} sent the revision the save was sent at
 * @param {{revision: number} | null | undefined} theirs the stored copy read after a 409
 */
export function refusalOutcome(refusal, sent, theirs) {
  const message = refusal.message;
  if (refusal.status === 409 && theirs && Number(theirs.revision) !== Number(sent)) return { kind: "conflict", theirs };
  const problems = problemsFromErrorBody(refusal.body);
  if (refusal.status === 400 && problems.length === 0) return { kind: "rejected", message };
  return { kind: "failed", message, problems };
}

/** Whether a refused save is worth reading the stored copy for: only a 409 may be somebody else's save. */
export function asksStored(status) {
  return status === 409;
}

/** One line for the header: what the document's relation to the node is. */
export function statusLine(s) {
  switch (s.status) {
    case "saving":
      return t("workflow-designer-session-saving");
    case "failed":
      return t("workflow-designer-session-not-saved", { failure: s.failure ?? t("workflow-designer-session-unknown-error") });
    case "rejected":
      return t("workflow-designer-session-not-saved-node-could-not-read", { failure: s.failure ?? t("workflow-designer-session-unknown-error") });
    case "conflict":
      return t("workflow-designer-session-conflict-somebody-saved-first");
    default:
      return dirty(s) ? t("workflow-designer-session-unsaved-edits") : t("workflow-designer-session-saved");
  }
}

/** The engine facts that say a stored workflow moved. */
export const WORKFLOW_FACTS = Object.freeze(["workflow_changed", "workflow_archived", "workflow_deleted"]);

/**
 * What the designer does with an engine fact about workflows: `leave` when
 * the workflow it is on was deleted, `remark` when its archive mark changed
 * (the record is read again, and the library beside it), `reload` when
 * another writer saved a newer revision (`remoteDecision`), else `ignore` —
 * a fact about another workflow, our own save echoed back, a session with
 * nothing stored yet.
 * @returns {"leave" | "remark" | "reload" | "ignore"}
 */
export function remoteAction(s, payload) {
  const id = s?.base?.id;
  if (!id || !payload || payload.workflow !== id) return "ignore";
  if (payload.type === "workflow_deleted") return "leave";
  if (payload.type === "workflow_archived") return "remark";
  if (payload.type === "workflow_changed") return remoteDecision(s, payload.revision);
  return "ignore";
}

/**
 * The same fact on a goal's Workflow tab, which shows the goal's workflow
 * and may hold a drawing of it. With nothing drawn the stored copy is read
 * again. With a drawing it is **not**: the drawing was made against the
 * revision in hand, and reading a newer one under it would let the save
 * carry that revision and write over the other writer's change unseen — the
 * stale revision is what makes the node refuse it. The person is told once
 * instead (`warn`). A run's copy is frozen: nothing reaches it.
 * @param {{workflow: string | null | undefined, drawing: boolean, running: boolean}} tab
 * @returns {"reload" | "warn" | "ignore"}
 */
export function goalTabRemote(tab, payload) {
  if (!tab?.workflow || !payload || payload.workflow !== tab.workflow) return "ignore";
  if (!WORKFLOW_FACTS.includes(payload.type) || tab.running) return "ignore";
  return tab.drawing ? "warn" : "reload";
}

