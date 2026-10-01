import type { AgentState, HostBlock, HostGreeting, SshOverview, SshPublicKey, SshResolved } from "../../types";

export type Tone = "ok" | "warn" | "danger" | "quiet";

/** A check pinned to a key for this session — the platform's own memory, never ssh's. */
export interface KeyCheck {
  host: string;
  greeting: HostGreeting;
  /** Unix seconds. */
  at: number;
}
export type KeyChecks = Record<string, KeyCheck>;

export interface KeyRowWords {
  name: string;
  path: string;
  algorithm: string;
  fingerprint: string;
  shortFingerprint: string;
  comment: string;
  loaded: boolean;
  publicLine: string;
  loadedWords: string;
}
export type NextStepKind = "register" | "check" | "host_key" | "ready";
export type NextStepAction = "check" | "host" | "profile" | null;
export interface NextStep {
  kind: NextStepKind;
  text: string;
  action: NextStepAction;
}
export interface KeyCardWords extends KeyRowWords {
  check: KeyCheck | null;
  next: NextStep;
}

export declare const PASSPHRASE_NOTE: string;
export declare const AGENT_START_COMMAND: string;
export declare function hostKeyPage(host: string): string | null;
export declare function hostPages(): { host: string; url: string }[];
export declare function shortFingerprint(fingerprint: string): string;
export declare function algorithmWords(algorithm: string): string;
export declare function keyRows(overview: SshOverview | null | undefined): KeyRowWords[];
export declare function agentWords(agent: AgentState): { tone: Tone; text: string; remedy: string | null };
export declare function hostRows(hosts: readonly HostBlock[] | null | undefined): {
  patterns: string;
  hostname: string | null;
  user: string | null;
  identity: string | null;
  identitiesOnly: boolean;
}[];
export declare function hostChoices(overview: SshOverview | null | undefined): string[];
export declare function greetingWords(greeting: HostGreeting, host: string): { tone: Tone; text: string };
export declare function rememberCheck(checks: KeyChecks | null | undefined, name: string, host: string, greeting: HostGreeting, at: number): KeyChecks;
export declare function checkWords(check: KeyCheck, now?: number): { tone: Tone; text: string; when: string };
export declare function nextStep(card: { check: KeyCheck | null }): NextStep;
export declare function keyCards(overview: SshOverview | null | undefined, checks: KeyChecks | null | undefined): KeyCardWords[];
export declare function resolvedWords(host: string, resolved: SshResolved, keys: readonly { name: string; path: string }[]): string;
export declare function busyKey(action: string, name: string): string;
export declare function suggestKeyName(owner: string): string;
export declare function validateKeyName(name: string, taken: readonly string[]): string | null;
export declare function hostBlockText(alias: string, hostname: string, user: string, key: string | null | undefined): string;
export declare function generatedWords(key: SshPublicKey): string;
export declare function loadedWords(name: string): string;
export declare function copiedForHostWords(host: string): string;
