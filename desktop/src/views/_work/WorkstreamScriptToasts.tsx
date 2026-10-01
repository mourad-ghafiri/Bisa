/**
 * The one workstream-script outcome nobody is looking at (ide/07 §Workstream
 * scripts): a **post-create** script that failed. The pre-create and clean
 * scripts refuse in the dialog the person is standing in; the post-create
 * script runs after the dialog closed and the workstream is already real, so
 * its failure would otherwise be a line in Pulse and nothing else. Mounted
 * once, inside the toast provider; draws nothing.
 */

import { useEngineEvents } from "../../bus";
import { useToast } from "../../ui";
import { t } from "../../i18n/l10n.mjs";

export function WorkstreamScriptToasts() {
  const toast = useToast();
  useEngineEvents((e) => {
    const p = e.payload;
    if (p.type !== "workstream_script_ran" || p.ok || p.phase !== "post_create") return;
    const [reason] = p.output.split("\n");
    toast.error(t("work-workstream-script-toasts-post-create-script-workstream-open-see", { reason }));
  });
  return null;
}
