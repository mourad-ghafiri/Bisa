/**
 * Settings › Git & code hosts › SSH keys (ide/04, ide/13): one card per
 * public key with its next step, ssh-agent with its remedy, the git hosts of
 * your ssh config, and two dialogs — a new key, a Host block to paste. The
 * path a key walks — onto the host, checked there, bound in a profile — is
 * the card's, one step at a time. Public material only: the node reads
 * `.pub` files and `ssh -G`, never a private key; nothing here writes your
 * ssh config or known_hosts. The words are `sshModel.mjs`'s.
 */

import { useState } from "react";
import { api, openExternal } from "../../api";
import { useEngineEvents } from "../../bus";
import { copyText } from "../../ui/clipboard";
import { Button, Card, Chip, CopyText, EmptyState, ErrorNote, ICON, Pending, ReadLine, Section, TextInput, useToast } from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { pendingRows, phase, readWords } from "./loadModel.mjs";
import { GenerateDialog } from "./ssh/GenerateDialog";
import { HostBlockDialog } from "./ssh/HostBlockDialog";
import { KeyCard } from "./ssh/KeyCard";
import type { KeyCardWords, KeyChecks } from "./sshModel.mjs";
import { agentWords, busyKey, copiedForHostWords, generatedWords, hostChoices, hostKeyPage, hostRows, keyCards, loadedWords, rememberCheck, resolvedWords } from "./sshModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

const WHAT = t("settings-git-profiles-panel-ssh-keys");

