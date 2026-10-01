/**
 * An agent step's effort pin (03 §Steps, 06 §Effort): which harnesses the
 * step may run on — so its picker offers what they take — and the sentence
 * under the pin. A step's `effort` is a pin as its `model` is: set, it
 * decides before the model's own, the agent's and the setting; absent, the
 * agent that runs the step decides. Pure, so `node --test` reads it.
 */

import { t } from "../../../i18n/l10n.mjs";
import { effortChoice, effortWords, effortsAcross, resolveEffort } from "../../_work/effortModel.mjs";

/**
 * The harnesses a step may run on, in its fallback order: the ones it names,
 * else the harness of the one agent it is assigned to. None when nobody can
 * say before the run — an input, a team, a person, or no assignee at all.
 * @param {{harness?: readonly string[], assignee?: object | null} | null | undefined} step
 * @param {readonly {id: string, harness: string}[] | null | undefined} agents
 * @returns {string[]}
 */
export function stepHarnesses(step, agents) {
  const named = (step?.harness ?? []).filter(Boolean);
  if (named.length > 0) return [...new Set(named)];
  const assignee = step?.assignee;
  const id = assignee && typeof assignee === "object" && "agent" in assignee ? assignee.agent : null;
  const agent = id ? (agents ?? []).find((a) => a.id === id) : undefined;
  return agent?.harness ? [agent.harness] : [];
}

/**
 * The harness field as it is typed — a fallback order, separated by commas
 * — read into the step's list: each harness once, in the order given, a
 * blank left out. Nothing typed is no list: the agent's own harness runs.
 * @param {unknown} text
 * @returns {string[]}
 */
export function harnessesFrom(text) {
  const named = String(text ?? "")
    .split(",")
    .map((h) => h.trim())
    .filter(Boolean);
  return [...new Set(named)];
}

/** The step's harnesses as the field shows them. @param {{harness?: readonly string[]} | null | undefined} step */
export function harnessText(step) {
  return (step?.harness ?? []).join(", ");
}

/**
 * What the effort picker of a step is told: the levels any of its harnesses
 * takes for its model, and whether that is **known** — every harness the
 * step may run on answered. One that did not answer is unknown, and so is a
 * step nobody can name a harness for: unknown is not none, and every level
 * may then be asked for.
 * @param {readonly (object | null | undefined)[] | null | undefined} answers one per harness, in the step's order; `null` for one that did not answer
 * @param {string | null | undefined} model the step's pinned model
 * @returns {{available: string[], known: boolean}}
 */
export function stepEfforts(answers, model) {
  const given = answers ?? [];
  const known = given.length > 0 && given.every((a) => a !== null && a !== undefined);
  return { available: known ? effortsAcross(given, model) : [], known };
}

/**
 * The answers that are this step's: the ones read for the harnesses it names
 * **now**. A read that lands after the step moved to other harnesses is the
 * last step's answer, and is nobody's.
 * @template A
 * @param {string} asked the harnesses asked about, as one key
 * @param {{asked: string, answers: A[]} | null | undefined} read what the last read came to
 * @returns {A[]}
 */
export function answersFor(asked, read) {
  return read && read.asked === asked ? read.answers : [];
}

/**
 * The sentence under the pin: where no level is listed, that; with nothing
 * pinned, who decides instead; pinned to `auto`, who names the level and
 * what runs while nobody does — the agent's own, which the step cannot know
 * before it is assigned; pinned to a level, what runs once it is fitted.
 * @param {{effort?: unknown} | null | undefined} step
 * @param {{available?: readonly string[], known?: boolean}} [facts]
 */
export function stepEffortHint(step, facts) {
  const pin = effortChoice(step?.effort);
  const words = effortWords(resolveEffort(pin, null, null, null), facts);
  if (words.runs === null) return words.hint;
  if (pin === null) return t("workflow-agent-step-model-effort-agent-decides");
  if (pin === "auto") return t("workflow-agent-step-model-effort-auto");
  return words.hint;
}
