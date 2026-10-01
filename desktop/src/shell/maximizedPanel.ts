/**
 * A floating panel's maximized frame — the one mechanism the Notes and Draw
 * overlays share. Maximized, a panel fills exactly the content column
 * (everything but the header, the footer and the sidebar): the box
 * `App.tsx` publishes from its content column's resize observer
 * (`contentBox.ts`), turned into a fixed-position style by
 * `contentBoxModel.maximizedStyle`. While it stands there it is a *surface*
 * (`ui/openSurfaces`), so the native browser layer yields instead of
 * painting over it. `null` means "not maximized, or the box is not measured
 * yet" — the panel then keeps its corner size, and re-renders the moment the
 * box lands. A hook in `shell/`, not `ui/`: it reads the app's own chrome.
 */
import { useContentBox } from "./contentBox";
import { maximizedStyle } from "./contentBoxModel.mjs";
import { useSurface } from "../ui/openSurfaces";

export type FixedStyle = NonNullable<ReturnType<typeof maximizedStyle>>;

export function useMaximizedPanel(open: boolean, maximized: boolean): FixedStyle | null {
  const box = useContentBox();
  // Maximized, the panel stands over whatever native layer shows: a surface, for as long as it does.
  useSurface(open && maximized);
  return maximized ? maximizedStyle(box) : null;
}