export function SshPanel() {
  const toast = useToast();
  const view = useAsync((s) => api.sshOverview(s), []);
  useEngineEvents((e) => {
    if (e.payload.type === "git_setup_changed") view.reload();
  });
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [checks, setChecks] = useState<KeyChecks>({});
  const [fresh, setFresh] = useState<string | null>(null);
  const [generating, setGenerating] = useState(false);
  const [blockDialog, setBlockDialog] = useState(false);
  const [askHost, setAskHost] = useState("");
  const [resolved, setResolved] = useState<{ host: string; text: string } | null>(null);

  const state = phase(view);
  if (state === "pending") return <Pending what={WHAT} rows={pendingRows(WHAT)} />;
  if (state === "failed") {
    return (
      <Card className="p-3">
        <ErrorNote error={view.error ?? t("settings-ssh-panel-ssh-directory-read")} retry={view.reload} />
        <p className="mt-2 text-2xs text-text-dim">{rich("settings-ssh-panel-read-by-node-blurb", { code: (inner) => <code className="font-mono">{inner}</code> })}</p>
      </Card>
    );
  }
  const data = view.data!;
  const cards = keyCards(data, checks);
  const agent = agentWords(data.agent);
  const hosts = hostChoices(data);
  const blocks = hostRows(data.hosts);
  const status = readWords({ what: WHAT, refreshing: view.refreshing, error: view.error, at: view.at, data }, Date.now() / 1000);

  /** One action at a time per key, never a lock on the whole panel. */
  const run = async <T,>(mark: string, fn: () => Promise<T>, onDone: (value: T) => void) => {
    setBusy((b) => new Set(b).add(mark));
    await attempt(fn, toast.error, onDone);
    setBusy((b) => {
      const next = new Set(b);
      next.delete(mark);
      return next;
    });
  };
  const load = (name: string) =>
    run(busyKey("load", name), () => api.sshLoad(name), () => {
      toast.ok(loadedWords(name));
      view.reload();
    });
  const check = (name: string, host: string, path: string) =>
    run(
      busyKey("check", name),
      () => api.sshTest(host, "git", path),
      (greeting) => setChecks((c) => rememberCheck(c, name, host, greeting, Date.now() / 1000)),
    );
  const addToHost = async (card: KeyCardWords, host: string) => {
    const page = hostKeyPage(host);
    if (!page) return;
    const copied = await copyText(card.publicLine);
    if (!copied) {
      toast.error(t("settings-ssh-panel-clipboard-refused-public-key-use-copy"));
      return;
    }
    toast.ok(copiedForHostWords(host));
    await openExternal(page).catch((e: unknown) => toast.error(e instanceof Error ? e.message : String(e)));
  };
  const ask = () => {
    const host = askHost.trim();
    if (!host) return;
    void run(busyKey("resolve", host), () => api.sshResolve(host), (r) => setResolved({ host, text: resolvedWords(host, r, cards) }));
  };

  return (
    <div className="flex max-w-3xl flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <ReadLine words={status} busy={view.loading || view.refreshing} onReload={view.reload} reloadLabel={t("settings-ssh-panel-read-ssh-directory-again")} />
        <span className="flex-1" />
        <Button size="sm" variant="primary" onClick={() => setGenerating(true)}>{t("settings-ssh-panel-generate-key")}</Button>
      </div>

      {/* ssh-agent: what it holds, and the one remedy when it is down. */}
      <Card className="flex flex-wrap items-center gap-2 p-3">
        <Chip tone={agent.tone === "warn" ? "warn" : agent.tone === "ok" ? "ok" : "quiet"} icon={agent.tone === "ok" ? ICON.ok : agent.tone === "warn" ? ICON.warn : undefined}>
          {/* for the machine: the program's name */}
          ssh-agent
        </Chip>
        <span className={`text-2xs ${agent.tone === "warn" ? "text-warn" : "text-text-dim"}`} role="status">
          {agent.text}
        </span>
        {agent.remedy && (
          <>
            <span className="flex-1" />
            <CopyText value={agent.remedy} label={agent.remedy} />
          </>
        )}
      </Card>

      <Section
        title={t("settings-ssh-panel-keys")}
        action={
          <span className="font-mono text-2xs text-text-dim" title={t("settings-ssh-panel-ssh-directory-node-reads-pub-files")}>
            {String(data.dir)}
          </span>
        }
      >
        {cards.length === 0 ? (
          <EmptyState
            icon={ICON.fingerprint}
            title={t("settings-ssh-panel-ssh-key-git-host-yet")}
            hint={t("settings-ssh-panel-generate-one-organization-card-then-walks")}
            action={
              <Button size="sm" variant="primary" onClick={() => setGenerating(true)}>{t("settings-ssh-panel-generate-key")}</Button>
            }
          />
        ) : (
          <ul className="flex flex-col gap-2" aria-label={t("settings-ssh-panel-keys-2")}>
            {cards.map((card) => (
              <KeyCard key={card.name} card={card} agentAvailable={data.agent.available} hosts={hosts} busy={busy} fresh={fresh === card.name} onLoad={(n) => void load(n)} onCheck={(n, h, p) => void check(n, h, p)} onAddToHost={(c, h) => void addToHost(c, h)} />
            ))}
          </ul>
        )}
      </Section>

      <Section
        title={t("settings-ssh-panel-git-hosts-ssh-config")}
        action={
          <Button size="sm" variant="ghost" onClick={() => setBlockDialog(true)}>{t("settings-ssh-panel-copy-host-block")}</Button>
        }
      >
        <Card className="flex flex-col gap-2 p-3">
          {blocks.length === 0 ? (
            <p className="text-2xs text-text-dim">{rich("settings-ssh-panel-no-git-host-blurb", { code: (inner) => <code className="font-mono">{inner}</code> })}</p>
          ) : (
            <ul className="flex flex-col gap-1 font-mono text-2xs" aria-label={t("settings-ssh-panel-host-blocks")}>
              {blocks.map((h) => (
                <li key={h.patterns} className="flex flex-wrap items-center gap-2 text-text-dim">
                  <span className="text-text">{t("settings-ssh-panel-host", { patterns: h.patterns })}</span>
                  {h.hostname && <span>{t("settings-ssh-panel-arrow-hostname", { hostname: h.hostname })}</span>}
                  {h.user && <span>{t("settings-ssh-panel-user", { user: h.user })}</span>}
                  {h.identity && <span>{t("settings-ssh-panel-key", { identity: h.identity })}</span>}
                  {h.identitiesOnly && <Chip tone="quiet">{t("settings-ssh-panel-key-only")}</Chip>}
                </li>
              ))}
            </ul>
          )}
          <form
            className="flex flex-wrap items-end gap-1.5 border-t border-border pt-2"
            onSubmit={(e) => {
              e.preventDefault();
              ask();
            }}
          >
            <label className="flex flex-col gap-0.5 text-2xs text-text-dim">{t("settings-ssh-panel-what-would-ssh-offer")}<TextInput list="ssh-ask-hosts" value={askHost} /* for the machine */ placeholder={hosts[0] ?? "github.com"} className="w-44 font-mono" aria-label={t("settings-ssh-panel-host-alias-resolve")} onChange={(e) => setAskHost(e.target.value)} />
              <datalist id="ssh-ask-hosts">
                {hosts.map((h) => (
                  <option key={h} value={h} />
                ))}
              </datalist>
            </label>
            <Button size="sm" type="submit" variant="ghost" disabled={!askHost.trim() || busy.has(busyKey("resolve", askHost.trim()))}>{t("settings-ssh-panel-ask")}</Button>
            <span className="text-2xs text-text-dim">{rich("settings-ssh-panel-offline-blurb", { code: (inner) => <code className="font-mono">{inner}</code> })}</span>
          </form>
          {resolved && (
            <p className="text-2xs text-text" role="status">
              {resolved.text}
            </p>
          )}
        </Card>
      </Section>

      {generating && (
        <GenerateDialog
          taken={cards.map((k) => k.name)}
          onClose={() => setGenerating(false)}
          onGenerated={(key) => {
            toast.ok(generatedWords(key));
            setFresh(key.name);
            setGenerating(false);
            view.reload();
          }}
          onError={toast.error}
        />
      )}
      {blockDialog && <HostBlockDialog keys={cards.map((k) => ({ name: k.name, path: k.path }))} hosts={hosts} onClose={() => setBlockDialog(false)} />}
    </div>
  );
}
