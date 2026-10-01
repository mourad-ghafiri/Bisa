/**
 * What a person can say back to an agent, with no React in it.
 *
 * An `AskKind` is a closed set — `{ kind: "decision" }` or `{ kind: "answer",
 * options?, multi? }` — and an `Answer` is one value that can carry all three
 * things a person routinely says at once: the options they picked, the
 * sentence the options did not cover, and that they do not know.
 *
 * # Why this is a module and not `useState` in a card
 *
 * Four callers read the same protocol and used to each guess at it: the Inbox
 * card, the goal's gate bar, the Inbox row summary and the Pulse's question
 * line. When `expects` went from the string `"text"` to `{ kind: "answer" }`,
 * two of the four kept comparing against the old string — and neither is a
 * build error, so the row silently read `undefined gate` and the Pulse line
 * silently took the wrong branch. One module means the next shape change
 * breaks one place loudly instead of four places quietly.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `activityModel.mjs`: there is
 * no jsdom in this repo, so everything a form can get *wrong* — what body a
 * selection sends, when Send is live, what "I'm not sure" actually says —
 * lives here where `node --test` can reach it. The `.tsx` is paint.
 *
 * # "I'm not sure" is an answer, never a decline
 *
 * `approve: false` reaches the waiting agent as *"The human DENIED. Do not
 * proceed with the questioned action."* For a long time the only escape from
 * a question was a link that sent exactly that, so the one door out of a
 * badly-framed question told the agent to stop. Every body built here for a
 * question is `approve: true`; not knowing is spelled `unsure`, which asks
 * for a narrower question instead of ending the work.
 */

import { t as tr } from "./i18n/l10n.mjs";

/** Trim, and treat an all-whitespace string as absent. */
function text(value) {
  const t = typeof value === "string" ? value.trim() : "";
  return t.length > 0 ? t : null;
}

/**
 * Is this a question a person answers, rather than a gate they decide?
 *
 * Anything that is not explicitly `answer` is a decision. That is the
 * conservative half: rendering Approve/Decline for a real question is a
 * visible mistake a person can refuse to make, while rendering an answer box
 * for a contract gate would quietly resolve a signature with prose.
 */
export function isAnswerAsk(expects) {
  return expects?.kind === "answer";
}

/**
 * The options a question offers, in the asker's own order.
 *
 * The order is *not* re-sorted to float the recommendation to the top. An
 * asker's ordering carries meaning it never states — cheapest first, safest
 * first, the order the work would happen in — and a list that reorders itself
 * around a flag destroys that to save one line of reading. The recommendation
 * is marked where it stands instead.
 *
 * Two things are normalised away, because both make an answer unresolvable
 * rather than merely ugly: an option with no id (nothing to send back) and a
 * second option carrying `recommended` (the schema says at most one, and two
 * stars is an asker with no opinion, dressed as advice).
 */
export function askOptions(expects) {
  if (!isAnswerAsk(expects) || !Array.isArray(expects.options)) return [];
  const seen = new Set();
  const out = [];
  let recommended = false;
  for (const o of expects.options) {
    const id = text(o?.id);
    if (!id || seen.has(id)) continue;
    seen.add(id);
    const first = o?.recommended === true && !recommended;
    if (first) recommended = true;
    out.push({
      id,
      label: text(o?.label) ?? id,
      detail: text(o?.detail),
      recommended: first,
    });
  }
  return out;
}

/**
 * May more than one option be chosen?
 *
 * `multi` on a question that offered no options is meaningless, so it answers
 * false — a form that read it anyway would draw checkboxes over nothing.
 */
export function isMulti(expects) {
  return isAnswerAsk(expects) && expects.multi === true && askOptions(expects).length > 0;
}

/**
 * Turn one option on or off.
 *
 * Single-choice replaces rather than refusing, the same rule the agent picker
 * holds: a control that makes you unpick before you can pick reads as broken.
 * Unpicking the one you are on is still allowed, because the free-text box and
 * "I'm not sure" are both answers that no option describes.
 */
