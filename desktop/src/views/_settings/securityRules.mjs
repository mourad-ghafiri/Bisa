/**
 * The words and the shapes of Settings › Security, pure over the node's
 * `GET /security/status` and the two `security.*.rules` settings. The three
 * panels draw; this file decides what a rule looks like, whether it can be
 * saved, and what a verdict, a harness or the classifier's readiness reads as.
 *
 * A rule is stored as the node reads it (`RedactRule` / `GuardRule`), so a row
 * a person edits here is the same object the engine compiles — there is no
 * second shape to keep in step.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** The twenty `security.*` keys the three security panels edit, spelled once for their `only` lists; the two `security.collaboration.*` keys are drawn beside the classifier's by `Settings.tsx`, which spells them. */
export const KEYS = {
  redactor: {
    enabled: "security.redactor.enabled",
    rules: "security.redactor.rules",
    builtinsOff: "security.redactor.builtins_off",
    envAuto: "security.redactor.env_auto",
  },
  guard: {
    enabled: "security.guard.enabled",
    rules: "security.guard.rules",
    builtinsOff: "security.guard.builtins_off",
    terminalHooks: "security.guard.terminal_hooks",
  },
  classifier: {
    enabled: "security.classifier.enabled",
    agent: "security.classifier.agent",
    deadline: "security.classifier.deadline_secs",
    onHarmful: "security.classifier.on_harmful",
    /** Who reads the redacted call: an agent's session, a bare harness, or the Decision-Making Agent. */
    provider: "security.classifier.provider",
    /** A `harness` provider's own three fields — shown only for that provider: the harness, its model, and how hard that model works. */
    harness: "security.classifier.harness",
    model: "security.classifier.model",
    effort: "security.classifier.effort",
  },
  /** What an agent reads from outside — a page, a review — and what a harmful reading does (11 — Security). */
  content: {
    screen: "security.content.screen",
    onHarmful: "security.content.on_harmful",
  },
  /** The hosts the platform's own outbound calls — a connector step's — may and may not reach. */
  net: {
    denyHosts: "security.net.deny_hosts",
    allowHosts: "security.net.allow_hosts",
  },
};

/**
 * The scalar keys each panel's generated section shows. The classifier's
 * agent is not among them: the panel above offers a picker over the enabled
 * agents rather than a box to type an id into. The two host lists are the
 * guard's second section — what the platform itself may reach.
 */
export const SCALARS = {
  redactor: [KEYS.redactor.enabled, KEYS.redactor.envAuto],
  guard: [KEYS.guard.enabled, KEYS.guard.terminalHooks],
  net: [KEYS.net.denyHosts, KEYS.net.allowHosts],
  classifier: [KEYS.classifier.enabled, KEYS.classifier.deadline, KEYS.classifier.onHarmful],
};

/**
 * The banner a panel draws when its feature is switched off on this node:
 * the rules below are listed, and idle.
 * @param {"redactor" | "guard"} feature
 */
export function offWords(feature) {
  if (feature === "redactor") {
    return {
      tone: "warn",
      text: tr("settings-security-rules-off-nothing-redacted-node-every-rule"),
    };
  }
  return {
    tone: "warn",
    text: tr("settings-security-rules-off-tool-call-judged-node-every"),
  };
}

/**
 * How many of this node's own environment variables are armed as detectors —
 * a count; the names sit in the built-ins list, a value is never shown.
 * @param {{ env_auto: boolean, env_detectors: number } | null | undefined} status
 */
export function envDetectorWords(status) {
  if (!status) return { tone: "quiet", text: tr("settings-security-rules-reading-policy") };
  if (!status.env_auto) return { tone: "quiet", text: tr("settings-security-rules-node-s-environment-read-only-rules") };
  const n = status.env_detectors;
  if (n === 0) return { tone: "quiet", text: tr("settings-security-rules-variable-node-s-environment-has-name") };
  return { tone: "ok", text: tr("settings-security-rules-value-values-node-s-environment-recognised", { n }) };
}

export const ACTIONS = ["allow", "deny", "ask", "classify"];
export const MATCHERS = ["command", "path", "tool", "any"];

/** A blank redaction rule, for *Add a rule*. */
export function blankRedactRule() {
  return { id: "", label: "", enabled: true, detector: { kind: "pattern", regex: "" }, origin: "user" };
}

/** A blank guard rule, for *Add a rule*. */
export function blankGuardRule() {
  return { id: "", label: "", enabled: true, action: "deny", matcher: { kind: "command", regex: "" }, origin: "user" };
}

const ID_SHAPE = /^[a-z][a-z0-9_-]{0,63}$/;
const ENV_SHAPE = /^[A-Z_][A-Z0-9_]*$/;

