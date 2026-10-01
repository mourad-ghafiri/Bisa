/**
 * About Bisa — the platform's name and the versions that matter: this
 * desktop's (`__APP_VERSION__`, baked in at build time) and the node's
 * (`GET /health`, read when the dialog opens), the workspace's data
 * directory and the person's npub, the two doors out — the website and the
 * source, baked in beside the version — and where the third-party marks'
 * licences are. The words are `aboutModel.mjs`'s; this draws them.
 */

import { api, inDesktopShell, openExternal, revealPath } from "../api";
import { Button, CopyText, Dialog, ICON, PlatformMark } from "../ui";
import { useAsync } from "../views/_work/useAsync";
import { PRODUCT, aboutFiles, aboutLinks, aboutRows, marksWords, versionCaution, versionLine } from "./aboutModel.mjs";
import type { LicenceFiles } from "./aboutModel.mjs";

/** Where the licence and the notices are, from the shell; nowhere in a browser session. */
async function licenceFiles(): Promise<LicenceFiles | null> {
  if (!inDesktopShell()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<LicenceFiles>("licence_files");
}
import { useWorkspace } from "./useWorkspaceData";
import { t } from "../i18n/l10n.mjs";

export function AboutDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const ws = useWorkspace();
  const health = useAsync((s) => (open ? api.health(s) : Promise.resolve(null)), [open]);
  const node = health.data?.version ?? null;
  const app = __APP_VERSION__;
  const caution = versionCaution(app, node);
  const rows = aboutRows({ app, node, dataDir: ws.info?.data_dir, npub: ws.info?.npub });
  const links = aboutLinks(__APP_HOMEPAGE__, __APP_REPOSITORY__);
  const shipped = useAsync(() => (open ? licenceFiles() : Promise.resolve(null)), [open]);
  const files = aboutFiles(shipped.data);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("shell-about-dialog-about", { PRODUCT })}
      description={health.loading && open ? t("shell-about-dialog-asking-node", { PRODUCT, app }) : versionLine(app, node)}
      width="max-w-md"
      footer={
        <Button size="sm" variant="primary" onClick={onClose}>{t("shell-about-dialog-close")}</Button>
      }
    >
      <div className="flex flex-col gap-2 text-2xs">
        {/* The platform's own mark beside its name — the one place the app
            shows it at rest besides the empty landing. */}
        <div className="flex items-center gap-3">
          <PlatformMark size={40} className="shrink-0" title={t("shell-about-dialog-mark", { PRODUCT })} />
          <div className="flex min-w-0 flex-col">
            <span className="text-sm font-semibold text-text">{PRODUCT}</span>
            <span className="text-text-dim">{versionLine(app, node)}</span>
          </div>
        </div>
        <p className="text-text-dim">{t("shell-about-dialog-runs-goals-through-coding-harnesses-already", { PRODUCT })}</p>
        {caution && (
          <p className="flex items-start gap-1.5 text-warn">
            <ICON.warn size={12} aria-hidden className="mt-0.5 shrink-0" />
            <span>{caution}</span>
          </p>
        )}
        <dl className="flex flex-col gap-1.5">
          {rows.map((r) => (
            <div key={r.label} className="flex items-center gap-2">
              <dt className="w-24 shrink-0 text-text-dim">{r.label}</dt>
              <dd className="min-w-0 flex-1 truncate font-mono text-text" title={r.value}>
                {r.copy ? <CopyText value={r.value} /> : r.value}
              </dd>
            </div>
          ))}
        </dl>
        {(links.length > 0 || files.length > 0) && (
          <div className="flex flex-wrap items-center gap-2">
            {links.map((l) => (
              <Button key={l.id} size="sm" variant="ghost" title={l.url} onClick={() => void openExternal(l.url)}>
                <ICON.open size={12} aria-hidden />
                {l.label}
              </Button>
            ))}
            {files.map((f) => (
              <Button key={f.id} size="sm" variant="ghost" title={f.path} onClick={() => void revealPath(f.path)}>
                <ICON.file size={12} aria-hidden />
                {f.label}
              </Button>
            ))}
          </div>
        )}
        <p className="text-3xs text-text-dim">{marksWords()}</p>
      </div>
    </Dialog>
  );
}
