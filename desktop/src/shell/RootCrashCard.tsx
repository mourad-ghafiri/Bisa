/**
 * The card where the window was, when the shell's own tree threw
 * (`rootCrashModel`). It draws nothing from the tree that fell — no kit
 * provider, no toast rail, no context — and every door works without the
 * node: try the tree again, reload the page, restart the node, reveal the
 * log, open the data folder, quit. Under the doors, the node's own line
 * while it is away (`nodeBootStore`), since a node that would not start is
 * the likeliest reason to be here.
 */

import { useEffect, useState } from "react";
import { inDesktopShell, revealDataFolder, revealLog } from "../api";
import { Button } from "../ui/Button";
import { restartNode } from "./nodeApi";
import { bootLine } from "./nodeBootModel.mjs";
import { useNodeBoot } from "./nodeBootStore";
import { doorRank, rootCrash, rootCrashDoors, rootCrashWords, rootDoorFailedWords, rootDoorWords, type RootDoor } from "./rootCrashModel.mjs";
import { requestQuit } from "./useCloseGuard";

export function RootCrashCard({ error, reset }: { error: Error; reset: () => void }) {
  // Mounted is crashed: the ways out read it (`closeFlow`'s `bare`) and
  // skip the question and the save, whose hosts went with the tree.
  useEffect(() => {
    rootCrash.markCrashed(error);
    return () => rootCrash.clearCrash();
  }, [error]);
  const boot = useNodeBoot();
  const line = bootLine(boot);
  const [failed, setFailed] = useState<string | null>(null);
  const words = rootCrashWords();
  const doors = rootCrashDoors(inDesktopShell());

  const open = async (door: RootDoor): Promise<void> => {
    switch (door) {
      case "try_again":
        reset();
        return;
      case "reload":
        window.location.reload();
        return;
      case "restart_node":
        await restartNode();
        return;
      case "reveal_log":
        await revealLog();
        return;
      case "open_data_folder":
        await revealDataFolder();
        return;
      case "quit":
        requestQuit("crash");
        return;
    }
  };
  const act = (door: RootDoor) => {
    setFailed(null);
    open(door).catch((e: unknown) => setFailed(rootDoorFailedWords(door, e)));
  };

  return (
    <div className="flex h-full items-center justify-center p-6">
      <div role="alert" className="w-full max-w-lg rounded-card border border-border bg-surface p-4 shadow-sm">
        <h1 className="text-sm font-semibold text-text">{words.title}</h1>
        <p className="mt-1 max-w-measure text-2xs leading-relaxed text-text-dim">{words.body}</p>
        <pre className="mt-3 max-h-40 overflow-auto rounded-control bg-surface-2 p-2 text-2xs text-text-dim">{error.message}</pre>
        {line && (
          <p role="status" className="mt-2 text-2xs text-text-dim">
            {line}
          </p>
        )}
        {failed && (
          <p role="status" className="mt-2 text-2xs text-danger">
            {failed}
          </p>
        )}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {doors.map((door) => (
            <Button key={door} size="sm" variant={doorRank(door)} onClick={() => act(door)}>
              {rootDoorWords(door)}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}
