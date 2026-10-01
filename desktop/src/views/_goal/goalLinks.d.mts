export declare function goalLinkRoots(
  projects: readonly { exists: boolean; goals: readonly string[]; project: { id: string } }[],
  goal: string,
): { scope: "workstream"; id: string }[];
