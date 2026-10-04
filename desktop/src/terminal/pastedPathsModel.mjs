/**
 * A file pasted or dropped onto a terminal is its path, as a Mac terminal
 * types it (ide/06 §The GPU, links and the keyboard), as facts: how a path is
 * quoted for a shell, how several are typed, whether a paste must ask the
 * shell what the clipboard holds, and what it then types.
 *
 * The web engine hands a paste or a drop a file's name and bytes and never
 * its path; the desktop shell reads the paths off the pasteboard
 * (`api.pasteboardHolds`, `api.droppedPaths`) and, for a picture with no file
 * behind it, writes the picture to a file of its own (`api.pasteImageToTemp`).
 * The wiring is `terminal/pathInput.ts`. Plain `.mjs`, so `node --test` reads it.
 */

/** The characters a path keeps as they are: letters and digits, and the punctuation no shell reads. */
const PLAIN = /^[A-Za-z0-9_\-./,:+@%=]$/;

/** A control character — a line break above all — which a backslash cannot carry through a shell. */
const CONTROL = /[\u0000-\u001f\u007f]/;

/**
 * A path as a shell reads it back whole: every character a shell would read —
 * a space, a quote, `$`, `&`, `(`, a glob — behind a backslash, as Terminal.app
 * types a dropped file; a letter beyond ASCII as it is. A path holding a
 * control character goes in single quotes instead, each `'` in it written `'\''`.
 * @param {string} path
 * @returns {string}
 */
export function shellQuoted(path) {
  const s = String(path ?? "");
  if (CONTROL.test(s)) return `'${s.replace(/'/g, "'\\''")}'`;
  let out = "";
  for (const ch of s) out += ch.codePointAt(0) > 0x7f || PLAIN.test(ch) ? ch : `\\${ch}`;
  return out;
}

/**
 * Paths as a terminal types them: each quoted, one space between.
 * @param {readonly string[]} paths
 * @returns {string}
 */
export function typedPaths(paths) {
  return (paths ?? []).map(shellQuoted).join(" ");
}

/**
 * Whether a paste must ask the shell what the clipboard holds: when it
 * carries files — the engine's name for a copy made in the file manager —
 * or no text at all, when the engine may be holding a picture back (the
 * composer's rule, `pastedImageModel.pasteIntake`). Any other paste is plain
 * text, and the terminal pastes it as it always has.
 * @param {{types: Iterable<string> | null | undefined, text: string}} paste
 * @returns {boolean}
 */
export function pasteAsksShell(paste) {
  return Array.from(paste?.types ?? []).includes("Files") || !paste?.text;
}

/**
 * What a paste the shell was asked about types: the copied files' paths
 * first — a copy in the file manager; else the text the paste carried; else
 * a picture, saved to a file whose path is typed; else the paste's own text,
 * empty — exactly what the terminal would have pasted.
 * @param {{paths?: readonly string[], image?: boolean} | null | undefined} holds what the shell read (`pasteboardHolds`)
 * @param {string} text the paste's own plain text
 * @returns {{kind: "paths", paths: string[]} | {kind: "text", text: string} | {kind: "picture"}}
 */
export function pasteChoice(holds, text) {
  const paths = (holds?.paths ?? []).filter((p) => typeof p === "string" && p.startsWith("/"));
  if (paths.length > 0) return { kind: "paths", paths };
  if (text) return { kind: "text", text };
  if (holds?.image) return { kind: "picture" };
  return { kind: "text", text: "" };
}
