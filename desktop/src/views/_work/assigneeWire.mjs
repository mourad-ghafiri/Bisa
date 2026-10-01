/**
 * The three shapes of a principal, and the one string that names any of them.
 *
 * `Assignee` travels two ways. A response carries the tagged object the store
 * writes (`{"team": "…"}`); every route that *accepts* one takes the compact
 * string instead (`agent:<id>` / `human:<64 hex>` / `team:<id>`), because a CLI
 * argument and a governance entry both need it flat. Both directions used to
 * be written out twice — once in `types.ts` for the boards and once in
 * `AssigneePicker.tsx` for the picker — which is the arrangement where one
 * copy gains a case and the other quietly does not.
 *
 * Plain `.mjs` with a `.d.mts` beside it, following `ui/fileTreeModel.mjs`:
 * there is no jsdom in this repo, so the way a rule gets a test is by not
 * living inside a component.
 *
 * **A bare id is not a wire form.** The server refuses one deliberately —
 * `7ZK…` could name an agent or a team, and guessing is how two assignment
 * surfaces end up disagreeing about who was named. {@link kindOf} answers
 * `null` for an unprefixed string, and {@link isWire} is what a submit path
 * asks; a display path decides for itself what to show for a key it cannot
 * read.
 */

/** The three kinds, in the order every grouped list shows them. */
export const ASSIGNEE_KINDS = ["agent", "human", "team"];

/** A person's key: 64 lowercase hex characters, as the routes spell it. */
const HUMAN_KEY = /^[0-9a-f]{64}$/;

/**
 * The kind a wire key names, or `null` for anything that is not a wire key.
 *
 * A bare id is refused rather than guessed at: the three prefixes are the
 * whole contract, and a reader that invented one would render a stored value
 * the routes would refuse.
 */
export function kindOf(key) {
  const text = typeof key === "string" ? key : "";
  if (text.startsWith("agent:")) return "agent";
  if (text.startsWith("human:")) return "human";
  if (text.startsWith("team:")) return "team";
  return null;
}

/** The id half of a wire key — everything after the first colon. */
export function idOf(key) {
  const text = typeof key === "string" ? key : "";
  const colon = text.indexOf(":");
  return colon === -1 ? text : text.slice(colon + 1);
}

/**
 * The structured `Assignee` a wire key means — what a response carries — or
 * `null` when the key is not a wire key.
 */
export function wireToAssignee(key) {
  const id = idOf(key);
  const kind = kindOf(key);
  if (kind === "human") return { human: id };
  if (kind === "team") return { team: id };
  if (kind === "agent") return { agent: id };
  return null;
}

/**
 * The `Assignee` a wire key means, or a thrown error naming the key. For a
 * submit path that has already asked {@link isWire}: a miss here is a bug, and
 * a bug is shown, never guessed around.
 */
export function requireWire(key) {
  const assignee = wireToAssignee(key);
  if (assignee === null) throw new Error(`Not an agent, team or person: ${key}`);
  return assignee;
}

/** The wire key for a structured `Assignee` — the inverse of the above. */
export function assigneeToWire(a) {
  if (a && "agent" in a) return `agent:${a.agent}`;
  if (a && "human" in a) return `human:${a.human}`;
  return `team:${a.team}`;
}

/**
 * Whether a key is a form the server will accept.
 *
 * Explicitly prefixed, non-empty id, and a person's key is checked for shape:
 * `human:bob` is a typo that would otherwise be stored as a pubkey nobody
 * has, and an assignment to a ghost fails silently at routing time.
 */
export function isWire(key) {
  const text = typeof key === "string" ? key : "";
  if (text.startsWith("agent:") || text.startsWith("team:")) return idOf(text).length > 0;
  if (text.startsWith("human:")) return HUMAN_KEY.test(idOf(text));
  return false;
}