/** A rule id from a label: `My key` → `my_key`. */
export function slugOf(label) {
  return String(label ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 64);
}

/**
 * What stops a redaction rule being saved, or `null`. The node's `problems`
 * are the truth about a pattern; this is the first look a person gets before
 * the round trip.
 * @param {{ id: string, label: string, detector: { kind: string, regex?: string, name?: string } }} rule
 * @param {readonly string[]} [taken] ids already in the list
 */
export function redactRuleProblem(rule, taken = []) {
  const base = ruleIdProblem(rule, taken);
  if (base) return base;
  const d = rule.detector ?? {};
  if (d.kind === "env_value") {
    if (!ENV_SHAPE.test(String(d.name ?? ""))) return tr("settings-security-rules-environment-variable-spelled-capitals-digits-underscores");
    return null;
  }
  return patternProblem(d.regex);
}

/**
 * What stops a guard rule being saved, or `null`.
 * @param {{ id: string, label: string, action: string, matcher: { kind: string, regex?: string, glob?: string, name?: string } }} rule
 * @param {readonly string[]} [taken]
 */
export function guardRuleProblem(rule, taken = []) {
  const base = ruleIdProblem(rule, taken);
  if (base) return base;
  if (!ACTIONS.includes(rule.action)) return tr("settings-security-rules-pick-what-rule-does");
  const m = rule.matcher ?? {};
  switch (m.kind) {
    case "command":
      return patternProblem(m.regex);
    case "path":
      return String(m.glob ?? "").trim() ? null : tr("settings-security-rules-path-rule-needs-glob");
    case "tool":
      return String(m.name ?? "").trim() ? null : tr("settings-security-rules-tool-rule-needs-tool-s-name");
    case "any":
      return null;
    default:
      return tr("settings-security-rules-pick-what-rule-looks");
  }
}

function ruleIdProblem(rule, taken) {
  if (!String(rule.label ?? "").trim()) return tr("settings-security-rules-give-rule-name");
  if (!ID_SHAPE.test(String(rule.id ?? ""))) return "the id is lowercase letters, digits, _ and -, starting with a letter";
  if (taken.filter((t) => t === rule.id).length > 1) return tr("settings-security-rules-another-rule-already-called", { rule: rule.id });
  return null;
}

function patternProblem(regex) {
  const text = String(regex ?? "");
  if (!text.trim()) return tr("settings-security-rules-pattern-needed");
  try {
    // A first look only: the node compiles with Rust's regex crate, whose
    // dialect is close to this one. Its `problems` list is the last word.
    new RegExp(text);
    return null;
  } catch (e) {
    return tr("settings-security-rules-not-a-pattern", { error: e instanceof Error ? e.message : String(e) });
  }
}

/**
 * The unsaved edits a rule editor holds, **one list a scope** — a rule list is
 * written whole, to the scope it was read from, so a draft is of its scope
 * and of no other: switching the scope shows that scope's rules (its own
 * draft, if one is open), and Save writes the draft on screen to the scope
 * on screen. Without it, a list edited at the workspace and saved after a
 * switch to *this machine* would replace the machine's rules with the
 * workspace's.
 * @template T
 * @param {Readonly<Record<string, T[]>>} drafts the open drafts, by scope
 * @param {string} scope the scope on screen
 * @param {T[]} stored what the scope's layer holds
 * @returns {{rules: T[], dirty: boolean}}
 */
export function draftOf(drafts, scope, stored) {
  const open = drafts[scope];
  return open ? { rules: open, dirty: true } : { rules: stored, dirty: false };
}

/** The drafts with `scope`'s set to `rules`. */
export function withDraft(drafts, scope, rules) {
  return { ...drafts, [scope]: rules };
}

/** The drafts without `scope`'s — it was saved, or discarded. The same object when it had none. */
export function withoutDraft(drafts, scope) {
  if (!(scope in drafts)) return drafts;
  const { [scope]: _gone, ...rest } = drafts;
  return rest;
}

/** Move the rule at `i` one place up or down; the same list when it cannot. */
export function moveRule(list, i, dir) {
  const j = i + dir;
  if (i < 0 || i >= list.length || j < 0 || j >= list.length) return list;
  const out = list.slice();
  [out[i], out[j]] = [out[j], out[i]];
  return out;
}

/** Toggle a built-in's id in the `builtins_off` list. */
export function toggleBuiltin(off, id, enabled) {
  const set = new Set(off);
  if (enabled) set.delete(id);
  else set.add(id);
  return [...set];
}

