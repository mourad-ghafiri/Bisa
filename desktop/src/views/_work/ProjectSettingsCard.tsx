/**
 * A project's own layer of the settings named, edited where the project is in
 * front of you (ide/13): About › Settings for the `git.*` and `workstreams.*`
 * keys and the editor's and the terminal's. Each row draws the kit's one
 * control per kind, says where its value comes from — *set for this
 * project*, *the workspace's value*, *the default* — and offers *Inherit*
 * when the project holds a value of its own. **Nothing here saves**: every
 * change is a draft the view's toolbar saves (`useProjectSettingsDraft`),
 * and a drafted row says so. The words are `projectSettingsModel.mjs`'s.
 */

import { useMemo } from "react";
import type { ResolvedSetting, SettingDef } from "../../types";
import { Button, Chip, Field, SkeletonRows, Tooltip } from "../../ui";
import { SettingControl } from "../_settings/SettingControl";
import { INHERIT } from "./projectSettingsDraftModel.mjs";
import { projectRow } from "./projectSettingsModel.mjs";
import type { ProjectSettingsDraft } from "./useProjectSettingsDraft";
import { t } from "../../i18n/l10n.mjs";

export function ProjectSettingsCard({
  keys,
  intro,
  draft,
}: {
  /** The registry keys this card edits, in order. */
  keys: readonly string[];
  intro?: string;
  draft: ProjectSettingsDraft;
}) {
  const defs = useMemo(() => {
    const byKey = new Map(draft.registry.map((d) => [d.key, d]));
    return keys.map((k) => byKey.get(k)).filter((d): d is SettingDef => d !== undefined);
  }, [draft.registry, keys]);
  if (draft.loading) return <SkeletonRows rows={3} />;

  return (
    <div className="flex flex-col gap-2.5">
      {intro && <p className="text-2xs text-text-dim">{intro}</p>}
      {defs.map((def) => {
        const r: ResolvedSetting = draft.resolved.get(def.key) ?? { key: def.key, value: def.default, origin: "default" };
        const row = projectRow(r);
        const drafted = def.key in draft.draft.settings;
        const inheriting = drafted && draft.draft.settings[def.key] === INHERIT;
        const value = inheriting ? r.value : draft.settingValue(def.key);
        const isBool = def.kind.type === "bool";
        const control = <SettingControl def={def} value={value} onChange={(v) => draft.editSetting(def.key, v)} disabled={inheriting} />;
        return (
          <div key={def.key} className="flex flex-wrap items-start gap-3 rounded-control border border-border bg-surface-2 p-2">
            <div className="min-w-0 flex-1">
              {isBool ? control : <Field label={def.label} hint={def.help || undefined}>{control}</Field>}
              {isBool && def.help && <p className="mt-1 text-2xs text-text-dim">{def.help}</p>}
              <p className="mt-1 flex flex-wrap items-center gap-1.5 text-2xs text-text-dim">
                <span className="font-mono">{def.key}</span>
                <span>· {inheriting ? t("work-project-settings-card-inherits-workspace-s-save") : row.origin}</span>
                {drafted && <Chip tone="accent">{inheriting ? t("work-project-settings-card-inherit") : t("work-project-settings-card-changed")}</Chip>}
              </p>
            </div>
            {drafted ? (
              <Tooltip label={t("work-project-settings-card-forget-change-row-goes-back-what")}>
                <span className="inline-flex">
                  <Button size="sm" variant="ghost" onClick={() => draft.forgetSetting(def.key)}>{t("work-conflict-block-undo")}</Button>
                </span>
              </Tooltip>
            ) : (
              row.own && (
                <Tooltip label={t("work-project-settings-card-clear-project-s-value-save-workspace")}>
                  <span className="inline-flex">
                    <Button size="sm" variant="ghost" onClick={() => draft.inheritSetting(def.key)}>{t("work-git-config-form-inherit")}</Button>
                  </span>
                </Tooltip>
              )
            )}
          </div>
        );
      })}
    </div>
  );
}
