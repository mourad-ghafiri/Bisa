/**
 * The one secret field's rules (ide/13 §Secret fields): what the box shows
 * and whether the eye has anything to show. A secret is hidden by default;
 * the eye shows and hides what was typed in this window. The node never
 * reads a stored secret back (11 — Security, I49), so once the draft is
 * empty and the node says one is stored, the box shows the mask and the eye
 * is out of work — typing replaces. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../i18n/l10n.mjs";

/** What a stored secret reads as — `bisa_core::MCP_MASK`, word for word, and the mask every box draws. */
export const MASK = "••••••";

/** Whether a value is the mask the node answered, not a value. */
export function isMasked(v) {
  return v === MASK;
}

/**
 * The box as it stands.
 * @param {{ draft: string | null | undefined, stored?: boolean | null, revealed?: boolean, what?: string }} facts
 * @returns {{ shows: "draft" | "stored" | "empty", value: string, inputType: "password" | "text", canReveal: boolean, eye: { label: string, pressed: boolean, title: string }, placeholder: string, dim: boolean }}
 */
export function secretView({ draft, stored = false, revealed = false, what = t("ui-secret-input-what-secret") }) {
  const typed = typeof draft === "string" && draft !== "" && !isMasked(draft);
  if (typed) {
    return {
      shows: "draft",
      value: draft,
      inputType: revealed ? "text" : "password",
      canReveal: true,
      eye: { label: eyeLabel(revealed, what), pressed: revealed, title: eyeLabel(revealed, what) },
      placeholder: "",
      dim: false,
    };
  }
  if (stored) {
    return {
      shows: "stored",
      value: MASK,
      inputType: "text",
      canReveal: false,
      eye: { label: t("ui-secret-input-show", { what }), pressed: false, title: t("ui-secret-input-stored-machine-node-never-shows-back", { what }) },
      placeholder: "",
      dim: true,
    };
  }
  return {
    shows: "empty",
    value: "",
    inputType: revealed ? "text" : "password",
    canReveal: false,
    eye: { label: eyeLabel(revealed, what), pressed: revealed, title: t("ui-secret-input-nothing-typed-yet") },
    placeholder: t("ui-secret-input-set"),
    dim: false,
  };
}

/** The eye's words. */
export function eyeLabel(revealed, what = t("ui-secret-input-what-secret")) {
  return revealed ? t("ui-secret-input-hide", { what }) : t("ui-secret-input-show", { what });
}

/** Whether a focus should select the whole box, so typing replaces the mask. */
export function replaceOnFocus(draft, stored) {
  return isMasked(draft) || ((draft ?? "") === "" && !!stored);
}

/**
 * A multiline secret folded to one line while hidden: the mask and how many lines it holds.
 * @param {string} text
 */
export function foldedWords(text) {
  const lines = String(text ?? "").split("\n").filter((l) => l.trim() !== "").length;
  if (lines === 0) return "";
  return t("ui-secret-input-folded", { mask: MASK, lines });
}
