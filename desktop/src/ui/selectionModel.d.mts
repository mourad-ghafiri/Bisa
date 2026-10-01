interface SelectionLike {
  isCollapsed: boolean;
  rangeCount: number;
  getRangeAt(i: number): { commonAncestorContainer: unknown };
}
export function endsSelection(selection: SelectionLike | null | undefined, container: { contains(node: never): boolean } | null | undefined): boolean;
