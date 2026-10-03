/**
 * One SSH key, one card (ide/04, ide/13): what it is, whether ssh-agent holds
 * it, the last check a person ran on it, and the one next step with the
 * button that does it. The doors out are explicit — *Add to a host* copies
 * the public key and opens the host's own SSH-keys page, *Use in a profile*
 * lands on Identity — and a check is this key on one host, its answer pinned
 * here. Public material only: the card never sees a private key.
 */

import { useState } from "react";
import { Button, Chip, CopyText, ICON, Menu, TextInput } from "../../../ui";
import { SettingsLink } from "../SettingsTabLink";
import type { KeyCardWords } from "../sshModel.mjs";
import { busyKey, checkWords, hostPages } from "../sshModel.mjs";
import { t } from "../../../i18n/l10n.mjs";

/** The tone classes the card's sentences use. */
const toneClass = (tone: string) => (tone === "ok" ? "text-ok" : tone === "danger" ? "text-danger" : tone === "warn" ? "text-warn" : "text-text-dim");

export function KeyCard({
  card,
  agentAvailable,
  hosts,
  busy,
  fresh,
  onLoad,
  onCheck,
  onAddToHost,
}: {
  card: KeyCardWords;
  agentAvailable: boolean;
  /** The hosts a check can name; free text allowed beside them. */
  hosts: readonly string[];
  /** Every action in flight, as `busyKey(action, name)`. */
  busy: ReadonlySet<string>;
  /** Generated in this session: the card says so once. */
  fresh: boolean;
  onLoad: (name: string) => void;
  onCheck: (name: string, host: string, path: string) => void;
  /** Copy the public key, then open the host's SSH-keys page. */
  onAddToHost: (card: KeyCardWords, host: string) => void;
}) {
  const [host, setHost] = useState(card.check?.host ?? hosts[0] ?? "github.com");
  const loading = busy.has(busyKey("load", card.name));
  const checking = busy.has(busyKey("check", card.name));
  const check = card.check ? checkWords(card.check) : null;
  const next = card.next;
  const hostId = `ssh-check-hosts-${card.name}`;
  return (
    <li className="flex flex-col gap-2 rounded-card border border-border bg-surface p-3" aria-label={card.name}>
      <div className="flex flex-wrap items-center gap-2">
        <ICON.fingerprint size={13} aria-hidden className="shrink-0 text-text-dim" />
        <span className="font-mono text-xs font-medium">{card.name}</span>
        <Chip tone="quiet">{card.algorithm}</Chip>
        <Chip tone={card.loaded ? "ok" : agentAvailable ? "quiet" : "warn"} icon={card.loaded ? ICON.ok : undefined}>
          {card.loadedWords}
        </Chip>
        {fresh && <Chip tone="accent">{t("settings-key-card-new")}</Chip>}
        <span className="flex-1" />
        <CopyText value={card.publicLine} label={t("settings-key-card-copy-public-key")} />
      </div>
      <p className="truncate font-mono text-2xs text-text-dim" title={`${card.fingerprint} · ${card.path}`}>
        {card.shortFingerprint}
        {card.comment ? ` · ${card.comment}` : ""} · {card.path}
      </p>

      {check && (
        <p className={`text-2xs ${toneClass(check.tone)}`} role="status">
          {check.text} <span className="text-text-dim">· {check.when}</span>
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2 rounded-control bg-surface-2 px-2 py-1.5 text-2xs">
        {next.kind === "ready" ? <ICON.ok size={12} aria-hidden className="shrink-0 text-ok" /> : <ICON.forward size={12} aria-hidden className="shrink-0 text-text-dim" />}
        <span className={next.kind === "ready" ? "text-text" : "text-text-dim"}>{next.text}</span>
        <span className="flex-1" />
        {next.action === "profile" && <SettingsLink tab="git">{t("settings-key-card-use-profile")}</SettingsLink>}
        {next.action === "host" && <AddToHost card={card} onAddToHost={onAddToHost} primary />}
      </div>

      <div className="flex flex-wrap items-end gap-2">
        <form
          className="flex flex-wrap items-end gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            if (host.trim()) onCheck(card.name, host.trim(), card.path);
          }}
        >
          <label className="flex flex-col gap-0.5 text-2xs text-text-dim">{t("settings-key-card-check")}<TextInput list={hostId} value={host} className="w-40 font-mono" aria-label={t("settings-key-card-host-check", { card: card.name })} onChange={(e) => setHost(e.target.value)} />
            <datalist id={hostId}>
              {hosts.map((h) => (
                <option key={h} value={h} />
              ))}
            </datalist>
          </label>
          <Button size="sm" type="submit" variant={next.action === "check" ? "default" : "ghost"} disabled={checking || !host.trim()}>
            {checking ? t("settings-mcp-panel-checking-2") : t("settings-code-host-panel-check")}
          </Button>
        </form>
        <span className="flex-1" />
        {next.action !== "host" && <AddToHost card={card} onAddToHost={onAddToHost} />}
        {!card.loaded && agentAvailable && (
          <Button size="sm" variant="ghost" disabled={loading} onClick={() => onLoad(card.name)}>
            {loading ? t("settings-key-card-loading") : t("settings-key-card-load-into-ssh-agent")}
          </Button>
        )}
        {next.action !== "profile" && <SettingsLink tab="git">{t("settings-key-card-use-profile")}</SettingsLink>}
      </div>
    </li>
  );
}

/** The menu of hosts with a public SSH-keys page: copy the key, open the page. */
function AddToHost({ card, onAddToHost, primary = false }: { card: KeyCardWords; onAddToHost: (card: KeyCardWords, host: string) => void; primary?: boolean }) {
  return (
    <Menu
      label={t("settings-key-card-add-host-2")}
      items={hostPages().map(({ host }) => ({
        label: host,
        icon: ICON.open,
        onSelect: () => onAddToHost(card, host),
      }))}
      trigger={
        // `primary` marks the card's next step: raised out of ghost, but a
        // per-card action never wears the screen's one primary.
        <Button size="sm" variant={primary ? "default" : "ghost"}>{t("settings-key-card-add-host")}</Button>
      }
    />
  );
}
