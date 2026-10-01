import type { PublishPolicy } from "../../types";

export declare const PUBLISH_POLICIES: readonly PublishPolicy[];
export declare const PUBLISH_LABEL: Readonly<Record<PublishPolicy, string>>;
export declare const PUBLISH_MEANING: Readonly<Record<PublishPolicy, string>>;
export declare const PUBLISH_TONE: Readonly<Record<PublishPolicy, "quiet" | "accent" | "warn">>;
export declare function publishOf(project: { publish?: PublishPolicy | null } | null | undefined): PublishPolicy;
