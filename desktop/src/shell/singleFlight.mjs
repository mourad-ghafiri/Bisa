/**
 * One request shared by everyone who asks while it is in the air (ide/12),
 * bound to none of them.
 *
 * The obvious single-flight — "return the running promise" — hands a second
 * caller a promise tied to the *first* caller's `AbortSignal`. Under React's
 * strict-mode mount → unmount → mount the first caller aborts, the shared
 * fetch dies with it, the second caller's `.catch` swallows the abort, and
 * nothing ever retries: the Files tab's name search came up empty forever.
 *
 * Here the flight owns its own controller. A caller *joins* it with a signal
 * of its own; leaving detaches that caller only. The fetch is aborted when
 * the **last** joiner leaves, a result still lands for whoever is left, and a
 * flight that fails or is aborted is forgotten so the next call starts again.
 *
 * Plain `.mjs`: the rule is what matters, and it is testable with a fake
 * `start`.
 */

function abortError() {
  const e = new Error("aborted");
  e.name = "AbortError";
  return e;
}

/**
 * @template T
 * @returns {{
 *   join(key: string, start: (signal: AbortSignal) => Promise<T>, signal?: AbortSignal | null): Promise<T>,
 *   inFlight(key: string): boolean,
 *   joiners(key: string): number,
 * }}
 */
export function createSingleFlight() {
  /** key → the flight in the air. */
  const flights = new Map();

  const forget = (key, flight) => {
    if (flights.get(key) === flight) flights.delete(key);
  };

  return {
    join(key, start, signal = null) {
      if (signal?.aborted) return Promise.reject(abortError());
      let flight = flights.get(key);
      if (!flight) {
        const controller = new AbortController();
        const f = { controller, joiners: 0, promise: null };
        f.promise = Promise.resolve()
          .then(() => start(controller.signal))
          .finally(() => forget(key, f));
        // A flight nobody is left to hear about must not become an unhandled
        // rejection; the joiners each hold their own copy.
        f.promise.catch(() => {});
        flights.set(key, f);
        flight = f;
      }
      flight.joiners += 1;
      let left = false;
      const leave = () => {
        if (left) return;
        left = true;
        flight.joiners -= 1;
        if (flight.joiners === 0 && flights.get(key) === flight) {
          flights.delete(key);
          flight.controller.abort();
        }
      };
      if (!signal) {
        return flight.promise.finally(() => {
          left = true;
        });
      }
      return new Promise((resolve, reject) => {
        const onAbort = () => {
          leave();
          reject(abortError());
        };
        signal.addEventListener("abort", onAbort, { once: true });
        flight.promise.then(
          (v) => {
            signal.removeEventListener("abort", onAbort);
            left = true;
            resolve(v);
          },
          (e) => {
            signal.removeEventListener("abort", onAbort);
            left = true;
            reject(e);
          },
        );
      });
    },
    inFlight(key) {
      return flights.has(key);
    },
    joiners(key) {
      return flights.get(key)?.joiners ?? 0;
    },
  };
}
