/**
 * The platform's own mark: the file `logo/logo.svg` at the repository root,
 * shown as an image.
 *
 * One file is the mark everywhere — this component shows it, `just app-icon`
 * rasterises it into the OS icon set under `src-tauri/icons/` — so a redraw
 * is a new file at that path and one command, and nothing in code draws or
 * generates a mark. It is an `<img>` and not inlined SVG because the artwork
 * declares gradient and clip ids, and the About dialog and the empty landing
 * could put it in one document twice. The mark carries its own colour, so
 * `tone` is accepted for the `Mark` contract and ignored, as the marks in
 * `harnessMarks.tsx` that wear no brand colour do.
 */

import type { MarkProps } from "./harnessMarks";
import logo from "../../../logo/logo.svg";

export function PlatformMark({ size = 14, className, title, tone: _tone, ...rest }: MarkProps) {
  return (
    <img
      src={logo}
      width={size}
      height={size}
      alt={title ?? ""}
      draggable={false}
      className={className}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : (rest["aria-hidden"] ?? true)}
    />
  );
}
