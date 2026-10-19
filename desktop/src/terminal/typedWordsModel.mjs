/**
 * Words the platform types into a harness's own prompt (ide/06 §Start words):
 * a request from the annotation tray, in a fresh tab once the harness has
 * drawn its prompt, or into a live one that is listening.
 *
 * A harness's TUI reads a newline as Enter, so a request of many lines
 * cannot be written as it is. Every shipped TUI harness asks the terminal for
 * **bracketed paste** (DECSET 2004), and xterm wraps a paste in the brackets
 * only when the program asked — so the tab does the same by hand: wrapped,
 * with every newline as the carriage return a paste carries, the whole block
 * lands in the input as text. A program that did not ask gets one line, the
 * newlines folded to spaces, which still reads as the request. Enter follows
 * one beat later, on its own, for a TUI that debounces a paste.
 *
 * Pure: the mount reads the mode off xterm and the store writes the bytes.
 */

/** The beat between the words and the Enter that submits them. */
export const ENTER_AFTER_MS = 120;
/** Enter, as a key sends it. */
export const ENTER = "\r";
// for the machine
const BRACKET_OPEN = "\u001b[200~";
// for the machine
const BRACKET_CLOSE = "\u001b[201~";

/**
 * The bytes to write for `text`, and the Enter to follow them.
 * @param {string} text
 * @param {boolean} bracketed whether the program asked for bracketed paste
 * @returns {{text: string, enter: string}}
 */
export function typedWords(text, bracketed) {
  const words = String(text ?? "").replace(/\r\n?/g, "\n").trim();
  if (!words) return { text: "", enter: "" };
  const body = bracketed ? `${BRACKET_OPEN}${words.replace(/\n/g, "\r")}${BRACKET_CLOSE}` : words.replace(/\s*\n+\s*/g, " ");
  return { text: body, enter: ENTER };
}
