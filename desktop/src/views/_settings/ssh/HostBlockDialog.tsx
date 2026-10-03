/**
 * *Copy a Host block…*: the `Host` block a person pastes into their own
 * `~/.ssh/config` — an alias that reaches one host with one key. The
 * platform reads that file and never writes it (ide/04); this dialog only
 * writes the text to the clipboard. Mounted while open, fresh every time.
 */

import { useState } from "react";
import { Button, CopyText, Dialog, Field, Select, TextInput } from "../../../ui";
import { hostBlockText } from "../sshModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

export function HostBlockDialog({
  keys,
  hosts,
  onClose,
}: {
  keys: readonly { name: string; path: string }[];
  /** The hosts a block usually reaches — a suggestion list, free text allowed. */
  hosts: readonly string[];
  onClose: () => void;
}) {
  const [alias, setAlias] = useState("github-work");
  const [hostname, setHostname] = useState(hosts[0] ?? "github.com");
  const [key, setKey] = useState(keys[0]?.path ?? "");
  const shownAlias = alias.trim() || "github-work";
  const shownHost = hostname.trim() || "github.com";
  const text = hostBlockText(shownAlias, shownHost, "git", key || null);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("settings-host-block-dialog-copy-host-block")}
      description={t("settings-host-block-dialog-host-block-makes-git-owner-repo", { shownAlias, shownHost })}
      footer={
        <>
          <Button size="sm" variant="ghost" onClick={onClose}>{t("settings-host-block-dialog-close")}</Button>
          <CopyText value={text} label={t("settings-host-block-dialog-copy-block")} />
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex flex-wrap items-end gap-2">
          <Field label={t("settings-host-block-dialog-alias")} hint={t("settings-host-block-dialog-what-remote-names-after-git")}>
            <TextInput value={alias} className="w-36 font-mono" onChange={(e) => setAlias(e.target.value)} />
          </Field>
          <Field label={t("settings-host-block-dialog-host")}>
            <TextInput list="ssh-block-hosts" value={hostname} className="w-44 font-mono" onChange={(e) => setHostname(e.target.value)} />
            <datalist id="ssh-block-hosts">
              {hosts.map((h) => (
                <option key={h} value={h} />
              ))}
            </datalist>
          </Field>
          <Field label={t("settings-host-block-dialog-key")} hint={t("settings-host-block-dialog-key-only-whatever-ssh-agent-holds")}>
            <Select value={key} className="w-56" onChange={(e) => setKey(e.target.value)}>
              <option value="">{t("settings-git-profiles-panel-whatever-ssh-would-offer")}</option>
              {keys.map((k) => (
                <option key={k.name} value={k.path}>
                  {k.name}
                </option>
              ))}
            </Select>
          </Field>
        </div>
        <pre className="overflow-x-auto rounded-control bg-surface-2 p-2 font-mono text-2xs text-text-dim">{text}</pre>
      </div>
    </Dialog>
  );
}
