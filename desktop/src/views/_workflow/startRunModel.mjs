/**
 * What the start dialog says once its act landed: adopting a proposal is
 * starting its run — by listening when the design begins on events — and a
 * plain start runs now, or queues behind the live run. One rule, not four
 * nested questions in the dialog. Plain `.mjs`, so `node --test` reads it.
 */

import { t } from "../../i18n/l10n.mjs";

/**
 * @param {{adopt?: boolean, listen?: boolean, queued?: boolean}} did
 * @returns {string}
 */
export function startedWords({ adopt = false, listen = false, queued = false } = {}) {
  if (adopt) return listen ? t("workflow-start-run-dialog-adopted-listening") : t("workflow-start-run-dialog-adopted-run-has-started");
  if (listen) return t("workflow-start-run-dialog-listening");
  return queued ? t("workflow-start-run-dialog-queued-starts-when-live-run-finishes") : t("workflow-run-workflow-dialog-run-has-started");
}
