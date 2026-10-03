/**
 * The coding agents this machine can actually run.
 *
 * Model lists come from the harness itself. An empty list means *unknown*,
 * never *unsupported* — so it says so, rather than implying the harness has
 * no models and quietly stopping you from typing one.
 */

import { api, openExternal } from "../../api";
import type { HarnessRow, ModelInfo } from "../../types";
import { Button, Card, Chip, Dot, EmptyState, ErrorNote, ICON, Pending, ReadLine, Section, Tooltip, copyText, harnessMark, useToast } from "../../ui";
import { commandsFor, platformOf } from "../../shell/setupModel.mjs";
import { HarnessUsageLine, UsageToggle } from "../../shell/HarnessUsageLine";
import { useAsync } from "../_work/useAsync";
import { pendingRows, phase, readWords } from "./loadModel.mjs";
import { harnessGuardWords } from "./securityRules.mjs";
import { t } from "../../i18n/l10n.mjs";

interface Probed {
  harnesses: HarnessRow[];
  models: Record<string, ModelInfo[] | null>;
}

/** null = the probe failed or returned nothing; we do not pretend otherwise. */
async function probe(signal: AbortSignal): Promise<Probed> {
  const { harnesses } = await api.harnesses(signal);
  const installed = harnesses.filter((h) => h.installed);
  const pairs = await Promise.all(
    installed.map(async (h) => {
      try {
        const r = await api.models(h.id, signal);
        return [h.id, r.models.length > 0 ? r.models : null] as const;
      } catch {
        return [h.id, null] as const;
      }
    }),
  );
  return { harnesses, models: Object.fromEntries(pairs) };
}

function Row({ h, models }: { h: HarnessRow; models: ModelInfo[] | null | undefined }) {
  const guard = harnessGuardWords(h);
  const Mark = harnessMark(h.id);
  return (
    <Card className={h.installed ? "" : "opacity-70"}>
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <Dot tone={h.installed ? "ok" : "neutral"} title={h.installed ? t("settings-harnesses-panel-is-installed") : t("settings-harnesses-panel-installed-2")} />
        <Mark size={14} tone="brand" aria-hidden className="shrink-0" />
        <span className="text-xs font-medium whitespace-nowrap">{h.label}</span>
        <Chip tone="neutral">{h.tier}</Chip>
        {/* Whether the Tool & Commands Guard can stop a call before it runs — Settings › Security says the same. */}
        <Tooltip
          label={
            h.tool_guard
              ? t("settings-harnesses-panel-harness-asks-node-before-tool-runs")
              : t("settings-harnesses-panel-harness-runs-under-own-sandbox-only")
          }
        >
          <span>
            <Chip tone={guard.tone} icon={h.tool_guard ? ICON.guard : undefined}>
              {guard.text}
            </Chip>
          </span>
        </Tooltip>
        <span className="tnum ml-auto truncate text-2xs text-text-dim">
          {h.version ?? (h.installed ? t("settings-harnesses-panel-is-installed") : (h.detail ?? t("settings-harnesses-panel-installed-2")))}
        </span>
        {h.installed && <UsageToggle harness={h.id} />}
      </div>

      {/* What the account has left — the same line the harness rows carry, read only while shown. */}
      {h.installed && <HarnessUsageLine harness={h.id} className="mt-1.5" />}

      {h.installed ? (
        <div className="mt-1.5 flex flex-wrap items-center gap-1">
          {models === undefined ? (
            <span className="text-2xs text-text-dim">{t("settings-harnesses-panel-asking")}</span>
          ) : models ? (
            models.slice(0, 6).map((m) => (
              <Chip key={m.id} tone="neutral" title={m.id}>
                {m.label ?? m.id}
              </Chip>
            ))
          ) : (
            <span className="text-2xs text-text-dim">{t("settings-harnesses-panel-does-list-models-type-one-hand")}</span>
          )}
          {models && models.length > 6 && (
            <span className="text-2xs text-text-dim">{t("settings-harnesses-panel-more-models", { more: models.length - 6 })}</span>
          )}
        </div>
      ) : (
        <InstallWords h={h} />
      )}
    </Card>
  );
}

