/**
 * One send per press: a verb that reaches the node holds its key until the
 * node answers, and a second press meanwhile sends nothing.
 *
 * A stop or a restart whose dialog closed at the click, a withdrawal, a
 * promotion, a document handed to a goal — each is one request. Without the
 * latch a double click, or an effect that ran again while the request was in
 * flight, made two: two copies in the library, `brief (2).pdf`.
 *
 * Plain `.mjs` with a `.d.mts` beside it: the rule is tested here, and a
 * component only holds one latch in a ref.
 */

/** A latch over any number of keys, none held. */
export function createLatch() {
  const held = new Set();
  return {
    /** Take `key`: true when it was free — send — false while it is held. */
    take(key) {
      if (held.has(key)) return false;
      held.add(key);
      return true;
    },
    /** The node answered, either way: `key` is free again. */
    release(key) {
      held.delete(key);
    },
    /** Whether `key` is held: the request is in flight. */
    holds(key) {
      return held.has(key);
    },
  };
}

/**
 * Run `send` under `key`: once while it is in flight, released when it
 * settles — fulfilled or rejected. Answers whether this call sent.
 * @param {ReturnType<typeof createLatch>} latch
 * @param {string} key
 * @param {() => Promise<unknown>} send
 */
export async function once(latch, key, send) {
  if (!latch.take(key)) return false;
  try {
    await send();
  } finally {
    latch.release(key);
  }
  return true;
}

/** The key of a set of uploads: their hashes, in order — the same files are the same send. */
export function uploadsKey(uploaded) {
  return (Array.isArray(uploaded) ? uploaded : []).map((u) => String(u?.sha256 ?? "")).join(",");
}
