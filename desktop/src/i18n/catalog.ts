/**
 * The catalog files, bundled: `locales/<lang>/**.ftl` read as text at build
 * time (Vite's `?raw` import, vite.dev/guide/assets §Importing Asset as
 * String). Every shipped language is in the bundle, so the one the window
 * speaks is installed synchronously before any module says a word — a table
 * built at import time (`Object.freeze({ label: t("…") })`) is in the right
 * language from the first frame. The CLI's file is the CLI's alone and is left
 * out of the window.
 */

const files = import.meta.glob<string>(["../../../locales/*/**/*.ftl", "!../../../locales/*/cli.ftl"], {
  query: "?raw",
  import: "default",
  eager: true,
});

/** A language's texts, in a stable order; empty for a language that does not ship. */
export function sourcesFor(locale: string): string[] {
  const prefix = `../../../locales/${locale}/`;
  return Object.keys(files)
    .filter((k) => k.startsWith(prefix))
    .sort()
    .map((k) => files[k]);
}
