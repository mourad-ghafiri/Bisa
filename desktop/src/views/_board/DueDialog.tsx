/**
 * Set or clear a card's due date (ide/16): one calendar day, `YYYY-MM-DD`,
 * written to the workstream record through the same `PATCH` as its name.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { Button, DateInput, Dialog, Field, failureText, useToast } from "../../ui";
import { dueTone, todayKey } from "./boardModel.mjs";
import { t } from "../../i18n/l10n.mjs";

export function DueDialog({
  open,
  wid,
  title,
  due,
  onClose,
  onSaved,
}: {
  open: boolean;
  wid: string;
  title: string;
  due: string | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const toast = useToast();
  const [value, setValue] = useState(due ?? "");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) setValue(due ?? "");
  }, [open, due]);
  const words = value ? dueTone(value, todayKey()).label : t("board-due-dialog-no-due-date");

  const write = async (next: string | null) => {
    setBusy(true);
    try {
      await api.patchWorkstream(wid, { due: next });
      onSaved();
      onClose();
    } catch (e) {
      toast.error(failureText("board", "due-dialog-failed", e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("board-due-dialog-due-date", { title })}
      description={t("board-due-dialog-day-not-instant-same-day-wherever")}
      width="max-w-sm"
      footer={
        <>
          {due && (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void write(null)}>{t("board-due-dialog-clear")}</Button>
          )}
          <Button size="sm" variant="ghost" disabled={busy} onClick={onClose}>{t("board-due-dialog-cancel")}</Button>
          <Button size="sm" variant="primary" disabled={busy || value === ""} onClick={() => void write(value)}>
            {busy ? t("board-due-dialog-setting") : t("board-due-dialog-set")}
          </Button>
        </>
      }
    >
      <Field label={t("board-due-dialog-due")} hint={words}>
        <DateInput value={value} onChange={setValue} autoFocus />
      </Field>
    </Dialog>
  );
}
