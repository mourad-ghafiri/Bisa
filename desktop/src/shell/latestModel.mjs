/**
 * One answer at a time, and the newest asked for: the order of reads a
 * surface makes of one thing — a document read on mount and again on every
 * burst of watcher frames, a listing asked for while the last one is still
 * out. Answers land in whatever order the network hands them back, so a
 * reader that applied each as it came could end on the **older** one: the
 * file as it was before the change that asked for the second read. And an
 * answer that lands after its reader left — a tab closed while its file was
 * being read — must write nothing: what it wrote nobody would ever forget.
 *
 * A reader takes a ticket as it asks (`begin`) and applies the answer only
 * if the ticket still `lands`: it is the newest, and the reader is there.
 * `close` is the reader leaving; `open` is it coming back (a component that
 * mounts, unmounts and mounts again, as React's strict mode does).
 *
 * Facts only, no React and no timer. Plain `.mjs`, so `node --test` reads it.
 */

/**
 * @returns {{begin: () => number, lands: (ticket: number) => boolean, close: () => void, open: () => void}}
 */
export function createLatest() {
  let issued = 0;
  let closed = false;
  return {
    /** A read begins: every ticket before this one is out of date. */
    begin() {
      issued += 1;
      return issued;
    },
    /** Whether this read's answer may be applied: it is the newest asked for, and the reader is still there. */
    lands(ticket) {
      return !closed && ticket === issued;
    },
    /** The reader left: nothing asked for so far lands, now or when it comes back. */
    close() {
      closed = true;
      issued += 1;
    },
    /** The reader is there again: what it asks from here on lands. */
    open() {
      closed = false;
    },
  };
}
