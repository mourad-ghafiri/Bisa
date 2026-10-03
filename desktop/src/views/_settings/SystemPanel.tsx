/**
 * Settings › Capabilities › System (ide/13, ide/01): what this Mac lets the
 * desktop app do — which notifications reach you, Full Disk Access and, for
 * voice mode later, the microphone. **Notifications** first: the OS's
 * permission (the plugin's word, asked for once with *Allow*), then the
 * master switch and one switch per category of what happened — asks,
 * failures, finished work, workflows, addons — the `notifications.*` machine
 * settings the shell's door follows (`shell/notifySettings.ts`); the words
 * are `notificationsSettingsModel.mjs`'s. Then the two grants: two facts per
 * grant, shown as two: what **macOS** says (the shell's
 * `system_permission_status`, never a prompt) and what **the platform** may
 * rely on (the `system.*` machine setting, off by default). Turning a switch on
 * asks for the grant when it is missing; turning it off revokes nothing — only
 * System Settings can — and the card says so. The words are
 * `systemPermissionsModel.mjs`'s.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, inDesktopShell } from "../../api";
import { useResolvedSettingsRead, useSettingsRegistry } from "../../shell/settingsStore";
import type { SettingDef } from "../../types";
import { Button, Card, Chip, Dot, ErrorNote, ICON, Pending, ReadLine, failureText, cn, useToast } from "../../ui";
import { permissionStatus, requestPermission } from "../../shell/systemPermissions";
import type { PermissionKind, PermissionReport } from "../../shell/systemPermissions";
import { attempt, useAsync } from "../_work/useAsync";
import { firstFailure, pendingRows, phaseOf, readWords } from "./loadModel.mjs";
import { SettingControl } from "./SettingControl";
import { KINDS, recommendation, requestVerb, settingKeyOf, statusWords, titleOf, unavailableWords } from "./systemPermissionsModel.mjs";
import { CATEGORY_KEYS, MASTER_KEY, askVerb, iconWords, permissionOf, permissionWords } from "./notificationsSettingsModel.mjs";
import type { NotifyPermission } from "./notificationsSettingsModel.mjs";
import { t } from "../../i18n/l10n.mjs";

const GLYPH: Record<PermissionKind, typeof ICON.system> = {
  full_disk_access: ICON.fullDiskAccess,
  microphone: ICON.microphone,
};

export function SystemPanel() {
  const registry = useSettingsRegistry();
  const resolved = useResolvedSettingsRead(null);
  const defs = useMemo(() => new Map((registry.data?.settings ?? []).map((d) => [d.key, d])), [registry.data]);
  const values = useMemo(() => new Map((resolved.data?.settings ?? []).map((r) => [r.key, r.value])), [resolved.data]);
  const desktop = inDesktopShell();

  // Both reads gate the switches: a switch drawn *off* before the resolved
  // values land would be a lie for a beat (ide/13 §Every panel reads the same way).
  const state = phaseOf([registry, resolved]);
  if (state === "failed") {
    return (
      <ErrorNote
        error={firstFailure([registry, resolved]) ?? t("settings-keymap-panel-settings-read-refused")}
        retry={() => {
          registry.reload();
          resolved.reload();
        }}
      />
    );
  }
  if (state === "pending") return <Pending what={t("settings-registry-panel-settings")} rows={pendingRows(t("settings-registry-panel-settings"))} />;

  const master = defs.get(MASTER_KEY);
  const categories = CATEGORY_KEYS.map((key) => defs.get(key)).filter((d): d is SettingDef => Boolean(d));
  return (
    <div className="flex flex-col gap-4">
      {!desktop && <p className="text-2xs text-text-dim">{unavailableWords("not_desktop")}</p>}
      {master && <NotificationsCard master={master} categories={categories} values={values} desktop={desktop} onWrote={resolved.reload} />}
      {KINDS.map((kind) => {
        const def = defs.get(settingKeyOf(kind));
        return def ? <GrantCard key={kind} kind={kind} def={def} enabled={Boolean(values.get(def.key))} desktop={desktop} onWrote={resolved.reload} /> : null;
      })}
    </div>
  );
}

/** The OS's notification permission, as the plugin answers it — or unavailable off the desktop app. */
async function readPermission(desktop: boolean): Promise<NotifyPermission> {
  if (!desktop) return "unavailable";
  const n = await import("@tauri-apps/plugin-notification");
  // A refusal is the webview's own word: the plugin answers granted or not, and *not* is two things.
  return permissionOf(await n.isPermissionGranted(), typeof Notification === "undefined" ? null : Notification.permission);
}

/**
 * Which notifications reach this person: the OS's permission, then the
 * master and one switch per category. The categories dim while the master
 * is off — they still say what they would let through.
 */
