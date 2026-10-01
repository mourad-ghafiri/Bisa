/**
 * The Board's settings keys, spelled once (`workstreams.board.*` in the
 * registry — `crates/bisa-core/src/settings.rs`), and their defaults
 * as the desktop falls back to them before the node answers.
 */

export const BOARD_ENABLED_KEY = "workstreams.board.enabled";
export const BOARD_DUE_SOON_KEY = "workstreams.board.due_soon_days";
export const BOARD_WIP_KEY = "workstreams.board.wip_limit";
export const BOARD_ARCHIVED_KEY = "workstreams.board.show_archived";

/** Every Board key — what Settings › Project IDE › Board shows, and the Workstreams panel omits. */
export const BOARD_KEYS = Object.freeze([BOARD_ENABLED_KEY, BOARD_DUE_SOON_KEY, BOARD_WIP_KEY, BOARD_ARCHIVED_KEY]);

export const BOARD_DEFAULTS = Object.freeze({ enabled: true, dueSoonDays: 3, wipLimit: 0, showArchived: false });
