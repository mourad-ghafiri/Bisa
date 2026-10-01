/**
 * About › Checkout's *Git config* card: who commits here and where that
 * comes from, then the repository's local layer as a form — every key the
 * platform knows, what this repository sets and what it inherits from the
 * global config, *Inherit* to clear a value. The form's edits are part of
 * the view's draft (`useProjectSettingsDraft`, `useConfigEdits`) and the
 * toolbar's Save writes them through `PUT /workstreams/{wid}/git/config`;
 * an identity that comes to resolve answers the platform's *who commits?*
 * and commits a refused settlement.
 */
import { Button, Chip, ErrorNote, ICON, Skeleton } from "../../ui";
import { GitConfigForm } from "./GitConfigForm";
import { formatIdent } from "./gitConfigModel.mjs";
import { identitySentence, identityState, pinOffer, suggestionWords } from "./gitIdentityModel.mjs";
import type { ProjectSettingsDraft } from "./useProjectSettingsDraft";
import { t } from "../../i18n/l10n.mjs";

export function GitConfigCard({ draft }: { draft: ProjectSettingsDraft }) {
  const view = draft.identity;
  const state = identityState(view);
  const suggestion = suggestionWords(view);
  const pin = pinOffer(view);
  if (!view && !draft.config && !draft.error) return <Skeleton className="h-24 w-full" />;
  if (!draft.config) return <ErrorNote error={draft.error ?? t("work-git-config-card-repository-s-git-config-could-not")} retry={draft.reload} />;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        {state === "local" && <Chip tone="ok" icon={ICON.ok}>{t("work-git-config-card-set-repository")}</Chip>}
        {state === "inherited" && view?.profile && <Chip tone="ok" icon={ICON.organization}>{t("work-git-config-card-from-profile", { profile: view.profile })}</Chip>}
        {state === "inherited" && !view?.profile && <Chip tone="quiet" icon={ICON.info}>{t("work-git-config-card-inherited-from-global-config")}</Chip>}
        {state === "missing" && <Chip tone="warn" icon={ICON.warn}>{t("work-git-config-card-nobody-set-commit-here")}</Chip>}
        {view?.name && view?.email && (
          <span className="min-w-0 truncate font-mono text-2xs text-text-dim">{formatIdent({ name: view.name, email: view.email })}</span>
        )}
      </div>
      <p className={`text-2xs ${state === "missing" ? "text-warn" : "text-text-dim"}`} role={state === "missing" ? "alert" : undefined}>
        {identitySentence(view)}
      </p>
      {/* One click to keep the inherited author whatever the global config becomes — the same local write as *Commit as*; *Inherit* on the row is the way back. */}
      {pin && (
        <div>
          <Button size="sm" variant="default" disabled={draft.saving} onClick={() => void draft.commitAs(pin.ident)}>
            {pin.button}
          </Button>
        </div>
      )}
      {suggestion && (
        <div className="flex flex-wrap items-center gap-2 rounded-control border border-border bg-surface-2 px-2 py-1.5">
          <ICON.account size={13} aria-hidden className="shrink-0 text-text-dim" />
          <span className="min-w-0 flex-1 text-2xs text-text-dim">{suggestion.sentence}</span>
          <Button size="sm" variant="primary" disabled={draft.saving} onClick={() => void draft.commitAs(suggestion.ident)}>
            {suggestion.button}
          </Button>
        </div>
      )}
      {/* A repository's account pin is the Connection card's Select, not a text field here. */}
      <GitConfigForm form={draft.form} scope="local" omit={["codehost.account"]} />
    </div>
  );
}
