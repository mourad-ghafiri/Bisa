import type { BlameLine } from "../../types";

export function gutterFor(
  lines: BlameLine[],
  when: (secs: number) => string,
): { line: number; text: string; hover: string }[];
