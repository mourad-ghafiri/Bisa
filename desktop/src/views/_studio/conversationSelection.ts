/**
 * Where an owner keeps *which conversation is picked* and *whether the list
 * is unfolded* (13 — Conversations): four strategies behind one shape, so
 * `useConversationSurface` reads the same facts however they are held.
 *
 * - **route** — the screen's own: `?conversation=` and `?conversations=1`
 *   in the address, so a link lands on a conversation and Back leaves it.
 *   The goal page's tab.
 * - **remembered** — the route, and a memory beside it
 *   (`conversationPickStore.ts`): a pick is remembered for the owner, a link's
 *   pick is copied in, *back* forgets it, and an address that names nothing
 *   comes back to the last one picked here. The Workflow Designer's Agent
 *   pane, whose every door — the library's card, the Inbox, the Pulse, an
 *   origin chip — opens a bare `#/workflows/<id>`.
 * - **kept** — the memory alone, the fold in state: a drawer that floats over
 *   whatever screen is open (Notes, Draw) must not write into that screen's
 *   address, yet reopened it lands on the conversation the person left there.
 * - **root** — the IDE's: the memory, the fold remembered per root
 *   (`agentPaneViewStore.ts`, so a list left open is open after a reload), and
 *   a `?conversation=` link consumed on arrival — copied into the memory and
 *   taken off the address with no history entry, so Back never loops on it.
 */

import { useEffect, useState } from "react";
import { setSearch, useSearchValue } from "../../router";
import { setListOpen as setFold, useListOpen as useFold } from "../_workbench/agentPaneViewStore";
import { useRememberedPick } from "./conversationPickStore";

export type Selection = "route" | "remembered" | "kept" | "root";

export interface ConversationSelection {
  /** The address's pick — `null` for a strategy the address never holds. */
  wanted: string | null;
  /** The owner's last pick — `null` for a strategy that remembers nothing. */
  remembered: string | null;
  listOpen: boolean;
  /** Pick one and fold the list. */
  pick: (id: string) => void;
  /** Leave the pick and fold the list: the surface stands on its own — the owner's thread, the rows, the empty state. */
  back: () => void;
  /** Take a pick that arrived another way — a link — into the memory. Stable; nothing where nothing is remembered. */
  keep: (id: string) => void;
  setListOpen: (open: boolean) => void;
}

/** The `keep` of a strategy that remembers nothing — one function, so it is stable. */
const keepNothing = () => {};

function useRouteSelection(): ConversationSelection {
  const [wanted] = useSearchValue("conversation");
  const [listParam, setListParam] = useSearchValue("conversations");
  return {
    wanted,
    remembered: null,
    listOpen: listParam === "1",
    pick: (id) => setSearch({ conversation: id, conversations: null }),
    // One address change, one history entry: the pick and the fold together.
    back: () => setSearch({ conversation: null, conversations: null }),
    keep: keepNothing,
    setListOpen: (open) => setListParam(open ? "1" : null),
  };
}

/** The route, remembering: every pick kept for the owner, *back* forgetting it. */
function remembering(route: ConversationSelection, remembered: string | null, keep: (id: string | null) => void): ConversationSelection {
  return {
    ...route,
    remembered,
    pick: (id) => {
      keep(id);
      route.pick(id);
    },
    back: () => {
      keep(null);
      route.back();
    },
    keep,
  };
}

/**
 * One of the four, chosen once at mount — a strategy, not a switch flipped
 * later. `owner` names what the memory is kept under (`ownerKey`).
 */
export function useConversationSelection(selection: Selection, owner: string): ConversationSelection {
  // Every hook is called unconditionally so the hook order is stable; only
  // one strategy is read. Cheap: two search reads, one memory read, one fold
  // read and a state pair.
  const route = useRouteSelection();
  const [kept, keep] = useRememberedPick(owner);
  const rootFold = useFold(owner);
  const [stateFold, setStateFold] = useState(false);
  // The IDE takes a link's pick into its memory and off the address, once.
  const linked = selection === "root" ? route.wanted : null;
  useEffect(() => {
    if (!linked) return;
    keep(linked);
    setFold(owner, false);
    setSearch({ conversation: null }, { replace: true });
  }, [linked, owner, keep]);
  switch (selection) {
    case "route":
      return route;
    case "remembered":
      return remembering(route, kept, keep);
    case "kept":
      return {
        wanted: null,
        remembered: kept,
        listOpen: stateFold,
        pick: (id) => {
          keep(id);
          setStateFold(false);
        },
        back: () => {
          keep(null);
          setStateFold(false);
        },
        keep,
        setListOpen: setStateFold,
      };
    case "root":
      return {
        wanted: null,
        remembered: kept,
        listOpen: rootFold,
        pick: (id) => {
          keep(id);
          setFold(owner, false);
        },
        back: () => {
          keep(null);
          setFold(owner, false);
        },
        keep,
        setListOpen: (open) => setFold(owner, open),
      };
  }
}
