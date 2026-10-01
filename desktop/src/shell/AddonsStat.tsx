/**
 * The footer's Addons read-out: the addon glyph, tinted while any window
 * shows, how many show of every addon installed as the tooltip — and, on a
 * click, the Addons popover (`AddonsOverlay`), the way the Pet read-out opens
 * its overlay. What shows is the store's one rule (`visibleAddons`), the same
 * the layer draws by.
 */

import { useState } from "react";
import { titleWords, visibleAddons } from "../addons/addonsModel.mjs";
import { useAddons } from "../addons/addonsStore";
import { ICON, Popover, Tooltip, cn } from "../ui";
import { AddonsOverlay } from "./AddonsOverlay";

export function AddonsStat() {
  const [open, setOpen] = useState(false);
  const { addons, hidden, layerShown, switchedOn } = useAddons();
  const showing = visibleAddons(addons, hidden, layerShown, switchedOn);
  const title = titleWords(showing.length, addons.length);
  return (
    <Popover
      label={title}
      side="top"
      align="end"
      open={open}
      onOpenChange={setOpen}
      className="w-[calc(var(--type-rem)*22)] max-w-[90vw]"
      trigger={
        <Tooltip label={title}>
          <span
            aria-label={title}
            aria-pressed={showing.length > 0}
            role="button"
            className={cn(
              "anim flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-control",
              showing.length > 0 ? "bg-accent-soft text-accent-ink" : "text-text-dim hover:bg-surface-2",
            )}
          >
            <ICON.addon size={13} aria-hidden />
          </span>
        </Tooltip>
      }
    >
      {open && <AddonsOverlay close={() => setOpen(false)} />}
    </Popover>
  );
}
