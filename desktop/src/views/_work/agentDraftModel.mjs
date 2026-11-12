/**
 * What the agent editor holds and what a save of it writes (06 — Agents
 * and teams). A **draft** is the agent as it stands, its lists copied so an
 * edit never reaches the roster's row. A **save** is one of three bodies:
 *
 * - a **core agent** — the General Agent, the Workflow Agent — changes only
 *   its harness, its model plan and its decision-making switch (15 §Where
 *   it is on), so only those three are sent: the store compares field by
 *   field and refuses any other that differs **by name**, and a stale draft
 *   would then be a refusal naming a field nobody touched;
 * - an **agent of yours** is written whole, in one request: its two
 *   reference lists are ordered, and replaced wholesale with the rest;
 * - a **new agent** names what it has — no plan that names nothing, no
 *   switch that is off, no picture it lacks.
 *
 * Pure, so `node --test` reads it; `AgentEditor.tsx` draws the form.
 */

import { withEffort } from "./effortModel.mjs";
import { DEFAULT_STRATEGY } from "./modelPlanModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/** What a core agent may change, in the order the body names them — the core's `check_update`. */
export const CORE_FIELDS = Object.freeze(["harness", "models", "decision_making"]);

/** Where the draft holds each of them. */
const CORE_VALUE = Object.freeze({ harness: (d) => d.harness, models: (d) => d.plan, decision_making: (d) => d.decision_making });

/**
 * The harness a new agent starts on: the first one installed on this machine,
 * or `null` when none is — nothing is picked for the person and no harness
 * id is made up, so an agent is never born on a harness this machine cannot
 * run. The list is the node's (`GET /harnesses`), in the node's order.
 * @param {readonly {id: string, installed?: boolean}[] | null | undefined} harnesses
 * @returns {string | null}
 */
export function startingHarness(harnesses) {
  return (harnesses ?? []).find((h) => h.installed)?.id ?? null;
}

/**
 * What the harness picker offers: every harness installed here — and the
 * draft's own when it is not among them (an agent whose harness was
 * uninstalled, or is another machine's), said as *not installed here*, so
 * the picker shows what the definition holds and a Save changes nothing the
 * person did not change.
 * @param {readonly {id: string, label: string, installed?: boolean}[] | null | undefined} harnesses
 * @param {string | null | undefined} current the draft's harness
 * @returns {{id: string, label: string, installed: boolean}[]}
 */
export function harnessChoices(harnesses, current) {
  const all = harnesses ?? [];
  const out = all.filter((h) => h.installed).map((h) => ({ id: h.id, label: h.label, installed: true }));
  if (current && !out.some((h) => h.id === current)) {
    const known = all.find((h) => h.id === current);
    out.push({ id: current, label: t("work-agent-draft-harness-not-installed-here", { harness: known?.label ?? current }), installed: false });
  }
  return out;
}

/**
 * Whether the editor may save: a name, a prompt, a harness, and no write on
 * its way. A draft with no harness — a machine with none installed — is
 * never sent: the node would be asked to make an agent nothing can run.
 * @param {{name: string, system_prompt: string, harness: string | null}} d
 * @param {boolean} busy
 */
export function maySaveAgent(d, busy) {
  return !busy && d.name.trim() !== "" && d.system_prompt.trim() !== "" && typeof d.harness === "string" && d.harness !== "";
}

/** A blank draft on `harness` (`null`: none picked yet): the owner alone instructs it, nothing attached, the Decision-Making Agent not asked. */
export function emptyDraft(harness = null) {
  return {
    name: "",
    photo: null,
    description: "",
    system_prompt: "",
    harness,
    plan: { strategy: DEFAULT_STRATEGY, models: [] },
    decision_making: false,
    respond: "owner_only", // for the machine
    skills: [],
    mcps: [],
    tags: [],
  };
}

/**
 * The draft of an agent as it stands. The plan's own effort rides with it;
 * absent, it inherits the setting.
 * @param {object} agent an `AgentDef`
 */
export function draftOf(agent) {
  return {
    name: agent.name,
    photo: agent.photo ?? null,
    description: agent.description ?? "",
    system_prompt: agent.system_prompt,
    harness: agent.harness,
    plan: withEffort({ strategy: agent.models?.strategy ?? DEFAULT_STRATEGY, models: (agent.models?.models ?? []).map((m) => ({ ...m })) }, agent.models?.effort),
    decision_making: agent.decision_making ?? false,
    respond: agent.respond,
    skills: [...(agent.skills ?? [])],
    mcps: [...(agent.mcps ?? [])],
    tags: [...(agent.tags ?? [])],
  };
}

