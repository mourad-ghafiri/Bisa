/**
 * The inputs a run is started with, as a form: initial values from the
 * definition's defaults, validation by kind, and the request the node takes.
 *
 * Plain `.mjs` with a `.d.mts` beside it, like `askModel.mjs`: what a start
 * form can get *wrong* — a required input left blank, a number that is not
 * one, a choice not in the list — is decided here where `node --test` can
 * reach it. The dialog is paint.
 */

import { t } from "../../i18n/l10n.mjs";

/** The value each input starts with: its default, else the kind's empty. */
export function initialValues(inputs) {
  const out = {};
  for (const i of inputs ?? []) {
    if (i.default !== undefined && i.default !== null) out[i.name] = i.default;
    else out[i.name] = i.kind === "bool" ? false : "";
  }
  return out;
}

/** Trim, and treat an all-whitespace string as absent. */
const text = (v) => (typeof v === "string" ? v.trim() : v);
const blank = (v) => v === undefined || v === null || (typeof v === "string" && v.trim() === "");

/**
 * One sentence per input that is wrong, keyed by name. An empty object is a
 * form that may be sent.
 */
export function validateInputs(inputs, values) {
  const errors = {};
  for (const i of inputs ?? []) {
    const v = values?.[i.name];
    if (blank(v)) {
      if (i.required) errors[i.name] = t("workflow-workflow-form-required");
      continue;
    }
    switch (i.kind) {
      case "number":
        if (typeof v !== "number" && (typeof v !== "string" || !Number.isFinite(Number(v)) || v.trim() === "")) {
          errors[i.name] = t("workflow-workflow-form-number");
        }
        break;
      case "bool":
        if (typeof v !== "boolean") errors[i.name] = t("workflow-workflow-form-yes-no");
        break;
      case "choice":
        if (!(i.options ?? []).includes(String(v))) errors[i.name] = t("workflow-workflow-form-one-options");
        break;
      case "assignee":
        if (!/^(agent|team):[a-z0-9][a-z0-9_-]*$|^human:[0-9a-f]{64}$/.test(String(v))) {
          errors[i.name] = t("workflow-workflow-form-agent-id-team-id-human-64");
        }
        break;
      case "project":
        if (!/^[0-9A-HJKMNP-TV-Z]{26}$/.test(String(v))) errors[i.name] = t("workflow-workflow-form-project-id");
        break;
      case "account":
        if (!/^[0-9A-HJKMNP-TV-Z]{26}$/.test(String(v))) errors[i.name] = t("workflow-workflow-form-account");
        break;
      default:
        break;
    }
  }
  return errors;
}

/**
 * The line under an input's field: whether it must be given; for a project,
 * what picking one does — given to a goal's work it is **attached to the
 * goal by the start**, given to a run in the workspace it is where the run's
 * agents work and nothing is attached — and what is wrong with what was
 * typed, last.
 * @param {{kind?: string, required?: boolean}} def
 * @param {string | null | undefined} error
 * @param {"goal" | "workspace"} home whom the run is for
 */
export function inputHint(def, error, home) {
  const parts = [def?.required ? t("workflow-workflow-form-required") : t("workflow-run-workflow-dialog-optional")];
  if (def?.kind === "project") parts.push(home === "workspace" ? t("workflow-inputs-form-project-run-works-there") : t("workflow-inputs-form-project-attached-by-start"));
  if (error) parts.push(error);
  return parts.join(" ");
}

/**
 * The accounts an `account` input offers: this machine's accounts of the
 * input's connector, the connector's default first, each by its label.
 * @param {readonly {id: string, label: string, default?: boolean}[] | null | undefined} accounts
 * @returns {{id: string, label: string, isDefault: boolean}[]}
 */
export function accountChoices(accounts) {
  return [...(accounts ?? [])]
    .map((a) => ({ id: a.id, label: a.label || a.id, isDefault: a.default === true }))
    .sort((a, b) => Number(b.isDefault) - Number(a.isDefault));
}

/**
 * The body `POST /goals/{id}/run` takes: every input the definition declares,
 * typed by its kind, with blanks dropped so the node's defaults apply.
 */
export function toRequest(inputs, values) {
  const out = {};
  for (const i of inputs ?? []) {
    const v = values?.[i.name];
    if (blank(v)) continue;
    switch (i.kind) {
      case "number":
        out[i.name] = Number(v);
        break;
      case "bool":
        out[i.name] = Boolean(v);
        break;
      default:
        out[i.name] = text(v);
        break;
    }
  }
  return out;
}

/**
 * A name no input has, for a new row: `input`, `input-2`, `input-3`. Inside
 * the input-name grammar and readable in a template.
 */
export function nextInputName(inputs) {
  const taken = new Set((inputs ?? []).map((i) => i.name));
  if (!taken.has("input")) return "input";
  for (let n = 2; ; n++) {
    const name = `input-${n}`;
    if (!taken.has(name)) return name;
  }
}

/** The kinds an input may be, in the order the editor offers them. */
export const INPUT_KINDS = Object.freeze(["text", "number", "bool", "choice", "assignee", "project", "account"]);

/** An input kind in words, never its wire slug: `bool` is *Yes or no*. */
export function inputKindWords(kind) {
  switch (kind) {
    case "text":
      return t("workflow-workflow-form-kind-text");
    case "number":
      return t("workflow-workflow-form-kind-number");
    case "bool":
      return t("workflow-workflow-form-kind-bool");
    case "choice":
      return t("workflow-workflow-form-kind-choice");
    case "assignee":
      return t("workflow-workflow-form-kind-assignee");
    case "project":
      return t("workflow-workflow-form-kind-project");
    case "account":
      return t("workflow-workflow-form-kind-account");
    default:
      return String(kind ?? "");
  }
}

/** The grammar of a step's id and of an input's name: what a template can read. */
const ID_GRAMMAR = /^[a-z][a-z0-9_-]{0,31}$/;

/**
 * Why an id or a name as typed cannot be taken — `grammar` or `taken` — or
 * `null` when it may. The current value is never taken by itself.
 * @param {string} draft
 * @param {string} current
 * @param {(name: string) => boolean} taken
 */
export function idProblem(draft, current, taken) {
  if (!ID_GRAMMAR.test(draft)) return "grammar";
  if (draft !== current && taken(draft)) return "taken";
  return null;
}

/**
 * The reason, in words, a field shows under what was typed — and, once the
 * field has put the old value back (`kept`), that it did: a refused rename is
 * said where it was typed, never undone in silence.
 * @param {"grammar" | "taken" | null} problem
 * @param {"step" | "input"} what
 * @param {string | null} [kept]
 */
export function idProblemWords(problem, what, kept = null) {
  if (problem === null) return null;
  const why =
    problem === "grammar"
      ? t("workflow-step-common-form-z-0-9-starts-letter-32")
      : what === "input"
        ? t("workflow-input-defs-editor-another-input-has-name")
        : t("workflow-step-common-form-another-step-has-id");
  return kept === null ? why : t("workflow-workflow-form-kept", { why, kept });
}

/** A choice's options as the field shows them while it is not being typed in. */
export function optionsText(options) {
  return (options ?? []).join(", ");
}

/** The options a typed list means: split on commas, trimmed, blanks and repeats dropped. */
export function optionsFrom(text) {
  const out = [];
  for (const part of String(text ?? "").split(",")) {
    const o = part.trim();
    if (o && !out.includes(o)) out.push(o);
  }
  return out;
}