/** The verdict's words for a chip. */
export function verdictWords(verdict) {
  switch (verdict) {
    case "deny":
    case "denied":
      return { tone: "danger", text: "refused" };
    case "ask":
    case "asked":
      return { tone: "warn", text: tr("settings-security-rules-asks") };
    case "classify":
      return { tone: "warn", text: tr("settings-security-rules-classifier-reads") };
    case "allow":
    case "allowed":
      return { tone: "ok", text: "allowed" };
    default:
      return { tone: "quiet", text: tr("settings-security-rules-rule-applies") };
  }
}

/** The reason the node records when a person's earlier answer decided a call. */
export const REMEMBERED_REASON = tr("settings-security-rules-remembered-from-earlier-answer-goal");

/**
 * Who decided, for a decision row: the rule by id, the classifier, or you —
 * and *you · remembered* when an earlier answer stood in for a new question.
 * @param {{ by: string, rule?: string | null, reason?: string | null }} d
 */
export function judgeWords(d) {
  if (d.by === "classifier") return "classifier";
  if (d.by === "person") return d.reason === REMEMBERED_REASON ? tr("settings-security-rules-remembered") : "you";
  return d.rule ?? "rule";
}

/** What a built-in or user rule's action means, in a phrase. */
export function actionWords(action) {
  switch (action) {
    case "allow":
      return tr("settings-security-rules-runs-without-asking");
    case "deny":
      return tr("settings-security-rules-refused-agent-hears-why");
    case "ask":
      return tr("settings-security-rules-put-inbox");
    case "classify":
      return tr("settings-security-rules-classifier-reads-first");
    default:
      return String(action ?? "");
  }
}

/** One phrase for what a rule looks at. */
export function matcherWords(matcher) {
  switch (matcher?.kind) {
    case "command":
      return tr("settings-security-rules-commands-matching", { regex: matcher.regex });
    case "path":
      return tr("settings-security-rules-paths-matching", { glob: matcher.glob });
    case "tool":
      return tr("settings-security-rules-tool", { matcher: matcher.name });
    case "any":
      return tr("settings-security-panels-every-call");
    default:
      return "";
  }
}

/** One phrase for how a redaction rule recognises a secret. */
export function detectorWords(detector) {
  switch (detector?.kind) {
    case "pattern":
      return tr("settings-security-rules-pattern", { regex: detector.regex });
    case "env_value":
      return tr("settings-security-rules-value", { detector: detector.name });
    default:
      return "";
  }
}

/**
 * How a harness stands with the guard.
 * @param {{ tool_guard: boolean, input_rewrite: boolean }} h
 */
export function harnessGuardWords(h) {
  if (h.tool_guard && h.input_rewrite) return { tone: "ok", text: tr("settings-security-rules-judged-before-runs-restores-placeholders") };
  if (h.tool_guard) return { tone: "ok", text: tr("settings-security-rules-judged-before-runs") };
  return { tone: "quiet", text: tr("settings-security-rules-observed-only-runs-under-own-sandbox") };
}

/**
 * The classifier's readiness line.
 * @param {{ classifier: { enabled: boolean, agent: string, deadline_secs: number }, classifier_ready: boolean, classifier_note?: string | null } | null | undefined} status
 */
export function readinessLine(status) {
  if (!status) return { tone: "quiet", text: tr("settings-security-rules-reading-policy") };
  const c = status.classifier;
  if (!c.enabled) return { tone: "quiet", text: tr("settings-security-rules-off-classify-rule-asks-instead") };
  if (status.classifier_ready) return { tone: "ok", text: tr("settings-security-rules-answers-within-s-ready", { agent: c.agent, deadline_secs: c.deadline_secs }) };
  return { tone: "warn", text: tr("settings-security-rules-classify-rule-asks-instead", { agent: c.agent, classifier_note: status.classifier_note ?? tr("settings-security-rules-ready") }) };
}

/**
 * Which of the classifier's registry keys a provider shows, beside the
 * provider picker itself: a `harness` provider names its own harness, its
 * model and that model's effort — a level, never `auto`; an `agent`
 * provider is a picker over the enabled agents, drawn by hand rather than a
 * registry row; a `decision_making_agent` provider takes no field here at
 * all — it points at Settings › Decision Making instead.
 * @param {string} provider
 * @returns {string[]}
 */
export function classifierFieldsFor(provider) {
  return provider === "harness" ? [KEYS.classifier.harness, KEYS.classifier.model, KEYS.classifier.effort] : [];
}

/** The list of problems the node reported, one line each. */
export function problemLines(problems, feature) {
  return (problems ?? []).filter((p) => p.feature === feature).map((p) => `${p.rule}: ${p.reason}`);
}
