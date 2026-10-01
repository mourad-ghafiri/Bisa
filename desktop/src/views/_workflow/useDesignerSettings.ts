/**
 * The designer's three canvas switches and its autosave delay, from the
 * settings registry (`workflow.*`). Defaults are
 * the registry's, so a node that cannot be asked draws the same canvas the
 * registry would.
 */

import { useMemo } from "react";
import { api } from "../../api";
import { useAsync } from "../_work/useAsync";
import type { DesignerSettings } from "./Designer";

export type DesignerScreenSettings = DesignerSettings & { autosaveDelayMs: number };

const DESIGNER_DEFAULTS: DesignerScreenSettings = {
  snap: true,
  grid: 16,
  minimap: false,
  autosaveDelayMs: 800,
};

export function useDesignerSettings(): DesignerScreenSettings {
  const { data } = useAsync((s) => api.settingsResolved(null, s), []);
  // One object per fetch, not per render: the designer memoises on it.
  return useMemo(() => {
    const list = (data as { settings?: { key: string; value: unknown }[] } | null)?.settings ?? [];
    const read = <T,>(key: string, fallback: T, ok: (v: unknown) => v is T): T => {
      const v = list.find((s) => s.key === key)?.value;
      return ok(v) ? v : fallback;
    };
    const isBool = (v: unknown): v is boolean => typeof v === "boolean";
    const isNum = (v: unknown): v is number => typeof v === "number";
    return {
      snap: read("workflow.designer.snap", DESIGNER_DEFAULTS.snap, isBool),
      grid: read("workflow.designer.grid", DESIGNER_DEFAULTS.grid, isNum),
      minimap: read("workflow.designer.minimap", DESIGNER_DEFAULTS.minimap, isBool),
      autosaveDelayMs: read("workflow.autosave.delay_ms", DESIGNER_DEFAULTS.autosaveDelayMs, isNum),
    };
  }, [data]);
}
