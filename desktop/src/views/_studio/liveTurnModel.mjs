/**
 * The turns in flight (13 — Conversations §The reply streams): what an
 * agent has said and thought so far in a scope, the tool it runs right now,
 * as the bus's `agent_streamed` frames add up to it; settled when the reply
 * lands (`agent_replied` names the message) and retired once that message
 * is in the timeline's page — so the row never blinks out before its
 * message is drawn. And the words of the thinking block and of the control
 * over every block: *auto · shown · hidden*. The turns are kept as
 * `Map<scope, Map<agent, LiveTurn>>`, never mutated: every change is a new
 * map, so a store over `useSyncExternalStore` can hand out the same
 * reference until something moved. Plain `.mjs`, so `node --test` reads it.
 */

import { t as tr } from "../../i18n/l10n.mjs";

/** No turn anywhere. */
export const NO_TURNS = Object.freeze(new Map());

/** The three ways the thinking is shown, in the menu's order. */
export const THINKING_MODES = Object.freeze(["auto", "shown", "hidden"]);

/** The mode before a choice: open while it is all there is, folded once the words begin. */
const FIRST_MODE = "auto";

const THINKING_LABEL = Object.freeze({ auto: tr("studio-conversation-mode-auto"), shown: tr("studio-live-turn-shown"), hidden: tr("studio-live-turn-hidden") });

/** One sentence each: what the mode does to every thinking block. */
const THINKING_MEANING = Object.freeze({
  auto: tr("studio-live-turn-open-while-agent-only-thinking-folds"),
  shown: tr("studio-live-turn-every-thinking-open-above-words-live"),
  hidden: tr("studio-live-turn-every-thinking-folded-one-line-open"),
});

/** The kit glyph each mode wears, by name in `ICON`. */
const THINKING_ICON = Object.freeze({ auto: "thinking", shown: "inspect", hidden: "hidden" });

/**
 * A remembered thinking mode, else the default.
 * @param {unknown} raw
 * @returns {"auto" | "shown" | "hidden"}
 */
export function thinkingMode(raw) {
  return THINKING_MODES.includes(raw) ? raw : FIRST_MODE;
}

/**
 * Whether one thinking block is open: this block's own press wins until the
 * mode changes; else `shown` is open, `hidden` is folded, and `auto` is open
 * only while the thinking is live and no words have begun.
 * @param {{ mode: string, own?: boolean | null, live: boolean, writing: boolean }} facts
 */
export function thinkingOpen({ mode, own = null, live, writing }) {
  if (typeof own === "boolean") return own;
  switch (thinkingMode(mode)) {
    case "shown":
      return true;
    case "hidden":
      return false;
    default:
      return live && !writing;
  }
}

/** The menu's three choices, in order, each with its meaning. */
export function thinkingChoices() {
  return THINKING_MODES.map((id) => ({ id, label: THINKING_LABEL[id], description: THINKING_MEANING[id], icon: THINKING_ICON[id] }));
}

/**
 * The trigger of the control: what it says and its native title.
 * @param {string} mode
 */
export function thinkingTriggerWords(mode) {
  const chosen = thinkingMode(mode);
  return { label: tr("studio-live-turn-thinking-3", { chosen: THINKING_LABEL[chosen] }), title: THINKING_MEANING[chosen], icon: THINKING_ICON[chosen] };
}

/**
 * A turn's shape: the words so far, the thinking so far, unix seconds it
 * began (zero when only the bus said so), the tool running now (null between
 * tools), and — once the reply landed — the id of the message it became.
 * @param {{text?: string, thinking?: string, since?: number, working?: string | null, landed?: string | null}} [from]
 */
function turn(from = {}) {
  return {
    text: from.text ?? "",
    thinking: from.thinking ?? "",
    since: Number(from.since) || 0,
    working: from.working ?? null,
    landed: from.landed ?? null,
  };
}

