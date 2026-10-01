/**
 * The one guard outcome nobody is looking at: a refusal. An allow is ambient
 * and a question lands in the Inbox, but a command the guard refused is a
 * turn the agent took differently than it meant to, and the person who set
 * the rule should hear it where they are — not only as a line in Pulse.
 * Mounted once, inside the toast provider; draws nothing.
 */

import { useEngineEvents } from "../../bus";
import { useToast } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function SecurityToasts() {
  const toast = useToast();
  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type !== "guard_decided" || p.verdict !== "denied") return;
    const who = p.by === "classifier" ? t("work-security-toasts-classifier") : p.rule ? t("work-security-toasts-rule-named", { rule: p.rule }) : t("work-security-toasts-guard");
    toast.error(t("work-security-toasts-refused-agent-told-why-see-pulse", { tool: p.tool, who, reason: p.reason, flag: (p.reason) ? "yes" : "no" }));
  });
  return null;
}
