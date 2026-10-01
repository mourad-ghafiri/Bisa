/**
 * A unified diff, coloured — the whole-workstream patch document's surface.
 * The caller says how tall it is; by flex in a column (`min-h-0 flex-1`),
 * never a fraction of the window, so the patch fills the centre it opens in.
 *
 * The order of the tests matters: `+++` and `---` are the file headers and
 * start with the same characters as content lines, so they have to be ruled
 * out before `+` and `-` are read as additions and deletions.
 */

/** Kept in one place so the two callers cannot drift on the ramp. */
function lineTone(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---")) return "text-text-dim";
  if (line.startsWith("+")) return "text-ok";
  if (line.startsWith("-")) return "text-danger";
  if (line.startsWith("@@")) return "text-accent-ink";
  return "";
}

export function DiffView({ diff, className }: { diff: string; className?: string }) {
  return (
    <div
      className={className ?? "overflow-auto rounded-control border border-border bg-surface-2"}
    >
      {/*
        `w-max min-w-full` rather than a wrap: a diff whose lines wrap stops
        being a diff — the column a change is in is part of what it says — so
        the block scrolls sideways inside its own border instead.
      */}
      <pre className="w-max min-w-full p-2 font-mono text-2xs leading-relaxed">
        {diff.split("\n").map((line, i) => (
          // A blank line renders as a space so the row keeps its height; an
          // empty div collapses and the patch loses its spacing.
          <div key={i} className={lineTone(line)}>
            {line || " "}
          </div>
        ))}
      </pre>
    </div>
  );
}