/** The `scope → agent → turn` maps with one agent's turn replaced. */
function withTurn(turns, scope, agent, next) {
  const agents = new Map(turns.get(scope) ?? []);
  agents.set(agent, next);
  const out = new Map(turns);
  out.set(scope, agents);
  return out;
}

/**
 * A frame's words and thinking appended to the agent's turn in the scope
 * — a first frame opens the turn — and its tool line, when the frame
 * carries one, replacing the last: `null` means the tool ended. A frame
 * with nothing in it changes nothing, and the same map comes back.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {{scope: string, agent: string, text?: string, thinking?: string, working?: string | null}} frame
 */
export function applyStreamed(turns, frame) {
  const text = frame?.text ?? "";
  const thinking = frame?.thinking ?? "";
  const saysWorking = !!frame && frame.working !== undefined;
  if (!frame?.scope || !frame?.agent || (text === "" && thinking === "" && !saysWorking)) return turns;
  const before = turns.get(frame.scope)?.get(frame.agent) ?? turn();
  const working = saysWorking ? (frame.working ?? null) : before.working;
  if (text === "" && thinking === "" && working === before.working && turns.get(frame.scope)?.has(frame.agent)) return turns;
  return withTurn(turns, frame.scope, frame.agent, {
    ...before,
    text: before.text + text,
    thinking: before.thinking + thinking,
    since: before.since || Math.floor(Date.now() / 1000),
    working,
  });
}

/**
 * The turns a reader that joined mid-turn is given (`GET
 * /conversations/{id}/live`), standing in for whatever the bus said of the
 * scope so far — the node's copy is whole, the bus's is from the moment the
 * reader arrived. An empty answer clears the scope.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {string} scope
 * @param {readonly {agent: string, text: string, thinking: string, since: number, working?: string | null}[]} rows
 */
export function primeTurns(turns, scope, rows) {
  const list = rows ?? [];
  if (list.length === 0) return clearScope(turns, scope);
  const agents = new Map();
  for (const r of list) if (r?.agent) agents.set(r.agent, turn(r));
  const out = new Map(turns);
  out.set(scope, agents);
  return out;
}

/**
 * The message an `agent_replied` frame says the turn became, or `null` when
 * nothing landed: `posted: false` is the node's word that no message was
 * written — the agent acted through tools alone, or its reply was refused —
 * and then no id is waited for, whatever the frame carries beside it. A
 * reply longer than one message lands as several, in order, and `message`
 * names the **last**: the live row gives way once the whole reply is drawn.
 * @param {{posted?: boolean, message?: string | null} | null | undefined} replied
 * @returns {string | null}
 */
export function landedOf(replied) {
  if (!replied || replied.posted === false) return null;
  return typeof replied.message === "string" && replied.message ? replied.message : null;
}

/**
 * The agent's reply landed as the message `message`: the turn stays on
 * screen, frozen, until that message is in the page (`retireLanded`). No
 * message — the agent acted through tools alone, or a refusal was posted
 * elsewhere — and the turn is cleared at once. So is a turn in a scope no
 * timeline is reading (`watched` false): nobody is waiting for its message
 * to be drawn, a frozen row would be retired by no page, and whoever opens
 * the scope later reads the message itself.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {string} scope
 * @param {string} agent
 * @param {string | null | undefined} message
 * @param {boolean} [watched] whether a timeline reads the scope
 */
export function settleTurn(turns, scope, agent, message, watched = true) {
  if (!message || !watched) return clearTurn(turns, scope, agent);
  const before = turns.get(scope)?.get(agent);
  if (!before) return turns;
  return withTurn(turns, scope, agent, { ...before, working: null, landed: message });
}

/**
 * Every settled turn whose message is now among `ids` is gone — the row's
 * message has taken its place. The same map comes back when none was.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {ReadonlySet<string>} ids
 */
export function retireLanded(turns, ids) {
  let out = turns;
  for (const [scope, agents] of turns) {
    for (const [agent, t] of agents) {
      if (t.landed && ids.has(t.landed)) out = clearTurn(out, scope, agent);
    }
  }
  return out;
}

