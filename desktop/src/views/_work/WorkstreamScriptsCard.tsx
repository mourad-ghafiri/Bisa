/**
 * The Workstream scripts card (ide/07 §Workstream scripts), under About › Settings:
 * three shell scripts a project's people write — run before a workstream is
 * created, once it exists, and before its checkout is deleted — and the
 * timeout that bounds each.
 *
 * Two facts, two stores. The **texts** are project settings and sync with the
 * project; the **trust** to run them is this machine's, a list of digests the
 * node keeps. The texts are edited into the view's draft and the toolbar's
 * Save writes them and approves them in one act — you wrote it, you approved
 * it. A text that arrived by sync is untrusted until somebody here reads it
 * and presses *Approve on this machine* on the toolbar; until then the engine
 * refuses to run it. The words and the rules are `workstreamScripts.mjs`'s.
 */

import { Chip, Field, NumberInput, SectionHeader, Skeleton, TextArea, useCollapsed } from "../../ui";
import type { ProjectSettingsDraft } from "./useProjectSettingsDraft";
import { ENV_VARS, PHASES, isDirty, trustLine } from "./workstreamScripts.mjs";
import { t } from "../../i18n/l10n.mjs";
import { rich } from "../../i18n/rich";

export function WorkstreamScriptsCard({ draft }: { draft: ProjectSettingsDraft }) {
  const [envFolded, toggleEnv] = useCollapsed("ide.scripts.env", true);
  const view = draft.scripts;
  const current = draft.current?.scripts;
  if (!view || !current) return <Skeleton className="h-24 w-full" />;
  const edits = draft.draft.scripts ?? current;
  const dirty = isDirty(current, edits);

  return (
    <div className="flex flex-col gap-3">
      <p className="max-w-measure text-2xs leading-relaxed text-text-dim">
        {rich("work-workstream-scripts-card-blurb", { code: (inner) => <code className="font-mono">{inner}</code> })}
      </p>
      {PHASES.map((p) => {
        const script = view.scripts.find((s) => s.phase === p.id);
        const trust = dirty ? null : trustLine(script);
        const problem = draft.problems[`script:${p.id}`] ?? null;
        return (
          <Field
            key={p.id}
            label={t("work-workstream-scripts-card-script", { p: p.label })}
            hint={
              problem ? (
                <span className="text-danger">{problem}</span>
              ) : (
                <>
                  {p.when}
                  {trust && (
                    <>
                      {" "}
                      <Chip tone={trust.tone}>{trust.text}</Chip>
                    </>
                  )}
                </>
              )
            }
          >
            <TextArea
              rows={3}
              spellCheck={false}
              className="font-mono text-2xs"
              placeholder={p.id === "post_create" ? t("work-workstream-scripts-card-npm-install") : p.id === "clean" ? t("work-workstream-scripts-card-docker-compose-down") : "cp .env.example .env"} // for the machine
              value={edits.texts[p.id] ?? ""}
              onChange={(e) => draft.editScripts({ ...edits, texts: { ...edits.texts, [p.id]: e.target.value } })}
            />
          </Field>
        );
      })}
      <Field label={t("work-workstream-scripts-card-timeout-seconds")} hint={t("work-workstream-scripts-card-how-long-script-may-run-before")}>
        <NumberInput value={edits.timeout} min={1} max={3600} className="w-28" onCommit={(timeout) => draft.editScripts({ ...edits, timeout })} />
      </Field>
      <div>
        <SectionHeader title={t("work-workstream-scripts-card-what-script-told")} open={!envFolded} onToggle={toggleEnv} />
        {!envFolded && (
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 pl-2 text-2xs text-text-dim">
            {ENV_VARS.map(([name, meaning]) => (
              <div key={name} className="[display:contents]">
                <dt className="font-mono text-text">{name}</dt>
                <dd>{meaning}</dd>
              </div>
            ))}
          </dl>
        )}
      </div>
    </div>
  );
}
