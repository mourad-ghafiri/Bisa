/**
 * The project's run command for a checkout (ide/18), read once for every
 * surface that names it and acted on in one way. The node resolves it
 * (`GET /workstreams/{wid}/run-command` — the command, and whether this
 * machine has approved it; 404 when the project sets none) and the shell
 * runs it (`terminal_open` with `run: true` asks the route itself), so
 * nothing here names a program or a path. The Terminal caret's *Run* item
 * and the Browser's ⌘⇧R door both call `runTheCommand`, so the two cannot
 * disagree about what an unapproved command does: it is not run — the
 * card that approves it opens instead.
 */

import { api } from "../api";
import { useEngineEvents } from "../bus";
import { useReloadOnReconnect } from "../ui/useReloadOnReconnect";
import { useAsync } from "../views/_work/useAsync";
import { openPanelView } from "../views/_workbench/rightPanelStore";
import { approvalWords } from "../views/_workbench/runCommandModel.mjs";
import type { RunFacts } from "../views/_workbench/runCommandModel.mjs";
import { openTerminalIn } from "./useTerminals";

/** The command and its approval for a checkout; `null` while there is none, or off a checkout. Re-read when a setting changes. */
export function useRunCommand(wid: string | null): { run: RunFacts | null; reload: () => void } {
  const read = useAsync(
    (s) =>
      wid
        ? api.runCommand(wid, s).then(
            (r): RunFacts | null => ({ command: r.command, trusted: r.trusted }),
            () => null,
          )
        : Promise.resolve(null),
    [wid],
  );
  useEngineEvents((e) => {
    if (e.payload.type === "settings_changed" && wid) read.reload();
  });
  useReloadOnReconnect(read.reload);
  return { run: read.data ?? null, reload: read.reload };
}

/**
 * Run the command in a terminal here — the tab *run · <command>* — when
 * this machine has approved it; else say so (`approvalWords`) and open the
 * card that approves it. One act for the Terminal's item and for ⌘⇧R.
 */
export function runTheCommand(wid: string, run: RunFacts, say: (words: string) => void): void {
  if (run.trusted) {
    openTerminalIn({ scope: "workstream", id: wid, label: run.command, run: true });
    return;
  }
  say(approvalWords(run.command));
  openPanelView("about", "settings", `workstream:${wid}`);
}
