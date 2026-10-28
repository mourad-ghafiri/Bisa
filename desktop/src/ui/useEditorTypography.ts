/**
 * What Monaco is told about type, from the settings that say it.
 *
 * `editor.font_family`, `editor.font_size`, `editor.line_height`,
 * `editor.minimap`, `editor.word_wrap` and `editor.theme` are settings the
 * registry declares. This hook is the one reader: the values
 * come through `useResolvedSettings`, so a change in Settings › Project IDE ›
 * Editor reaches a mounted editor on the next `settings_changed`, and an
 * empty `font_family` means the theme's own `--font-mono` — which is how the
 * *Code font* dial (`appearance.font_mono`) reaches the editor without a
 * second setting for the same face.
 */

import { useMemo } from "react";
import { boolOf, numberOf, settingOf } from "../shell/settingsModel.mjs";
import { useResolvedSettings } from "../shell/useResolvedSettings";

export interface EditorTypography {
  fontFamily: string;
  /** Pixels. */
  fontSize: number;
  /** Pixels — Monaco takes an absolute line height, so the ratio is multiplied here. */
  lineHeight: number;
  minimap: boolean;
  /** Monaco's own vocabulary for `editor.word_wrap`. */
  wordWrap: "off" | "on" | "bounded";
  /** `editor.theme` as set: `follow_app`, or a name of Monaco's own (`editorTheme.monacoThemeFor` decides which stands). */
  theme: string;
}

/** The registry's defaults, so an editor mounted before the settings arrive is already right. */
const EDITOR_TYPOGRAPHY_DEFAULTS: EditorTypography = {
  fontFamily: "var(--font-mono, ui-monospace, monospace)",
  fontSize: 14,
  lineHeight: 21,
  minimap: false,
  wordWrap: "off",
  theme: "follow_app",
};

const WORD_WRAP = new Set(["off", "on", "bounded"]);

export function editorTypographyFrom(resolved: readonly { key: string; value: unknown }[] | null): EditorTypography {
  const family = settingOf(resolved, "editor.font_family", "");
  const fontSize = numberOf(resolved, "editor.font_size", EDITOR_TYPOGRAPHY_DEFAULTS.fontSize, { min: 8, max: 32 });
  const ratio = numberOf(resolved, "editor.line_height", 1.5, { min: 1, max: 2.5 });
  const wrap = settingOf(resolved, "editor.word_wrap", EDITOR_TYPOGRAPHY_DEFAULTS.wordWrap);
  const theme = settingOf(resolved, "editor.theme", EDITOR_TYPOGRAPHY_DEFAULTS.theme);
  return {
    fontFamily: typeof family === "string" && family.trim() ? family.trim() : EDITOR_TYPOGRAPHY_DEFAULTS.fontFamily,
    fontSize,
    lineHeight: Math.round(fontSize * ratio),
    minimap: boolOf(resolved, "editor.minimap", EDITOR_TYPOGRAPHY_DEFAULTS.minimap),
    wordWrap: (WORD_WRAP.has(String(wrap)) ? wrap : EDITOR_TYPOGRAPHY_DEFAULTS.wordWrap) as EditorTypography["wordWrap"],
    theme: typeof theme === "string" && theme.trim() ? theme.trim() : EDITOR_TYPOGRAPHY_DEFAULTS.theme,
  };
}

export function useEditorTypography(): EditorTypography {
  const { resolved } = useResolvedSettings(null);
  return useMemo(() => editorTypographyFrom(resolved), [resolved]);
}
