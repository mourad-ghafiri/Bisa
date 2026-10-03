/**
 * Settings › Library › Addons: what floats over the app, what each may do,
 * and where a new one comes from (18 — Addons).
 *
 * Three sections. **Installed**: every addon here — running or off, built in
 * or imported — with its switch, one switch per permission it declares (a
 * grant is exactly the declaration, never wider), and *Remove*. **From the
 * catalog**: the built-ins not yet installed, each behind the review that
 * names what it asks for before *Install*. **Import an addon**: the folder
 * picker, the node's own judgement of the folder (`POST /addons/validate`)
 * shown before anything is copied, then the same review with the grants
 * left to the person — nothing is granted by default. Above them, the
 * machine's one switch, `addons.enabled`. Every fact is the addons store's
 * — the list, the machine's switch — read here, kept current by the layer's
 * one sync; a switch flipped here reads as flipped at once and comes back if
 * the node refuses.
 *
 * A manifest's own words — name, description — are the developer's content
 * and shown as text; every sentence around them is the catalog's.
 */

import { useState } from "react";
import { ApiError, api, inDesktopShell, pickFolder } from "../../api";
import { t } from "../../i18n/l10n.mjs";
import { tx } from "../../i18n/l10n.mjs";
import type { Addon, AddonManifest, AddonOffer, AddonPermission, AddonProblem } from "../../types";
import { Button, Card, Chip, ConfirmDialog, ErrorNote, Pending, Section, Switch, useToast } from "../../ui";
import { declaredOf, grantToggle, isGranted, offerRows, originWords, permissionWords, problemLines, reviewWords, sortAddons, sortedPermissions, stateWords } from "../../addons/addonsModel.mjs";
import { ADDONS_ENABLED_KEY, importAddon, installAddonFromCatalog, refreshAddons, removeAddon, setAddonEnabled, setAddonGrants, useAddons } from "../../addons/addonsStore";
import { useAsync } from "../_work/useAsync";
import { pendingRows } from "./loadModel.mjs";

/** What the review is about: a built-in to install, or an addon's folder to import with the grants chosen so far. */
type Review =
  | { kind: "catalog"; slug: string; manifest: AddonManifest }
  | { kind: "folder"; path: string; manifest: AddonManifest; granted: AddonPermission[]; enabled: boolean };

function PermissionSwitches({ addon, busy, onChange }: { addon: Addon; busy: boolean; onChange: (granted: AddonPermission[]) => void }) {
  const declared = sortedPermissions(declaredOf(addon.manifest));
  if (declared.length === 0) return <p className="text-2xs text-text-dim">{t("settings-addons-panel-asks-nothing")}</p>;
  return (
    <ul className="flex flex-col gap-1">
      {declared.map((p, i) => (
        <li key={i}>
          <Switch checked={isGranted(addon.granted, p)} disabled={busy} onChange={(on) => onChange(grantToggle(addon.granted, declared, p, on))} label={permissionWords(p)} />
        </li>
      ))}
    </ul>
  );
}

function InstalledRow({ addon, onError }: { addon: Addon; onError: (e: string | null) => void }) {
  const [busy, setBusy] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const act = async (f: () => Promise<void>, fallback: string) => {
    setBusy(true);
    try {
      await f();
      onError(null);
    } catch (e) {
      onError(e instanceof ApiError ? e.message : fallback);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            {/* content, never translated: the manifest's name and version. */}
            <span className="truncate text-sm font-medium text-text">{addon.manifest.name}</span>
            <span className="text-2xs text-text-dim">{addon.manifest.version}</span>
            <Chip tone="neutral">{originWords(addon.origin)}</Chip>
            <Chip tone={addon.active ? "accent" : "neutral"}>{stateWords(addon)}</Chip>
          </div>
          {/* content, never translated: the manifest's description. */}
          <p className="mt-1 max-w-measure text-2xs leading-relaxed text-text-dim">{addon.manifest.description}</p>
        </div>
        <Switch
          checked={addon.enabled}
          disabled={busy || (!addon.enabled && !addon.files_present)}
          onChange={(on) => void act(() => setAddonEnabled(addon.id, on), t("settings-addons-panel-could-not-switch"))}
          label={t("settings-addons-panel-on")}
        />
      </div>
      <div className="mt-3 flex flex-col gap-2">
        <p className="text-2xs font-semibold text-text-dim">{t("settings-addons-panel-may")}</p>
        <PermissionSwitches addon={addon} busy={busy} onChange={(granted) => void act(() => setAddonGrants(addon.id, granted), t("settings-addons-panel-could-not-grant"))} />
      </div>
      <div className="mt-3 flex items-center justify-between gap-2">
        {!addon.files_present && <p className="text-2xs text-text-dim">{t("settings-addons-panel-files-not-here")}</p>}
        <span className="flex-1" />
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => setConfirmRemove(true)}>{t("settings-addons-panel-remove")}</Button>
      </div>
      <ConfirmDialog
        open={confirmRemove}
        onClose={() => setConfirmRemove(false)}
        onConfirm={() => {
          setConfirmRemove(false);
          void act(() => removeAddon(addon.id), t("settings-addons-panel-could-not-remove"));
        }}
        title={t("settings-addons-panel-remove-title", { name: addon.manifest.name })}
        body={t("settings-addons-panel-remove-body")}
        confirmLabel={t("settings-addons-panel-remove")}
        danger
      />
    </Card>
  );
}

