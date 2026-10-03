/**
 * What the centre shows when nothing is active: where you are, and the three
 * things people do first. For a workbench with no root at all (a workspace
 * with no projects yet) it is the way in.
 */

import { CommandHint } from "../../shell/CommandHint";
import { Button, ICON, PlatformMark } from "../../ui";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function CenterLanding({
  title,
  subtitle,
  path,
  onOpenFile,
  onNewTerminal,
  onAgents,
  onNewWorkstream,
}: {
  title: string;
  subtitle?: string | null;
  path?: string | null;
  onOpenFile: () => void;
  onNewTerminal: (() => void) | null;
  onAgents: (() => void) | null;
  /** Present on a project's primary: a branch of its own is one click away. */
  onNewWorkstream?: (() => void) | null;
}) {
  return (
    <div className="flex h-full min-w-0 flex-col items-center justify-center gap-4 p-8 text-center">
      {/* As wide as the column allows and no wider: on a narrow window the words wrap and the path ends in an ellipsis, never past the column's edges. */}
      <div className="flex w-full max-w-md min-w-0 flex-col gap-1">
        <h3 className="text-base font-semibold tracking-tight">{title}</h3>
        {subtitle && <p className="text-2xs text-text-dim">{subtitle}</p>}
        {path && <p className="truncate font-mono text-2xs text-text-dim" title={path}>{path}</p>}
      </div>
      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button size="sm" onClick={onOpenFile}>
          <ICON.search size={12} aria-hidden />{t("workbench-center-landing-open-file")}<CommandHint id="quick_open" />
        </Button>
        {onNewTerminal && (
          <Button size="sm" onClick={onNewTerminal}>
            <ICON.harness size={12} aria-hidden />{t("workbench-center-landing-new-terminal")}<CommandHint id="new_terminal" />
          </Button>
        )}
        {onAgents && (
          <Button size="sm" onClick={onAgents}>
            <ICON.agent size={12} aria-hidden />{t("workbench-center-landing-talk-agents")}<CommandHint id="toggle_ide_mode" />
          </Button>
        )}
        {onNewWorkstream && (
          <Button size="sm" onClick={onNewWorkstream}>
            <ICON.workstream size={12} aria-hidden />{t("workbench-center-landing-open-workstream")}<CommandHint id="new_workstream" />
          </Button>
        )}
      </div>
      <ul className="mt-2 flex flex-col gap-1 text-2xs text-text-dim">
        <li>{rich("workbench-center-landing-panels-legend", { show_terminal: <CommandHint id="show_terminal" />, toggle_right_panel: <CommandHint id="toggle_right_panel" />, toggle_rail: <CommandHint id="toggle_rail" /> })}</li>
        <li>{rich("workbench-center-landing-tabs-legend", { git_panel: <CommandHint id="git_panel" />, panel_files: <CommandHint id="panel_files" />, rename: <CommandHint id="rename" />, close_tab: <CommandHint id="close_tab" />, reopen_tab: <CommandHint id="reopen_tab" /> })}</li>
      </ul>
    </div>
  );
}

/** The way in when the workspace has no project yet. */
export function EmptyIdeLanding({ onNewProject }: { onNewProject: () => void }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
      <PlatformMark size={48} title={t("workbench-center-landing-bisa-mark")} />
      <div className="flex max-w-md flex-col gap-1">
        <h3 className="text-base font-semibold tracking-tight">{t("workbench-center-landing-add-project-get-started")}</h3>
        <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
          {t("workbench-center-landing-project-is-a-folder")}
        </p>
      </div>
      <Button size="sm" variant="primary" onClick={onNewProject}>
        <ICON.add size={12} aria-hidden />{t("workbench-center-landing-new-project")}</Button>
    </div>
  );
}
