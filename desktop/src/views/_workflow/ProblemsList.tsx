/**
 * What is wrong with a definition, by step and kind.
 *
 * The node's validator is the only authority; this list is its answer in
 * words a designer can act on (`stepKinds.mjs`'s `PROBLEM_KIND_LABEL`) with
 * the server's own sentence beneath. A row with a step is a link to it.
 * When the node could not judge the current body (`unreadable`), one line
 * above the list says so — the list is then the node's last word, not its
 * word on what is on the canvas now.
 */

import { t, tx } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";
import type { Problem } from "../../types";
import { ICON } from "../../ui";
import { PROBLEM_KIND_LABEL } from "./stepKinds.mjs";

export function ProblemsList({
  problems,
  unreadable,
  onSelect,
  className,
}: {
  problems: readonly Problem[];
  /** Why the node could not read the current design, when it could not. */
  unreadable?: string | null;
  onSelect?: (step: string) => void;
  className?: string;
}) {
  const notice = unreadable ? (
    <p className="mb-1 flex items-start gap-1.5 rounded-control border border-danger/30 bg-danger-soft/40 px-2 py-1.5 text-2xs text-danger">
      <ICON.warn size={12} aria-hidden className="mt-0.5 shrink-0" />
      <span>{rich("workflow-problems-list-node-could-not-read-design", { detail: <span className="text-text-dim">{unreadable}</span> })}</span>
    </p>
  ) : null;
  if (problems.length === 0) {
    return (
      <div className={className}>
        {notice}
        <p className="flex items-center gap-1.5 text-2xs text-ok">
          <ICON.ok size={12} aria-hidden />
          {unreadable ? t("workflow-problems-list-no-problems-last-design-read") : t("workflow-problems-list-no-problems-can-run")}
        </p>
      </div>
    );
  }
  return (
    <div className={className}>
      {notice}
      <ul className="flex flex-col gap-1">
      {problems.map((p, i) => {
        const body = (
          <>
            <span className="flex items-center gap-1.5 text-2xs font-medium text-danger">
              <ICON.warn size={12} aria-hidden className="shrink-0" />
              {p.step ? <code className="font-mono">{p.step}</code> : <span>{t("workflow-problems-list-workflow")}</span>}
              <span className="text-text-dim">·</span>
              <span>{PROBLEM_KIND_LABEL[p.kind] ?? p.kind}</span>
            </span>
            <span className="mt-0.5 block text-2xs text-text-dim">{tx(p.text)}</span>
          </>
        );
        return (
          <li key={`${p.step ?? ""}:${p.kind}:${i}`} className="rounded-control border border-danger/30 bg-danger-soft/40 px-2 py-1.5">
            {p.step && onSelect ? (
              <button type="button" className="w-full text-left" onClick={() => onSelect(p.step!)}>
                {body}
              </button>
            ) : (
              body
            )}
          </li>
        );
      })}
      </ul>
    </div>
  );
}