export function toggleChoice(selected, id, multi) {
  const current = Array.isArray(selected) ? selected : [];
  if (current.includes(id)) return current.filter((k) => k !== id);
  return multi ? [...current, id] : [id];
}

/**
 * Drop what the question does not offer, and cap a single-choice list at one.
 *
 * The engine matches an answer by id and never by label, so an id the
 * question never issued resolves to nothing at the other end — it is not a
 * harmless extra, it is a choice that silently did not count.
 */
export function keepOffered(expects, selected) {
  const offered = new Set(askOptions(expects).map((o) => o.id));
  const kept = (Array.isArray(selected) ? selected : []).filter((id) => offered.has(id));
  return isMulti(expects) ? kept : kept.slice(0, 1);
}

/**
 * Is there enough here to send?
 *
 * Either a chosen option or a typed sentence will do, and neither is required
 * of the other: a question with options still gets a text box, because the
 * options an agent thought of are not the answers that exist.
 *
 * A decision needs nothing — its rationale is optional in both directions,
 * and a gate that would not approve without one would be asking for an essay
 * to say yes.
 */
export function canSubmit(action, form) {
  if (!isAnswerAsk(action?.expects)) return true;
  return keepOffered(action.expects, form?.selected).length > 0 || text(form?.text) !== null;
}

/**
 * What one submission says, or `null` when there is nothing to say.
 *
 * `null` is only ever returned for a question with neither a choice nor a
 * sentence — which is what keeps Send and the body in agreement, instead of a
 * disabled button and a builder that would each decide it separately.
 */
export function decideBody(action, form) {
  const gate = action?.gate_id ?? undefined;
  // A step-scoped ask names its step: several may wait at once, and a held
  // `wait` step is released only by name while an adoption or an amendment
  // is still owed a decision — an unnamed approval goes to that instead.
  const step = typeof action?.step === "string" && action.step ? action.step : undefined;
  const typed = text(form?.text);

  if (!isAnswerAsk(action?.expects)) {
    // A gate is genuinely binary and gains no third door. The free text here
    // is a rationale, which is a reason for a verdict rather than an answer
    // to a question, and the two must not be sent under one name. `inputs`
    // ride along for one gate — adopting a workflow starts its run with
    // them — and only on an approval: a decline starts nothing.
    const approve = form?.approve !== false;
    const inputs = approve && form?.inputs && Object.keys(form.inputs).length > 0 ? form.inputs : null;
    return {
      approve,
      ...(gate ? { gate } : {}),
      ...(step ? { step } : {}),
      ...(typed ? { rationale: typed } : {}),
      ...(inputs ? { inputs } : {}),
    };
  }

  const unsure = form?.unsure === true;
  // Saying "I'm not sure" while an option is ticked is two answers at once.
  // The ticks lose, because the sentence the person just pressed is the newer
  // statement — and a recorded choice they meant to abandon is the one thing
  // the agent must not act on.
  const selected = unsure ? [] : keepOffered(action.expects, form?.selected);
  if (!unsure && selected.length === 0 && !typed) return null;

  return {
    approve: true,
    ...(gate ? { gate } : {}),
    ...(step ? { step } : {}),
    answer: { selected, ...(typed ? { text: typed } : {}), unsure },
  };
}

/**
 * What a recorded answer reads as, in one phrase, for an activity line.
 *
 * Option *ids* rather than labels, because the journal keeps what was chosen
 * and not what it was called — the point of separating them is that rewording
 * a question afterwards cannot change what a recorded answer meant.
 */
export function answerSummary(answer) {
  if (!answer || typeof answer !== "object") return null;
  const picked = (Array.isArray(answer.selected) ? answer.selected : []).filter(Boolean);
  const said = text(answer.text);
  const parts = [];
  if (answer.unsure === true) parts.push(tr("app-ask-not-sure"));
  if (picked.length > 0) parts.push(tr("app-ask-chose", { picked: picked.join(", ") }));
  if (said) parts.push(`“${said}”`);
  return parts.length > 0 ? parts.join(" — ") : null;
}
