/**
 * A conversation surface, as one hook every painter shares (13 —
 * Conversations): the goal page's Conversation tab, the Workflow Designer's
 * Agent pane, the drawers beside a note or a drawing, and the IDE's Agent
 * pane and Agent mode all read the list of the conversations about their
 * thing, the one they are on, whether the list is unfolded, a search over
 * the words said (the node's), live or archived, and *New conversation*.
 * Starting one is **one click and no name**: the conversation is made
 * untitled and opened at once, and the thread's ⋮ menu names it later. One
 * hook, so a rule cannot hold on one surface and not another.
 *
 * **The pick is the owner's, the list is a view.** What the surface is on is
 * an id — the address's or the owner's remembered one (`pickedId`) — whose
 * record is read by id (`useConversation`) and held to the owner
 * (`belongsTo`): the list's search, its *Archived* switch, its page and a
 * reload in flight never change it. A pick that is gone, or another owner's
 * (a stale link), is dropped through the selection's `back`; a network error
 * drops nothing. Where the pick lives is the caller's strategy
 * (`conversationSelection.ts`): the address for a screen, remembered for the
 * designer's pane and the drawers, the IDE's root. The list's search and its
 * switch are the owner's view (`useViewState`, under the source's place):
 * leaving the screen, and a restart, come back to the list as it was narrowed.
 */

import { useEffect } from "react";
import type { ConversationView } from "../../types";
import { useToast } from "../../ui";
import { startConversation, useConversation, useConversations } from "../_workbench/conversationsStore";
import { attempt } from "../_work/useAsync";
import { useViewState } from "../../shell/viewMemoryStore";
import { flagValue, textValue } from "../../shell/viewValuesModel.mjs";
import { belongsTo, listQuery, ownerKey, ownerPlace, pickedId, selectedOf, surfaceView, surfaceWords } from "./conversationSurfaceModel.mjs";
import type { SurfaceSource, SurfaceView, SurfaceWords } from "./conversationSurfaceModel.mjs";
import { useConversationSelection, type Selection } from "./conversationSelection";
import { titleOf } from "./conversationsModel.mjs";

export interface ConversationSurfaceState {
  source: SurfaceSource;
  /** What the list shows: the search's and the switch's rows. */
  rows: readonly ConversationView[];
  loading: boolean;
  /** The list's error while it has no rows, or the pick's record's — never a *not found*, which drops the pick instead. */
  error: string | null;
  reload: () => void;
  q: string;
  setQ: (q: string) => void;
  archived: boolean;
  setArchived: (archived: boolean) => void;
  /** The list is unfolded — the door's state. */
  listOpen: boolean;
  toggleList: () => void;
  /** The id the surface is on, whatever the list shows. */
  pickId: string | null;
  /** Its record, once read and found to be this owner's. */
  selected: ConversationView | null;
  view: SurfaceView;
  /** The picked conversation's name while a thread shows. */
  title: string | null;
  pick: (id: string) => void;
  back: () => void;
  /** *New conversation*: made untitled and opened at once. */
  start: () => void;
  /** The bar's one door: to the list, and back. */
  door: { label: string; count: number; hint: string; open: boolean; disabled: boolean; onToggle: () => void };
  words: SurfaceWords;
  /** The owner has a surface of its own to stand on while nothing is picked — the goal's thread. */
  hasFallback: boolean;
}

export function useConversationSurface(source: SurfaceSource, selection: Selection, opts: { fallback?: boolean } = {}): ConversationSurfaceState {
  const toast = useToast();
  const hasFallback = opts.fallback === true;
  const sel = useConversationSelection(selection, ownerKey(source.owner));
  const place = source.place ?? ownerPlace(source.owner);
  const [q, setQ] = useViewState(place, "conversations.q", "", textValue);
  const [archived, setArchived] = useViewState(place, "conversations.archived", false, flagValue);
  const list = useConversations(listQuery(source, { q, archived }));
  const pickId = pickedId(sel.wanted, sel.remembered);
  // The pick's record, by id — the list's row when it is there, the record otherwise.
  const record = useConversation(pickId);
  const found = selectedOf(list.rows, pickId) ?? record.data ?? null;
  const ours = found !== null && belongsTo(found, source);
  const selected = ours ? found : null;
  // A record that is not this owner's — a stale link, another owner's — or
  // gone (the node's *not found*) is dropped; a network error drops nothing.
  const { back, wanted, keep } = sel;
  useEffect(() => {
    if (pickId && ((found && !ours) || record.missing)) back();
  }, [pickId, found, ours, record.missing, back]);
  // A pick that arrived by link — the address names one of ours — is the
  // owner's last pick from now on, so the next bare visit lands on it.
  const shown = selected?.id ?? null;
  useEffect(() => {
    if (wanted && shown === wanted) keep(wanted);
  }, [wanted, shown, keep]);
  // An id is named and its record not read yet: nothing flashes between
  // *New conversation* and the new thread, or between a link and its page.
  const settling = pickId !== null && selected === null && record.loading;
  const error: string | null = (list.rows.length === 0 ? list.error : null) ?? (record.error && !record.missing ? record.error : null) ?? null;
  const count = list.rows.length;
  const view = surfaceView({ selected, settling, error, listOpen: sel.listOpen, loading: list.loading, count, hasFallback });
  const words = surfaceWords(source, { count, open: view === "list", picked: selected !== null, hasFallback });
  const start = () =>
    void attempt(
      async () => {
        const made = await startConversation(source.owner);
        sel.pick(made.id);
        list.reload();
      },
      toast.error,
    );
  return {
    source,
    rows: list.rows,
    loading: list.loading,
    error,
    reload: list.reload,
    q,
    setQ,
    archived,
    setArchived,
    listOpen: sel.listOpen,
    toggleList: () => sel.setListOpen(!sel.listOpen),
    pickId,
    selected,
    view,
    title: selected ? titleOf(selected) : null,
    pick: sel.pick,
    back: sel.back,
    start,
    door: {
      label: words.door,
      count,
      hint: words.hint,
      open: view === "list",
      // The list is the only view there is — nothing picked, no surface of the
      // owner's own to go back to: a press would show what already shows.
      disabled: view === "list" && selected === null && !hasFallback,
      onToggle: () => sel.setListOpen(!sel.listOpen),
    },
    words,
    hasFallback,
  };
}
