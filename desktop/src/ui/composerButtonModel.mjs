/**
 * The composer's one trailing slot: *Send*, or *Stop* — never both, never
 * neither. What Cursor and Copilot do while a reply is being generated, and
 * what a person expects: the button they just pressed becomes the way to
 * take it back.
 *
 * Three shapes. `stop` — the roster says a turn of this conversation is
 * stoppable — is *Stop*, enabled. `busy` — the message is posting, or posted
 * within the handover grace and the roster has not said `starting` yet — is
 * Stop-shaped but disabled, so no frame between the POST and the first
 * roster frame shows *Send* again. Otherwise *Send*. Plain `.mjs` so
 * `node --test` reads the rule itself.
 */

import { t } from "../i18n/l10n.mjs";

/**
 * How long after `onSend` resolves the slot stays Stop-shaped waiting for
 * the roster to name a stoppable session. A message that wakes nobody — a
 * channel post with no agent — falls back to *Send* when it lapses.
 */
export const SEND_HANDOVER_GRACE_MS = 1200;

/**
 * Whether the handover has lapsed: nothing is settled (`null`), or the grace
 * has passed since it did.
 * @param {number | null} settledAt when `onSend` resolved, ms; `null` while posting or before any send
 * @param {number} now
 * @param {number} [graceMs]
 */
export function handoverElapsed(settledAt, now, graceMs = SEND_HANDOVER_GRACE_MS) {
  return settledAt !== null && now - settledAt >= graceMs;
}

/**
 * The button the slot draws.
 * @param {{ busy: boolean, stop: { label?: string } | null | undefined, sendable: boolean, disabled: boolean }} state
 * @returns {{ kind: "send" | "sending" | "stop", label: string, hint: string, enabled: boolean }}
 */
export function composerButton({ busy, stop, sendable, disabled }) {
  if (stop) return { kind: "stop", label: stop.label ?? t("ui-composer-button-stop"), hint: t("ui-composer-button-stop-session-working-here-message-typed"), enabled: true };
  if (busy) return { kind: "sending", label: t("ui-composer-button-stop"), hint: t("ui-composer-button-starting"), enabled: false };
  return {
    kind: "send",
    label: t("ui-composer-button-send"),
    hint: sendable ? t("ui-composer-button-send-enter-enter") : disabled ? "" : t("ui-composer-button-over-context-budget-remove-chip-send"),
    enabled: sendable && !disabled,
  };
}
