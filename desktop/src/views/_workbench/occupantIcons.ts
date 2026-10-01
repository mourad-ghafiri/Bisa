/** One glyph per occupant of the right panel — the strip and the header buttons share it. */
import { ICON, type LucideIcon } from "../../ui";
import type { Occupant } from "./rightPanelModel.mjs";

export const OCCUPANT_ICON: Record<Occupant, LucideIcon> = {
  files: ICON.folder,
  git: ICON.repository,
  agents: ICON.agent,
  about: ICON.info,
  workstreams: ICON.workstream,
};
