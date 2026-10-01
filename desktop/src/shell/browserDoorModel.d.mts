/** Types for `browserDoorModel.mjs`, plain JavaScript so `node --test` reads it. */

export interface DoorTab {
  key: string;
  label: string;
  home: { scope: string; id: string } | null;
  where: string; busy: boolean; headless?: boolean;
}

export interface DoorMenuItem {
  id: string;
  label: string;
  icon: string;
  danger?: boolean;
  separatorBefore?: boolean;
}

export declare function doorWords(facts: { count: number; busy: number; showing: boolean }): { label: string; count: number; hint: string };
export declare function doorMenu(facts: { tabs: readonly DoorTab[]; here: { scope: string; id: string } | null; active: string | null; showing: boolean }): DoorMenuItem[];
export declare function busyWords(n: number): string;
