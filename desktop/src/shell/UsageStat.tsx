/**
 * The footer's left: what one harness's account has left — its mark and
 * name, then its windows on one compact line, `5h ▰▰ 24% · resets in 2 h
 * 10 min  Weekly ▰▱ 41%  Fable ▱▱ 9%`, and a refresh — and, on a click,
 * every installed harness with its usage and the choice of which one stays
 * here (`UsagePicker`). Which one that is: the person's pin
 * (`footerUsageStore`) through `footerUsageModel.pinnedHarness`. Nothing is
 * drawn when no harness is installed. The refresh stands beside the trigger,
 * never inside it: the trigger is a button, and a button holds no other.
 */

import { Popover, harnessMark } from "../ui";
import { RefreshUsage, UsageLine } from "./HarnessUsageLine";
import { pickerRows, pinnedHarness, usageStatWords } from "./footerUsageModel.mjs";
import { usePinnedUsage } from "./footerUsageStore";
import { useHarnessUsage } from "./harnessUsageStore";
import { usageWords } from "./harnessUsageModel.mjs";
import { useLaunchableHarnesses } from "./useHarnesses";
import { UsagePicker } from "./UsagePicker";

export function UsageStat() {
  const rows = useLaunchableHarnesses();
  const pinned = usePinnedUsage();
  const id = pinnedHarness(pinned, rows);
  const entry = useHarnessUsage(id, id !== null);
  if (id === null) return null;
  const row = rows.find((r) => r.id === id);
  const label = row?.label ?? id;
  const Mark = harnessMark(id);
  const words = usageStatWords(label, usageWords(entry.state));
  return (
    <span className="flex min-w-0 items-center gap-0.5">
      <Popover
        label={words}
        side="top"
        align="start"
        className="w-96 max-w-[90vw]"
        trigger={
          <span title={words} className="anim flex h-6 min-w-0 max-w-[60vw] cursor-pointer items-center gap-1.5 rounded-control px-1.5 text-2xs hover:bg-surface-2">
            <Mark size={13} aria-hidden className="shrink-0 text-text-dim" />
            <span className="shrink-0 font-medium text-text">{label}</span>
            <UsageLine harness={id} entry={entry} compact refresh={false} />
          </span>
        }
      >
        <UsagePicker rows={pickerRows(rows)} pinned={id} />
      </Popover>
      <RefreshUsage harness={id} loading={entry.loading} />
    </span>
  );
}
