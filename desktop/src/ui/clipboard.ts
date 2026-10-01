/**
 * The one door to the clipboard — for words and for a picture.
 *
 * In the desktop the shell writes it (`copy_text`, `copy_image`): the
 * webview allows `navigator.clipboard` only inside a user gesture, and a
 * menu's chosen action runs after the menu has left — a timeout later, no
 * gesture — so every *Copy path* from a context menu was refused and
 * swallowed, and a screenshot copied after the seconds its snapshot takes
 * would be too. The shell has no such rule. In a browser the webview's
 * clipboard is the only one there is: absent in an insecure context,
 * refusing without a gesture. Either way a copy answers whether it happened
 * — the caller says *Copied.* or *The clipboard refused.* and never assumes.
 */

import { inDesktopShell, setClipboardImage, setClipboardText } from "../api";

/** Put `text` on the clipboard; `true` when it landed. */
export async function copyText(text: string): Promise<boolean> {
  if (inDesktopShell()) return setClipboardText(text);
  const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
  if (!clipboard?.writeText) return false;
  try {
    await clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Put a PNG on the clipboard as a picture; `true` when it landed. */
export async function copyImage(png: Uint8Array): Promise<boolean> {
  if (inDesktopShell()) return setClipboardImage(png);
  const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
  if (!clipboard?.write || typeof ClipboardItem === "undefined") return false;
  try {
    const blob = new Blob([png.slice().buffer as ArrayBuffer], { type: "image/png" });
    await clipboard.write([new ClipboardItem({ "image/png": blob })]);
    return true;
  } catch {
    return false;
  }
}
