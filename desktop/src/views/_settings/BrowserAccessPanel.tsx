/**
 * Settings › Capabilities › Browser: the embedded browser's switch and who
 * may drive it (ide/18). Two registry keys drawn by hand — `browser.enabled`
 * as a switch on the status card, since it is this machine's and says at
 * once whether there is a browser here at all; `browser.agents` as the same
 * three-way switch the policy is, with the policy's own sentence under it
 * instead of the raw token the generic control would print;
 * `browser.agents.headless` the same way — when an agent's tab is kept out
 * of sight (ide/18 §Headless tabs); and `browser.agents.scripts` — whether
 * an agent may evaluate a script in a page (browser_eval). The rest of the
 * group — the home page, what a restart remembers, the reach, the
 * screenshot's width — is the registry panel's under this one.
 *
 * `browser.enabled` is written at the machine scope, the two policies at
 * the workspace's — a project overrides them in its own settings, as the
 * origin badge says.
 */

import { useState } from "react";
import { api } from "../../api";
import { browserAvailable } from "../../browser/session";
import { useBrowserPrefs } from "../../shell/browserPrefsStore";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { useBrowsers } from "../../shell/useBrowsers";
import { boolOf, choiceOf, stringOf } from "../../shell/settingsModel.mjs";
import { Button, Card, Chip, ErrorNote, ICON, Pending, SegmentedControl, Section, Switch, useToast } from "../../ui";
import { AGENTS_KEY, DEFAULT_HEADLESS, DEFAULT_POLICY, DEFAULT_REACH, DEFAULT_SCRIPTS, ENABLED_KEY, HEADLESS_KEY, HEADLESS_MODES, POLICIES, REACH_KEY, SCRIPTS_KEY, SCRIPTS_MODES, headlessSegments, headlessWords, policySegments, policyWords, reachWords, scriptsSegments, scriptsWords, statusWords } from "./browserSettingsModel.mjs";
import type { BrowserPolicy, HeadlessMode, ScriptsMode } from "./browserSettingsModel.mjs";
import { pendingRows } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { OriginBadge } from "./OriginBadge";