function ReviewDialog({ review, busy, onClose, onConfirm, onGrant, onEnable }: { review: Review | null; busy: boolean; onClose: () => void; onConfirm: () => void; onGrant: (granted: AddonPermission[]) => void; onEnable: (on: boolean) => void }) {
  if (!review) return <ConfirmDialog open={false} onClose={onClose} onConfirm={onConfirm} title="" body="" />;
  const words = reviewWords(review.manifest);
  const declared = sortedPermissions(declaredOf(review.manifest));
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      onConfirm={onConfirm}
      title={words.title}
      confirmLabel={busy ? t("settings-addons-panel-installing") : review.kind === "catalog" ? t("settings-addons-panel-install") : t("settings-addons-panel-import")}
      body={
        <div className="flex flex-col gap-2 text-2xs">
          <p className="text-text-dim">{words.version}</p>
          <p>{words.lead}</p>
          {review.kind === "catalog" ? (
            <ul className="list-disc pl-4">
              {words.lines.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          ) : (
            <>
              <ul className="flex flex-col gap-1">
                {declared.map((p, i) => (
                  <li key={i}>
                    <Switch checked={isGranted(review.granted, p)} onChange={(on) => onGrant(grantToggle(review.granted, declared, p, on))} label={permissionWords(p)} />
                  </li>
                ))}
              </ul>
              <Switch checked={review.enabled} onChange={onEnable} label={t("settings-addons-panel-turn-on-at-once")} />
            </>
          )}
          <p className="text-text-dim">{t("settings-addons-panel-review-walls")}</p>
        </div>
      }
    />
  );
}

