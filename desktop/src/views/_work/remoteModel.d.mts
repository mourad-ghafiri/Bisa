export declare const REMOTE_PROTOCOLS: readonly string[];
export declare function parseRemote(url: string | null | undefined): { protocol: string; host: string; owner: string; name: string } | null;
export declare function remoteSummary(url: string | null | undefined): string;
export declare function protocolWords(protocol: string | null | undefined): { label: string; transport: "ssh" | "https" | "local" | "other" };
export declare function isRemoteUrl(url: string | null | undefined): boolean;
