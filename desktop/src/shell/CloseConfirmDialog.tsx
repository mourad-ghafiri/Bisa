/**
 * The one confirmation the app asks before something ends — a live shell
 * closing, a harness terminating, the window closing or the app quitting.
 * Mounted once in `App`; every asker parks its question in
 * `terminalCloseGuard.askClose` and this renders it — the question's words,
 * then the door to the switch that governs it (`closeGuardModel.CONFIRM_DOORS`),
 * a link that drops the question as it is followed: a person who went to
 * change the setting closes again afterwards.
 */

import { ConfirmDialog } from "../ui";
import { SettingsLink } from "../views/_settings/SettingsTabLink";
import { cancelClose, confirmClose, usePendingClose } from "./terminalCloseGuard";
import { t } from "../i18n/l10n.mjs";

export function CloseConfirmDialog() {
  const pending = usePendingClose();
  return (
    <ConfirmDialog
      open={pending !== null}
      onClose={cancelClose}
      onConfirm={confirmClose}
      title={pending?.title ?? ""}
      body={
        pending ? (
          <>
            <p>{pending.body}</p>
            <p className="mt-2">
              {pending.door.lead}{" "}
              <SettingsLink tab={pending.door.tab} className="text-xs" onFollow={cancelClose}>
                {pending.door.path}
              </SettingsLink>
              {pending.door.switch ? ` — ${pending.door.switch}` : ""}.
            </p>
          </>
        ) : (
          ""
        )
      }
      confirmLabel={pending?.confirmLabel ?? t("shell-about-dialog-close")}
      danger
    />
  );
}
