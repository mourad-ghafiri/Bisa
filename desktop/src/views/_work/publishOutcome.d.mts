export type RefusalKind = "manual" | "no_goal" | "declined" | "not_ready" | "state" | "error";

export declare function refusalOf(err: { status: number; code?: string | null; message: string }): {
  kind: RefusalKind;
  detail: string;
};

/** This checkout's `workstream_publish_failed` as the banner's `failed` kind; `null` for any other frame. */
export declare function publishFailure(payload: unknown, wid: string): { kind: "failed"; what: string; reason: string } | null;
