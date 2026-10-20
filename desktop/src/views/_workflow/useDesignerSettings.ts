/**
 * The designer's three canvas switches and its autosave delay, from the
 * settings registry (`workflow.*`), read as `designerSettingsModel.mjs`
 * says: the registry's defaults when the node cannot be asked, every number
 * held to its range.
 */

import { useMemo } from "react";
import { api } from "../../api";
import { useAsync } from "../_work/useAsync";
import { designerSettingsOf, type DesignerScreenSettings } from "./designerSettingsModel.mjs";

export type { DesignerScreenSettings };

export function useDesignerSettings(): DesignerScreenSettings {
  const { data } = useAsync((s) => api.settingsResolved(null, s), []);
  // One object per fetch, not per render: the designer memoises on it.
  return useMemo(() => designerSettingsOf((data as { settings?: { key: string; value: unknown }[] } | null)?.settings ?? []), [data]);
}
