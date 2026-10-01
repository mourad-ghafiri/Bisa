/**
 * Which public hook secrets are on screen, waiting to be taken — the rule
 * `hookSecretsStore.ts` keeps by. A secret is shown once: what a decision
 * minted joins what is still shown, and a hook whose secret was minted again
 * meanwhile (a rotation, a second adoption) shows its newest alone, since
 * the older one no longer opens anything.
 */

/**
 * What is shown after `minted` arrives: the ones still shown whose hook was
 * not minted again, then the new ones, in the order they came. The same list
 * when nothing was minted.
 * @template {{ path: string }} S
 * @param {readonly S[]} shown
 * @param {readonly S[]} minted
 * @returns {readonly S[]}
 */
export function shownWith(shown, minted) {
  if (!minted || minted.length === 0) return shown;
  const again = new Set(minted.map((s) => s.path));
  return [...shown.filter((s) => !again.has(s.path)), ...minted];
}

/**
 * What an act that armed a listener minted: the secrets its answer carries
 * — turning a workflow On, a goal's start or adoption, listening again. A
 * secret is disclosed by no read route, so every such act shows what it
 * minted or hands it to the store; none drops it.
 * @template S
 * @param {{secrets?: readonly S[] | null} | null | undefined} answer
 * @returns {readonly S[]}
 */
export function mintedBy(answer) {
  return Array.isArray(answer?.secrets) ? answer.secrets : [];
}

/**
 * The secret a hook start's form shows after a rotation: the one minted for
 * that very step, else none — the form of another step never wears it.
 * @template {{ step: string }} S
 * @param {S | null | undefined} secret
 * @param {string} step
 * @returns {S | null}
 */
export function shownFor(secret, step) {
  return secret && secret.step === step ? secret : null;
}