/** One value as the wire would carry it — what two drafts are compared by. */
const said = (value) => JSON.stringify(value ?? null);

/** An agent's whole definition as a PATCH names it, field by field. */
const patchFields = (d) => ({
  name: d.name,
  photo: d.photo,
  description: d.description || null,
  system_prompt: d.system_prompt,
  harness: d.harness,
  models: d.plan,
  decision_making: d.decision_making,
  respond: d.respond,
  skills: d.skills,
  mcps: d.mcps,
  tags: d.tags,
});

/**
 * Whether the draft differs from what the dialog opened on — by what it
 * would write, so a field typed and typed back is no change, and a list
 * re-picked in the same order is none either.
 * @param {object | null | undefined} draft @param {object | null | undefined} seeded the draft as the dialog opened
 */
export function dirty(draft, seeded) {
  return said(draft) !== said(seeded);
}

/**
 * What a dialog does when asked to close — by Escape, a click outside, its
 * ✕ or its Cancel, all one door:
 *
 * - `stay` while a save is on its way: the answer is coming, and a dialog
 *   closed under it would report a save nobody saw land;
 * - `ask` when there are edits: a long prompt is not dropped by a stray key;
 * - `close` otherwise.
 * @param {{dirty: boolean, busy: boolean}} standing
 * @returns {"stay" | "ask" | "close"}
 */
export function mayClose({ dirty: edited, busy }) {
  if (busy) return "stay";
  return edited ? "ask" : "close";
}

/**
 * The plan's models the chosen harness does not list — kept from the
 * harness the agent ran on before, or typed by hand. They stay in the plan
 * (a harness may run a model it does not advertise) and are marked, so a
 * change of harness never leaves another harness's models in the plan
 * unsaid. Nothing is marked until this harness has answered with a list:
 * unknown is not *none*.
 * @param {{models?: readonly {model: string}[]} | null | undefined} plan
 * @param {{models?: readonly {id: string}[]} | null | undefined} answered what this harness answered (`answeredBy`), or `null`
 * @returns {string[]}
 */
export function foreignModels(plan, answered) {
  const listed = answered?.models ?? [];
  if (listed.length === 0) return [];
  const ids = new Set(listed.map((m) => m.id));
  return (plan?.models ?? []).map((m) => m.model).filter((id) => !ids.has(id));
}

/**
 * What a save sends: a patch of the agent — a core agent's three fields, or
 * the fields of its definition **that the editor changed** — or the creation
 * of a new one.
 *
 * `seeded` is the draft as the dialog opened. With it, a patch names only
 * what differs: the node changes the fields a body names on the record as
 * it stands, so an editor left open while a skill was attached elsewhere
 * saves its rename and leaves the skill. Without it — nothing to compare
 * with — the whole definition is named.
 * @param {ReturnType<typeof emptyDraft>} d the draft
 * @param {{id: string} | null} agent the agent edited; `null` for a new one
 * @param {boolean} core whether it is a core agent
 * @param {ReturnType<typeof emptyDraft> | null} [seeded] the draft as the dialog opened
 * @returns {{kind: "patch", id: string, body: object} | {kind: "create", body: object}}
 */
export function saveOf(d, agent, core, seeded = null) {
  if (agent && core) return { kind: "patch", id: agent.id, body: Object.fromEntries(CORE_FIELDS.map((field) => [field, CORE_VALUE[field](d)])) };
  if (agent) {
    const now = patchFields(d);
    if (!seeded) return { kind: "patch", id: agent.id, body: now };
    const before = patchFields(seeded);
    return { kind: "patch", id: agent.id, body: Object.fromEntries(Object.entries(now).filter(([field, value]) => said(value) !== said(before[field]))) };
  }
  return {
    kind: "create",
    body: {
      name: d.name,
      ...(d.photo ? { photo: d.photo } : {}),
      system_prompt: d.system_prompt,
      harness: d.harness,
      // A plan that names no model may still name an effort.
      ...((d.plan.models ?? []).length || d.plan.effort ? { models: d.plan } : {}),
      ...(d.description ? { description: d.description } : {}),
      ...(d.decision_making ? { decision_making: d.decision_making } : {}),
      respond: d.respond,
      skills: d.skills,
      mcps: d.mcps,
      tags: d.tags,
    },
  };
}
