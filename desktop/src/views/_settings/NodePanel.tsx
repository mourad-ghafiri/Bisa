/**
 * The engine and the process hosting it.
 *
 * Pausing parks every loop at a safe boundary; nothing is aborted and no work
 * is lost. `GET /pause` reads the flag — the chip says where the engine
 * stands the moment the panel opens — and the two buttons write it; the
 * answer to a write is what the chip then says.
 */

import { useEffect, useState } from "react";
import { api } from "../../api";
import { Button, Card, Chip, ReadLine, Section, useToast } from "../../ui";
import { attempt, useAsync } from "../_work/useAsync";
import { bootLine } from "../../shell/nodeBootModel.mjs";
import { useNodeBoot } from "../../shell/nodeBootStore";
import { useWorkspace } from "../../shell/useWorkspaceData";
import { readWords } from "./loadModel.mjs";
import { t, tx } from "../../i18n/l10n.mjs";

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function NodePanel() {
  const toast = useToast();
  const health = useAsync((s) => api.health(s), []);
  const { data, error, reload } = health;
  const checking = health.loading || health.refreshing;
  // The chip carries the first check; the line speaks of re-reads and of when the last answer came.
  const status = readWords({ what: t("settings-node-panel-node-2"), refreshing: health.refreshing, error, at: health.at, data }, Date.now() / 1000);
  const [paused, setPaused] = useState<boolean | null>(null);
  const flag = useAsync((s) => api.paused(s), []);
  useEffect(() => {
    if (flag.data) setPaused(flag.data.paused);
  }, [flag.data]);
  const [busy, setBusy] = useState(false);
  // The shell's own account of its node while it is away: the boot's phase,
  // or why it is not running and when the next try comes.
  const boot = useNodeBoot();
  const account = bootLine(boot);
  // What the last open worked around (`GET /workspace`): a file moved under
  // quarantine/, a record skipped, a run ended. Nothing when the workspace
  // is sound — most of the time, the section is not there.
  const problems = useWorkspace().info?.problems ?? [];

  const setPause = async (want: boolean) => {
    setBusy(true);
    await attempt(
      async () => {
        const r = want ? await api.pause() : await api.resume();
        setPaused(r.paused);
        toast.ok(r.paused ? t("settings-node-panel-engine-paused") : t("settings-node-panel-engine-running"));
      },
      toast.error,
    );
    setBusy(false);
  };

  const restart = async () => {
    if (!isTauri()) {
      toast.info(t("settings-node-panel-sidecar-only-managed-desktop-app"));
      return;
    }
    await attempt(
      async () => {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("restart_node");
        toast.ok(t("settings-node-panel-node-restarting"));
        setTimeout(reload, 1500);
      },
      toast.error,
    );
  };

  return (
    <div className="flex flex-col gap-6">
      <Section title={t("settings-node-panel-engine")}>
        <Card>
          <div className="flex items-center gap-2">
            <Button disabled={busy} onClick={() => void setPause(true)}>{t("settings-node-panel-pause")}</Button>
            <Button disabled={busy} onClick={() => void setPause(false)}>{t("settings-node-panel-resume")}</Button>
            {paused !== null && (
              <Chip tone={paused ? "warn" : "ok"}>{paused ? t("settings-node-panel-paused") : t("settings-node-panel-running")}</Chip>
            )}
          </div>
          <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-node-panel-pausing-parks-every-loop-safe-boundary")}</p>
        </Card>
      </Section>

      <Section title={t("settings-node-panel-node")}>
        <Card>
          <div className="flex items-center gap-2">
            <Chip tone={checking ? "quiet" : data?.ok ? "ok" : error ? "danger" : "quiet"}>
              {checking ? t("settings-system-permissions-checking") : data?.ok ? t("settings-node-panel-healthy") : error ? t("settings-node-panel-unreachable") : t("settings-code-host-panel-read")}
            </Chip>
            {data?.version && <Chip tone="quiet">{t("settings-node-panel-v", { version: data.version })}</Chip>}
            <span className="ml-auto">
              <ReadLine words={status} busy={checking} onReload={reload} reloadLabel={t("settings-code-host-panel-check")} />
            </span>
            <Button size="sm" onClick={() => void restart()}>{t("settings-node-panel-restart-sidecar")}</Button>
          </div>
          {account && (
            <p role="status" className={`mt-2 max-w-measure text-2xs leading-relaxed ${boot.failure ? "text-danger" : "text-text-dim"}`}>
              {account}
            </p>
          )}
          <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">
            {isTauri()
              ? t("settings-node-panel-app-runs-node-child-process-restarting")
              : t("settings-node-panel-build-talks-node-started-yourself-so")}
          </p>
        </Card>
      </Section>

      {problems.length > 0 && (
        <Section title={t("settings-node-panel-workspace")}>
          <Card>
            <div className="flex items-center gap-2">
              <Chip tone="warn">{t("settings-node-panel-workspace-problems-count", { n: problems.length })}</Chip>
            </div>
            <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-node-panel-workspace-problems-lead")}</p>
            <ul className="mt-2 flex flex-col divide-y divide-hairline" aria-label={t("settings-node-panel-workspace")}>
              {problems.map((p, i) => (
                <li key={`${p.kind}:${p.path}:${i}`} className="py-2 first:pt-0 last:pb-0">
                  <p className="max-w-measure text-2xs leading-relaxed text-text">{tx(p.text)}</p>
                  <p className="mt-0.5 break-all font-mono text-3xs text-text-dim">{p.path}</p>
                  {p.quarantined && <p className="mt-0.5 break-all font-mono text-3xs text-text-dim">{t("settings-node-panel-workspace-moved-to", { path: p.quarantined })}</p>}
                </li>
              ))}
            </ul>
            <p className="mt-2 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-node-panel-workspace-problems-check")}</p>
          </Card>
        </Section>
      )}
    </div>
  );
}
