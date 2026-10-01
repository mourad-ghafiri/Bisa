/**
 * The publishing policy's words — what each of the three values means for a
 * push or a pull request leaving this machine — mirrored from the core's
 * `PublishPolicy` and checked against it in the test.
 */

import { t } from "../../i18n/l10n.mjs";

export const PUBLISH_POLICIES = Object.freeze(["manual", "gated", "auto"]);

export const PUBLISH_LABEL = Object.freeze({
  manual: "manual",
  gated: "gated",
  auto: "automatic",
});

export const PUBLISH_MEANING = Object.freeze({
  manual: t("work-publish-policy-bisa-will-not-push-open-pull"),
  gated: t("work-publish-policy-push-pull-request-opens-gate-inbox"),
  auto: t("work-publish-policy-push-leaves-machine-soon-agent-asks"),
});

/** Theme roles: `auto` is the one that leaves unasked, so it is the warning. */
export const PUBLISH_TONE = Object.freeze({
  manual: "quiet",
  gated: "accent",
  auto: "warn",
});

/** A project's policy; the core's default (`auto`) when the record carries none. */
export function publishOf(project) {
  const p = project?.publish;
  return PUBLISH_POLICIES.includes(p) ? p : "auto";
}
