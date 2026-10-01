import type { AccountFacts, ConnectionCheck, IdentityFacts, RepoConnection, Transport } from "../../types";

export type Tone = "ok" | "warn" | "danger" | "quiet";

export declare const CAUTION_IDS: readonly string[];
export declare const ACCOUNT_SOURCES: readonly string[];

export declare function remoteRow(remote: RepoConnection["remote"]): {
  summary: string;
  url: string;
  protocol: string;
  transport: "ssh" | "https" | "local" | "other";
  alias: string | null;
  owner: string | null;
  note: string | null;
} | null;
export declare function profileRow(profile: RepoConnection["profile"]): { tone: Tone; text: string };
export declare function identityRow(identity: IdentityFacts, profile: RepoConnection["profile"]): { tone: Tone; who: string | null; origin: string };
export declare function transportRow(transport: Transport): { tone: Tone; text: string; detail: string | null };
export declare function accountRow(account: AccountFacts, codeHost: string | null, profile: RepoConnection["profile"]): { tone: Tone; login: string | null; source: string };
export declare function cautionMeta(id: string, codeHost?: string | null): { tone: "warn" | "danger"; tab: "git" | "git-ssh" | "github" | "gitlab" | "bitbucket" | null };
export declare function hasCautions(connection: RepoConnection | null | undefined): boolean;
export declare function firstCaution(connection: RepoConnection | null | undefined): string | null;
export declare function checkLines(check: ConnectionCheck): { tone: Tone; text: string }[];