/**
 * The agent's turn in the scope is over — its reply landed and is drawn, or
 * its session ended with nothing to say. The same map comes back when there
 * was nothing to clear.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {string} scope
 * @param {string} agent
 */
export function clearTurn(turns, scope, agent) {
  const agents = turns.get(scope);
  if (!agents?.has(agent)) return turns;
  const out = new Map(turns);
  if (agents.size === 1) {
    out.delete(scope);
  } else {
    const rest = new Map(agents);
    rest.delete(agent);
    out.set(scope, rest);
  }
  return out;
}

/**
 * A scope's settled turns gone, its turns still in flight kept: the last
 * timeline that read the scope left, so no page will ever draw the messages
 * the frozen rows wait for. The same map comes back when none was settled.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {string} scope
 */
export function dropSettled(turns, scope) {
  let out = turns;
  for (const [agent, t] of turns.get(scope) ?? []) if (t.landed) out = clearTurn(out, scope, agent);
  return out;
}

/** Every turn of a scope gone — the conversation was deleted, or the node says nothing runs. */
export function clearScope(turns, scope) {
  if (!turns.has(scope)) return turns;
  const out = new Map(turns);
  out.delete(scope);
  return out;
}

/**
 * The turns in flight in a scope, oldest first, each with its agent — what
 * a timeline draws at its foot. Empty for a scope nothing runs in.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {string} scope
 */
export function turnsOf(turns, scope) {
  const agents = turns.get(scope);
  if (!agents) return [];
  return [...agents.entries()].map(([agent, t]) => ({ agent, ...t })).sort((a, b) => a.since - b.since);
}

/**
 * The agents with a row to draw in a scope, oldest turn first — one with
 * words, thinking or a tool running. An agent working with nothing said yet
 * is the footer's line, not a row.
 * @param {ReadonlyMap<string, ReadonlyMap<string, object>>} turns
 * @param {string} scope
 * @returns {string[]}
 */
export function agentsOf(turns, scope) {
  return turnsOf(turns, scope)
    .filter((t) => t.text || t.thinking || t.working)
    .map((t) => t.agent);
}

/** Whether two agent lists name the same agents in the same order — a stable array is kept when they do. */
export function sameAgents(a, b) {
  return a.length === b.length && a.every((id, i) => id === b[i]);
}

/**
 * The last `n` characters of a thinking, on one line — what a folded block
 * shows moving while the agent still thinks. Never splits a surrogate pair.
 * @param {string} text
 * @param {number} [n]
 */
export function glimpse(text, n = 80) {
  const flat = String(text ?? "").replace(/\s+/g, " ").trim();
  if (flat.length <= n) return flat;
  let start = flat.length - n;
  const code = flat.charCodeAt(start);
  if (code >= 0xdc00 && code <= 0xdfff) start += 1;
  return `…${flat.slice(start)}`;
}

/** *1.2k chars* — how long a thinking is, in the block's header. @param {number} chars */
export function lengthWords(chars) {
  const n = Number(chars) || 0;
  if (n < 1000) return tr("studio-live-turn-chars", { n });
  return tr("studio-live-turn-k-chars", { k: (n / 1000).toFixed(n < 10_000 ? 1 : 0) });
}

/**
 * The thinking block's header: its word, its length, and what a press does
 * — while live, that it is still being written and for how long.
 * @param {number} chars
 * @param {boolean} open
 * @param {boolean} [live]
 * @param {number} [elapsedS] seconds since the turn began, when known
 */
export function thinkingWords(chars, open, live = false, elapsedS = 0) {
  const length = lengthWords(chars);
  const seconds = Math.max(0, Math.floor(Number(elapsedS) || 0));
  return {
    label: live ? (seconds > 0 ? tr("studio-live-turn-thinking-s", { seconds }) : tr("studio-live-turn-thinking")) : tr("studio-live-turn-thinking-2"),
    length,
    hint: open ? tr("studio-live-turn-hide-thinking") : tr("studio-live-turn-show-thinking", { length }),
  };
}
