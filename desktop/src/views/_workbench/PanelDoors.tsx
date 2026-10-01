/**
 * The workbench's top bar's two side toggles (ide/02), each at the end of
 * the bar it moves: the project rail's first, before the project's name
 * (`ProjectRailToggle`); the right panel's last (`RightPanelToggle`). Every
 * occupant of the right panel is a tab on its rail (`OccupantRail.tsx`) —
 * the bar holds no door into the column. A toggle is its glyph alone and
 * reads pressed while its side is showing; the words are the model's
 * (`toggleWords`) and the name and the chord are its tooltip — no hint in
 * the bar.
 */

import { useChord } from "../../shell/useKeymap";
import { Button, ICON, Tooltip } from "../../ui";
import { setProjectRailOpen } from "./projectRailStore";
import { setRightPanelOpen } from "./rightPanelStore";
import { toggleWords } from "./workbenchChromeModel.mjs";
import type { ToggleSide } from "./workbenchChromeModel.mjs";

const TOGGLE_ICON = { rail: ICON.panelLeft, right: ICON.panelRight } as const;

/** One side's toggle: its glyph alone, the words and the chord as its tooltip; pressed while that side is showing. */
function PanelToggle({ side, open, onToggle }: { side: ToggleSide; open: boolean; onToggle: (open: boolean) => void }) {
  const words = toggleWords(side, open);
  const chord = useChord(words.command);
  const Icon = TOGGLE_ICON[side];
  return (
    <Tooltip label={chord ? `${words.label} · ${chord}` : words.label}>
      <span className="inline-flex">
        {/* A square the rail's tabs' size (h-8 w-8), so the right toggle sits
            over the rail's column and the left over the project rail's edge. */}
        <Button size="icon" className="h-8 w-8" aria-pressed={open} aria-label={words.label} onClick={() => onToggle(!open)}>
          <Icon size={13} aria-hidden />
        </Button>
      </span>
    </Tooltip>
  );
}

/** The header's first button: the project rail's toggle, on the side it moves. */
export function ProjectRailToggle({ open }: { open: boolean }) {
  return <PanelToggle side="rail" open={open} onToggle={setProjectRailOpen} />;
}

/** The header's last button: the right panel's toggle, over the rail's column. */
export function RightPanelToggle({ open }: { open: boolean }) {
  // The header's padding is px-3 (12px); the rail's tabs are w-8 centred in
  // w-10, 4px from the edge. The toggle pulls in by the difference so its
  // column is the rail's — aligned with the tabs below, not with the text.
  return (
    <div className="-mr-2 flex items-center">
      <PanelToggle side="right" open={open} onToggle={setRightPanelOpen} />
    </div>
  );
}
