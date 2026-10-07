/**
 * The doors beside a node that is away, opened (`nodeBootModel.nodeDoors`):
 * none of them needs the node — the node may be the thing that is not
 * there. *Restart now* asks the shell to start it again; the two folders
 * open in the file manager through the shell's own knowledge of them
 * (`folders.rs`); *Quit* is the one way out, through the close flow.
 */

import { revealDataFolder, revealLog } from "../api";
import { restartNode } from "./nodeApi";
import type { NodeDoor } from "./nodeBootModel.mjs";
import { requestQuit } from "./useCloseGuard";

export async function openNodeDoor(door: NodeDoor): Promise<void> {
  switch (door) {
    case "restart_now":
      await restartNode();
      return;
    case "reveal_log":
      await revealLog();
      return;
    case "open_data_folder":
      await revealDataFolder();
      return;
    case "quit":
      requestQuit("footer");
      return;
  }
}
