/**
 * A redacted secret in rendered text.
 *
 * An agent that was handed `«secret:github_token:7f3a2c»` quotes it back the
 * same way, and the reply is stored with the placeholder in it — nothing an
 * agent hands back is restored. To the person reading it, the placeholder is
 * theirs to recognise: they know which token they pasted. So it renders as a
 * small chip naming the *kind* — `github token` — with the full placeholder
 * in the tooltip, rather than as a run of guillemets and hex.
 *
 * Works on the HTML micromark produced: the guillemets survive escaping, and
 * a placeholder can never sit inside a tag, so a plain replacement over the
 * whole string is safe.
 */

import { t } from "../i18n/l10n.mjs";

/** A kind is a rule id, possibly prefixed (`env:MY_KEY`, `user:team_key`): letters, digits, `_ . : -`. */
const PLACEHOLDER = /«secret:([A-Za-z0-9_][A-Za-z0-9_.:-]*?):([0-9a-f]{6,})»/g;

/** `env:MY_KEY` → `MY_KEY`, `github_token` → `github token`, `user:team_key` → `team key`. */
export function placeholderKindWords(kind) {
  const [prefix, rest] = String(kind).includes(":") ? String(kind).split(":", 2) : [null, String(kind)];
  if (prefix === "env") return rest;
  return rest.replace(/_/g, " ");
}

function escapeAttr(s) {
  return String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * Every placeholder in `html` as a chip; the string unchanged when there is none.
 * @param {string} html
 * @returns {string}
 */
export function placeholderChips(html) {
  if (!html || !html.includes("«secret:")) return html;
  return html.replace(PLACEHOLDER, (whole, kind) => {
    const words = placeholderKindWords(kind);
    return `<span class="placeholder-chip" title="${escapeAttr(t("ui-placeholder-chips-secret-title", { whole }))}">${escapeAttr(words)}</span>`;
  });
}