function NotificationsCard({ master, categories, values, desktop, onWrote }: { master: SettingDef; categories: SettingDef[]; values: Map<string, unknown>; desktop: boolean; onWrote: () => void }) {
  const toast = useToast();
  const [writing, setWriting] = useState<string | null>(null);
  const [permission, setPermission] = useState<NotifyPermission | null>(null);
  const read = useCallback(() => void readPermission(desktop).then(setPermission, () => setPermission("unavailable")), [desktop]);
  // The person comes back from System Settings: read the permission again.
  useEffect(() => {
    read();
    if (!desktop) return;
    window.addEventListener("focus", read);
    return () => window.removeEventListener("focus", read);
  }, [desktop, read]);
  const enabled = values.get(master.key) !== false;
  const words = permissionWords(permission, enabled);
  const verb = desktop ? askVerb(permission) : null;

  const allow = async () => {
    try {
      const n = await import("@tauri-apps/plugin-notification");
      const answer = await n.requestPermission();
      setPermission(permissionOf(false, answer));
      if (answer === "granted") toast.ok(t("settings-notifications-allowed"));
    } catch (e) {
      toast.error(failureText("settings", "system-panel-failed", e));
    }
  };
  const write = async (def: SettingDef, on: unknown) => {
    setWriting(def.key);
    await attempt(() => api.setSettings("machine", { [def.key]: Boolean(on) }), toast.error, onWrote);
    setWriting(null);
  };

  return (
    <Card className="flex flex-col gap-2" data-notifications-card>
      <div className="flex flex-wrap items-center gap-2">
        <Dot tone={words.tone === "ok" ? "ok" : words.tone === "warn" ? "warn" : "neutral"} title={words.label} />
        <ICON.notification size={14} aria-hidden className="shrink-0 text-text-dim" />
        <span className="text-xs font-medium">{t("settings-notifications-title")}</span>
        {/* The sentence is the line under the row, not a tooltip a keyboard never reaches. */}
        <Chip tone={words.tone}>{words.label}</Chip>
        <span className="flex-1" />
        {verb && (
          <Button size="sm" onClick={() => void allow()}>
            {verb}
          </Button>
        )}
      </div>
      <p className="text-2xs text-text-dim">{words.sentence}</p>
      <SettingControl def={master} value={enabled} onChange={(v) => void write(master, v)} disabled={writing !== null} />
      <p className="text-2xs text-text-dim">{master.help}</p>
      <div className={cn("flex flex-col gap-2 border-t border-hairline pt-2", !enabled && "opacity-60")} aria-disabled={!enabled}>
        {categories.map((def) => (
          <div key={def.key} className="flex flex-col gap-0.5">
            <SettingControl def={def} value={values.get(def.key) ?? def.default} onChange={(v) => void write(def, v)} disabled={writing !== null || !enabled} />
            <p className="text-2xs text-text-dim">{def.help}</p>
          </div>
        ))}
      </div>
      <p className="rounded-control bg-surface-2/50 p-2 text-2xs leading-relaxed text-text-dim">{iconWords()}</p>
    </Card>
  );
}

function GrantCard({ kind, def, enabled, desktop, onWrote }: { kind: PermissionKind; def: SettingDef; enabled: boolean; desktop: boolean; onWrote: () => void }) {
  const toast = useToast();
  const [writing, setWriting] = useState(false);
  const report = useAsync<PermissionReport | null>(() => (desktop ? permissionStatus(kind) : Promise.resolve(null)), [kind, desktop]);
  // The person comes back from System Settings: read the grant again. The reload is a stable door (`useAsync`).
  const reloadReport = report.reload;
  useEffect(() => {
    if (!desktop) return;
    const again = () => reloadReport();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [desktop, kind, reloadReport]);

  const status = report.data?.status ?? null;
  // The grant is asked of macOS: until it answers, the card says so rather than *off*.
  const asking = desktop && report.data === null && !report.error;
  const grant = asking ? null : readWords({ what: t("settings-system-panel-grant"), refreshing: report.refreshing, error: report.error, at: report.at, data: report.data }, Date.now() / 1000);
  const words = statusWords(kind, desktop ? status : "unsupported", enabled);
  const verb = desktop ? requestVerb(kind, status) : null;
  const rec = recommendation(kind);
  const Glyph = GLYPH[kind];

  const ask = async () => {
    await attempt(() => requestPermission(kind), toast.error, (r) => {
      if (r?.status === "granted") toast.ok(t("settings-system-panel-granted", { kind: titleOf(kind) }));
      report.reload();
    });
  };
  const toggle = async (on: unknown) => {
    setWriting(true);
    await attempt(() => api.setSettings("machine", { [def.key]: Boolean(on) }), toast.error, () => {
      onWrote();
      // On, and macOS has not granted it: ask now, the way the switch promised.
      if (on && desktop && status !== "granted" && status !== "unsupported") void ask();
    });
    setWriting(false);
  };

  return (
    <Card className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Dot tone={words.tone === "ok" ? "ok" : words.tone === "warn" ? "warn" : "neutral"} title={words.label} />
        <Glyph size={14} aria-hidden className="shrink-0 text-text-dim" />
        <span className="text-xs font-medium">{titleOf(kind)}</span>
        {asking ? (
          <Chip tone="quiet">{t("settings-system-panel-asking-macos")}</Chip>
        ) : (
          <Chip tone={words.tone}>{words.label}</Chip>
        )}
        <span className="flex-1" />
        {desktop && <ReadLine words={grant} busy={report.loading || report.refreshing} onReload={report.reload} reloadLabel={t("settings-code-host-panel-check-again")} />}
        {verb && (
          <Button size="sm" onClick={() => void ask()}>
            {verb}
          </Button>
        )}
      </div>
      {asking ? <Pending what={t("settings-system-panel-grant")} rows={0} /> : <p className="text-2xs text-text-dim">{words.sentence}</p>}
      {report.data?.detail && <p className="text-2xs text-text-dim">{unavailableWords(report.data.detail)}</p>}
      <SettingControl def={def} value={enabled} onChange={(v) => void toggle(v)} disabled={writing} />
      <p className="text-2xs text-text-dim">{def.help}</p>
      <div className="flex flex-col gap-1 rounded-control bg-surface-2/50 p-2 text-2xs leading-relaxed text-text-dim">
        <p>{rec.why}</p>
        <p>{rec.safety}</p>
        {rec.how && <p>{rec.how}</p>}
      </div>
    </Card>
  );
}
