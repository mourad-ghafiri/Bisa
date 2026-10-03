import type { CodeHostConnection, CodeHostHealth, InstallHints, LoginPlan } from "../../types";

export type Tone = "ok" | "warn" | "danger" | "quiet";

export declare function envVar(kind: string): string;
export declare function cliLine(health: CodeHostHealth | null | undefined): { tone: "ok" | "warn" | "quiet"; text: string; action: "signin" | "install" | null };
export declare function installLines(hints: InstallHints | null | undefined): { label: string; command: string }[];
export declare function resolvesLine(health: CodeHostHealth | null | undefined): { tone: "ok" | "warn"; text: string };
export declare function connectionLine(connection: CodeHostConnection | null | undefined, label?: string): { tone: Tone; text: string };
export declare function accountRows(health: CodeHostHealth | null | undefined): { login: string; source: string; isDefault: boolean; sourceWords: string }[];
export declare function envOverrideLine(health: CodeHostHealth | null | undefined): string | null;
export declare function helpersLine(health: CodeHostHealth | null | undefined): { tone: "ok" | "quiet"; text: string } | null;
export declare function defaultLine(health: CodeHostHealth | null | undefined): string;
export declare function storeHint(health: CodeHostHealth | null | undefined): string;
export declare function tokenWords(kind: string): { placeholder: string; needsLogin: boolean; scopes: string };
/** Why *Add account* cannot be pressed yet, or `null` when it can. */
export declare function addBlockedWords(form: { ready: boolean; token: string; sent: string | null; needsLogin: boolean; login: string }): string | null;
export declare function loginWords(plan: LoginPlan | null | undefined): { button: string; blurb: string; opens: "terminal" | "install" | "token" | null };
export declare function addedWords(login: string, connection: CodeHostConnection | null | undefined, label?: string): string;
export declare function defaultWords(login: string | null): string;
export declare function signingInWords(kind: string): string;

export declare function signInPlan(
  canOpenTerminal: boolean,
  kind: string,
  label: string,
  host: string | null | undefined,
): { ok: true; terminal: { scope: "machine"; id: "home"; label: string; login: { kind: string; host: string } } } | { ok: false; why: string };
export declare function signInExitKey(sessions: readonly { key: string; exitedAt: number | null; login?: { kind: string } | null }[], kind: string): string;
export declare function checksStillListed<T>(checks: Record<string, T>, logins: readonly string[]): Record<string, T>;
