/**
 * Settings › Automation › Goals: how a new goal moves. One registry key,
 * `goals.default_mode`, drawn as the same three-way switch the New Goal
 * dialog shows, with the mode's own sentence under it instead of the raw
 * token the generic control would print; under it `goals.auto.ceiling` —
 * where a step's ceiling is in an auto goal: a step that may write runs
 * commands too, or each step's own stands — and `goals.auto.permissions`
 * — what an auto goal does above that ceiling: the classifier reads it, or
 * you are asked — drawn the same way. The repair budget sits under them as
 * a registry row.
 *
 * Written at the workspace scope — a preference the whole workspace shares
 * (ide/13) — so the origin badge and *Reset* say what the registry panel's
 * rows say. A goal keeps the mode it was captured in: this is the default
 * for the next one.
 */

import { useState } from "react";
import { api } from "../../api";
import { useResolvedSettings } from "../../shell/useResolvedSettings";
import { choiceOf } from "../../shell/settingsModel.mjs";
import type { GoalMode } from "../../types";
import { Button, Card, ErrorNote, ICON, Pending, SegmentedControl, Section, failureText, useToast } from "../../ui";
import { AUTO_CEILING, AUTO_CEILING_KEY, AUTO_CEILING_LABEL, AUTO_CEILING_MEANING, AUTO_PERMISSIONS, AUTO_PERMISSIONS_KEY, AUTO_PERMISSIONS_LABEL, AUTO_PERMISSIONS_MEANING, DEFAULT_AUTO_CEILING, DEFAULT_AUTO_PERMISSIONS, DEFAULT_MODE, DEFAULT_MODE_KEY, GOAL_MODES, MODE_MEANING, modeSegments } from "../_goal/goalMode.mjs";
import type { AutoCeiling, AutoPermissions } from "../_goal/goalMode.mjs";
import { pendingRows } from "./loadModel.mjs";
import { t } from "../../i18n/l10n.mjs";
import { OriginBadge } from "./OriginBadge";

export function GoalsPanel() {
  const toast = useToast();
  const { resolved, error, reload } = useResolvedSettings(null);
  const [busy, setBusy] = useState(false);
  const mode = choiceOf(resolved, DEFAULT_MODE_KEY, GOAL_MODES, DEFAULT_MODE);
  const origin = resolved?.find((s) => s.key === DEFAULT_MODE_KEY)?.origin ?? "default";
  const options = modeSegments().map((s) => ({ id: s.id, label: s.label, icon: ICON[s.icon] }));
  const ceiling = choiceOf(resolved, AUTO_CEILING_KEY, AUTO_CEILING, DEFAULT_AUTO_CEILING);
  const ceilingOptions = AUTO_CEILING.map((id) => ({ id, label: AUTO_CEILING_LABEL[id] }));
  const above = choiceOf(resolved, AUTO_PERMISSIONS_KEY, AUTO_PERMISSIONS, DEFAULT_AUTO_PERMISSIONS);
  const aboveOptions = AUTO_PERMISSIONS.map((id) => ({ id, label: AUTO_PERMISSIONS_LABEL[id] }));

  /** Write one choice at the workspace scope — unless it already reads `next`, or a write is in flight. */
  const writeChoice = async (key: string, current: string, next: string) => {
    if (next === current || busy) return;
    setBusy(true);
    try {
      await api.setSettings("workspace", { [key]: next });
      reload();
    } catch (e) {
      toast.error(failureText("settings", "goals-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };
  const write = (next: GoalMode) => writeChoice(DEFAULT_MODE_KEY, mode, next);
  const writeCeiling = (next: AutoCeiling) => writeChoice(AUTO_CEILING_KEY, ceiling, next);
  const writeAbove = (next: AutoPermissions) => writeChoice(AUTO_PERMISSIONS_KEY, above, next);
  const reset = async () => {
    setBusy(true);
    try {
      await api.unsetSetting("workspace", DEFAULT_MODE_KEY);
      reload();
    } catch (e) {
      toast.error(failureText("settings", "goals-panel-failed", e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Section title={t("settings-goals-panel-how-new-goal-moves")}>
      <Card>
        {/* The switch draws its true value or nothing: a default shown before the read lands is a lie for a beat. */}
        {!resolved && !error && <Pending what={t("settings-browser-access-panel-resolved-values")} rows={pendingRows(t("settings-browser-access-panel-resolved-values"))} />}
        {!resolved && error && <ErrorNote error={error} retry={reload} />}
        {resolved && (
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <span className="mb-1 block text-2xs font-medium text-text-dim">{t("settings-goals-panel-new-goals-start")}</span>
            <SegmentedControl options={options} value={mode} onChange={(v) => void write(v)} label={t("settings-goals-panel-default-mode")} size="sm" />
            <span className="mt-1 block text-2xs text-text-dim">
              {MODE_MEANING[mode]} {t("settings-goals-panel-goal-keeps-mode-captured-in")}
            </span>
            {/* Where a step's ceiling is in an auto goal: a step that may write runs commands too, or each step's own stands. */}
            <span className="mb-1 mt-3 block text-2xs font-medium text-text-dim">{t("settings-goals-panel-step-s-ceiling-auto-goal")}</span>
            <SegmentedControl options={ceilingOptions} value={ceiling} onChange={(v) => void writeCeiling(v)} label={t("settings-goals-panel-step-s-ceiling-auto-goal")} size="sm" />
            <span className="mt-1 block text-2xs text-text-dim">{AUTO_CEILING_MEANING[ceiling]} {t("settings-goals-panel-guided-manual-goal-keeps-step-s-ceiling")}</span>
            {/* What an auto goal does above that ceiling — the guard's rules and the redactor come first either way. */}
            <span className="mb-1 mt-3 block text-2xs font-medium text-text-dim">{t("settings-goals-panel-above-step-s-ceiling-auto-goal")}</span>
            <SegmentedControl options={aboveOptions} value={above} onChange={(v) => void writeAbove(v)} label={t("settings-goals-panel-above-step-s-ceiling-auto-goal")} size="sm" />
            <span className="mt-1 block text-2xs text-text-dim">{AUTO_PERMISSIONS_MEANING[above]} {t("settings-goals-panel-guided-manual-goal-always-asks")}</span>
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
