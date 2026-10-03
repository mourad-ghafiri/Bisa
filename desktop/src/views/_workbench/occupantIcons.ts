/**
 * One glyph per occupant of the right panel — the strip and the header
 * buttons share it. The Git tab wears the Git mark: git is the tool there,
 * not one of its concepts (`ui/GitMark.tsx`).
 */
import { GitMark, ICON, type LucideIcon, type Mark } from "../../ui";
import type { Occupant } from "./rightPanelModel.mjs";

export const OCCUPANT_ICON: Record<Occupant, LucideIcon | Mark> = {
  files: ICON.folder,
  git: GitMark,
  agents: ICON.agent,
  about: ICON.info,
  workstreams: ICON.workstream,
};
