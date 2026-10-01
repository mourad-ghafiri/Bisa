/**
 * The resolved settings for a project (or the workspace): the one import
 * path every screen reads them through. The values are `settingsStore.ts`'s
 * — kept across screens and re-read on every `settings_changed` — so a
 * control never reads from nothing on mount.
 */
export { useResolvedSettings } from "./settingsStore";
