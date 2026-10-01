/**
 * Which mark a harness id wears (ide/06): the compiled-in ids as they are,
 * the two presets that have a mark of their own, and the prefixed families —
 * `acp:<agent>` is any ACP agent, `custom:<name>` any custom descriptor —
 * folded to one mark each. Pure, so `node --test` holds it equal to the Rust
 * catalog (`crates/bisa-harness/src/catalog.rs`).
 */

/** Every mark `harnessMarks.tsx` draws, by the id it stands for. */
export const MARK_IDS = Object.freeze(["claude-code", "codex", "pi", "omp", "opencode", "copilot", "grok", "goose", "cursor-agent", "acp", "custom"]);

/**
 * The mark for a harness id, or `null` for one nothing here knows — the
 * caller draws the generic terminal glyph then.
 * @param {string | null | undefined} id
 * @returns {"claude-code" | "codex" | "pi" | "omp" | "opencode" | "copilot" | "grok" | "goose" | "cursor-agent" | "acp" | "custom" | null}
 */
export function markIdOf(id) {
  if (!id) return null;
  if (MARK_IDS.includes(id)) return id;
  const colon = id.indexOf(":");
  if (colon < 0) return null;
  const prefix = id.slice(0, colon);
  const rest = id.slice(colon + 1);
  if (prefix === "acp") return "acp";
  if (prefix === "custom") return "custom";
  if (prefix === "preset" && MARK_IDS.includes(rest)) return rest;
  return null;
}
