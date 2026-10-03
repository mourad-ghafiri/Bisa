/**
 * The footer's Addons read-out: the addon glyph, tinted while any window
 * shows, how many show of every addon installed as the tooltip — and, on a
 * click, the Addons popover (`AddonsOverlay`), the way the Pet read-out opens
 * its overlay. What shows is the store's one rule (`visibleAddons`), the same
 * the layer draws by. The trigger is the popover's own `<button>`
 * (`asChild`), pressed while any window shows; the tooltip hangs on a plain
 * wrapper around it.
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
    <Tooltip label={title}>
      <span className="inline-flex shrink-0">
        <Popover
          asChild
          side="top"
          align="end"
          open={open}
          onOpenChange={setOpen}
          className="w-[calc(var(--type-rem)*22)] max-w-[90vw]"
          trigger={
            <button
              type="button"
              aria-label={title}
              aria-pressed={showing.length > 0}
              className={cn(
                "anim flex h-6 w-6 shrink-0 items-center justify-center rounded-control",
                showing.length > 0 ? "bg-selected text-text" : "text-text-dim hover:bg-surface-2 hover:text-text",
              )}
            >
              <ICON.addon size={13} aria-hidden />
            </button>
          }
        >
          {open && <AddonsOverlay close={() => setOpen(false)} />}
        </Popover>
      </span>
    </Tooltip>
  );
}