/**
 * How a missing harness is installed: this platform's official lines to
 * copy and the page they come from (16 — The setup gate), else the bare hint
 * a preset or a custom descriptor carries.
 */
function InstallWords({ h }: { h: HarnessRow }) {
  const toast = useToast();
  const lines = commandsFor(h.install, platformOf(typeof navigator === "undefined" ? null : navigator));
  if (!h.install) return h.install_hint ? <p className="mt-1.5 text-2xs text-text-dim">{h.install_hint}</p> : null;
  return (
    <div className="mt-1.5 flex flex-col gap-1">
      {lines.map((c) => (
        <div key={c} className="flex items-center gap-2">
          <code className="min-w-0 flex-1 truncate font-mono text-2xs text-text-dim" title={c}>
            {c}
          </code>
          <Button size="sm" variant="ghost" onClick={() => void copyText(c).then((ok) => (ok ? toast.ok(t("settings-harnesses-panel-copied")) : toast.error(t("settings-harnesses-panel-clipboard-refused"))))}>{t("settings-harnesses-panel-copy")}</Button>
        </div>
      ))}
      <div>
        <Button size="sm" variant="ghost" onClick={() => void openExternal(h.install!.url).catch(() => toast.error(t("settings-harnesses-panel-browser-did-open")))}>
          <ICON.open size={11} aria-hidden />{t("settings-harnesses-panel-open-official-docs")}</Button>
      </div>
    </div>
  );
}

export function HarnessesPanel() {
  const probed = useAsync(probe, []);
  const { data, error, reload } = probed;

  // The probe runs every harness's `--version`: the first answer is a
  // `Pending` where the rows go; a re-read keeps the rows and says so.
  const state = phase(probed);
  if (state === "pending") return <Pending what={t("settings-harnesses-panel-harnesses")} rows={pendingRows(t("settings-harnesses-panel-harnesses"))} />;
  if (state === "failed") return <ErrorNote error={error ?? t("settings-harnesses-panel-probe-refused")} retry={reload} />;
  const status = readWords({ what: t("settings-harnesses-panel-harnesses"), refreshing: probed.refreshing, error, at: probed.at, data }, Date.now() / 1000);

  const rows = data?.harnesses ?? [];
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={ICON.harness}
        title={t("settings-harnesses-panel-harnesses-found")}
        hint={t("settings-harnesses-panel-install-claude-code-codex-opencode-any")}
        action={<Button onClick={reload}>{t("settings-harnesses-panel-probe-again")}</Button>}
      />
    );
  }

  const installed = rows.filter((h) => h.installed);
  const missing = rows.filter((h) => !h.installed);

  return (
    <div className="flex flex-col gap-6">
      <ReadLine words={status} busy={probed.refreshing} onReload={reload} reloadLabel={t("settings-harnesses-panel-probe-again")} />
      <Section
        title={t("settings-harnesses-panel-available", { installed: installed.length })}
        action={
          <Button size="sm" variant="ghost" disabled={probed.refreshing} onClick={reload}>
            {t("settings-harnesses-panel-probe-again")}
          </Button>
        }
      >
        <div className="flex flex-col gap-2">
          {installed.map((h) => (
            <Row key={h.id} h={h} models={data?.models[h.id]} />
          ))}
        </div>
      </Section>

      {missing.length > 0 && (
        <Section title={t("settings-harnesses-panel-installed", { missing: missing.length })}>
          <div className="flex flex-col gap-2">
            {missing.map((h) => (
              <Row key={h.id} h={h} models={undefined} />
            ))}
          </div>
        </Section>
      )}
    </div>
  );
}
