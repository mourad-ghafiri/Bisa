/**
 * Types for `logModel.mjs`, which is plain JavaScript so `node --test` can
 * import it without a build step.
 */
import type { ResolvedRow } from "./shell/settingsModel.mjs";

export type LogLevel = "error" | "warn" | "info" | "debug" | "trace";
export type LogRotation = "hourly" | "daily";

/** The file layer's configuration — the crate's `LogConfig`, on the wire. */
export interface LogConfig {
  enabled: boolean;
  level: LogLevel;
  rotation: LogRotation;
  keep_files: number;
}

/** One event as the shell's `log_event` command takes it. */
export interface LogEvent {
  level: LogLevel;
  target: string;
  message: string;
  fields: Record<string, unknown>;
}

export declare const LEVELS: readonly LogLevel[];
export declare const ROTATIONS: readonly LogRotation[];
export declare const KEYS: Readonly<{ enabled: string; level: string; rotation: string; keepFiles: string }>;
export declare const DEFAULT_CONFIG: Readonly<LogConfig>;
export declare const MAX_MESSAGE: number;
export declare const MAX_FIELD: number;
export declare const MAX_FIELDS: number;
export declare const STACK_LINES: number;

/** Whether a line at `level` passes the `threshold` in force. */
export declare function passes(level: string, threshold: string): boolean;
/** The resolved `logging.*` settings as a configuration; absent is the default. */
export declare function configFrom(resolved: readonly ResolvedRow[] | null | undefined): LogConfig;
/** An error's name, message, status, path, code and the first lines of its stack. */
export declare function errorFields(error: unknown): Record<string, unknown>;
/** One event, bounded, as the shell takes it. */
export declare function shapeEvent(
  level: string,
  target: string,
  message: string,
  fields?: Record<string, unknown>,
): LogEvent;
