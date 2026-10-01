/**
 * Types for `namesModel.mjs`, which is plain JavaScript so `node --test`
 * can import it without a build step.
 */

/** Every known principal's name by pubkey: a member's own label, an agent's name, *You* for yourself until you give a name. */
export declare function principalNames(
  me: string | null | undefined,
  members: readonly { pubkey: string; label?: string | null }[] | null | undefined,
  agents: readonly { pubkey: string; name: string }[] | null | undefined,
): Map<string, string>;
/** A principal's name, else the head of its key. */
export declare function nameIn(names: ReadonlyMap<string, string>, pubkey: string): string;
