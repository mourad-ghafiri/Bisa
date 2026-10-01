/**
 * *Generate key…*: an ed25519 pair in the SSH directory, named for the
 * organization it is for, no passphrase from the platform (ide/04). Mounted
 * only while open, so every opening starts with empty fields and never
 * refuses the name it suggested last time.
 */

import { useState } from "react";
import { api } from "../../../api";
import type { SshPublicKey } from "../../../types";
import { Button, Dialog, Field, TextInput } from "../../../ui";
import { attempt } from "../../_work/useAsync";
import { PASSPHRASE_NOTE, suggestKeyName, validateKeyName } from "../sshModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

export function GenerateDialog({
  taken,
  onClose,
  onGenerated,
  onError,
}: {
  /** The names already in the directory — refused here before the round trip. */
  taken: readonly string[];
  onClose: () => void;
  onGenerated: (key: SshPublicKey) => void;
  onError: (message: string) => void;
}) {
  const [owner, setOwner] = useState("");
  const [name, setName] = useState("");
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState(false);
  const effective = name.trim() || suggestKeyName(owner);
  const problem = validateKeyName(effective, taken);
  const submit = async () => {
    if (problem || busy) return;
    setBusy(true);
    await attempt(() => api.sshGenerate({ name: effective, comment: comment.trim() }), onError, onGenerated);
    setBusy(false);
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("settings-generate-dialog-generate-ssh-key")}
      description={t("settings-generate-dialog-ed25519-key-pair-ssh-directory-one")}
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>{t("settings-connectors-panel-cancel")}</Button>
          <Button size="sm" variant="primary" disabled={!!problem || busy} onClick={() => void submit()}>
            {busy ? t("settings-generate-dialog-generating") : t("settings-generate-dialog-generate")}
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <Field label={t("settings-generate-dialog-which-organization-account")} hint={t("settings-generate-dialog-suggests-file-name-key-itself-knows")}>
          <TextInput value={owner} /* content, never translated */ placeholder="acme" autoFocus onChange={(e) => setOwner(e.target.value)} />
        </Field>
        <Field label={t("settings-generate-dialog-file-name")} hint={problem ? <span className="text-danger">{problem}</span> : t("settings-generate-dialog-private-key-pub-beside", { effective })}>
          <TextInput value={name} placeholder={suggestKeyName(owner)} className="font-mono" onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={t("settings-generate-dialog-comment")} hint={t("settings-generate-dialog-carried-public-key-email-machine-so")}>
          <TextInput value={comment} /* content, never translated */ placeholder="you@acme.example" onChange={(e) => setComment(e.target.value)} />
        </Field>
        <p className="text-2xs text-text-dim">{PASSPHRASE_NOTE}</p>
      </form>
    </Dialog>
  );
}
