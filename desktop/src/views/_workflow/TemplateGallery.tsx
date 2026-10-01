/**
 * The catalog's workflow templates as cards, in sections by domain, each with
 * one action: *Use template*, which installs it (and the agents its steps
 * name) and opens the designer on the copy. The screen hands in the entries
 * it already searched and narrowed; this grid draws them.
 *
 * Installing is the only write here, and it is idempotent on the node: a
 * template already installed answers with nothing created, and the gallery
 * says so by opening the copy that exists.
 */

import { useState } from "react";
import { api } from "../../api";
import { navigate } from "../../router";
import type { CatalogEntry } from "../../types";
import { useToast } from "../../ui";
import { attempt } from "../_work/useAsync";
import { domainLabel, groupByDomain } from "./libraryModel.mjs";
import { LIBRARY_GRID } from "./libraryLayout.mjs";
import { installedHit, installedRow, installedWords } from "./workflowPickerModel.mjs";
import { TemplateCard } from "./TemplateCard";
import { t } from "../../i18n/l10n.mjs";

/** *Use template*: install, then open the copy — the one that exists when it already was. */
function useTemplateInstall(onInstalled?: () => void) {
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const use = async (slug: string) => {
    setBusy(slug);
    await attempt(
      async () => {
        const { installed } = await api.installCatalogEntry("workflow", slug);
        let id = installedHit(installed, slug)?.id ?? null;
        if (id === null) {
          // Already installed: find the copy the workspace holds, wherever
          // it is filed — the library, or a goal a person moved it to.
          const { workflows } = await api.workflows({ scope: "all" });
          id = installedRow(installed, workflows, slug)?.workflow.id ?? null;
        }
        toast.ok(installedWords(slug, installed));
        onInstalled?.();
        if (id) navigate({ name: "workflow", id });
        else toast.error(t("workflow-template-gallery-installed-but-copy-could-not-found", { slug }));
      },
      toast.error,
    );
    setBusy(null);
  };
  return { busy, use };
}

export function TemplateGallery({ entries, onInstalled }: { entries: readonly CatalogEntry[]; onInstalled?: () => void }) {
  const { busy, use } = useTemplateInstall(onInstalled);
  return (
    <div className="flex flex-col gap-5">
      {groupByDomain(entries, (e) => e.tags).map(([domain, list]) => (
        <section key={domain}>
          <h3 className="mb-2 text-2xs font-semibold tracking-wide text-text-dim uppercase">{domainLabel(domain)}</h3>
          <div className={LIBRARY_GRID}>
            {list.map((e) => (
              <TemplateCard key={e.slug} entry={e} busy={busy === e.slug} onUse={() => void use(e.slug)} />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
