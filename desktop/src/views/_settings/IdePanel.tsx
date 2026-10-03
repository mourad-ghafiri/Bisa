/**
 * Settings › Project IDE › IDE: which centre a workstream opens with —
 * **Project** (documents and terminals), **Agent** (the conversation with
 * the agents) or **Board** (every workstream's card). One registry key,
 * `ide.default_mode`, drawn as the same switch the IDE's header shows, with
 * words instead of the raw tokens the generic control would print. The
 * Board is offered while Settings › Project IDE › Board has it on.
 *
 * Written at the workspace scope — a preference you would want on your other
 * machine (ide/13) — so the origin badge and *Reset* say what the registry
 * panel's rows say. The mode a workstream is in *now* is not here: that is
 * furniture, remembered per root by the IDE (`ideModeStore.ts`).
 */

import { useState } from "react";
import { api } from "../../api";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { boolOf, choiceOf } from "../../shell/settingsModel.mjs";
import { Button, Card, ErrorNote, ICON, Pending, SegmentedControl, Section, failureText, useToast } from "../../ui";
import { BOARD_DEFAULTS, BOARD_ENABLED_KEY } from "../_board/boardSettings.mjs";
import { DEFAULT_MODE, DEFAULT_MODE_KEY, MODES, modeFor, type IdeMode } from "../_workbench/ideModeModel.mjs";
import { modeSegments, modeWords } from "../_workbench/workbenchChromeModel.mjs";
import { pendingRows } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { OriginBadge } from "./OriginBadge";

export function IdePanel() {
  const toast = useToast();
  const { resolved, error, reload } = useResolvedSettings(null);
  const [busy, setBusy] = useState(false);
  const boardEnabled = boolOf(resolved, BOARD_ENABLED_KEY, BOARD_DEFAULTS.enabled);
  const mode = modeFor(undefined, choiceOf(resolved, DEFAULT_MODE_KEY, MODES, DEFAULT_MODE), boardEnabled);
  const origin = resolved?.find((s) => s.key === DEFAULT_MODE_KEY)?.origin ?? "default";
  const options = modeSegments(boardEnabled).map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon] }));

  const write = async (next: IdeMode) => {
    if (next === mode || busy) return;
    setBusy(true);
    try {
      await api.setSettings("workspace", { [DEFAULT_MODE_KEY]: next });
      reload();
    } catch (e) {
      toast.error(failureText("settings", "ide-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };
  const reset = async () => {
    setBusy(true);
    try {
      await api.unsetSetting("workspace", DEFAULT_MODE_KEY);
      reload();
    } catch (e) {
      toast.error(failureText("settings", "ide-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section title={t("settings-ide-panel-centre")}>
      <Card>
        {/* The switch draws its true value or nothing: a default shown before the read lands is a lie for a beat. */}
        {!resolved && !error && <Pending what={t("settings-browser-access-panel-resolved-values")} rows={pendingRows(t("settings-browser-access-panel-resolved-values"))} />}
        {!resolved && error && <ErrorNote error={error} retry={reload} />}
        {resolved && (
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-ide-panel-open-project-ide")}</span>
            <SegmentedControl options={options} value={mode} onChange={(v) => void write(v as IdeMode)} label={t("settings-goals-panel-default-mode")} size="sm" />
            <span className="mt-1 block text-2xs text-text-dim">
              {t("settings-ide-panel-workstream-already-switched-keeps-own", { hint: modeWords(mode).hint })}
            </span>
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
  );
}
