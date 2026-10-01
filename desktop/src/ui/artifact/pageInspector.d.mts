import type { InspectorStyles, InspectorTheme } from "./inspectorTheme.mjs";

export declare const MAX_EXCERPT_BYTES: number;
export declare const MAX_SELECTOR_CHARS: number;
export declare const MAX_TEXT_CHARS: number;
export declare const CUT_TAIL: string;

/** The inspector's modes: the page's own, or the pointer picking — a box open is the page's own held state. */
export type InspectMode = "off" | "picking";

export declare const INSPECTOR_MESSAGES: {
  readonly INSPECT: "bisa:inspect";
  readonly MARKS: "bisa:marks";
  readonly THEME: "bisa:theme";
  readonly HOVER: "bisa:hover";
  readonly PICK: "bisa:pick";
  readonly NOTE: "bisa:note";
  readonly CLOSED: "bisa:closed";
  readonly MARKED: "bisa:marked";
  readonly ESCAPE: "bisa:escape";
  readonly FIND: "bisa:find";
  readonly FOUND: "bisa:found";
  readonly PLACE: "bisa:place";
  readonly PLACE_TO: "bisa:place-to";
  readonly READ: "bisa:read";
  readonly FIND_TEXT: "bisa:find-text";
  readonly SNAPSHOT: "bisa:snapshot";
  readonly CLICK: "bisa:click";
  readonly FILL: "bisa:fill";
  readonly TYPE: "bisa:type";
  readonly PRESS: "bisa:press";
  readonly SELECT: "bisa:select";
  readonly POINT: "bisa:point";
  readonly SCROLL: "bisa:scroll";
  readonly WAIT: "bisa:wait";
  readonly CONSOLE: "bisa:console";
  readonly EVAL: "bisa:eval";
  readonly ANSWER: "bisa:answer";
  readonly TITLE: "bisa:title";
  readonly KEY: "bisa:key";
};
export declare const MAX_READ_CHARS: number;
export declare const MAX_NOTE_CHARS: number;
export declare const MAX_DIALOGS: number;
export declare const MAX_CONSOLE_LINES: number;
export declare const MAX_LINE_CHARS: number;
export declare const MAX_WAIT_MS: number;
export declare const IDLE_MS: number;

/** An element's rectangle in the frame's viewport — the frame fills its box, so the box's too. */
export interface InspectorRect {
  top: number;
  left: number;
  width: number;
  height: number;
}
/** A badge the frame draws: the element's locator, its number, and the note a box on it opens with. */
export interface InspectorMark {
  selector: string;
  n: number;
  note: string;
}
/** One of a picked element's ancestors: its tag word and its locator. */
export interface InspectorCrumb {
  tag: string;
  selector: string;
}
export interface InspectorPick {
  type: "bisa:pick";
  selector: string;
  tag: string;
  /** The head of the element's HTML, cut to `MAX_EXCERPT_BYTES`. */
  excerpt: string;
  text: string;
  rect: InspectorRect;
  /** The element's ancestors, outermost (`html`) first — the crumbs a note box draws. */
  ancestors: InspectorCrumb[];
}
/** The note typed for an element in the page's own box, with the element it is about. */
export interface InspectorNote {
  type: "bisa:note";
  selector: string;
  tag: string;
  excerpt: string;
  text: string;
  note: string;
}
export interface InspectorFound {
  type: "bisa:found";
  index: number;
  count: number;
}

