/**
 * A settings panel generated from the registry, for the two scopes that are
 * not a project's: the **workspace** and this **machine**.
 *
 * The node serves the registry (`GET /settings/registry`) and the resolved
 * values (`GET /settings/resolved`) — both kept by `shell/settingsStore.ts`
 * across panels — and this renders one control per key
 * in the group it was given. Nothing here knows a key by name: adding a
 * setting in `bisa-core` grows this panel a control, the reference page
 * a row, and the write path its allowed scopes — with no change in this file.
 *
 * Every control carries an **origin badge** — `default`, `machine`,
 * `workspace` — saying which scope the value on screen came from, and a
 * *Set at* choice over the scopes the key allows here (`OriginBadge`,
 * `settingOriginModel.mjs`); a write to a scope a nearer one stands over
 * says whose value still wins. A project's own layer
 * is never read or written from Settings (ide/13): a key that allows project
 * scope is edited for one project in the Project IDE — `ProjectSettingsCard`
 * under About › Settings — where the project is in front of you.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../api";
import { useResolvedSettingsRead, useSettingsRegistry } from "../../shell/settingsStore";
import type { ResolvedSetting, SettingDef, SettingScope } from "../../types";
import { Button, Card, ErrorNote, Field, ICON, Pending, Select, Tooltip, copyText, useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { pendingRows, phase } from "./loadModel.mjs";
import { OriginBadge } from "./OriginBadge";
import { SavedNote, useSavedNote } from "./SavedNote";
import { SettingControl } from "./SettingControl";
import { SAVED_NOTE_MS } from "./settingDraftModel.mjs";
import { defaultTarget, mayReset, scopeWord, setAtWords, shadowWords, writableScopes } from "./settingOriginModel.mjs";
import { t } from "../../i18n/l10n.mjs";

/**
 * The key's own spelling, for a config file or a bug report: a glyph that
 * copies it, the key itself as its tooltip — the row reads by its label, and
 * the raw key is there when it is asked for.
 */
function KeyCopy({ settingKey }: { settingKey: string }) {
  const toast = useToast();
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(false), SAVED_NOTE_MS);
    return () => clearTimeout(id);
  }, [copied]);
  const Glyph = copied ? ICON.check : ICON.copy;
  return (
    <Tooltip label={<span className="font-mono">{settingKey}</span>}>
      <Button
        size="icon"
        variant="ghost"
        className="h-6 w-6"
        aria-label={copied ? t("ui-field-copied") : t("settings-registry-panel-copy-key", { key: settingKey })}
        onClick={() => void copyText(settingKey).then((ok) => (ok ? setCopied(true) : toast.error(t("ui-mermaid-view-clipboard-refused"))))}
      >
        <Glyph size={12} aria-hidden />
      </Button>
    </Tooltip>
  );
}

