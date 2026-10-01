import type { RelayCheck, RelayHealth, SyncReport } from "../../types";

export type Tone = "ok" | "warn" | "danger" | "quiet";
export declare function relayTone(relay: RelayHealth | null | undefined): { tone: Tone; word: string };
export declare function relayWords(relay: RelayHealth | null | undefined, now?: number): string;
export declare function relayProblem(relay: RelayHealth | null | undefined): string;
export declare function checkWords(check: RelayCheck | null | undefined): { tone: Tone; text: string } | null;
/** The check's answer the field may draw: the one about the address it holds now. */
export declare function checkAbout<C extends { url: string }>(check: C | null | undefined, url: string | null | undefined): C | null;
export declare function relayEntry(raw: string | null | undefined, relays: readonly string[]): { ok: false; reason: string } | { ok: true; reason: string; url: string };
export declare const OFF_LINE: string;
export declare function syncSummary(report: SyncReport | null | undefined): { running: boolean; enabled: boolean; line: string; tone: Tone };
export declare function checkAllWords(checks: readonly Pick<RelayCheck, "url" | "ok">[] | null | undefined): { tone: Tone; text: string } | null;
export declare function defaultRelays(defs: readonly { key: string; default?: unknown }[] | null | undefined): string[];
export declare function isDefaultList(relays: readonly string[] | null | undefined, defaults: readonly string[] | null | undefined): boolean;
/** Whether an engine fact moved the wire — the relays, or a `sync.*` setting — so its readers read again. */
export declare function wireMoved(payload: { type?: string; keys?: readonly string[] } | null | undefined): boolean;
