/**
 * One catalog template in the library: the graph the row carries as its
 * thumbnail, the template's name and whether it is installed (the state
 * line), what it is for, how big and what installing brings (the agents its
 * steps name, the templates its `spawn` steps start), its tags — and one
 * verb beside the card's words: *Use template*, which installs it and opens
 * the copy, or *Open* on one already here.
 */

import type { CatalogEntry } from "../../types";
import { Button, ICON, TagChips } from "../../ui";
import { LibraryCard } from "./LibraryCard";
import { t } from "../../i18n/l10n.mjs";

export function TemplateCard({ entry, busy, onUse }: { entry: CatalogEntry; busy: boolean; onUse: () => void }) {
  const steps = entry.workflow?.steps ?? [];
  const meta = [t("workflow-goal-workflow-tab-steps", { steps: steps.length })];
  if (entry.requires.length > 0) meta.push(t("workflow-template-card-brings", { list: entry.requires.join(", ") }));
  return (
    <LibraryCard
      definition={{ name: entry.name, steps }}
      icon={<ICON.template size={14} aria-hidden />}
      title={entry.name}
      status={
        entry.installed
          ? { tone: "ok", icon: "ok", words: t("workflow-template-card-installed"), live: false }
          : { tone: "quiet", icon: "template", words: t("workflow-template-card-not-installed"), live: false }
      }
      description={entry.description}
      meta={meta}
      footer={
        <>
          <TagChips tags={[...entry.tags]} max={3} />
          {/* A per-card verb: a primary on every card of a grid is a wall of accent with no single ask among them. */}
          <Button size="sm" variant="default" className="ml-auto" disabled={busy} onClick={onUse}>
            {busy ? (entry.installed ? t("workflow-template-card-opening") : t("workflow-template-card-installing")) : entry.installed ? t("workflow-template-card-open") : t("workflow-template-card-use-template")}
          </Button>
        </>
      }
    />
  );
}