/** The page's answer to the driver's request `id` (ide/18). */
/** A dialog the page raised while a request ran, answered for the agent. */
export interface InspectorDialog {
  kind: "alert" | "confirm" | "prompt";
  message: string;
  answer: string | null;
}
/** One line the page wrote to its console, or an error it raised. */
export interface InspectorConsoleLine {
  level: string;
  text: string;
  at: number;
}
/** Where the page stands, and how big it is. */
export interface InspectorScroll {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface InspectorAnswer {
  type: "bisa:answer";
  id: string;
  ok: boolean;
  url: string;
  title: string;
  text: string | null;
  error: string | null;
  /** The element the act landed on, as a path. */
  selector: string | null;
  /** How many a find or a snapshot counted, a console holds. */
  count: number | null;
  waitedMs: number | null;
  scroll: InspectorScroll | null;
  /** A script's value, JSON — or a cut string when it was too long. */
  value: unknown;
  dialogs: InspectorDialog[];
  console: InspectorConsoleLine[];
}
/** The page said its URL and title — it loaded, or moved. */
export interface InspectorTitle {
  type: "bisa:title";
  url: string;
  title: string;
}
/** A browser chord pressed inside a browser tab's page (ide/18): the keymap command it means. */
export type BrowserCommand = "focus_address" | "browser_reload" | "browser_back" | "browser_forward" | "new_browser_tab" | "close_tab";
export declare const BROWSER_COMMANDS: readonly BrowserCommand[];
/** The script message handler's name on every browser tab's webview. */
export declare const BROWSER_DOOR: string;
export interface InspectorKey {
  type: "bisa:key";
  command: BrowserCommand;
}
export type InspectorMessage =
  | InspectorFound
  | { type: "bisa:place"; top: number; left: number }
  | InspectorAnswer
  | InspectorTitle
  | InspectorKey
  | { type: "bisa:hover"; tag: string; rect: InspectorRect }
  | InspectorPick
  | InspectorNote
  | { type: "bisa:closed" }
  | { type: "bisa:marked"; lost: number[] }
  | { type: "bisa:escape" };

export declare function inspectMessage(mode: InspectMode): { type: "bisa:inspect"; mode: InspectMode };
export declare function marksMessage(marks: readonly InspectorMark[]): { type: "bisa:marks"; marks: InspectorMark[] };
export declare function cutBytes(s: string, max: number): string;
/** A message from the frame, typed and bounded — null for anything else. */
export declare function parseInspectorMessage(data: unknown): InspectorMessage | null;
/** The app's word to the frame: wear this theme. */
export declare function themeMessage(theme: InspectorTheme): { type: "bisa:theme"; styles: InspectorStyles };
/** The frame's program (ide/03 §Annotate): the core over `postMessage`, dressed in the theme given. */
export declare function inspectorScript(theme: InspectorTheme): string;
/** The browser tab's program (ide/18): the same core over the desktop's IPC door, with the driver, relaying the browser chords given, dressed in the theme given. */
export declare function browserScript(relay: readonly { key: string; shift: boolean; alt: boolean; command: string }[], theme: InspectorTheme): string;
export declare function readMessage(id: string, target?: string | null, format?: "text" | "html"): { type: "bisa:read"; id: string; target: string | null; format: "text" | "html" };
export declare function findTextMessage(id: string, query: string): { type: "bisa:find-text"; id: string; query: string };
export declare function snapshotMessage(id: string, target?: string | null, all?: boolean): { type: "bisa:snapshot"; id: string; target: string | null; all: boolean };
export declare function clickMessage(id: string, target: string): { type: "bisa:click"; id: string; target: string };
export declare function fillMessage(id: string, target: string, text: string): { type: "bisa:fill"; id: string; target: string; text: string };
export declare function typeMessage(id: string, target: string, text: string, clear?: boolean, submit?: boolean): { type: "bisa:type"; id: string; target: string; text: string; clear: boolean; submit: boolean };
export declare function pressMessage(id: string, key: string, target?: string | null, modifiers?: readonly string[]): { type: "bisa:press"; id: string; key: string; target: string | null; modifiers: string[] };
export declare function selectMessage(id: string, target: string, value?: string | null, label?: string | null): { type: "bisa:select"; id: string; target: string; value: string | null; label: string | null };
export declare function pointMessage(id: string, target: string): { type: "bisa:point"; id: string; target: string };
export declare function scrollMessage(id: string, to?: string | null, byX?: number, byY?: number): { type: "bisa:scroll"; id: string; to: string | null; byX: number; byY: number };
export declare function waitMessage(id: string, until: "selector" | "text" | "gone" | "idle", target?: string | null, query?: string | null, timeoutMs?: number): { type: "bisa:wait"; id: string; until: string; target: string | null; query: string | null; timeoutMs: number };
export declare function consoleMessage(id: string, clear?: boolean): { type: "bisa:console"; id: string; clear: boolean };
export declare function evalMessage(id: string, expression: string): { type: "bisa:eval"; id: string; expression: string };

export declare function placeToMessage(place: { top: number; left: number } | null | undefined): { type: "bisa:place-to"; top: number; left: number };
export declare function findMessage(find: { query: string; regex: boolean; caseSensitive: boolean } | null | undefined, index: number): { type: "bisa:find"; query: string; regex: boolean; caseSensitive: boolean; index: number };
