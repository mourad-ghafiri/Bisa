/**
 * The bundled `@fontsource` faces are imported from `shell/theme.ts` for their
 * side effect — each injects the `@font-face` rules for one family — only once a
 * non-system font dial is chosen. A side-effect CSS import needs a
 * module declaration; these give the `@fontsource` paths one.
 */
declare module "@fontsource/*";
declare module "@fontsource-variable/*";
