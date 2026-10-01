/**
 * The aux pane's transcript occupant (ide/09): one session's transcript,
 * whole, from `?aux=transcript&auxId=<session id>` — drawn by the shell from
 * the URL, so it opens beside any screen a session is named on and survives
 * Back and a reload. The header names the session (the agent, the harness
 * and model behind it), its state mark and whether the tail still follows;
 * under it the tail itself (`SessionTranscript`); in the IDE, *Open as a
 * tab* puts the same transcript in the centre.
 */

import { setSearch, useRoute } from "../router";
import { Button, ICON, SessionMark, Tooltip } from "../ui";
import { SessionTranscript } from "../views/_work/SessionTranscript";
import { transcriptTabTitle, transcriptTitle, transcriptWords } from "../views/_work/sessionTranscriptModel.mjs";
import { openDoc } from "../views/_workbench/workbenchStore";
import { rootKey, tabId } from "../views/_workbench/workbenchModel.mjs";
import { useSessionRow } from "./useSessionRow";
import { t } from "../i18n/l10n.mjs";

export function TranscriptPane({ session }: { session: string | null }) {
  const route = useRoute();
  const row = useSessionRow(session ?? "");
  if (!session) return <p className="p-3 text-2xs text-text-dim">{t("shell-transcript-pane-nothing-show-address-names-session")}</p>;
  const line = transcriptWords(row);
  const inWorkbench = route.name === "workbench";
  const openAsTab = () => {
    if (!inWorkbench) return;
    const tab = { kind: "transcript" as const, session, title: transcriptTabTitle(row) };
    openDoc(rootKey(route.scope, route.id), tab);
    setSearch({ doc: tabId(tab), aux: null, auxId: null });
  };
  return (
    <div className="flex h-full min-h-0 flex-col gap-2 p-3">
      <div className="flex shrink-0 items-center gap-2 text-2xs">
        {row && <SessionMark state={row.state} />}
        <span className="min-w-0 truncate font-mono text-text" title={session}>
          {transcriptTitle(row)}
        </span>
        <span className="flex-1" />
        {inWorkbench && (
          <Tooltip label={t("shell-artifact-pane-open-document-tab-centre")}>
            <span className="inline-flex">
              <Button size="sm" variant="ghost" onClick={openAsTab} aria-label={t("shell-artifact-pane-open-tab")}>
                <ICON.file size={13} aria-hidden />
              </Button>
            </span>
          </Tooltip>
        )}
      </div>
      <p className="shrink-0 text-2xs text-text-dim">{line.words}</p>
      <SessionTranscript session={session} live={line.live} className="min-h-0 flex-1" />
    </div>
  );
}
