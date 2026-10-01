/**
 * An id the roster's list does not hold — an address somebody followed, a
 * record the store left out because it could not read it. The screen asks
 * the node for that one record and says what the node said
 * (`rosterModel.absentRecord`): its own *nothing answers to this id* note
 * when there is no such thing, the node's sentence — naming the file — when
 * the record is there and cannot be read, and the list read again when the
 * record is simply newer than the list.
 */

import { useEffect, type ReactNode } from "react";
import { Card, ErrorNote, Pending } from "../../ui";
import { absentRecord } from "../rosterModel.mjs";
import { useAsync } from "./useAsync";

export function AbsentRecord({
  id,
  what,
  read,
  onHere,
  gone,
}: {
  id: string;
  /** The record's word for the *reading …* line. */
  what: string;
  /** The single read of the record. */
  read: (id: string, signal: AbortSignal) => Promise<unknown>;
  /** The node has it after all: the list is read again. */
  onHere: () => void;
  /** The screen's own note for an id nothing answers to. */
  gone: ReactNode;
}) {
  const record = useAsync((s) => read(id, s), [id]);
  const said = absentRecord(record);
  const here = said.state === "here";
  useEffect(() => {
    if (here) onHere();
    // The list's reload is the screen's; a fresh closure each render is the same door.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [here]);
  if (said.state === "gone") return <>{gone}</>;
  if (said.state === "unreadable") return <ErrorNote error={said.sentence} retry={record.reload} />;
  return (
    <Card>
      <Pending what={what} rows={1} />
    </Card>
  );
}
