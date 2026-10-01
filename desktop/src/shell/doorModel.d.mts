export const DOOR_TTL_MS: number;

export interface Doors {
  readonly pending: Readonly<Record<string, { readonly detail: unknown; readonly at: number }>>;
}

export function emptyDoors(): Doors;
export function pendDoor(doors: Doors, event: string, detail: unknown, now: number): Doors;
export function takeDoor(doors: Doors, event: string, now: number): { doors: Doors; request: { detail: unknown } | null };