export function AddonsPanel() {
  const { addons, loaded, failed, switchedOn } = useAddons();
  const offers = useAsync((s) => api.addonOffers(s), [addons.length]);
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);
  const [problems, setProblems] = useState<AddonProblem[] | null>(null);
  const [review, setReview] = useState<Review | null>(null);
  const [busy, setBusy] = useState(false);

  // The node says `settings_changed`; the store re-reads and every surface follows.
  const setSwitch = async (on: boolean) => {
    try {
      await api.setSettings("machine", { [ADDONS_ENABLED_KEY]: on });
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("settings-addons-panel-could-not-switch"));
    }
  };

  const startImport = async () => {
    const path = await pickFolder(t("settings-addons-panel-choose-folder"));
    if (!path) return;
    setBusy(true);
    try {
      // The node reads the folder — the webview never does — and answers the
      // manifest with every problem; a clean folder opens the review with
      // nothing granted yet.
      const { manifest, problems: found } = await api.validateAddon(path);
      setProblems(found.length > 0 ? found : null);
      if (found.length > 0) return;
      setReview({ kind: "folder", path, manifest, granted: [], enabled: true });
      setError(null);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("settings-addons-panel-folder-not-addon"));
    } finally {
      setBusy(false);
    }
  };

  const confirm = async () => {
    if (!review) return;
    setBusy(true);
    try {
      if (review.kind === "catalog") {
        await installAddonFromCatalog(review.slug);
        toast.ok(t("settings-addons-panel-installed-name", { name: review.manifest.name }));
      } else {
        const addon = await importAddon(review.path, review.granted, review.enabled);
        toast.ok(t("settings-addons-panel-imported", { name: addon.manifest.name }));
      }
      setReview(null);
      setError(null);
      offers.reload();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : t("settings-addons-panel-could-not-install"));
      setReview(null);
    } finally {
      setBusy(false);
    }
  };

  const installed = sortAddons(addons);
  const offer = offerRows(offers.data?.offers ?? []);

  return (
    <div className="flex flex-col gap-6">
      <Section title={t("settings-addons-panel-addons")}>
        <Card>
          <p className="mb-3 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-addons-panel-blurb")}</p>
          <Switch checked={switchedOn} onChange={(on) => void setSwitch(on)} label={t("settings-addons-panel-show-addons-machine")} hint={t("settings-addons-panel-machine-hint")} />
        </Card>
      </Section>

      <Section title={t("settings-addons-panel-installed")}>
        {error && <ErrorNote error={error} />}
        {/* The list's own read refused: said, with a way to ask again — never *none installed*, which nobody checked. */}
        {failed && <ErrorNote error={failed} retry={() => void refreshAddons()} />}
        {!loaded ? (
          failed ? null : (
            <Pending what={t("settings-addons-panel-addons")} rows={pendingRows(t("settings-addons-panel-addons"))} />
          )
        ) : installed.length === 0 ? (
          <Card>
            <p className="text-2xs text-text-dim">{t("settings-addons-panel-none-installed")}</p>
          </Card>
        ) : (
          <div className="flex flex-col gap-3">
            {installed.map((a) => (
              <InstalledRow key={a.id} addon={a} onError={setError} />
            ))}
          </div>
        )}
      </Section>

      <Section title={t("settings-addons-panel-from-catalog")}>
        {offers.error && <ErrorNote error={offers.error} retry={offers.reload} />}
        {offers.loading && !offers.data ? (
          <Pending what={t("settings-addons-panel-catalog")} rows={pendingRows(t("settings-addons-panel-catalog"))} />
        ) : offer.length === 0 ? (
          <Card>
            <p className="text-2xs text-text-dim">{t("settings-addons-panel-catalog-all-installed")}</p>
          </Card>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2">
            {offer.map((o) => (
              <OfferRow key={o.slug} offer={o} onInstall={() => setReview({ kind: "catalog", slug: o.slug, manifest: o.manifest })} />
            ))}
          </div>
        )}
      </Section>

      <Section title={t("settings-addons-panel-import-addon")}>
        <Card>
          <p className="mb-3 max-w-measure text-2xs leading-relaxed text-text-dim">{t("settings-addons-panel-import-blurb")}</p>
          {problems && (
            <div className="mb-3">
              <ErrorNote error={t("settings-addons-panel-folder-problems", { n: problems.length })} />
              <ul className="mt-2 list-disc pl-4 text-2xs text-text-dim">
                {problemLines(problems, tx).map((line, i) => (
                  <li key={i}>{line}</li>
                ))}
              </ul>
            </div>
          )}
          {inDesktopShell() ? (
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => void startImport()}>{t("settings-addons-panel-import-addon")}</Button>
          ) : (
            <p className="text-2xs text-text-dim">{t("settings-addons-panel-importing-needs-desktop")}</p>
          )}
        </Card>
      </Section>

      <ReviewDialog
        review={review}
        busy={busy}
        onClose={() => setReview(null)}
        onConfirm={() => void confirm()}
        onGrant={(granted) => setReview((r) => (r && r.kind === "folder" ? { ...r, granted } : r))}
        onEnable={(enabled) => setReview((r) => (r && r.kind === "folder" ? { ...r, enabled } : r))}
      />
    </div>
  );
}

function OfferRow({ offer, onInstall }: { offer: AddonOffer; onInstall: () => void }) {
  return (
    <Card>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          {/* content, never translated: the built-in's own name and description. */}
          <p className="truncate text-sm font-medium text-text">{offer.manifest.name}</p>
          <p className="mt-1 text-2xs leading-relaxed text-text-dim">{offer.manifest.description}</p>
        </div>
        <Button size="sm" variant="ghost" onClick={onInstall}>{t("settings-addons-panel-install")}</Button>
      </div>
    </Card>
  );
}
