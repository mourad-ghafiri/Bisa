/**
 * The editor's theme, derived from the token roles (ide/03).
 *
 * Monaco names its colours by key; the app names its colours by role, and
 * nothing in the app names a colour directly. So the editor theme is a
 * function from the resolved role values to Monaco's keys — every role the
 * theme contract requires maps to at least one Monaco colour, and a test
 * asserts it, so a new role cannot leave the editor painted in a colour the
 * theme did not choose.
 *
 * Pure: takes the roles as strings, returns a Monaco `IStandaloneThemeData`.
 * Reading the roles off the document is the caller's job (`monaco.ts`).
 */

/** Every role the theme contract requires, as the editor uses them. */
export const EDITOR_ROLE_MAP = Object.freeze({
  bg: ["editor.background", "editorGutter.background", "minimap.background"],
  surface: ["editorWidget.background", "editorSuggestWidget.background", "editorHoverWidget.background", "input.background"],
  "surface-2": ["editor.lineHighlightBackground", "editorWidget.border", "scrollbarSlider.background"],
  border: ["editorWidget.border", "editorIndentGuide.background", "editorRuler.foreground", "input.border"],
  text: ["editor.foreground", "editorLineNumber.activeForeground", "editorSuggestWidget.foreground", "input.foreground"],
  "text-dim": ["editorLineNumber.foreground", "editorWhitespace.foreground", "editorCodeLens.foreground", "editorHint.foreground"],
  accent: ["editorCursor.foreground", "focusBorder", "editorLink.activeForeground", "editorBracketMatch.border"],
  "accent-ink": ["editorSuggestWidget.highlightForeground", "editorHoverWidget.statusBarBackground"],
  "accent-soft": ["editor.selectionBackground", "editor.findMatchHighlightBackground", "editorSuggestWidget.selectedBackground", "editor.wordHighlightBackground"],
  "accent-contrast": ["editorSuggestWidget.selectedForeground", "editorCursor.background"],
  warn: ["editorWarning.foreground", "editorOverviewRuler.warningForeground"],
  "warn-soft": ["editorMarkerNavigationWarning.background"],
  danger: ["editorError.foreground", "editorOverviewRuler.errorForeground", "diffEditor.removedTextBackground"],
  "danger-soft": ["diffEditor.removedLineBackground", "editorMarkerNavigationError.background"],
  ok: ["editorInfo.foreground", "diffEditor.insertedTextBackground"],
  "ok-soft": ["diffEditor.insertedLineBackground"],
  overlay: ["editorOverviewRuler.border", "widget.shadow"],
});

/** Tokens the syntax colours draw from — roles, never raw colours. */
const TOKEN_RULES = Object.freeze([
  ["comment", "text-dim", "italic"],
  ["keyword", "accent-ink", "bold"],
  ["string", "ok", ""],
  ["number", "warn", ""],
  ["type", "accent", ""],
  ["delimiter", "text-dim", ""],
  ["tag", "accent-ink", ""],
  ["attribute.name", "text", ""],
  ["attribute.value", "ok", ""],
  ["invalid", "danger", ""],
]);

/**
 * Monaco wants `#rrggbb[aa]`. The app's roles are `oklch(...)` in the CSS,
 * but `getComputedStyle` resolves them to `rgb()`/`rgba()`/`oklch()` per
 * browser — `resolveToHex` in `monaco.ts` normalises through a canvas. This
 * function only checks the shape it was handed.
 */
export function isHex(value) {
  return /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(value);
}

/**
 * Build the theme from resolved roles (`{ bg: "#…", surface: "#…", … }`).
 * A role missing from `roles` is left to Monaco's base theme rather than
 * painted black, and reported in `missing` so a caller can log it once.
 */
export function editorThemeFor(roles, scheme) {
  const colors = {};
  const missing = [];
  for (const [role, keys] of Object.entries(EDITOR_ROLE_MAP)) {
    const value = roles[role];
    if (!value || !isHex(value)) {
      missing.push(role);
      continue;
    }
    for (const key of keys) {
      if (!(key in colors)) colors[key] = value;
    }
  }
  const rules = TOKEN_RULES.filter(([, role]) => isHex(roles[role] ?? "")).map(
    ([token, role, fontStyle]) => ({
      token,
      foreground: roles[role].slice(1, 7),
      ...(fontStyle ? { fontStyle } : {}),
    }),
  );
  return {
    theme: {
      base: scheme === "dark" ? "vs-dark" : "vs",
      inherit: true,
      rules,
      colors,
    },
    missing,
  };
}
