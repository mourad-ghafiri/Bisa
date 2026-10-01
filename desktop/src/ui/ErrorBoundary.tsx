/**
 * One bad render should cost you a screen, not the window.
 *
 * Before this, an exception anywhere in a view unmounted the whole tree and
 * left a blank webview with the error only in the console — the app looked
 * dead. Now the shell survives: the sidebar stays, and the failing region
 * explains itself and offers a way out.
 *
 * `OverlayBoundary` is the boundary for a host with no screen of its own —
 * a dialog, an overlay, a toast rail mounted once in the shell. A throw
 * there used to be the one thing the screen's boundary could not catch, so
 * the window went to the root card and stayed there across *Try again* and
 * a relaunch. Now the host is closed, one toast says so, and it opens again
 * when `resetKey` moves — the route, so the next screen gets it back.
 */

import { errorFields, log } from "../log";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { toaster } from "./Toast";

interface Props {
  children: ReactNode;
  /** Changing this resets the boundary — pass the route key. */
  resetKey?: string;
  /**
   * Where the crash is written down. The kit does not know the app's log,
   * so the app hands one in; without it the console gets the line.
   */
  onError?: (error: Error, info: ErrorInfo) => void;
  /** What to draw instead of the card; `reset` clears the error. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
  /**
   * Show the diagnostic log's folder — the card's way to what was written
   * down. The kit does not know where the log is, so the app hands this in;
   * without it the card offers no such button.
   */
  onReveal?: () => void;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    if (this.props.onError) this.props.onError(error, info);
    else log.error("view", "a view crashed", { ...errorFields(error), stack: info.componentStack ?? undefined });
  }

  componentDidUpdate(prev: Props): void {
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    if (this.props.fallback) return this.props.fallback(error, () => this.setState({ error: null }));
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="max-w-lg rounded-card border border-border bg-surface p-4">
          <p className="text-xs font-semibold text-text">This screen hit an error.</p>
          <p className="mt-1 text-2xs text-text-dim">
            The rest of the app is fine — move to another screen, or reload. {CRASH_WRITTEN_WORDS}
          </p>
          <pre className="mt-3 max-h-40 overflow-auto rounded-control bg-surface-2 p-2 text-2xs text-text-dim">
            {error.message}
          </pre>
          <div className="mt-3 flex gap-2">
            <button
              type="button"
              onClick={() => this.setState({ error: null })}
              className="anim h-8 rounded-control border border-border bg-surface px-3 text-xs font-medium hover:bg-surface-2"
            >
              Try again
            </button>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="anim h-8 rounded-control border border-accent bg-accent px-3 text-xs font-medium text-accent-contrast hover:opacity-90"
            >
              Reload
            </button>
            {this.props.onReveal && (
              <button
                type="button"
                onClick={this.props.onReveal}
                className="anim ml-auto h-8 rounded-control border border-border bg-surface px-3 text-xs font-medium hover:bg-surface-2"
              >
                Reveal the log
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }
}

/** The sentence the card owes: where the crash went. */
const CRASH_WRITTEN_WORDS = "The details are in the diagnostic log.";

/** A closed host draws nothing. */
const NOTHING = (): ReactNode => null;

/**
 * The boundary for a host with no screen of its own. `name` is what the
 * toast calls it — *the who-commits dialog*, *the palette*.
 */
export function OverlayBoundary({
  name,
  resetKey,
  onError,
  children,
}: {
  name: string;
  resetKey?: string;
  onError?: (error: Error, info: ErrorInfo) => void;
  children: ReactNode;
}) {
  return (
    <ErrorBoundary
      resetKey={resetKey}
      fallback={NOTHING}
      onError={(error, info) => {
        toaster.error(overlayClosedWords(name));
        if (onError) onError(error, info);
        else log.error("view", `${name} crashed`, { ...errorFields(error), stack: info.componentStack ?? undefined });
      }}
    >
      {children}
    </ErrorBoundary>
  );
}

/** The one sentence a closed host leaves behind. */
export function overlayClosedWords(name: string): string {
  return `The ${name} hit an error and was closed. It opens again on the next screen.`;
}
