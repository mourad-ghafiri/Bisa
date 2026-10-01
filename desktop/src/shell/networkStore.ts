/**
 * The footer's network read-out — one module store, polled, on the
 * `statsStore` pattern (awake-gated, `inFlight`-guarded, paused when
 * nothing reads it or the window is hidden).
 *
 * One tick reads two things and keeps them as one part: this Mac's network
 * facts from the desktop shell (`networkApi.ts` — the internet probe and
 * the public IP the echo service at `network.public_ip_url` answers, the
 * interfaces, the VPN, the route, the resolvers, System Settings' proxy;
 * `null` off the shell) and what the platform's own calls leave through
 * from the node (`GET /network`). Beside them, the window's own word —
 * `navigator.onLine` — kept the moment it flips. The part is compared as
 * JSON so a reading that did not move re-renders nothing. Its cadence is
 * `cache.desktop.network_poll_ms`; `refreshNetwork` reads again on demand —
 * a `settings_changed` naming a `network.*` key, the window regaining focus,
 * the window's `online` or `offline`. Settings › Network reads this store
 * too: one read, one cadence.
 */

import { useEffect, useSyncExternalStore } from "react";
import { api } from "../api";
import type { NetworkStatus } from "../types";
import { networkFacts, type NetworkFacts } from "./networkApi";
import { isHidden, onVisibilityChange } from "./visibility";

let pollMs = 30_000;

/** Retune the cadence; restarts the timer at the new interval. */
export function setNetworkPollMs(ms: number): void {
  if (!Number.isFinite(ms) || ms <= 0 || ms === pollMs) return;
  pollMs = ms;
  if (timer) {
    clearInterval(timer);
    timer = null;
    schedule();
  }
}

/** The cadence in force — the overlay's footnote says it. */
export function networkPoll(): number {
  return pollMs;
}

let publicIpUrl = "https://api.ipify.org";

/** The echo service `network.public_ip_url` names; empty asks none. A change reads again. */
export function setPublicIpUrl(url: string): void {
  if (url === publicIpUrl) return;
  publicIpUrl = url;
  void refreshNetwork();
}

export interface NetworkPart {
  /** This Mac's network; `null` off the shell or before the first read. */
  readonly facts: NetworkFacts | null;
  /** What the platform's calls leave through; `null` before the node answered. */
  readonly status: NetworkStatus | null;
  /** The window's own word — `navigator.onLine`; `null` where there is no window. */
  readonly online: boolean | null;
  /** Why the facts read refused, `null` when it answered. */
  readonly error: string | null;
  /** When the last read landed, unix seconds; `null` before one did. */
  readonly readAt: number | null;
}

function windowOnline(): boolean | null {
  return typeof navigator !== "undefined" && typeof navigator.onLine === "boolean" ? navigator.onLine : null;
}

let part: NetworkPart = { facts: null, status: null, online: windowOnline(), error: null, readAt: null };

const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;
let awake = false;
let inFlight = false;

function notify() {
  for (const l of listeners) l();
}

function setPart(next: NetworkPart) {
  // `readAt` moves on every read and is not a change worth a repaint on its
  // own; the facts, the status, the window's word and the error are compared as read.
  const seen = (p: NetworkPart) => JSON.stringify({ f: p.facts, s: p.status, o: p.online, e: p.error });
  const same = seen(next) === seen(part);
  part = same ? { ...part, readAt: next.readAt } : next;
  if (!same) notify();
}

async function tick(): Promise<void> {
  if (inFlight) return;
  inFlight = true;
  try {
    const [read, status] = await Promise.all([
      networkFacts(publicIpUrl).then(
        (facts) => ({ facts, error: null as string | null }),
        (e: unknown) => ({ facts: null, error: e instanceof Error ? e.message : String(e) }),
      ),
      api.network().catch(() => null),
    ]);
    setPart({ facts: read.facts, error: read.error, status: status ?? part.status, online: windowOnline(), readAt: Date.now() / 1000 });
  } finally {
    inFlight = false;
  }
}

/** Read again now — after a `network.*` write, when the window comes back, when its network flips. */
export function refreshNetwork(): Promise<void> {
  return awake ? tick() : Promise.resolve();
}

function schedule(): void {
  const run = awake && listeners.size > 0 && !isHidden();
  if (run && !timer) {
    void tick();
    timer = setInterval(() => void tick(), pollMs);
  } else if (!run && timer) {
    clearInterval(timer);
    timer = null;
  }
}

// A hidden window reads nothing; on becoming visible it refreshes at once.
onVisibilityChange(schedule);

// The window's own word lands the moment it flips — DOWN in a second, not
// at the next tick — and a read follows; focus reads again too.
if (typeof window !== "undefined") {
  const flip = () => {
    setPart({ ...part, online: windowOnline() });
    void refreshNetwork();
  };
  window.addEventListener("online", flip);
  window.addEventListener("offline", flip);
  window.addEventListener("focus", () => void refreshNetwork());
}

function subscribe(l: () => void) {
  listeners.add(l);
  schedule();
  return () => {
    listeners.delete(l);
    schedule();
  };
}

/** This Mac's network and what the platform leaves through, for the footer's read-out and Settings › Network. */
export function useNetwork(): NetworkPart {
  useEffect(() => {
    awake = true;
    schedule();
  }, []);
  return useSyncExternalStore(subscribe, () => part, () => part);
}