export function BrowserAccessPanel() {
  const toast = useToast();
  const { resolved, error, reload } = useResolvedSettings(null);
  const { sessions } = useBrowsers();
  useBrowserPrefs();
  const [busy, setBusy] = useState(false);
  const enabled = boolOf(resolved, ENABLED_KEY, true);
  const policy = choiceOf(resolved, AGENTS_KEY, POLICIES, DEFAULT_POLICY);
  const reach = stringOf(resolved, REACH_KEY, DEFAULT_REACH);
  const headless = choiceOf(resolved, HEADLESS_KEY, HEADLESS_MODES, DEFAULT_HEADLESS);
  const scripts = choiceOf(resolved, SCRIPTS_KEY, SCRIPTS_MODES, DEFAULT_SCRIPTS);
  const origin = resolved?.find((s) => s.key === AGENTS_KEY)?.origin ?? "default";
  const headlessOrigin = resolved?.find((s) => s.key === HEADLESS_KEY)?.origin ?? "default";
  const scriptsOrigin = resolved?.find((s) => s.key === SCRIPTS_KEY)?.origin ?? "default";
  const status = statusWords({ available: browserAvailable(), enabled, tabs: sessions.length, headless: sessions.filter((s) => s.headless).length });
  const options = policySegments().map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon as keyof typeof ICON] }));
  const headlessOptions = headlessSegments().map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon as keyof typeof ICON] }));
  const scriptsOptions = scriptsSegments().map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon as keyof typeof ICON] }));

  const write = async (scope: "machine" | "workspace", key: string, value: unknown) => {
    if (busy) return;
    setBusy(true);
    try {
      await api.setSettings(scope, { [key]: value });
      reload();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const reset = async (key: string = AGENTS_KEY) => {
    setBusy(true);
    try {
      await api.unsetSetting("workspace", key);
      reload();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Section title={t("settings-browser-access-panel-embedded-browser")}>
        <Card>
          {!resolved && !error && <Pending what={t("settings-browser-access-panel-resolved-values")} rows={pendingRows(t("settings-browser-access-panel-resolved-values"))} />}
          {!resolved && error && <ErrorNote error={error} retry={reload} />}
          {resolved && (
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <div className="mb-1 flex items-center gap-2">
                  <Chip tone={status.tone}>{status.label}</Chip>
                  <span className="text-2xs text-text-dim">{status.sentence}</span>
                </div>
                <Switch
                  checked={enabled}
                  disabled={busy || !browserAvailable()}
                  onChange={(v) => void write("machine", ENABLED_KEY, v)}
                  label={t("settings-browser-access-panel-embedded-browser-machine")}
                  hint={t("settings-browser-access-panel-off-tab-opens-every-launcher-says")}
                />
              </div>
            </div>
          )}
        </Card>
      </Section>
      <Section title={t("settings-browser-access-panel-which-agents-may-drive")}>
        <Card>
          {resolved && (
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-browser-access-panel-browser-tools-answer")}</span>
                <SegmentedControl options={options} value={policy} onChange={(v: BrowserPolicy) => void write("workspace", AGENTS_KEY, v)} label={t("settings-browser-access-panel-which-agents-may-use-browser")} size="sm" />
                <span className="mt-1 block text-2xs text-text-dim">{policyWords(policy)}</span>
                <span className="mt-1 block text-2xs text-text-dim">{reachWords(reach)}</span>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1.5">
                <OriginBadge origin={origin} />
                <span className="text-2xs text-text-dim">{t("settings-browser-access-panel-set-workspace")}</span>
                {origin === "workspace" && (
                  <Button size="sm" variant="ghost" disabled={busy} onClick={() => void reset()}>{t("settings-appearance-panel-reset")}</Button>
                )}
              </div>
            </div>
          )}
        </Card>
      </Section>
      <Section title={t("settings-browser-access-panel-agents-tabs-out-sight")}>
        <Card>
          {resolved && (
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-browser-access-panel-tab-agent-opens-kept-out-sight")}</span>
                <SegmentedControl options={headlessOptions} value={headless} onChange={(v: HeadlessMode) => void write("workspace", HEADLESS_KEY, v)} label={t("settings-browser-access-panel-when-agent-s-tab-kept-out")} size="sm" />
                <span className="mt-1 block text-2xs text-text-dim">{headlessWords(headless)}</span>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1.5">
                <OriginBadge origin={headlessOrigin} />
                <span className="text-2xs text-text-dim">{t("settings-browser-access-panel-set-workspace")}</span>
                {headlessOrigin === "workspace" && (
                  <Button size="sm" variant="ghost" disabled={busy} onClick={() => void reset(HEADLESS_KEY)}>{t("settings-appearance-panel-reset")}</Button>
                )}
              </div>
            </div>
          )}
        </Card>
      </Section>
      <Section title={t("settings-browser-access-panel-scripts-page")}>
        <Card>
          {resolved && (
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-browser-access-panel-agent-may-evaluate-script-tab")}</span>
                <SegmentedControl options={scriptsOptions} value={scripts} onChange={(v: ScriptsMode) => void write("workspace", SCRIPTS_KEY, v)} label={t("settings-browser-access-panel-whether-agent-may-run-script-page")} size="sm" />
                <span className="mt-1 block text-2xs text-text-dim">{scriptsWords(scripts)}</span>
              </div>
              <div className="flex shrink-0 flex-col items-end gap-1.5">
                <OriginBadge origin={scriptsOrigin} />
                <span className="text-2xs text-text-dim">{t("settings-browser-access-panel-set-workspace")}</span>
                {scriptsOrigin === "workspace" && (
                  <Button size="sm" variant="ghost" disabled={busy} onClick={() => void reset(SCRIPTS_KEY)}>{t("settings-appearance-panel-reset")}</Button>
                )}
              </div>
            </div>
          )}
        </Card>
      </Section>
    </>
  );
}
