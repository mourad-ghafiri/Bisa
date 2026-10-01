/** Types for `notificationsSettingsModel.mjs`. */
export type NotifyPermission = "granted" | "denied" | "default" | "unavailable";

export declare const MASTER_KEY: string;
export declare const CATEGORY_KEYS: readonly string[];
export declare const PERMISSIONS: readonly ("granted" | "denied" | "default")[];
/** The OS's permission, from the plugin's answer and the webview's own word. */
export declare function permissionOf(granted: unknown, word: unknown): "granted" | "denied" | "default";
export interface PermissionWords {
  tone: "ok" | "warn" | "neutral" | "quiet";
  label: string;
  sentence: string;
}
export declare function permissionWords(permission: NotifyPermission | null, enabled: boolean): PermissionWords;
export declare function askVerb(permission: NotifyPermission | null): string | null;
export declare function iconWords(): string;
