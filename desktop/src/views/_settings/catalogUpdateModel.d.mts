export declare const UPDATABLE_KINDS: readonly ["skill", "agent", "team"];

export declare function rowAction(entry: {
  kind: string;
  installed: boolean;
  installed_differs?: boolean | null;
}): "install" | "update" | "installed";

export declare function updateOutcome(
  updated: { restated: boolean },
  installed: Record<string, unknown>,
): "restated" | "created" | "unchanged";
