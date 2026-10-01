import type { RemoteInspection } from "../../types";

/** What the inspection is asked about: the URL of a clone or the folder of an import, else nothing. */
export declare function inspectBody(provenance: "new" | "clone" | "import", url: string, path: string): { url: string } | { path: string } | null;
export declare function inspectionLine(inspection: RemoteInspection | null | undefined, chosen?: string | null): { tone: "ok" | "warn" | "quiet"; text: string; hostLabel: string | null } | null;
export declare function accountOptions(inspection: RemoteInspection | null | undefined): { login: string; words: string; suggested: boolean }[];
export declare function pinWrite(login: string | null | undefined): { key: string; value: string } | null;
export declare function signInCaution(inspection: RemoteInspection | null | undefined): { text: string; tab: string | null } | null;
