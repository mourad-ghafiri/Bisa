/**
 * The notify step's scope as the form lists it. Plain `.mjs`, so
 * `node --test` reads it.
 */

/**
 * Whether the scope is one the select carries: none, a channel the
 * workspace has, or a text input the workflow declares. A scope the list
 * does not carry — a goal or workstream id, an input no longer declared —
 * is shown as it is rather than as the first option.
 * @param {string | null | undefined} scope
 * @param {readonly string[]} channelIds
 * @param {readonly string[]} textInputNames
 */
export function scopeListed(scope, channelIds, textInputNames) {
  const s = scope ?? "";
  return s === "" || (channelIds ?? []).includes(s) || (textInputNames ?? []).some((name) => `{inputs.${name}}` === s);
}
