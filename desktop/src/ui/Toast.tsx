/**
 * Transient confirmations, on sonner.
 *
 * `useToast()` keeps its three methods and `ToastProvider` keeps its place at
 * the top of the tree, so nothing that calls `toast.ok("saved")` had to
 * change. What changed underneath is what the hand-rolled version never did:
 * stacking with a height-aware collapse, swipe to dismiss, pausing the timer
 * while the window is unfocused so a toast raised while you were in another
 * app is still there when you come back, and an `aria-live` region that
 * announces one toast rather than re-reading the stack.
 *
 * The context is kept even though sonner's `toast()` works from anywhere,
 * because it is what makes a missing `<ToastProvider>` throw at the call site
 * instead of dropping messages into a renderer that is not mounted.
 */

import { Toaster, toast as sonner } from "sonner";
import { createContext, useContext, useMemo, type ReactNode } from "react";

interface ToastApi {
  info: (text: string) => void;
  ok: (text: string) => void;
  error: (text: string) => void;
}

const Ctx = createContext<ToastApi | null>(null);

/** Errors linger: they usually need reading, not glancing at. */
const INFO_MS = 3500;
const ERROR_MS = 8000;

/**
 * The same three calls as a module: for the git operations that run in a
 * store rather than a component (ide/04 — a *Committed* toast must land after
 * the panel that started the commit was switched away from). Sonner renders
 * wherever `<Toaster>` is mounted, which is the provider's place at the top
 * of the tree; a call before that mounts is dropped by sonner, not thrown.
 */
export const toaster: ToastApi = Object.freeze({
  info: (t: string) => void sonner(t, { duration: INFO_MS }),
  ok: (t: string) => void sonner.success(t, { duration: INFO_MS }),
  error: (t: string) => void sonner.error(t, { duration: ERROR_MS }),
});

const BASE =
  "pointer-events-auto flex w-full items-start gap-2 rounded-control border px-3 py-2 text-xs shadow-lg";

export function ToastProvider({ children }: { children: ReactNode }) {
  const api = useMemo<ToastApi>(() => toaster, []);

  return (
    <Ctx.Provider value={api}>
      {children}
      <Toaster
        position="bottom-center"
        // Above the window's footer, whatever its density: sonner's own 24px
        // put a toast over the status bar's meters.
        offset={{ bottom: "calc(var(--spacing-chrome) + 0.75rem)" }}
        // Sonner's own light/dark switch reads the OS, which is the wrong
        // source once a theme can be chosen. Unstyled toasts painted from our
        // roles follow whichever theme is mounted, for free.
        toastOptions={{
          unstyled: true,
          classNames: {
            toast: `${BASE} border-border bg-surface text-text`,
            success: `${BASE} border-transparent bg-ok-soft text-ok`,
            error: `${BASE} border-transparent bg-danger-soft text-danger`,
          },
        }}
      />
    </Ctx.Provider>
  );
}

export function useToast(): ToastApi {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useToast must be used inside <ToastProvider>");
  return ctx;
}