function Row({ def, resolved, onChanged }: { def: SettingDef; resolved: ResolvedSetting; onChanged: () => void }) {
  const toast = useToast();
  const [target, setTarget] = useState<SettingScope>(() => defaultTarget(def));
  const [resetting, setResetting] = useState(false);
  const [savedAt, markSaved] = useSavedNote();
  const allowed = writableScopes(def);
  // A value held nearer than the scope this row writes to keeps winning: said, so a write that lands does not look like a click that did nothing.
  const shadow = shadowWords(resolved.origin, target);

  // The control is never disabled under its own write: a box that greys out
  // between keystrokes loses the caret. A write that lands says *Saved*.
  const write = async (v: unknown) => {
    const ok = await attempt(() => api.setSettings(target, { [def.key]: v }), toast.error);
    if (ok) {
      markSaved();
      onChanged();
    }
  };
  const reset = async () => {
    setResetting(true);
    const ok = await attempt(() => api.unsetSetting(target, def.key), toast.error);
    setResetting(false);
    if (ok) {
      markSaved();
      onChanged();
    }
  };

  const isBool = def.kind.type === "bool";
  return (
    <div className="flex flex-wrap items-start gap-3 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0 flex-1">
        {isBool ? (
          <SettingControl def={def} value={resolved.value} onChange={(v) => void write(v)} disabled={false} />
        ) : (
          <Field label={def.label} hint={def.help || undefined}>
            <SettingControl def={def} value={resolved.value} onChange={(v) => void write(v)} disabled={false} />
          </Field>
        )}
        {isBool && def.help && <p className="mt-1 max-w-measure text-2xs leading-relaxed text-text-dim">{def.help}</p>}
        {def.scopes.includes("project") && <p className="mt-1 text-2xs text-text-dim">{t("settings-registry-panel-project-may-set-own-under-about")}</p>}
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1.5">
        <div className="flex items-center gap-1.5">
          <SavedNote at={savedAt} />
          <OriginBadge origin={resolved.origin} />
          <KeyCopy settingKey={def.key} />
        </div>
        {allowed.length > 1 ? (
          <label className="flex items-center gap-1 whitespace-nowrap text-2xs text-text-dim">{t("settings-registry-panel-set")}<Select value={target} className="h-6 py-0 text-2xs" onChange={(e) => setTarget(e.target.value as SettingScope)}>
              {allowed.map((s) => (
                <option key={s} value={s}>
                  {scopeWord(s)}
                </option>
              ))}
            </Select>
          </label>
        ) : (
          <span className="text-2xs text-text-dim">{setAtWords(target)}</span>
        )}
        {shadow !== null && <span className="max-w-48 text-right text-2xs text-warn">{shadow}</span>}
        {mayReset(resolved.origin, target) && (
          <Button size="sm" variant="ghost" disabled={resetting} onClick={() => void reset()}>{t("settings-appearance-panel-reset")}</Button>
        )}
      </div>
    </div>
  );
}

/**
 * `omit` hides keys a hand-written panel above edits with a control of its
 * own (Settings → Git's *Who commits*), so one setting has one control.
 * `only` keeps just the keys named — for a group split across several panels
 * (Settings → Security's three), where each shows its own scalars.
 */
export function RegistryPanel({
  group,
  intro,
  omit,
  only,
}: {
  group: string;
  intro?: string;
  omit?: readonly string[];
  only?: readonly string[];
}) {
  // Kept across panels by the settings store, and read again there on every
  // `settings_changed` — a panel that comes back draws its rows at once.
  const registry = useSettingsRegistry();
  const resolved = useResolvedSettingsRead(null);
  const reload = useCallback(() => resolved.reload(), [resolved]);

  const defs = useMemo(
    () =>
      (registry.data?.settings ?? []).filter(
        (d) => d.group === group && !omit?.includes(d.key) && (!only || only.includes(d.key)),
      ),
    [registry.data, group, omit, only],
  );
  const byKey = useMemo(() => new Map((resolved.data?.settings ?? []).map((r) => [r.key, r])), [resolved.data]);

  const shape = phase(registry);
  if (shape === "pending") return <Pending what={t("settings-registry-panel-settings")} rows={pendingRows(t("settings-registry-panel-settings"))} />;
  if (shape === "failed") return <ErrorNote error={registry.error ?? t("settings-registry-panel-registry-read-refused")} retry={registry.reload} />;

  // A row draws its true value and origin, or a one-row *reading the resolved
  // values…* — never *default* for a value the workspace set.
  const values = phase(resolved);
  if (defs.length === 0) return null;
  return (
    <div className="flex flex-col gap-3">
      {intro && <p className="max-w-measure text-2xs leading-relaxed text-text-dim">{intro}</p>}
      {values === "failed" && <ErrorNote error={resolved.error ?? t("settings-registry-panel-resolved-values-refused")} retry={resolved.reload} />}
      {/* One card, rows on hairlines: a box per setting read as a form inside a form. */}
      <Card className="flex flex-col divide-y divide-hairline">
        {defs.map((def) => {
          const r = byKey.get(def.key);
          if (!r) {
            return (
              <div key={def.key} className="py-3 first:pt-0 last:pb-0">
                <p className="text-2xs font-medium text-text">{def.label}</p>
                {values === "pending" ? <Pending what={t("settings-browser-access-panel-resolved-values")} rows={pendingRows(t("settings-browser-access-panel-resolved-values"))} className="mt-1" /> : <p className="mt-1 text-2xs text-text-dim">{t("settings-registry-panel-value-read-key")}</p>}
              </div>
            );
          }
          return <Row key={def.key} def={def} resolved={r} onChanged={reload} />;
        })}
      </Card>
    </div>
  );
}
