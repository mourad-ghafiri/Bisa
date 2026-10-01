/** Re-exports the library cards share, and the origin readers. */

import type { Workflow } from "../../types";

export { Chip, ICON, TagChips } from "../../ui";

/** The catalog slug a workflow was installed from, if any. */
export function catalogSlugOf(origin: Workflow["origin"]): string | null {
  return origin.origin === "catalog" ? origin.slug : null;
}
