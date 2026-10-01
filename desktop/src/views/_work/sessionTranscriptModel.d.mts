import type { SessionRow } from "../../types";

type Row = Pick<SessionRow, "agent" | "harness" | "model" | "effort" | "state">;

export declare function transcriptTitle(row: Partial<Row> | null | undefined): string;
export declare function transcriptWords(row: Pick<Row, "state"> | null | undefined): { words: string; live: boolean };
export declare function transcriptTabTitle(row: Partial<Pick<Row, "agent" | "harness">> | null | undefined): string;
